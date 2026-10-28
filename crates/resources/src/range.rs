use crate::{CHUNK_BYTES, Library, Resource, Result, address, fingerprint};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderMap, IF_RANGE, RANGE};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    uri: String,
    length: u64,
    validator: String,
    #[serde(default)]
    expected_hash: Option<String>,
}

impl Snapshot {
    fn key(&self) -> String {
        fingerprint(
            format!(
                "{}:{}:{:?}",
                self.validator, self.length, self.expected_hash
            )
            .as_bytes(),
        )
    }
}

pub struct RangeReader {
    library: Library,
    snapshot: Snapshot,
    directory: PathBuf,
    position: u64,
    local: Option<File>,
    offline: bool,
    _http: Option<crate::HttpFile>,
}

fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}

fn validator(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(ETAG)
        .filter(|value| !value.as_bytes().starts_with(b"W/"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value.len() >= 2
                && value.starts_with('"')
                && value.ends_with('"')
                && value.as_bytes()[1..value.len() - 1]
                    .iter()
                    .all(|byte| *byte == 0x21 || (0x23..=0x7e).contains(byte))
        })
}

impl RangeReader {
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let uri = crate::address(&path.to_string_lossy())?.to_string();
        Self::local(Library::new(PathBuf::new()), path, &uri)
    }
    pub fn cached(library: &Library, resource: &Resource) -> (u64, Option<u64>) {
        let directory = library.cache_key(&resource.uri);
        let snapshot = fs::read(directory.join("snapshot.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok());
        let Some(snapshot) = snapshot else {
            return (0, None);
        };
        if snapshot.expected_hash != resource.hash {
            return (0, None);
        }
        let directory = directory.join(snapshot.key());
        let cached = fs::read_dir(directory)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.parse::<u64>().is_ok())
            })
            .filter_map(|entry| entry.metadata().ok())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .sum::<u64>();
        (cached.min(snapshot.length), Some(snapshot.length))
    }
    pub fn open(library: Library, resource: &Resource) -> Result<Self> {
        Self::open_with(library, resource, true)
    }

    pub(crate) fn partial(library: Library, resource: &Resource) -> Result<Self> {
        Self::open_with(library, resource, false)
    }

    fn open_with(library: Library, resource: &Resource, fallback: bool) -> Result<Self> {
        resource.validate()?;
        let reference = crate::Reference {
            target: resource.uri.clone(),
            locations: resource.locations.clone(),
            hash: resource.hash.clone(),
            ..Default::default()
        };
        let mut failures = Vec::new();
        for address in reference.addresses(&library)? {
            let mut candidate = resource.clone();
            candidate.uri = address.uri;
            candidate.locations.clear();
            match Self::open_one(library.clone(), &candidate, fallback) {
                Ok(reader) => return Ok(reader),
                Err(error) => failures.push(error),
            }
        }
        Err(failures.join("\n"))
    }

    fn open_one(library: Library, resource: &Resource, fallback: bool) -> Result<Self> {
        resource.validate()?;
        if resource.hash.is_some()
            && let Some(entry) = library.find(resource)?
        {
            return Self::local(library, &entry.path, &resource.uri);
        }
        let uri = address(&resource.uri)?;
        if uri.scheme() == "file" {
            if let Some(hash) = &resource.hash {
                let entry = library.register_in_place(
                    &uri.to_file_path().map_err(|_| "Invalid local path")?,
                    &resource.uri,
                    &resource.media_type,
                    Some(hash),
                )?;
                return Self::local(library, &entry.path, &resource.uri);
            }
            return Self::local(
                library,
                &uri.to_file_path().map_err(|_| "Invalid file address")?,
                &resource.uri,
            );
        }
        let index = library.cache_key(&resource.uri);
        let mut full_required = false;
        let discovered = (|| {
            let response = library
                .client()?
                .get(uri)
                .header(RANGE, "bytes=0-0")
                .send()
                .map_err(|error| error.to_string())?;
            if !crate::http::range_cacheable(response.headers()) {
                full_required = true;
                return Err("The source requires a full response for its cache policy".to_string());
            }
            if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                full_required = response.status() == reqwest::StatusCode::OK;
                return Err("The source requires a full download".to_string());
            }
            let total = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit('/').next())
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|length| *length > 0)
                .ok_or("The source did not declare its content length")?;
            let Some(validator) = validator(response.headers()) else {
                full_required = true;
                return Err(
                    "The source requires a full download to keep its version consistent".into(),
                );
            };
            let validator = validator.to_string();
            Ok(Snapshot {
                uri: resource.uri.clone(),
                length: total,
                validator,
                expected_hash: resource.hash.clone(),
            })
        })();
        let (snapshot, offline) = match discovered {
            Ok(snapshot) => {
                fs::create_dir_all(&index).map_err(|error| error.to_string())?;
                let bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
                let mut temporary =
                    tempfile::NamedTempFile::new_in(&index).map_err(|error| error.to_string())?;
                temporary
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())?;
                temporary
                    .persist(index.join("snapshot.json"))
                    .map_err(|error| error.to_string())?;
                (snapshot, false)
            }
            Err(_) if full_required => {
                if !fallback {
                    return Err("The source requires a full download".into());
                }
                return Self::full(library, resource);
            }
            Err(problem) => {
                match fs::read(index.join("snapshot.json")).and_then(|bytes| {
                    if resource.hash.is_some() {
                        Ok(bytes)
                    } else {
                        Err(io::Error::other("Choose a retained offline version"))
                    }
                }) {
                    Ok(bytes) => (
                        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?,
                        true,
                    ),
                    Err(_) => {
                        if !fallback {
                            return Err(problem);
                        }
                        return Self::full(library, resource);
                    }
                }
            }
        };
        if snapshot.expected_hash != resource.hash {
            return Err("Cached parts belong to another source revision".into());
        }
        let directory = index.join(snapshot.key());
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        Ok(Self {
            library,
            snapshot,
            directory,
            position: 0,
            local: None,
            offline,
            _http: None,
        })
    }

    fn full(library: Library, resource: &Resource) -> Result<Self> {
        let response = library.http_file(&resource.uri, Default::default(), u64::MAX, false)?;
        if let Some(expected) = &resource.hash {
            let mut file = File::open(&response.path).map_err(|error| error.to_string())?;
            let mut hasher = blake3::Hasher::new();
            io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
            if !expected.eq_ignore_ascii_case(&hasher.finalize().to_hex()) {
                return Err("The source does not match its content hash".into());
            }
        }
        let mut reader = Self::local(library, &response.path, &resource.uri)?;
        reader._http = Some(response);
        Ok(reader)
    }

    fn local(library: Library, path: &std::path::Path, uri: &str) -> Result<Self> {
        let local = File::open(path).map_err(|error| error.to_string())?;
        let length = local.metadata().map_err(|error| error.to_string())?.len();
        Ok(Self {
            library,
            snapshot: Snapshot {
                uri: uri.into(),
                length,
                validator: format!(
                    "{:?}",
                    local
                        .metadata()
                        .map_err(|error| error.to_string())?
                        .modified()
                        .map_err(|error| error.to_string())?
                ),
                expected_hash: None,
            },
            directory: PathBuf::new(),
            position: 0,
            local: Some(local),
            offline: false,
            _http: None,
        })
    }

    pub fn len(&self) -> u64 {
        self.snapshot.length
    }
    pub fn is_empty(&self) -> bool {
        self.snapshot.length == 0
    }
    pub fn version(&self) -> String {
        fingerprint(
            format!(
                "{}:{}:{}",
                self.snapshot.uri, self.snapshot.validator, self.snapshot.length
            )
            .as_bytes(),
        )
    }

    fn verify_complete(&self, added: bool) -> io::Result<()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let Some(expected) = &self.snapshot.expected_hash else {
            return Ok(());
        };
        let _guard = LOCK.lock().map_err(error)?;
        let count_path = self.directory.join("count");
        if added && self.directory.join("verified").exists() {
            fs::remove_file(self.directory.join("verified"))?;
        }
        let previous = fs::read_to_string(&count_path)
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        let count = if let Some(previous) = previous {
            previous.saturating_add(u64::from(added))
        } else {
            fs::read_dir(&self.directory)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.parse::<u64>().is_ok())
                })
                .count() as u64
        };
        if previous != Some(count) {
            let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
            write!(temporary, "{count}")?;
            temporary.persist(&count_path).map_err(error)?;
        }
        let needed = self.snapshot.length.div_ceil(CHUNK_BYTES as u64);
        if count < needed || self.directory.join("verified").is_file() {
            return Ok(());
        }
        let mut hasher = blake3::Hasher::new();
        let mut total = 0u64;
        for index in 0..needed {
            let mut file = match File::open(self.directory.join(index.to_string())) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::remove_file(count_path)?;
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            total = total
                .checked_add(io::copy(&mut file, &mut hasher)?)
                .ok_or_else(|| error("Invalid cached resource length"))?;
        }
        if total != self.snapshot.length
            || !expected.eq_ignore_ascii_case(&hasher.finalize().to_hex())
        {
            return Err(error(
                "The complete cached resource does not match its declared content hash",
            ));
        }
        File::create(self.directory.join("verified"))?;
        Ok(())
    }

    fn chunk(&self, index: u64) -> io::Result<Vec<u8>> {
        let path = self.directory.join(index.to_string());
        let start = index
            .checked_mul(CHUNK_BYTES as u64)
            .ok_or_else(|| error("Invalid range"))?;
        let end = start
            .saturating_add(CHUNK_BYTES as u64)
            .min(self.snapshot.length)
            .checked_sub(1)
            .ok_or_else(|| error("Invalid range"))?;
        if let Ok(bytes) = fs::read(&path)
            && bytes.len() as u64 == end - start + 1
        {
            self.verify_complete(false)?;
            return Ok(bytes);
        }
        if self.offline {
            return Err(error(
                "This part is not cached. Connect or download the complete resource.",
            ));
        }
        let mut response = self
            .library
            .client()
            .map_err(error)?
            .get(address(&self.snapshot.uri).map_err(error)?)
            .header(RANGE, format!("bytes={start}-{end}"))
            .header(IF_RANGE, &self.snapshot.validator)
            .send()
            .map_err(error)?;
        let expected = format!("bytes {start}-{end}/{}", self.snapshot.length);
        if response.status() == reqwest::StatusCode::OK {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "This source requires a complete download. Keep it offline to read it.",
            ));
        }
        if !crate::http::range_cacheable(response.headers())
            || response.status() != reqwest::StatusCode::PARTIAL_CONTENT
            || validator(response.headers()) != Some(self.snapshot.validator.as_str())
            || response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                != Some(expected.as_str())
            || response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|length| length != end - start + 1)
        {
            return Err(error(
                "The source changed or no longer supports partial access. Reopen it.",
            ));
        }
        let mut bytes = Vec::new();
        (&mut response)
            .take(end - start + 2)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != end - start + 1 {
            return Err(error("The source returned an incomplete range"));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        temporary.write_all(&bytes)?;
        let added = match temporary.persist_noclobber(&path) {
            Ok(_) => true,
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(error.into()),
        };
        self.verify_complete(added)?;
        Ok(bytes)
    }
}

impl Read for RangeReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if let Some(local) = &mut self.local {
            return local.read(output);
        }
        if self.position >= self.snapshot.length || output.is_empty() {
            return Ok(0);
        }
        let index = self.position / CHUNK_BYTES as u64;
        let offset = (self.position % CHUNK_BYTES as u64) as usize;
        let bytes = self.chunk(index)?;
        if offset >= bytes.len() {
            return Err(error("A cached range is incomplete"));
        }
        let count = output.len().min(bytes.len() - offset);
        output[..count].copy_from_slice(&bytes[offset..offset + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if let Some(local) = &mut self.local {
            return local.seek(position);
        }
        let position = match position {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::End(delta) => i128::from(self.snapshot.length) + i128::from(delta),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
        };
        self.position = u64::try_from(position).map_err(|_| error("Invalid seek position"))?;
        Ok(self.position)
    }
}
