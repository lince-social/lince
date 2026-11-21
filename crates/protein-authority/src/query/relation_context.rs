//! Family selection for Relation color context.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Include {
    #[serde(default)]
    pub families: Vec<String>,
}

impl Include {
    pub fn valid(&self) -> bool {
        self.families.len() <= 128
            && self.families.iter().all(|family| {
                !family.trim().is_empty() && family.len() <= 128 && !family.contains('\0')
            })
    }
}
