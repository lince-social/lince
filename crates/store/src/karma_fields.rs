use chrono::{DateTime, Utc};
use nucleus::karma::rule_field::{RuleConsequence, RuleFieldKind};
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{StoreError, recurrence::Recurrence};

#[derive(Clone, Debug, Serialize)]
pub struct Field {
    pub uid: String,
    pub kind: RuleFieldKind,
    pub source: String,
    pub revision: i64,
}

pub fn invalid(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn read(row: sqlx::sqlite::SqliteRow) -> Result<Field, StoreError> {
    let kind: String = row.get("kind");
    Ok(Field {
        uid: row.get("uid"),
        kind: RuleFieldKind::ALL
            .into_iter()
            .find(|value| value.as_str() == kind)
            .ok_or_else(|| invalid("Unknown Karma field"))?,
        source: row.get("source"),
        revision: row.get("revision"),
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Field>, StoreError> {
    sqlx::query("SELECT * FROM karma_field WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(read)
        .transpose()
}

pub async fn for_rule(pool: &SqlitePool, uid: &str) -> Result<Vec<Field>, StoreError> {
    sqlx::query("SELECT f.* FROM karma_field f JOIN karma_field_binding b ON b.field_uid = f.uid WHERE b.rule_uid = ? ORDER BY f.kind")
        .bind(uid).fetch_all(pool).await?.into_iter().map(read).collect()
}

pub async fn readers(pool: &SqlitePool, uid: &str) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT rule_uid FROM karma_field_binding WHERE field_uid = ? ORDER BY rule_uid",
    )
    .bind(uid)
    .fetch_all(pool)
    .await
}

pub async fn replay(pool: &SqlitePool, request: &str) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT result_uid FROM karma_editor_request WHERE request_id = ?")
        .bind(request)
        .fetch_optional(pool)
        .await
}

pub async fn replace_inline(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM karma_field_binding WHERE rule_uid = ?")
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    let row = sqlx::query(
        "SELECT condition_src, gate, record_uid, consequences_json FROM recurrence WHERE uid = ?",
    )
    .bind(uid)
    .fetch_one(&mut **tx)
    .await?;
    let Some(condition) = row.get::<Option<String>, _>("condition_src") else {
        return Ok(());
    };
    let consequence = RuleConsequence {
        target: row.get("record_uid"),
        consequences: serde_json::from_str(&row.get::<String, _>("consequences_json"))
            .map_err(|_| invalid("Unreadable consequences"))?,
    };
    let sources = [condition, row.get("gate"), consequence.as_text()];
    for (kind, source) in RuleFieldKind::ALL.into_iter().zip(sources) {
        let field = Field {
            uid: nucleus::new_uid("kf"),
            kind,
            source,
            revision: 1,
        };
        insert_field(tx, &field).await?;
        bind(tx, uid, &field).await?;
    }
    Ok(())
}

async fn insert_field(tx: &mut Transaction<'_, Sqlite>, field: &Field) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO karma_field(uid, kind, source, revision) VALUES (?, ?, ?, ?)")
        .bind(&field.uid)
        .bind(field.kind.as_str())
        .bind(&field.source)
        .bind(field.revision)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn bind(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &str,
    field: &Field,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO karma_field_binding(rule_uid, kind, field_uid) VALUES (?, ?, ?) ON CONFLICT(rule_uid, kind) DO UPDATE SET field_uid = excluded.field_uid")
        .bind(rule).bind(field.kind.as_str()).bind(&field.uid).execute(&mut **tx).await?;
    Ok(())
}

pub struct Selection {
    pub field: Field,
    pub fresh: bool,
}

pub async fn save_rule(
    pool: &SqlitePool,
    rule: &Recurrence,
    fields: &[Selection],
    request: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    write_rule(&mut tx, rule, request, now).await?;
    for selection in fields {
        if selection.fresh {
            insert_field(&mut tx, &selection.field).await?;
        } else {
            let revision: Option<i64> = sqlx::query_scalar(
                "SELECT revision FROM karma_field WHERE uid = ? AND kind = ? AND source = ?",
            )
            .bind(&selection.field.uid)
            .bind(selection.field.kind.as_str())
            .bind(&selection.field.source)
            .fetch_optional(&mut *tx)
            .await?;
            if revision != Some(selection.field.revision) {
                return Err(invalid("This shared field changed. Refresh before saving."));
            }
        }
        bind(&mut tx, &rule.uid, &selection.field).await?;
    }
    remember(&mut tx, request, &rule.uid).await?;
    tx.commit().await
}

pub async fn revise_field(
    pool: &SqlitePool,
    field: &Field,
    rules: &[Recurrence],
    request: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let changed = sqlx::query(
        "UPDATE karma_field SET source = ?, revision = revision + 1 WHERE uid = ? AND revision = ?",
    )
    .bind(&field.source)
    .bind(&field.uid)
    .bind(field.revision)
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(invalid("This shared field changed. Refresh before saving."));
    }
    let current: Vec<String> = sqlx::query_scalar(
        "SELECT rule_uid FROM karma_field_binding WHERE field_uid = ? ORDER BY rule_uid",
    )
    .bind(&field.uid)
    .fetch_all(&mut *tx)
    .await?;
    let mut expected: Vec<_> = rules.iter().map(|rule| rule.uid.clone()).collect();
    expected.sort();
    if current != expected {
        return Err(invalid(
            "The rules using this field changed. Refresh before saving.",
        ));
    }
    for rule in rules {
        write_rule(&mut tx, rule, &format!("{request}:{}", rule.uid), now).await?;
    }
    remember(&mut tx, request, &field.uid).await?;
    tx.commit().await
}

async fn remember(
    tx: &mut Transaction<'_, Sqlite>,
    request: &str,
    uid: &str,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO karma_editor_request(request_id, result_uid) VALUES (?, ?)")
        .bind(request)
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn write_rule(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &Recurrence,
    request: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let condition = rule
        .condition
        .as_ref()
        .ok_or_else(|| invalid("A Karma rule needs a condition"))?;
    let consequences = serde_json::to_string(&rule.consequences)
        .map_err(|_| invalid("Unreadable consequences"))?;
    let cadence =
        serde_json::to_string(&rule.cadence).map_err(|_| invalid("Unreadable cadence"))?;
    let at = crate::facts::instant(now);
    if rule.revision == 0 {
        sqlx::query("INSERT INTO recurrence(uid, record_uid, consequences_json, condition_src, gate, carry, note, cadence_json, anchor_at, state, revision, actor_uid, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)")
            .bind(&rule.uid).bind(&rule.record_uid).bind(&consequences).bind(&condition.source)
            .bind(condition.gate.as_text()).bind(condition.carry.as_text()).bind(&rule.note)
            .bind(&cadence).bind(&rule.anchor_at).bind(&rule.state).bind(&rule.actor_uid).bind(&at).bind(&at)
            .execute(&mut **tx).await?;
    } else {
        let changed = sqlx::query("UPDATE recurrence SET record_uid = ?, consequences_json = ?, condition_src = ?, gate = ?, carry = ?, revision = revision + 1, actor_uid = ?, updated_at = ? WHERE uid = ? AND revision = ?")
            .bind(&rule.record_uid).bind(&consequences).bind(&condition.source).bind(condition.gate.as_text())
            .bind(condition.carry.as_text()).bind(&rule.actor_uid).bind(&at).bind(&rule.uid).bind(rule.revision)
            .execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(invalid("This rule changed. Refresh before saving."));
        }
    }
    sqlx::query("INSERT INTO recurrence_revision(uid, recurrence_uid, revision, kind, consequences_json, condition_src, gate, carry, note, cadence_json, anchor_at, state, request_id, actor_uid, at) SELECT ?, uid, revision, ?, consequences_json, condition_src, gate, carry, note, cadence_json, anchor_at, state, ?, actor_uid, updated_at FROM recurrence WHERE uid = ?")
        .bind(nucleus::new_uid("recr")).bind(if rule.revision == 0 { "created" } else { "revised" })
        .bind(request).bind(&rule.uid).execute(&mut **tx).await?;
    Ok(())
}
