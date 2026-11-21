//! Exact Fact quantities, causal metadata, and compatible SHA-256 chain records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{id::new_uid, json::lowercase_hex, karma::DecimalValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CauseKind {
    UserEdit,
    Rule,
    Settlement,
    Sync,
    Signal,
    Action,
    Fiote,
    Checkpoint,
    TextEdit,
    Compensation,
}

impl CauseKind {
    /// Returns the stable wire spelling of a cause kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserEdit => "user_edit",
            Self::Rule => "rule",
            Self::Settlement => "settlement",
            Self::Sync => "sync",
            Self::Signal => "signal",
            Self::Action => "action",
            Self::Fiote => "fiote",
            Self::Checkpoint => "checkpoint",
            Self::TextEdit => "text_edit",
            Self::Compensation => "compensation",
        }
    }

    /// Parses a persisted cause kind.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "user_edit" => Self::UserEdit,
            "rule" => Self::Rule,
            "settlement" => Self::Settlement,
            "sync" => Self::Sync,
            "signal" => Self::Signal,
            "action" => Self::Action,
            "fiote" => Self::Fiote,
            "checkpoint" => Self::Checkpoint,
            "text_edit" => Self::TextEdit,
            "compensation" => Self::Compensation,
            _ => return None,
        })
    }
}

/// Identifies why a Fact was recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cause {
    pub kind: CauseKind,
    pub uid: Option<String>,
}

impl Cause {
    /// Creates the cause of a direct edit.
    pub fn user_edit() -> Self {
        Self {
            kind: CauseKind::UserEdit,
            uid: None,
        }
    }

    /// Creates a cause referencing the rule that ran.
    pub fn rule(uid: impl Into<String>) -> Self {
        Self {
            kind: CauseKind::Rule,
            uid: Some(uid.into()),
        }
    }

    /// Creates a cause referencing the settlement.
    pub fn settlement(uid: impl Into<String>) -> Self {
        Self {
            kind: CauseKind::Settlement,
            uid: Some(uid.into()),
        }
    }

    /// Creates a cause referencing the observed Signal.
    pub fn signal(uid: impl Into<String>) -> Self {
        Self {
            kind: CauseKind::Signal,
            uid: Some(uid.into()),
        }
    }
}

/// The inputs supplied before assigning chain metadata to a Fact.
#[derive(Debug, Clone)]
pub struct NewFact {
    pub uid: Option<String>,
    pub record_uid: String,
    pub delta: DecimalValue,
    pub at: Option<DateTime<Utc>>,
    pub actor_uid: Option<String>,
    pub cause: Cause,
    pub payload: Option<String>,
}

impl NewFact {
    /// Creates an unsealed change with an exact quantity.
    pub fn quantity(record_uid: impl Into<String>, delta: DecimalValue, cause: Cause) -> Self {
        Self {
            uid: None,
            record_uid: record_uid.into(),
            delta,
            at: None,
            actor_uid: None,
            cause,
            payload: None,
        }
    }

    /// Creates an unsealed change using the legacy floating-point quantity conversion.
    pub fn quantity_f64(record_uid: impl Into<String>, delta: f64, cause: Cause) -> Self {
        Self::quantity(record_uid, decimal_from_f64(delta), cause)
    }
}

/// Converts a legacy numeric input, using a scale-zero delta when conversion fails.
pub fn decimal_from_f64(value: f64) -> DecimalValue {
    DecimalValue::from_f64_lossy(value).unwrap_or(zero_delta())
}

/// Creates an exact zero delta at scale zero.
pub fn zero_delta() -> DecimalValue {
    DecimalValue::from_integer(0)
}

/// A confirmed, hash-linked change to a Record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub uid: String,
    pub record_uid: String,
    pub delta: DecimalValue,
    pub at: DateTime<Utc>,
    pub actor_uid: Option<String>,
    pub cause: Cause,
    pub payload: Option<String>,
    pub prev_hash: String,
    pub hash: String,
    pub signature: Option<String>,
}

fn canonical(f: &Fact) -> String {
    format!(
        "{}|{}|{}:{}|{}|{}|{}|{}|{}",
        f.uid,
        f.record_uid,
        f.delta.scale(),
        f.delta.canonical(),
        f.at.to_rfc3339(),
        f.actor_uid.as_deref().unwrap_or(""),
        f.cause.kind.as_str(),
        f.cause.uid.as_deref().unwrap_or(""),
        f.payload.as_deref().unwrap_or(""),
    )
}

/// Assigns a Fact's identity and timestamp defaults, then seals its chain hash.
pub fn seal(new: NewFact, prev_hash: &str, now: DateTime<Utc>) -> Fact {
    let mut fact = Fact {
        uid: new.uid.unwrap_or_else(|| new_uid("f")),
        record_uid: new.record_uid,
        delta: new.delta,
        at: new.at.unwrap_or(now),
        actor_uid: new.actor_uid,
        cause: new.cause,
        payload: new.payload,
        prev_hash: prev_hash.to_string(),
        hash: String::new(),
        signature: None,
    };
    fact.hash = chain_hash(&fact);
    fact
}

/// Verifies the stored hash against the Fact and its recorded predecessor hash.
pub fn verify_chain_step(fact: &Fact) -> bool {
    chain_hash(fact) == fact.hash
}

fn chain_hash(fact: &Fact) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fact.prev_hash.as_bytes());
    hasher.update(canonical(fact).as_bytes());
    lowercase_hex(&hasher.finalize())
}

/// Returns the lowercase SHA-256 digest of the supplied bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    lowercase_hex(&Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_chains_and_verifies() {
        let now = Utc::now();
        let f1 = seal(
            NewFact::quantity_f64("r_A", -1.0, Cause::user_edit()),
            "genesis",
            now,
        );
        assert!(verify_chain_step(&f1));
        let f2 = seal(
            NewFact::quantity_f64("r_A", 5.0, Cause::rule("r_RULE")),
            &f1.hash,
            now,
        );
        assert!(verify_chain_step(&f2));
        assert_eq!(f2.prev_hash, f1.hash);

        let mut tampered = f2;
        tampered.delta = decimal_from_f64(500.0);
        assert!(!verify_chain_step(&tampered));
    }

    #[test]
    fn declared_precision_is_part_of_the_chain() {
        let now = Utc::now();
        let mk = |delta: DecimalValue| {
            seal(
                NewFact {
                    uid: Some("f_FIXED".into()),
                    record_uid: "r_A".into(),
                    delta,
                    at: Some(now),
                    actor_uid: None,
                    cause: Cause::user_edit(),
                    payload: None,
                },
                "genesis",
                now,
            )
        };
        let coarse = mk(DecimalValue::parse_canonical(1, "1.5").unwrap());
        let fine = mk(DecimalValue::parse_canonical(2, "1.50").unwrap());
        assert_ne!(coarse.hash, fine.hash);
        assert!(verify_chain_step(&coarse) && verify_chain_step(&fine));
    }

    #[test]
    fn float_hazards_do_not_reach_the_chain() {
        let sum = decimal_from_f64(0.1)
            .aligned_add(decimal_from_f64(0.2))
            .expect("0.1 + 0.2 is exact at scale 1");
        assert_eq!(sum.canonical(), "0.3");
        assert_eq!(decimal_from_f64(0.1).canonical(), "0.1");
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(decimal_from_f64(value), DecimalValue::from_integer(0));
        }
    }

    #[test]
    fn genesis_and_linked_facts_keep_canonical_and_hash_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let now = "2026-07-05T10:00:00Z".parse()?;
        let genesis = seal(
            NewFact {
                uid: Some("f_GENESIS".into()),
                record_uid: "r_A".into(),
                delta: DecimalValue::parse_canonical(2, "2.50")?,
                at: None,
                actor_uid: None,
                cause: Cause::user_edit(),
                payload: None,
            },
            "genesis",
            now,
        );
        let linked = seal(
            NewFact {
                uid: Some("f_LINKED".into()),
                record_uid: "r_A".into(),
                delta: DecimalValue::parse_canonical(3, "-0.125")?,
                at: Some("2026-07-05T10:00:01.123456789Z".parse()?),
                actor_uid: Some("r_ANA".into()),
                cause: Cause::settlement("t_X"),
                payload: Some("linha|é\n".into()),
            },
            &genesis.hash,
            now,
        );
        assert_eq!(
            canonical(&genesis),
            "f_GENESIS|r_A|2:2.50|2026-07-05T10:00:00+00:00||user_edit||"
        );
        assert_eq!(
            canonical(&linked),
            "f_LINKED|r_A|3:-0.125|2026-07-05T10:00:01.123456789+00:00|r_ANA|settlement|t_X|linha|é\n"
        );
        assert_eq!(
            genesis.hash,
            "f5897901e9b9c6608450bc623619a247f06a2691d49649e7786051f2e3b12561"
        );
        assert_eq!(
            linked.hash,
            "57b3007f574c25263eda133bd947f2a131aaeb26c24a5aa2a803e28147d64c55"
        );
        assert!(verify_chain_step(&genesis));
        assert!(verify_chain_step(&linked));
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        Ok(())
    }
}
