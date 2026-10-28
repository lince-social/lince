use crate::{Resource, Result, address, fingerprint};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

static CATALOG: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub hash: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub uri: String,
    pub media_type: String,
    #[serde(default)]
    pub resolved_uri: Option<String>,
    #[serde(default)]
    pub registered_at: u64,
}

#[derive(Clone, Debug)]
pub struct Library {
    root: PathBuf,
    cache: PathBuf,
    client: Arc<OnceLock<Result<reqwest::blocking::Client>>>,
    bases: Arc<Mutex<BTreeMap<String, (String, bool)>>>,
    local_files: bool,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let cache = root.join("cache");
        Self {
            root,
            cache,
            client: Arc::new(OnceLock::new()),
            bases: Arc::new(Mutex::new(BTreeMap::new())),
            local_files: true,
        }
    }

    pub fn remote(mut self) -> Self {
        self.local_files = false;
        self
    }
    pub fn allows_local_files(&self) -> bool {
        self.local_files
    }

    pub fn with_cache(mut self, cache: impl Into<PathBuf>) -> Self {
        self.cache = cache.into();
        self
    }
    pub fn cache(&self) -> &Path {
        &self.cache
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn client(&self) -> Result<reqwest::blocking::Client> {
        self.client
            .get_or_init(|| {
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(120))
                    .redirect(reqwest::redirect::Policy::custom(|attempt| {
                        if attempt.previous().len() >= 8
                            || address(attempt.url().as_str()).is_err()
                            || !matches!(attempt.url().scheme(), "http" | "https")
                        {
                            attempt.stop()
                        } else {
                            attempt.follow()
                        }
                    }))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .clone()
    }

    fn catalogue(&self) -> PathBuf {
        self.root.join("files").join("catalogue.json")
    }

    fn entries(&self) -> Result<BTreeMap<String, Entry>> {
        match fs::read(self.catalogue()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn find(&self, resource: &Resource) -> Result<Option<Entry>> {
        let _guard = CATALOG
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        let entries = self.entries()?;
        let found = resource
            .hash
            .as_ref()
            .and_then(|hash| {
                entries
                    .get(&format!("lince-blob:{}", hash.to_ascii_lowercase()))
                    .filter(|entry| entry.path.is_file())
                    .or_else(|| {
                        entries.values().find(|entry| {
                            entry.path.is_file() && entry.hash.eq_ignore_ascii_case(hash)
                        })
                    })
            })
            .or_else(|| entries.get(&resource.uri));
        let found = found
            .filter(|entry| {
                entry.path.is_file()
                    && resource
                        .hash
                        .as_ref()
                        .is_none_or(|hash| entry.hash.eq_ignore_ascii_case(hash))
            })
            .cloned();
        drop(_guard);
        if let Some(entry) = &found
            && entry.path != self.root.join("files").join(&entry.hash)
        {
            let mut source = File::open(&entry.path).map_err(|error| error.to_string())?;
            let mut hasher = blake3::Hasher::new();
            std::io::copy(&mut source, &mut hasher).map_err(|error| error.to_string())?;
            if !entry.hash.eq_ignore_ascii_case(&hasher.finalize().to_hex()) {
                return Ok(None);
            }
        }
        Ok(found)
    }

    pub fn register(
        &self,
        path: &Path,
        uri: &str,
        media_type: &str,
        expected: Option<&str>,
    ) -> Result<Entry> {
        self.register_with_base(path, uri, media_type, expected, None)
    }

    pub fn register_in_place(
        &self,
        path: &Path,
        uri: &str,
        media_type: &str,
        expected: Option<&str>,
    ) -> Result<Entry> {
        let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
        let mut file = File::open(&path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("Choose an ordinary file".into());
        }
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
        let hash = hasher.finalize().to_hex().to_string();
        if expected.is_some_and(|expected| !expected.eq_ignore_ascii_case(&hash)) {
            return Err("The local file does not match its content hash".into());
        }
        fs::create_dir_all(self.root.join("files")).map_err(|error| error.to_string())?;
        self.save_entry(Entry {
            hash,
            path,
            bytes: metadata.len(),
            uri: uri.into(),
            media_type: media_type.into(),
            resolved_uri: None,
            registered_at: 0,
        })
    }

    pub fn retain_bytes(&self, bytes: &[u8], uri: &str, base: &str) -> Result<Entry> {
        self.register_reader(bytes, uri, "text/x-lingua", None, Some(base.into()))
    }

    pub fn latest(&self, prefix: &str) -> Result<Option<Entry>> {
        let _guard = CATALOG
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        Ok(self
            .entries()?
            .into_values()
            .filter(|entry| entry.uri.starts_with(prefix) && entry.path.is_file())
            .max_by_key(|entry| entry.registered_at))
    }

    fn register_with_base(
        &self,
        path: &Path,
        uri: &str,
        media_type: &str,
        expected: Option<&str>,
        resolved_uri: Option<String>,
    ) -> Result<Entry> {
        let source = File::open(path).map_err(|error| error.to_string())?;
        if !source
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Choose an ordinary media file".into());
        }
        self.register_reader(source, uri, media_type, expected, resolved_uri)
    }

    fn register_reader(
        &self,
        mut source: impl Read,
        uri: &str,
        media_type: &str,
        expected: Option<&str>,
        resolved_uri: Option<String>,
    ) -> Result<Entry> {
        let files = self.root.join("files");
        fs::create_dir_all(&files).map_err(|error| error.to_string())?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(&files).map_err(|error| error.to_string())?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut bytes = 0u64;
        loop {
            let count = source
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            temporary
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            bytes = bytes
                .checked_add(count as u64)
                .ok_or("Resource length overflow")?;
        }
        let hash = hasher.finalize().to_hex().to_string();
        if expected.is_some_and(|expected| !expected.eq_ignore_ascii_case(&hash)) {
            return Err(
                "The downloaded resource does not match its declared content identity".into(),
            );
        }
        let destination = files.join(&hash);
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        if !destination.exists() {
            if let Err(error) = temporary.persist_noclobber(&destination)
                && error.error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(error.to_string());
            }
        }
        let entry = Entry {
            hash,
            path: destination,
            bytes,
            uri: uri.into(),
            media_type: media_type.into(),
            resolved_uri,
            registered_at: 0,
        };
        self.save_entry(entry)
    }

    fn save_entry(&self, mut entry: Entry) -> Result<Entry> {
        entry.registered_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let _guard = CATALOG
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        let mut entries = self.entries()?;
        entries.insert(format!("lince-blob:{}", entry.hash), entry.clone());
        entries.insert(entry.uri.clone(), entry.clone());
        let mut temporary = tempfile::NamedTempFile::new_in(self.root.join("files"))
            .map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut temporary, &entries).map_err(|error| error.to_string())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temporary
            .persist(self.catalogue())
            .map_err(|error| error.to_string())?;
        Ok(entry)
    }

    pub fn fetch(&self, resource: &Resource) -> Result<Entry> {
        resource.validate()?;
        let reference = crate::Reference {
            target: resource.uri.clone(),
            locations: resource.locations.clone(),
            hash: resource.hash.clone(),
            ..Default::default()
        };
        let mut failures = Vec::new();
        for address in reference.addresses(self)? {
            let mut candidate = resource.clone();
            candidate.uri = address.uri;
            candidate.locations.clear();
            match self.fetch_one(&candidate) {
                Ok(entry) => return Ok(entry),
                Err(error) => failures.push(error),
            }
        }
        Err(failures.join("\n"))
    }

    fn fetch_one(&self, resource: &Resource) -> Result<Entry> {
        resource.validate()?;
        if resource.hash.is_some()
            && let Some(entry) = self.find(resource)?
        {
            if entry.path == self.root.join("files").join(&entry.hash) {
                return Ok(entry);
            }
            return self.register(
                &entry.path,
                &resource.uri,
                &resource.media_type,
                Some(&entry.hash),
            );
        }
        let uri = address(&resource.uri)?;
        if uri.scheme() == "file" {
            return self.register(
                &uri.to_file_path().map_err(|_| "Invalid file address")?,
                &resource.uri,
                &resource.media_type,
                resource.hash.as_deref(),
            );
        }
        let cache = self.cache.join("downloads");
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        if let Ok(mut reader) = crate::RangeReader::partial(self.clone(), resource) {
            let mut temporary =
                tempfile::NamedTempFile::new_in(&cache).map_err(|error| error.to_string())?;
            match std::io::copy(&mut reader, &mut temporary) {
                Ok(_) => {
                    return self.register(
                        temporary.path(),
                        &resource.uri,
                        &resource.media_type,
                        resource.hash.as_deref(),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let response = self.http_file(&resource.uri, Default::default(), u64::MAX, false)?;
        self.register_with_base(
            &response.path,
            &resource.uri,
            &resource.media_type,
            resource.hash.as_deref(),
            Some(response.uri.clone()),
        )
    }

    pub fn text(&self, uri: &str, limit: u64) -> Result<String> {
        let resource = Resource {
            uri: uri.into(),
            ..Default::default()
        };
        let entry = self.fetch(&resource)?;
        if entry.bytes > limit {
            return Err("This source document exceeds its per-document parsing limit".into());
        }
        fs::read_to_string(entry.path).map_err(|error| error.to_string())
    }

    pub fn source_text(&self, uri: &str, limit: u64, refresh: bool) -> Result<String> {
        let resource = Resource {
            uri: uri.into(),
            ..Default::default()
        };
        let url = address(uri)?;
        if url.scheme() == "file" {
            if !self.local_files {
                return Err("Local files require the local owner session".into());
            }
            let path = url.to_file_path().map_err(|_| "Invalid local source")?;
            let file = File::open(path).map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            file.take(limit + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 > limit {
                return Err("Source document exceeds its parsing limit".into());
            }
            let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
            let hash = fingerprint(text.as_bytes());
            if self.find(&resource)?.is_none_or(|entry| entry.hash != hash) {
                if let Some(mut entry) = self.find(&Resource {
                    hash: Some(hash),
                    ..resource
                })? {
                    entry.uri = uri.into();
                    entry.resolved_uri = None;
                    self.save_entry(entry)?;
                } else {
                    self.register_reader(text.as_bytes(), uri, "text/x-lingua", None, None)?;
                }
            }
            return Ok(text);
        }
        let response = self.http_file(uri, Default::default(), limit, refresh)?;
        self.bases
            .lock()
            .map_err(|error| error.to_string())?
            .insert(uri.into(), (response.uri, response.cached));
        fs::read_to_string(response.path).map_err(|error| error.to_string())
    }

    pub fn source_cached(&self, uri: &str) -> bool {
        self.bases
            .lock()
            .ok()
            .and_then(|bases| bases.get(uri).map(|entry| entry.1))
            .unwrap_or(false)
    }

    pub fn source_base(&self, uri: &str) -> Result<String> {
        if let Some(base) = self
            .bases
            .lock()
            .map_err(|error| error.to_string())?
            .get(uri)
        {
            return Ok(base.0.clone());
        }
        Ok(self
            .find(&Resource {
                uri: uri.into(),
                ..Default::default()
            })?
            .and_then(|entry| entry.resolved_uri)
            .unwrap_or_else(|| uri.into()))
    }

    pub(crate) fn cache_key(&self, uri: &str) -> PathBuf {
        self.cache.join("ranges").join(fingerprint(uri.as_bytes()))
    }
}
