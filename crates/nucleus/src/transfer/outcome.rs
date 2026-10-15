use crate::{DecimalValue, vocabulary::reference::MeaningReference};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.transfer-outcome";
pub const MAX_BYTES: usize = 32768;
pub const MAX_HISTORY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Giver,
    Receiver,
}

impl Role {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Giver => "giver_statement",
            Self::Receiver => "receiver_statement",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Measure {
    pub meaning: MeaningReference,
    pub unit: Option<MeaningReference>,
    pub object: Option<String>,
}

impl Measure {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.meaning.validate()?;
        if let Some(unit) = &self.unit {
            unit.validate()?;
        }
        if self
            .object
            .as_deref()
            .is_some_and(|uid| !crate::valid_uid(uid, "r"))
        {
            return Err("Choose a canonical assertion object Record");
        }
        Ok(())
    }

    pub fn key(&self) -> Result<String, String> {
        self.validate().map_err(str::to_owned)?;
        crate::karma::canonical_hash("lince.transfer-outcome.measure.v1", self)
            .map(|hash| hash.as_str().trim_start_matches("sha256:").to_owned())
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NeedSelection {
    pub need: String,
    pub expected_need_revision: i64,
    pub assertion: String,
    pub vocabulary: String,
    pub unit_vocabulary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NeedTerms {
    pub selection: NeedSelection,
    pub measure: Measure,
    pub assertion_role: String,
    pub need_quantity: DecimalValue,
    pub requested: DecimalValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    NeedBinding {
        terms: NeedTerms,
        supersedes: Option<String>,
    },
    Statement {
        role: Role,
        measure: Measure,
        quantity: DecimalValue,
        withdrawn: bool,
        need_binding: Option<String>,
        supersedes: Option<String>,
    },
}

impl Content {
    pub fn measure(&self) -> &Measure {
        match self {
            Self::NeedBinding { terms, .. } => &terms.measure,
            Self::Statement { measure, .. } => measure,
        }
    }

    pub fn supersedes(&self) -> Option<&str> {
        match self {
            Self::NeedBinding { supersedes, .. } | Self::Statement { supersedes, .. } => {
                supersedes.as_deref()
            }
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::NeedBinding { .. } => "need_binding",
            Self::Statement { role, .. } => role.kind(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub domain: String,
    pub record: String,
    pub transfer: String,
    pub occurrence: String,
    pub agreement_revision: u64,
    pub author: String,
    pub request_id: String,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    BindNeed {
        selection: NeedSelection,
        expected_measure: Measure,
        supersedes: Option<String>,
    },
    State {
        role: Role,
        measure: Measure,
        amount: String,
        #[serde(default)]
        withdrawn: bool,
        need_binding: Option<String>,
        supersedes: Option<String>,
    },
}

impl Request {
    pub fn amount(value: &str, withdrawn: bool) -> Result<DecimalValue, String> {
        if value.len() > 128 {
            return Err("Outcome amount is too large".into());
        }
        let value = DecimalValue::parse_inferred(value).map_err(|error| error.to_string())?;
        if value.is_negative() || (withdrawn && !value.is_zero()) {
            return Err("Use a nonnegative amount; withdrawing a statement requires zero".into());
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_amounts_keep_exact_decimal_precision_and_explicit_withdrawal() {
        let amount = "9007199254740993.000000000000000001";
        assert_eq!(Request::amount(amount, false).unwrap().canonical(), amount);
        assert!(Request::amount("-1", false).is_err());
        assert!(Request::amount("1", true).is_err());
        assert!(Request::amount("0", true).unwrap().is_zero());
    }
}
