use serde::{Deserialize, Serialize};

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
    pub quantity: crate::DecimalValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    TransferSimulation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedResult {
    pub verdict: super::Verdict,
    pub finished: bool,
    pub stopped_at_ms: i64,
    pub selected_coverage: Vec<super::Coverage>,
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
    pub fn parse(body: &str) -> Option<Self> {
        if body.len() > 64 * 1024 {
            return None;
        }
        let shared: Self = serde_json::from_str(body).ok()?;
        shared.valid().then_some(shared)
    }

    pub fn valid(&self) -> bool {
        let reference = |value: &str| !value.is_empty() && value.len() <= 256;
        reference(&self.id)
            && reference(&self.transfer)
            && self.revision > 0
            && self.duration_ms > 0
            && self.assumptions.len() <= 64
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
                result.selected_coverage.len() <= 64
                    && result
                        .selected_coverage
                        .iter()
                        .enumerate()
                        .all(|(index, coverage)| {
                            coverage.check == format!("Selected check {}", index + 1)
                        })
            })
            && (!self.assumptions.is_empty() || self.result.is_some())
    }

    pub fn disclosed(
        &self,
        transfer: &str,
        revision: u64,
        access: &std::collections::BTreeMap<String, crate::transfer::disclosure::ItemAccess>,
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
