// non-lean-yet
// Scope: const MAX_REPORT_BYTES, const REPORT_LIFETIME, struct Report.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 3, 4,
//   8, 9, 10, 11, 12, 13, 14, 15.

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
