use super::{Delegation, PostingAuthority, Snippet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.social.gossip";
pub const MAX_PEERS: usize = 32;
pub const MAX_ENTRIES: i64 = 30_000;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;

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
    pub send: bool,
    pub receive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub post: String,
    pub hash: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Payload {
    Snippet {
        document: Box<Snippet>,
    },
    ProfileAuthority {
        proof: Box<Snippet>,
        authority: Box<Delegation>,
    },
    PostingAuthority {
        proof: Box<Snippet>,
        authority: Box<PostingAuthority>,
    },
}

impl Payload {
    pub fn proof(&self) -> &Snippet {
        match self {
            Self::Snippet { document } => document,
            Self::ProfileAuthority { proof, .. } | Self::PostingAuthority { proof, .. } => proof,
        }
    }

    pub fn expires_at(&self) -> i64 {
        match self {
            Self::Snippet { document } => document.expires_at,
            Self::ProfileAuthority { authority, .. } => authority.expires_at,
            Self::PostingAuthority { authority, .. } => authority.expires_at,
        }
    }

    pub fn control(&self) -> bool {
        match self {
            Self::Snippet { document } => document.state != super::PostState::Active,
            _ => true,
        }
    }
}
