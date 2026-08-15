//! Visibility rules (blueprint XV.1): default hidden; whole-row grants in v1.
//! Enforcement lives in exactly one place — Protein's `execute_for`.

use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityRuleRow {
    pub uid: String,
    pub subject_kind: String,
    pub subject_uid: Option<String>,
    pub target_uid: String,
    pub field: Option<String>,
    pub grant_level: String,
}

pub async fn grant(
    pool: &SqlitePool,
    subject_kind: &str, // organ | actor | public | fiote
    subject_uid: Option<&str>,
    target_uid: &str,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("v");
    sqlx::query(
        "INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level)
         VALUES (?, ?, ?, ?, 'visible')",
    )
    .bind(&uid)
    .bind(subject_kind)
    .bind(subject_uid)
    .bind(target_uid)
    .execute(pool)
    .await?;
    Ok(uid)
}

/// Every target visible to a subject: its own grants, public ones, and what it
/// made itself.
///
/// That last clause is INTRINSIC — the same idea as a Transfer's creator and
/// parties (see below), which were never expressible as explicit rules either.
/// Without it, turning auth on makes a Cell look empty to the very person using
/// it: every read is gated by this set, so a record you created a second ago
/// and shared with nobody is invisible to you. That is not a policy anyone
/// chose; it is the absence of one, and it became acute when `--server` made
/// login mandatory.
///
/// Note what this deliberately does NOT do: records committed with no actor at
/// all (created while the Cell ran with auth off, so no Person was on the
/// connection) stay invisible to everyone. Making them visible is a real policy
/// question — on a personal Cell that later enables auth it is obviously right,
/// on a shared one it would disclose everything predating the first account —
/// and it is not this function's to answer silently.
pub async fn visible_targets(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT target_uid AS uid FROM visibility_rule
          WHERE grant_level = 'visible' AND (subject_uid = ? OR subject_kind = 'public')
         UNION
         SELECT DISTINCT record_uid AS uid FROM fact WHERE actor_uid = ?",
    )
    .bind(subject_uid)
    .bind(subject_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.get("uid"))
    .collect())
}

/// Exact persisted disclosure grants for one target. Intrinsic Transfer
/// recipients (creator, parties, invitees) are derived by the Transfer
/// Protein and remain distinct from these explicit rules.
pub async fn rules_for_target(
    pool: &SqlitePool,
    target_uid: &str,
) -> Result<Vec<VisibilityRuleRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, subject_kind, subject_uid, target_uid, field, grant_level
         FROM visibility_rule WHERE target_uid = ?
         ORDER BY subject_kind, subject_uid, field, uid",
    )
    .bind(target_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| VisibilityRuleRow {
        uid: row.get("uid"),
        subject_kind: row.get("subject_kind"),
        subject_uid: row.get("subject_uid"),
        target_uid: row.get("target_uid"),
        field: row.get("field"),
        grant_level: row.get("grant_level"),
    })
    .collect())
}

// ---------------------------------------------------------------------------
// Per-record hiding on the sync feed (Ontology §12, C5)
// ---------------------------------------------------------------------------
//
// The gate above answers "what may this logged-in Person READ here". This one
// answers a different question — "what may this contact Organ RECEIVE" — and
// the two default OPPOSITE ways on purpose. Local reads are default-hidden
// because a Cell holds other people's records. The sync feed is default-shared
// because §12 says so: pairing with a contact and switching `sync_out` on IS
// the grant, and hiding is a named exception to it.
//
// Reusing `visible_targets` here would have inverted that: every contact would
// see nothing until each Record was granted one at a time, which is not the
// design and would have looked like sync breaking rather than a policy.
// `subject_kind = 'organ'` with `grant_level = 'hidden'` is the exception list,
// and the column has carried both levels since `0001_init.sql`.

/// The Records this contact is not to receive. Usually empty, which is why
/// every caller checks that first and pays nothing when it is.
pub async fn hidden_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT target_uid AS uid FROM visibility_rule
          WHERE subject_kind = 'organ' AND subject_uid = ? AND grant_level = 'hidden'",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("uid"))
    .collect())
}

/// Hide or unhide one Record from one contact. Idempotent both ways, because
/// the surface is a toggle and a double click must not leave two rows behind
/// that a later unhide only half removes.
pub async fn set_hidden_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
    target_uid: &str,
    hidden: bool,
) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM visibility_rule
          WHERE subject_kind = 'organ' AND subject_uid = ? AND target_uid = ?
            AND grant_level = 'hidden'",
    )
    .bind(organ_uid)
    .bind(target_uid)
    .execute(pool)
    .await?;
    if hidden {
        sqlx::query(
            "INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'organ', ?, ?, 'hidden')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(organ_uid)
        .bind(target_uid)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// One hidden Record as a surface needs it: a person recognises a Record by
/// its head or slug, never by its uid, and a panel listing bare uids is a list
/// nobody can audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiddenRecordRow {
    pub uid: String,
    pub head: String,
    pub slug: Option<String>,
}

/// The hide list for one contact, named. A rule whose Record was deleted since
/// still appears, with an empty head — dropping it would make the rule
/// invisible while it still filters, and a rule nobody can see is one nobody
/// can remove.
pub async fn hidden_records_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<HiddenRecordRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT v.target_uid AS uid,
                COALESCE(r.head, '') AS head,
                r.slug AS slug
           FROM visibility_rule v
           LEFT JOIN record r ON r.uid = v.target_uid
          WHERE v.subject_kind = 'organ' AND v.subject_uid = ?
            AND v.grant_level = 'hidden'
          ORDER BY head, uid",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| HiddenRecordRow {
        uid: row.get("uid"),
        head: row.get("head"),
        slug: row.get("slug"),
    })
    .collect())
}

/// The Records an op is ABOUT — the same shape as `replica::root_for_op`, and
/// deliberately next to nothing else: an op names a table and a uid, and only
/// this mapping says which Record's policy governs it.
///
/// An Assertion returns BOTH endpoints and is withheld if EITHER is hidden.
/// Hiding one side and letting a link to it through would disclose the hidden
/// uid and its relation, which is most of what hiding was for — the same
/// reason `root_for_link` refuses to join two roots.
pub async fn records_of_op(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(match tbl {
        "record" => vec![uid.to_string()],
        "fact" => sqlx::query_scalar::<_, Option<String>>(
            "SELECT record_uid FROM fact WHERE uid = ?",
        )
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .flatten()
        .into_iter()
        .collect(),
        "record_assertion" => sqlx::query(
            "SELECT subject_uid, object_uid FROM record_assertion WHERE uid = ?",
        )
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .into_iter()
        .flat_map(|row| {
            let subject: String = row.get("subject_uid");
            let object: Option<String> = row.get("object_uid");
            std::iter::once(subject).chain(object)
        })
        .collect(),
        // An unknown table is not silently shared. A fourth logged table is a
        // change to this mapping, and defaulting to "no Record governs it"
        // would make the omission invisible until it leaked.
        _ => Vec::new(),
    })
}

/// Whether this op must be kept out of `hidden`'s owner's feed.
///
/// Unlike the field scope, there is NO tombstone exemption here, and the
/// difference is the point. A narrowed contact still holds the Record, so
/// withholding its delete would strand it. A hidden Record is one they were
/// never to have — sending the delete would tell them it existed. §12's
/// honest half applies instead: hiding stops what travels NEXT, and whatever
/// they already received stays received.
pub async fn op_hidden_from(
    pool: &SqlitePool,
    hidden: &HashSet<String>,
    tbl: &str,
    uid: &str,
) -> Result<bool, StoreError> {
    if hidden.is_empty() {
        return Ok(false);
    }
    // A `record` op is the overwhelming majority and needs no query at all.
    if tbl == "record" {
        return Ok(hidden.contains(uid));
    }
    Ok(records_of_op(pool, tbl, uid)
        .await?
        .iter()
        .any(|record| hidden.contains(record)))
}
