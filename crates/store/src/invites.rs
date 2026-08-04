//! Thread invites: a request to talk, before anyone has agreed to anything.
//!
//! **Everything here is local and stays local.** An invite is written with
//! plain SQL rather than through `records::create`, for the same reason
//! `organs::add_contact` does: the Record write path logs an op and enqueues
//! it to every known contact. An invite must never travel — "Bea is asking to
//! talk to me" is nobody else's business, and the offer itself already arrived
//! over the wire. Reach for `records::create` here and the invite becomes a
//! broadcast.
//!
//! The Record row exists so the invite is visible to Protein like anything
//! else; the `lince.invite` extension mirrors who it is from, so a surface can
//! render it without a new Protein source. Both are written the same
//! unlogged way.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

/// The extension namespace carrying an invite's sender and offered root — a
/// local projection for display, the way `lince.pairing` mirrors the invite
/// code onto the Organ record.
pub const EXTENSION: &str = "lince.invite";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub record_uid: String,
    pub from_organ: String,
    pub root: String,
    pub title: String,
    pub created_at: String,
}

/// Record an offer as a pending invite.
///
/// Returns `Ok(None)` when this Organ already has one pending: refusing a
/// second is the anti-spam rule, and it is not an error — the sender is told
/// their offer was received either way, because telling them apart would leak
/// whether the last one was ignored or declined.
pub async fn put(
    pool: &SqlitePool,
    from_organ: &str,
    root: &str,
    title: &str,
) -> Result<Option<Invite>, StoreError> {
    let uid = nucleus::new_uid("r");
    let now = Utc::now().to_rfc3339();
    let title = if title.trim().is_empty() {
        "A conversation"
    } else {
        title.trim()
    };

    // Unlogged on purpose — see the module comment.
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             created_at, updated_at)
         VALUES (?, NULL, ?, ?, '', '0', 0, ?, ?)",
    )
    .bind(&uid)
    .bind(nucleus::RecordKind::ThreadInvite.as_str())
    .bind(title)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // The UNIQUE on `from_organ` is what enforces one pending per Organ, and
    // it is enforced HERE rather than by a preceding query because an Organ is
    // several Cells and two of them can offer at the same moment.
    let claimed = sqlx::query(
        "INSERT OR IGNORE INTO thread_invite (record_uid, from_organ, root, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(from_organ)
    .bind(root)
    .bind(&now)
    .execute(pool)
    .await?
    .rows_affected()
        == 1;

    if !claimed {
        // Someone else's invite holds the slot. Undo the Record so a refused
        // invite leaves nothing behind to render.
        sqlx::query("DELETE FROM record WHERE uid = ?")
            .bind(&uid)
            .execute(pool)
            .await?;
        return Ok(None);
    }

    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace) DO UPDATE SET fds = excluded.fds",
    )
    .bind(&uid)
    .bind(EXTENSION)
    .bind(serde_json::json!({ "from_organ": from_organ, "root": root }).to_string())
    .execute(pool)
    .await?;

    Ok(Some(Invite {
        record_uid: uid,
        from_organ: from_organ.to_string(),
        root: root.to_string(),
        title: title.to_string(),
        created_at: now,
    }))
}

fn map(row: sqlx::sqlite::SqliteRow) -> Invite {
    Invite {
        record_uid: row.get("record_uid"),
        from_organ: row.get("from_organ"),
        root: row.get("root"),
        title: row.get("head"),
        created_at: row.get("created_at"),
    }
}

pub async fn get(pool: &SqlitePool, record_uid: &str) -> Result<Option<Invite>, StoreError> {
    Ok(sqlx::query(
        "SELECT i.*, r.head FROM thread_invite i
           JOIN record r ON r.uid = i.record_uid
          WHERE i.record_uid = ?",
    )
    .bind(record_uid)
    .fetch_optional(pool)
    .await?
    .map(map))
}

pub async fn pending(pool: &SqlitePool) -> Result<Vec<Invite>, StoreError> {
    Ok(sqlx::query(
        "SELECT i.*, r.head FROM thread_invite i
           JOIN record r ON r.uid = i.record_uid
          ORDER BY i.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

/// Clear an invite once it has been answered.
///
/// Both exits — accept and decline — come through here, and neither leaves the
/// Record behind. There is deliberately no "dismiss": clearing an invite
/// without answering the grant would leave the sender waiting forever while
/// the Organ slot stayed occupied, so the invite could never be re-sent
/// either.
pub async fn clear(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM record_extension WHERE record_uid = ?")
        .bind(record_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM thread_invite WHERE record_uid = ?")
        .bind(record_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM record WHERE uid = ?")
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}
