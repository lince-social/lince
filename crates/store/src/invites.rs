use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const EXTENSION: &str = "lince.invite";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub record_uid: String,
    pub from_organ: String,
    pub root: String,
    pub title: String,
    pub created_at: String,
}

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

    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, NULL, ?, ?, '', '0', 0, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(nucleus::RecordKind::ThreadInvite.as_str())
    .bind(title)
    .bind(from_organ)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

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
