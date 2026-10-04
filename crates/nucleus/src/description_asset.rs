use serde::{Deserialize, Serialize};

pub const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Drawing,
    Png,
    Webp,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Drawing => "drawing",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Put {
        record: String,
        kind: Kind,
        data_base64: String,
    },
    Get {
        record: String,
        asset: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Stored { asset: String },
    Data { kind: Kind, data_base64: String },
}

pub fn reference(record: &str, asset: &str) -> String {
    format!("asset:{record}/{asset}")
}

pub fn parse_reference(source: &str) -> Option<(&str, &str)> {
    let (record, asset) = source.strip_prefix("asset:")?.split_once('/')?;
    (crate::valid_uid(record, "r")
        && asset.len() == 64
        && asset
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    .then_some((record, asset))
}
