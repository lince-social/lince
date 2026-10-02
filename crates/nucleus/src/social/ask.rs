use super::{Search, Snippet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.social.ask";
pub const MAX_PEERS: usize = 32;
pub const MAX_WORK: u8 = 12;
pub const MAX_BYTES: u32 = 192 * 1024;
pub const MAX_RESULTS: u8 = 50;
pub const MAX_DEPTH: u8 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub peers: Vec<ContactConsent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactConsent {
    pub organ: String,
    pub ask: bool,
    pub answer: bool,
    pub forward: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub query: Search,
    pub issued_at: i64,
    pub deadline: i64,
    pub work: u8,
    pub bytes: u32,
    pub results: u8,
    pub depth: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub id: String,
    pub documents: Vec<Snippet>,
    pub partial: bool,
}
