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
