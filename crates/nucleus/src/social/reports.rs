use super::*;

pub const MAX_REPORT_BYTES: usize = 16 * 1024;
pub const REPORT_LIFETIME: i64 = 7 * 86400;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub protocol: String,
    pub id: String,
    pub service: String,
    pub document: Snippet,
    pub explanation: String,
    pub created_at: i64,
    pub expires_at: i64,
}
