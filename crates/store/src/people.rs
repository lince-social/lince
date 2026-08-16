//! Standing: whether a Person still uses this Organ (Ontology C3).
//!
//! People leave. Someone stops using Lince, moves out of the household, quits
//! the project — and the Organ that holds their Person keeps holding it,
//! because their history is real and stays. What must stop is their ABILITY TO
//! ACT: the password that still logs in, the session still open, the role still
//! carrying permissions. Deleting the Person would be the wrong tool by a wide
//! margin — it would strand every Fact they signed, every Assertion pointing at
//! them, and every message they sent, and none of that stopped being true.
//!
//! So standing is a separate, reversible flag over a Person that stays whole.
//! Deactivation removes access and nothing else; the name still renders in
//! history, past ops stay admissible, and reactivating restores exactly what
//! was there. People come back.
//!
//! **Absence means active**, the same failure direction as `executor`: a
//! deactivation that never arrives leaves someone able to log in, which is
//! visible and fixable; the reverse would lock out an Organ over a missing row.
//!
//! **Standing is written HERE and travels between this Organ's own Cells**, as
//! a Record extension, because deactivating on the laptop while the VPS Cell
//! still accepts the password is not deactivation. It is filtered out of every
//! contact's feed and refused inbound from anyone but ourselves
//! (`engine::sync`) — a contact who could set this field could lock an Organ
//! out of its own Cell.
//!
//! Two things this deliberately is NOT. It is not key recovery: a Person whose
//! signing key is lost is a different problem with a different answer (roster
//! succession), and conflating them would make "I lost my phone" and "she left"
//! the same button. And it is not absorbing what they leave behind — open
//! Needs, custody, unfinished Transfers — which is never automatic and belongs
//! in a Decision Queue a human answers.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::StoreError;

/// Where a Person's standing is written.
///
/// One namespace per subject, one key inside it, so the extension has room for
/// later Person-level state without a second namespace to keep in step. Two
/// dots, like `executor`, which the wire's `rsplit_once` field split handles by
/// construction: the namespace may contain dots, the key may not.
pub const NAMESPACE: &str = "lince.person";

/// The key inside [`NAMESPACE`] that carries standing.
pub const STANDING_KEY: &str = "standing";

/// Is this op field this module's? Used by the sync filter, which must answer
/// from the field name alone — it sees ops, not records.
pub fn is_standing_field(field: &str) -> bool {
    field == format!("{NAMESPACE}.{STANDING_KEY}")
}

/// Why someone can no longer act, and since when.
///
/// `note` is for the OWNER's memory ("moved out, July"), never shown to the
/// person refused — a login refusal must say the same words whatever the cause.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Standing {
    pub active: bool,
    /// When the current standing was set. Present on deactivation; a
    /// reactivation clears the whole record rather than dating itself, because
    /// "active since" is not a thing anyone needs and absence is the norm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Stop this Person acting here, reversibly.
pub async fn deactivate(
    pool: &SqlitePool,
    person_uid: &str,
    at: &str,
    note: Option<&str>,
) -> Result<(), StoreError> {
    write(
        pool,
        person_uid,
        Some(Standing {
            active: false,
            at: Some(at.to_string()),
            note: note.map(str::to_string),
        }),
    )
    .await
}

/// Let them act again. Writes `null` rather than deleting the key, because the
/// absence of a key and the deletion of a key are the same op-log event only if
/// something travels — a removed key produces no op, so a peer holding the
/// deactivation would keep it forever.
pub async fn reactivate(pool: &SqlitePool, person_uid: &str) -> Result<(), StoreError> {
    write(pool, person_uid, None).await
}

async fn write(
    pool: &SqlitePool,
    person_uid: &str,
    standing: Option<Standing>,
) -> Result<(), StoreError> {
    let value = match standing {
        Some(standing) => serde_json::to_value(standing).map_err(|error| {
            sqlx::Error::Protocol(format!("Person standing is not serialisable: {error}"))
        })?,
        None => serde_json::Value::Null,
    };
    crate::records::set_extension(
        pool,
        person_uid,
        NAMESPACE,
        &serde_json::json!({ STANDING_KEY: value }),
    )
    .await
}

/// This Person's standing, or `None` when nothing has ever been written.
pub async fn standing(pool: &SqlitePool, person_uid: &str) -> Result<Option<Standing>, StoreError> {
    let Some(fds) = crate::records::get_extension(pool, person_uid, NAMESPACE).await? else {
        return Ok(None);
    };
    match fds.get(STANDING_KEY) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => Ok(serde_json::from_value(value.clone()).ok()),
    }
}

/// May this Person act here? Absence means yes.
///
/// A stored value that does not parse also means yes, and that is the same
/// choice made deliberately twice: standing is written by us and read by us, so
/// an unparseable one is our bug, and a bug that locks the owner out of their
/// own Organ is worse than one that leaves an ex-member able to log in until
/// someone notices.
pub async fn is_active(pool: &SqlitePool, person_uid: &str) -> Result<bool, StoreError> {
    Ok(standing(pool, person_uid)
        .await?
        .is_none_or(|standing| standing.active))
}

/// Everyone currently deactivated, newest first — the list the owner reads to
/// answer "who did I turn off, and when?".
pub async fn deactivated(pool: &SqlitePool) -> Result<Vec<(String, Standing)>, StoreError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT record_uid, fds FROM record_extension WHERE namespace = ?",
    )
    .bind(NAMESPACE)
    .fetch_all(pool)
    .await?;
    let mut out: Vec<(String, Standing)> = rows
        .into_iter()
        .filter_map(|(uid, fds)| {
            let value: serde_json::Value = serde_json::from_str(&fds).ok()?;
            let standing: Standing = serde_json::from_value(value.get(STANDING_KEY)?.clone()).ok()?;
            (!standing.active).then_some((uid, standing))
        })
        .collect();
    out.sort_by(|a, b| b.1.at.cmp(&a.1.at));
    Ok(out)
}
