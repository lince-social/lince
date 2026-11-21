//! Transfer simulation shares carry assumptions or selected results for one revision.
//! Their selected check labels encode order without exposing the original check names.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Coverage, Verdict};
use crate::{DecimalValue, transfer::disclosure::ItemAccess};

const MAX_SHARE_BYTES: usize = 64 * 1024;
const MAX_REFERENCE_BYTES: usize = 256;
const MAX_ASSUMPTIONS: usize = 64;
const MAX_SELECTED_CHECKS: usize = 64;
const CHECK_PREFIX: &str = "Selected check ";

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferSource {
    pub transfer: String,
    pub revision: u64,
    pub promise: String,
    pub exchange: String,
    #[serde(default)]
    pub occurrence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assumption {
    pub source: TransferSource,
    pub after_ms: i64,
    pub quantity: DecimalValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    TransferSimulation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedResult {
    pub verdict: Verdict,
    pub finished: bool,
    pub stopped_at_ms: i64,
    pub selected_coverage: Vec<Coverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shared {
    pub kind: Kind,
    pub id: String,
    pub transfer: String,
    pub revision: u64,
    pub source_at_ms: i64,
    pub duration_ms: i64,
    pub assumptions: Vec<Assumption>,
    pub result: Option<SelectedResult>,
}

impl Shared {
    /// Decodes a bounded share and verifies its structure and redacted labels.
    pub fn parse(body: &str) -> Option<Self> {
        if body.len() > MAX_SHARE_BYTES {
            return None;
        }
        let shared: Self = serde_json::from_str(body).ok()?;
        shared.valid().then_some(shared)
    }

    /// Checks that the share covers one transfer revision and contains only admitted fields.
    pub fn valid(&self) -> bool {
        let reference = |value: &str| !value.is_empty() && value.len() <= MAX_REFERENCE_BYTES;
        reference(&self.id)
            && reference(&self.transfer)
            && self.revision > 0
            && self.duration_ms > 0
            && self.assumptions.len() <= MAX_ASSUMPTIONS
            && self.assumptions.iter().all(|assumption| {
                assumption.source.transfer == self.transfer
                    && assumption.source.revision == self.revision
                    && reference(&assumption.source.promise)
                    && reference(&assumption.source.exchange)
                    && assumption
                        .source
                        .occurrence
                        .as_deref()
                        .is_none_or(reference)
                    && assumption.quantity.is_positive()
                    && assumption.after_ms >= 0
                    && assumption.after_ms <= self.duration_ms
            })
            && self.result.as_ref().is_none_or(|result| {
                result.selected_coverage.len() <= MAX_SELECTED_CHECKS
                    && result
                        .selected_coverage
                        .iter()
                        .enumerate()
                        .all(|(index, coverage)| selected_label(&coverage.check, index + 1))
            })
            && (!self.assumptions.is_empty() || self.result.is_some())
    }

    /// Checks the requested transfer revision and both disclosure grants for each assumption.
    pub fn disclosed(
        &self,
        transfer: &str,
        revision: u64,
        access: &BTreeMap<String, ItemAccess>,
    ) -> bool {
        self.valid()
            && self.transfer == transfer
            && self.revision == revision
            && self.assumptions.iter().all(|assumption| {
                access
                    .get(&assumption.source.promise)
                    .is_some_and(|access| access.quantity && access.parties)
            })
    }
}

fn selected_label(label: &str, position: usize) -> bool {
    let Some(number) = label.strip_prefix(CHECK_PREFIX) else {
        return false;
    };
    !number.starts_with(['0', '+']) && number.parse::<usize>().ok() == Some(position)
}

#[cfg(test)]
mod tests {
    use super::selected_label;

    #[test]
    fn selected_labels_are_canonical_and_ordered() {
        assert!(selected_label("Selected check 1", 1));
        assert!(selected_label("Selected check 64", 64));
        for invalid in [
            "Personal check",
            "Selected check 2",
            "Selected check 01",
            "Selected check +1",
            "Selected check 1 ",
        ] {
            assert!(!selected_label(invalid, 1), "{invalid}");
        }
    }
}
