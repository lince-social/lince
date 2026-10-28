use crate::{Library, Result, address, fingerprint};
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Stored {
    uri: String,
    headers: BTreeMap<String, String>,
    vary: BTreeMap<String, Option<String>>,
    response_at: u64,
    initial_age: u64,
    body: String,
    bytes: u64,
}

pub struct HttpFile {
    pub path: PathBuf,
    pub uri: String,
    pub cached: bool,
    _temporary: Option<tempfile::NamedTempFile>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn date(value: Option<&String>) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc2822(value?).ok()?;
    u64::try_from(parsed.timestamp()).ok()
}

fn headers(values: &HeaderMap) -> BTreeMap<String, String> {
    values
        .keys()
        .filter_map(|name| {
            let values = values
                .get_all(name)
                .iter()
                .map(|value| value.to_str())
                .collect::<std::result::Result<Vec<_>, _>>()
                .ok()?;
            Some((name.as_str().to_owned(), values.join(", ")))
        })
        .collect()
}

fn directives(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut output = BTreeMap::new();
    for value in headers
        .get("cache-control")
        .into_iter()
        .flat_map(|value| value.split(','))
    {
        let (key, value) = value.trim().split_once('=').unwrap_or((value.trim(), ""));
        let key = key.to_ascii_lowercase();
        let value = value.trim().trim_matches('"').to_owned();
        if key == "max-age" && output.contains_key(&key) {
            output.insert("no-cache".into(), String::new());
        }
        output.insert(key, value);
    }
    output
}

pub(crate) fn range_cacheable(values: &HeaderMap) -> bool {
    let values = headers(values);
    let controls = directives(&values);
    !controls.contains_key("no-store")
        && !controls.contains_key("no-cache")
        && !values.get("vary").is_some_and(|value| {
            value.split(',').any(|name| {
                !matches!(
                    name.trim().to_ascii_lowercase().as_str(),
                    "accept-encoding" | "range"
                )
            })
        })
}

impl Stored {
    fn fresh(&self, at: u64) -> bool {
        let controls = directives(&self.headers);
        if controls.contains_key("no-cache") || controls.contains_key("no-store") {
            return false;
        }
        let lifetime = controls
            .get("max-age")
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| {
                date(self.headers.get("expires")).map(|expires| {
                    expires
                        .saturating_sub(date(self.headers.get("date")).unwrap_or(self.response_at))
                })
            })
            .unwrap_or(0);
        self.initial_age
            .saturating_add(at.saturating_sub(self.response_at))
            < lifetime
    }

    fn matches(&self, request: &BTreeMap<String, String>) -> bool {
        self.vary
            .iter()
            .all(|(name, value)| request.get(name) == value.as_ref())
    }
}

fn flight(key: &str) -> Result<Arc<Mutex<()>>> {
    static FLIGHTS: OnceLock<Mutex<BTreeMap<String, Weak<Mutex<()>>>>> = OnceLock::new();
    let mut flights = FLIGHTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?;
    flights.retain(|_, value| value.strong_count() > 0);
    let lock = flights
        .get(key)
        .and_then(Weak::upgrade)
        .unwrap_or_else(|| Arc::new(Mutex::new(())));
    flights.insert(key.into(), Arc::downgrade(&lock));
    Ok(lock)
}

impl Library {
    pub fn http_file(
        &self,
        uri: &str,
        request: HeaderMap,
        limit: u64,
        refresh: bool,
    ) -> Result<HttpFile> {
        let mut uri = address(uri)?;
        if !matches!(uri.scheme(), "http" | "https") {
            return Err("Choose an HTTP(S) address".into());
        }
        uri.set_fragment(None);
        let request = self
            .client()?
            .get(uri.clone())
            .headers(request)
            .header("Accept-Encoding", "identity")
            .build()
            .map_err(|error| error.to_string())?;
        let request_headers = headers(request.headers());
        let key = fingerprint(
            format!(
                "GET {uri}\n{}",
                serde_json::to_string(&request_headers).map_err(|error| error.to_string())?
            )
            .as_bytes(),
        );
        let lock = flight(&format!("{}:{key}", self.cache().display()))?;
        let _guard = lock.lock().map_err(|error| error.to_string())?;
        let directory = self.cache().join("http");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let metadata = directory.join(format!("{key}.json"));
        let stored = fs::read(&metadata)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .filter(|stored| {
                stored.matches(&request_headers)
                    && stored.bytes <= limit
                    && directory.join(&stored.body).is_file()
            });
        if !refresh
            && let Some(stored) = &stored
            && stored.fresh(now())
        {
            return Ok(HttpFile {
                path: directory.join(&stored.body),
                uri: stored.uri.clone(),
                cached: true,
                _temporary: None,
            });
        }
        let mut request = request;
        if let Some(stored) = &stored {
            for (response, condition) in [
                ("etag", "if-none-match"),
                ("last-modified", "if-modified-since"),
            ] {
                if let Some(value) = stored.headers.get(response) {
                    request.headers_mut().insert(
                        HeaderName::from_bytes(condition.as_bytes())
                            .map_err(|error| error.to_string())?,
                        HeaderValue::from_str(value).map_err(|error| error.to_string())?,
                    );
                    break;
                }
            }
        }
        let started = now();
        let mut response = self
            .client()?
            .execute(request)
            .map_err(|error| error.to_string())?;
        let response_at = now();
        let received_headers = headers(response.headers());
        let not_modified = response.status() == StatusCode::NOT_MODIFIED;
        let (mut values, resolved) = if not_modified {
            let stored = stored
                .as_ref()
                .ok_or("Unexpected 304 without a stored representation")?;
            let mut values = stored.headers.clone();
            values.remove("age");
            values.remove("date");
            values.extend(received_headers);
            (values, response.url().to_string())
        } else {
            if response.status() != StatusCode::OK {
                return Err(format!("HTTP {} for {uri}", response.status()));
            }
            (received_headers, response.url().to_string())
        };
        let controls = directives(&values);
        let mut vary = BTreeMap::new();
        let mut storable = !controls.contains_key("no-store");
        for name in values
            .get("vary")
            .into_iter()
            .flat_map(|value| value.split(','))
        {
            let name = name.trim().to_ascii_lowercase();
            if name == "*" || name.is_empty() {
                storable = false;
            }
            vary.insert(name.clone(), request_headers.get(&name).cloned());
        }
        let age = values
            .get("age")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let initial_age = response_at
            .saturating_sub(date(values.get("date")).unwrap_or(response_at))
            .max(age.saturating_add(response_at.saturating_sub(started)));
        let mut temporary =
            tempfile::NamedTempFile::new_in(&directory).map_err(|error| error.to_string())?;
        let mut hasher = blake3::Hasher::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        let mut previous;
        let input: &mut dyn Read = if not_modified {
            previous = fs::File::open(directory.join(&stored.as_ref().unwrap().body))
                .map_err(|error| error.to_string())?;
            &mut previous
        } else {
            &mut response
        };
        loop {
            let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            bytes = bytes
                .checked_add(count as u64)
                .ok_or("Resource length overflow")?;
            if bytes > limit {
                return Err("The source exceeds its per-operation size limit".into());
            }
            hasher.update(&buffer[..count]);
            temporary
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
        if !storable {
            if metadata.exists() {
                fs::remove_file(&metadata).map_err(|error| error.to_string())?;
            }
            if let Some(stored) = &stored {
                let path = directory.join(&stored.body);
                if path.exists() {
                    fs::remove_file(path).map_err(|error| error.to_string())?;
                }
            }
            return Ok(HttpFile {
                path: temporary.path().into(),
                uri: resolved,
                cached: false,
                _temporary: Some(temporary),
            });
        }
        values.remove("connection");
        let body = format!("{key}-{}.body", hasher.finalize().to_hex());
        let path = directory.join(&body);
        if !path.exists() {
            temporary
                .persist(&path)
                .map_err(|error| error.to_string())?;
        }
        let stored = Stored {
            uri: resolved.clone(),
            headers: values,
            vary,
            response_at,
            initial_age,
            body,
            bytes,
        };
        let mut temporary =
            tempfile::NamedTempFile::new_in(&directory).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut temporary, &stored).map_err(|error| error.to_string())?;
        temporary
            .persist(metadata)
            .map_err(|error| error.to_string())?;
        Ok(HttpFile {
            path,
            uri: resolved,
            cached: not_modified,
            _temporary: None,
        })
    }
}
