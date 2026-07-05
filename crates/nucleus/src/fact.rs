//! Facts (blueprint Part II). The Fact is the truth; the quantity is the cache.
//! `seal` is pure: it takes the previous hash and the clock's `at` from the caller,
//! so DST can replay a log byte-for-byte.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
        Self { kind: CauseKind::UserEdit, uid: None }
    }
    pub fn rule(uid: impl Into<String>) -> Self {
        Self { kind: CauseKind::Rule, uid: Some(uid.into()) }
    }
    pub fn settlement(uid: impl Into<String>) -> Self {
        Self { kind: CauseKind::Settlement, uid: Some(uid.into()) }
    }
    pub fn signal(uid: impl Into<String>) -> Self {
        Self { kind: CauseKind::Signal, uid: Some(uid.into()) }
    }
}

/// A fact not yet sealed into the chain. `uid`/`at` may be preset (sync import
/// replays foreign facts verbatim); otherwise the sealer assigns them.
#[derive(Debug, Clone)]
pub struct NewFact {
    pub uid: Option<String>,
    pub record_uid: String,
    pub delta: f64,
    pub at: Option<DateTime<Utc>>,
    pub actor_uid: Option<String>,
    pub cause: Cause,
    pub payload: Option<String>,
}

impl NewFact {
    pub fn quantity(record_uid: impl Into<String>, delta: f64, cause: Cause) -> Self {
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub uid: String,
    pub record_uid: String,
    pub delta: f64,
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
fn canonical(f: &Fact) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        f.uid,
        f.record_uid,
        f.delta,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_chains_and_verifies() {
        let now = Utc::now();
        let f1 = seal(
            NewFact::quantity("r_A", -1.0, Cause::user_edit()),
            "genesis",
            now,
        );
        assert!(verify_chain_step(&f1));
        let f2 = seal(
            NewFact::quantity("r_A", 5.0, Cause::rule("r_RULE")),
            &f1.hash,
            now,
        );
        assert!(verify_chain_step(&f2));
        assert_eq!(f2.prev_hash, f1.hash);

        let mut tampered = f2.clone();
        tampered.delta = 500.0;
        assert!(!verify_chain_step(&tampered));
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
                    delta: 2.5,
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
