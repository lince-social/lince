use crate::{DecimalValue, transfer::outcome::NeedSelection, trust_analysis::query::Query};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub need: NeedSelection,
    pub offers: Vec<String>,
    pub sources: Query,
    pub minimum_experience: String,
    pub from: Option<String>,
    pub until: Option<String>,
    pub within_metres: Option<u32>,
}
impl Request {
    pub fn validate(&self) -> Result<DecimalValue, String> {
        self.sources.validate()?;
        if self.offers.len() > 32 || self.offers.iter().any(|uid| !crate::valid_uid(uid, "r")) {
            return Err(
                "Choose at most 32 candidate Transfer Records, or search local visible offers"
                    .into(),
            );
        }
        if self
            .offers
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.offers.len()
        {
            return Err("Candidate Transfers must be distinct".into());
        }
        let minimum = DecimalValue::parse_inferred(&self.minimum_experience)
            .map_err(|error| error.to_string())?;
        if minimum.is_negative() {
            return Err("Minimum experience cannot be negative".into());
        }
        let from = self
            .from
            .as_deref()
            .map(chrono::DateTime::parse_from_rfc3339)
            .transpose()
            .map_err(|_| "Use a start time with timezone")?;
        let until = self
            .until
            .as_deref()
            .map(chrono::DateTime::parse_from_rfc3339)
            .transpose()
            .map_err(|_| "Use an end time with timezone")?;
        if from.zip(until).is_some_and(|(a, b)| a >= b) || self.within_metres == Some(0) {
            return Err("Choose an ordered time window and a positive geographic range".into());
        }
        Ok(minimum)
    }
}
