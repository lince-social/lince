use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.stock-policy.private";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    RecomputeOffer,
    PreserveOffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Recipe {
    Offer {
        transfer_uid: String,
        promise_uid: String,
        retain: String,
        mode: Mode,
    },
    Replenish {
        below: String,
        target: String,
        upstream_transfer_uid: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub uid: String,
    pub controller_uid: String,
    pub stock_uid: String,
    pub recipe: Recipe,
}

impl Definition {
    pub fn validate(&self) -> Result<(), String> {
        if [&self.uid, &self.controller_uid, &self.stock_uid]
            .iter()
            .any(|uid| !crate::valid_uid(uid, "r"))
            || self.uid == self.stock_uid
        {
            return Err("Choose a stock Record, a controller and a separate policy Record".into());
        }
        let number = |value: &str| {
            crate::DecimalValue::parse_inferred(value)
                .map_err(|_| "Enter an exact decimal quantity".to_owned())
                .and_then(|value| {
                    if value.is_negative() {
                        Err("Planning quantities cannot be negative".into())
                    } else {
                        Ok(value)
                    }
                })
        };
        match &self.recipe {
            Recipe::Offer {
                transfer_uid,
                promise_uid,
                retain,
                ..
            } => {
                if !crate::valid_uid(transfer_uid, "r")
                    || promise_uid.is_empty()
                    || promise_uid.len() > 200
                {
                    return Err("Choose the outgoing Transfer promise".into());
                }
                number(retain)?;
            }
            Recipe::Replenish {
                below,
                target,
                upstream_transfer_uid,
            } => {
                if !crate::valid_uid(upstream_transfer_uid, "r")
                    || number(target)?.exact_numeric_cmp(number(below)?)
                        != std::cmp::Ordering::Greater
                {
                    return Err("Choose an upstream Transfer and a target above the replenishment threshold".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub actual: String,
    pub available: String,
    pub reserved: String,
    pub reserved_offer: String,
    pub offered: String,
    pub proposed: String,
    pub personal: String,
    pub shortfall: String,
    pub frozen: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub definition: Definition,
    pub unit_uid: Option<String>,
    pub expected_revision: u64,
    pub source: crate::transfer::karma::Snapshot,
    pub preview: Preview,
    pub basis: String,
}

impl Review {
    pub fn commitment(&self) -> Result<String, String> {
        crate::karma::canonical_hash("lince.stock-policy.v1", self)
            .map(|hash| hash.as_str().to_owned())
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub review: Review,
    pub commitment: String,
    pub revision: u64,
    pub rule_uid: Option<String>,
    pub enabled: bool,
    pub reason: String,
    pub pending_need: Option<String>,
    pub managed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Inspect {
        definition: Definition,
    },
    Review {
        definition: Definition,
    },
    Enable {
        review: Box<Review>,
        commitment: String,
    },
    Pause {
        policy_uid: String,
        controller_uid: String,
        expected_revision: u64,
    },
}
