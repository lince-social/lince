//! Facts (blueprint Part II). The Fact is the truth; the quantity is the cache.
//! `seal` is pure: it takes the previous hash and the clock's `at` from the caller,
//! so DST can replay a log byte-for-byte.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::karma::DecimalValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cause {
    pub kind: CauseKind,
    pub uid: Option<String>,
}

impl Cause {
    pub fn user_edit() -> Self {
        Self {
            kind: CauseKind::UserEdit,
            uid: None,
        }
    }
    pub fn rule(uid: impl Into<String>) -> Self {
        Self {
            kind: CauseKind::Rule,
            uid: Some(uid.into()),
        }
    }
    pub fn settlement(uid: impl Into<String>) -> Self {
        Self {
            kind: CauseKind::Settlement,
            uid: Some(uid.into()),
        }
    }
    pub fn signal(uid: impl Into<String>) -> Self {
        Self {
            kind: CauseKind::Signal,
            uid: Some(uid.into()),
        }
    }
}

/// A fact not yet sealed into the chain. `uid`/`at` may be preset (sync import
/// replays foreign facts verbatim); otherwise the sealer assigns them.
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
    /// An exact movement. This is the constructor a Karma-computed amount uses:
    /// the decimal it evaluated is the decimal that lands in the chain.
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

    /// The legacy-producer door: a caller that still computes its delta in
    /// `f64` (transfers, senses, the old rule fold) converts here, visibly.
    /// Every use of this is a site E0.2/E0.3 still has to make exact — that is
    /// why it is a separate name rather than a `From` impl.
    pub fn quantity_f64(record_uid: impl Into<String>, delta: f64, cause: Cause) -> Self {
        Self::quantity(record_uid, decimal_from_f64(delta), cause)
    }
}

/// Convert a legacy `f64` amount for the Ledger. Non-finite input cannot be a
/// quantity; it becomes zero rather than poisoning a chain, since the callers
/// are infallible constructors.
pub fn decimal_from_f64(value: f64) -> DecimalValue {
    DecimalValue::from_f64_lossy(value)
        .unwrap_or_else(|_| DecimalValue::from_mantissa(0, 0).expect("scale 0 is always valid"))
}

/// The zero movement — checkpoints and anchors carry no quantity.
pub fn zero_delta() -> DecimalValue {
    DecimalValue::from_mantissa(0, 0).expect("scale 0 is always valid")
}

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

/// Canonical serialization hashed into the chain. Field order is normative;
/// changing it breaks every existing chain.
///
/// The delta enters as `scale:canonical-text`, so the exact pair is covered by
/// the Fact's hash and signature rather than annotated beside them. Carrying
/// the scale distinguishes a declared `1.5` from a declared `1.50`; carrying
/// the text rather than a float removes the old hazard where `0.1 + 0.2`
/// hashed as `0.30000000000000004` and chain determinism depended on the
/// float formatter.
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

/// Seal a NewFact into the chain: assign uid/at when absent, chain the hash.
/// Signatures are applied by Trust (Part XI) after sealing; None until then.
pub fn seal(new: NewFact, prev_hash: &str, now: DateTime<Utc>) -> Fact {
    let mut fact = Fact {
        uid: new.uid.unwrap_or_else(|| crate::id::new_uid("f")),
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
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(canonical(&fact).as_bytes());
    fact.hash = hex(&hasher.finalize());
    fact
}

/// Verify one chain step: recompute the hash from prev_hash + canonical form.
pub fn verify_chain_step(fact: &Fact) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(fact.prev_hash.as_bytes());
    hasher.update(canonical(fact).as_bytes());
    hex(&hasher.finalize()) == fact.hash
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// SHA-256 of arbitrary bytes as lowercase hex — used by compaction to anchor
/// an archive file's content into the Ledger (blueprint II.2).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
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

        let mut tampered = f2.clone();
        tampered.delta = decimal_from_f64(500.0);
        assert!(!verify_chain_step(&tampered));
    }

    #[test]
    fn declared_precision_is_part_of_the_chain() {
        // `1.5` and `1.50` are the same number and different declarations; the
        // hash preimage carries the scale, so they are different Facts.
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
        // The old preimage interpolated an f64, so this pair hashed as
        // "0.30000000000000004" and chain bytes depended on the formatter.
        let sum = decimal_from_f64(0.1)
            .aligned_add(decimal_from_f64(0.2))
            .expect("0.1 + 0.2 is exact at scale 1");
        assert_eq!(sum.canonical(), "0.3");
        assert_eq!(decimal_from_f64(0.1).canonical(), "0.1");
    }

    #[test]
    fn seal_is_deterministic_with_preset_identity() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-05T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mk = || {
            seal(
                NewFact {
                    uid: Some("f_FIXED".into()),
                    record_uid: "r_A".into(),
                    delta: decimal_from_f64(2.5),
                    at: Some(now),
                    actor_uid: Some("r_ANA".into()),
                    cause: Cause::settlement("t_X"),
                    payload: None,
                },
                "genesis",
                now,
            )
        };
        assert_eq!(mk().hash, mk().hash, "DST: replay produces identical chain");
    }
}
