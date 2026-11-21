// non-lean-yet
// Scope: struct AgreementGuard.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgreementGuard {
    pub level: u8,
    pub change_uid: Option<String>,
}
