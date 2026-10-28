mod archive;
mod http;
pub use http::HttpFile;
mod input;
mod library;
#[cfg(feature = "publication")]
pub mod publication;
mod range;
pub mod reference;
mod server;
pub mod video;

pub use input::MediaInput;
pub use library::{Entry, Library};
pub use range::RangeReader;
pub use reference::Reference;
pub use reqwest::Url;
use serde::{Deserialize, Serialize};
pub use server::SourceServer;

pub const RESOURCE_NAMESPACE: &str = "lince.resource";
pub const SOURCE_NAMESPACE: &str = "lince.source";
pub const INSTANCE_NAMESPACE: &str = "lince.instance";
pub const CHUNK_BYTES: usize = 64 * 1024;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Locator {
    pub href: String,
    pub fragment: String,
    pub quote: String,
    pub image: String,
    pub page: Option<usize>,
    pub end_page: Option<usize>,
    pub region: Option<[f32; 4]>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

impl Locator {
    pub fn validate(&self) -> Result<()> {
        if self.href.len() > 4096
            || self.fragment.len() > 4096
            || self.quote.len() > 65536
            || self.image.len() > 4096
            || self.page == Some(0)
            || self
                .end_page
                .is_some_and(|end| end < self.page.unwrap_or(1))
            || self.region.is_some_and(|region| {
                region
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    || region[2] <= 0.0
                    || region[3] <= 0.0
                    || region[0] + region[2] > 1.0001
                    || region[1] + region[3] > 1.0001
            })
            || self
                .end_ms
                .is_some_and(|end| end <= self.start_ms.unwrap_or(0))
        {
            return Err("Choose a valid page, passage, image region or time interval".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Representation {
    pub label: String,
    pub uri: String,
    pub media_type: String,
    pub hash: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Resource {
    pub uri: String,
    pub locations: Vec<String>,
    pub source_uri: String,
    pub title: String,
    pub media_type: String,
    pub hash: Option<String>,
    pub aliases: Vec<String>,
    pub locator: Locator,
    pub representations: Vec<Representation>,
    pub attribution: String,
    pub license: String,
}

impl Resource {
    pub fn representation(&self, index: usize) -> Result<Self> {
        let selected = self
            .representations
            .get(index)
            .ok_or("Choose an available representation")?;
        let mut resource = self.clone();
        resource.uri = selected.uri.clone();
        resource.locations.clear();
        resource.hash = selected.hash.clone();
        if !selected.media_type.is_empty() {
            resource.media_type = selected.media_type.clone();
        }
        resource.validate()?;
        Ok(resource)
    }

    pub fn validate(&self) -> Result<()> {
        self.locator.validate()?;
        if self.uri.is_empty()
            || self.uri.len() > 8192
            || self.uri.contains('\0')
            || self.title.len() > 65536
            || self.aliases.len() > 256
            || self.locations.len() > 64
            || self
                .locations
                .iter()
                .any(|uri| uri.len() > 8192 || uri.contains('\0'))
            || self.representations.len() > 128
            || self.representations.iter().any(|representation| {
                representation.uri.is_empty()
                    || representation.uri.len() > 8192
                    || representation.uri.contains('\0')
                    || representation.label.len() > 4096
                    || representation.hash.as_ref().is_some_and(|hash| {
                        hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
            })
            || self.hash.as_ref().is_some_and(|hash| {
                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err("Check the resource address and content identity".into());
        }
        Ok(())
    }
}

pub fn address(value: &str) -> Result<Url> {
    let url = Url::parse(value)
        .or_else(|_| {
            std::path::absolute(value)
                .map_err(|_| ())
                .and_then(|path| Url::from_file_path(path).map_err(|_| ()))
        })
        .map_err(|_| "Choose a file path or HTTP(S) address")?;
    if !matches!(url.scheme(), "http" | "https" | "file")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Resources accept file paths and HTTP(S) addresses without credentials".into());
    }
    Ok(url)
}

pub fn resolve(base: &str, value: &str) -> Result<String> {
    let base = address(base)?;
    let resolved = base.join(value).map_err(|error| error.to_string())?;
    let resolved = address(resolved.as_str())?;
    if matches!(base.scheme(), "http" | "https") && resolved.scheme() == "file" {
        return Err("An online publication cannot reference a local file".into());
    }
    Ok(resolved.to_string())
}

pub fn fingerprint(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
