// non-lean-yet
// Scope: struct MediaInput.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 37, 48, 50, 51.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   26, 41.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 10.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 3.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 3, 4,
//   5, 10.

use crate::{Library, RangeReader, Resource, Result, SourceServer, address};

pub struct MediaInput {
    pub uri: String,
    pub version: String,
    _server: Option<SourceServer>,
}

impl MediaInput {
    pub fn open(library: &Library, resource: &Resource) -> Result<Self> {
        resource.validate()?;
        if resource.hash.is_some()
            && let Some(entry) = library.find(resource)?
        {
            return Ok(Self {
                uri: entry.path.to_string_lossy().into_owned(),
                version: entry.hash,
                _server: None,
            });
        }
        let uri = address(&resource.uri)?;
        if uri.scheme() == "file" && resource.locations.is_empty() {
            if resource.hash.is_some() {
                let entry = library.register_in_place(
                    &uri.to_file_path()
                        .map_err(|_| "Invalid local media address")?,
                    &resource.uri,
                    &resource.media_type,
                    resource.hash.as_deref(),
                )?;
                return Ok(Self {
                    uri: entry.path.to_string_lossy().into_owned(),
                    version: entry.hash,
                    _server: None,
                });
            }
            let version = RangeReader::open(library.clone(), resource)?.version();
            return Ok(Self {
                uri: uri
                    .to_file_path()
                    .map_err(|_| "Invalid local media file")?
                    .to_string_lossy()
                    .into_owned(),
                version,
                _server: None,
            });
        }
        match SourceServer::start(library.clone(), resource) {
            Ok(server) => Ok(Self {
                uri: server.uri.clone(),
                version: server.version.clone(),
                _server: Some(server),
            }),
            Err(error) => {
                if let Some(entry) = library.find(resource)? {
                    Ok(Self {
                        uri: entry.path.to_string_lossy().into_owned(),
                        version: entry.hash,
                        _server: None,
                    })
                } else {
                    Err(error)
                }
            }
        }
    }
}
