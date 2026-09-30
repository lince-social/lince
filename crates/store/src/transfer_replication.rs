use std::collections::{BTreeMap, BTreeSet};

use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::StoreError;

mod policy;
pub mod schema;

pub const VECTOR_PREFIX: &str = "transfer-stream:";
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CHANGES: usize = 20_000;

pub fn invalid(error: impl std::fmt::Display) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub table: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prerequisite {
    pub table: String,
    pub row: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionData {
    pub organ: String,
    pub cell: String,
    pub sequence: i64,
    pub previous: String,
    pub changes: Vec<Change>,
    pub facts: Vec<Fact>,
    pub prerequisites: Vec<Prerequisite>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub transaction: TransactionData,
    pub signature: String,
}

impl Message {
    pub fn signing_bytes(transaction: &TransactionData) -> Result<Vec<u8>, StoreError> {
        let mut bytes = b"lince/transfer-replication/1\n".to_vec();
        bytes.extend(serde_json::to_vec(transaction).map_err(invalid)?);
        Ok(bytes)
    }

    pub fn hash(&self) -> Result<String, StoreError> {
        nucleus::karma::canonical_hash("lince.transfer.replication.message.v1", self)
            .map(|hash| hash.as_str().to_owned())
            .map_err(invalid)
    }
}

pub async fn install(pool: &SqlitePool) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
            .fetch_all(&mut *tx)
            .await?;
    for name in tables.into_iter().filter(|name| schema::allowed(name)) {
        let table = schema::Table::read(&mut tx, &name).await?;
        let local = "(SELECT uid FROM record WHERE slug = 'local-cell')";
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            let row = if operation == "DELETE" {
                "old."
            } else {
                "new."
            };
            let before = if operation == "INSERT" {
                "NULL".into()
            } else {
                table.json("old.")
            };
            let after = if operation == "DELETE" {
                "NULL".into()
            } else {
                table.json("new.")
            };
            let key = format!(
                "json_array({})",
                table
                    .keys
                    .iter()
                    .map(|key| format!("{row}{}", schema::quoted(key)))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let promise = if name == "promise" {
                format!(" AND {row}transfer_uid IS NOT NULL")
            } else {
                String::new()
            };
            let changed = if operation == "UPDATE" {
                format!(" AND {before} IS NOT {after}")
            } else {
                String::new()
            };
            let condition = format!(
                "(SELECT importing FROM transfer_sync_control WHERE id = 1) = 0 AND {local} IS NOT NULL{promise}{changed}{}",
                policy::capture_condition(&name, operation, &key)
            );
            sqlx::query(&format!(
                "DROP TRIGGER IF EXISTS {}",
                schema::quoted(&format!("transfer_sync_{operation}_{name}"))
            ))
            .execute(&mut *tx)
            .await?;
            let sql = format!(
                "CREATE TRIGGER IF NOT EXISTS {} AFTER {operation} ON {} WHEN {condition} BEGIN SELECT CASE WHEN EXISTS (SELECT 1 FROM transfer_sync_owner WHERE table_name = '{name}' AND row_key = {key} AND cell_uid != {local}) THEN RAISE(ABORT, 'submit Transfer changes to the Cell that owns this Transfer state') END; INSERT INTO transfer_sync_owner(table_name,row_key,cell_uid) SELECT '{name}',{key},{local} WHERE NOT EXISTS (SELECT 1 FROM transfer_sync_owner WHERE table_name = '{name}' AND row_key = {key}); INSERT INTO transfer_sync_journal(organ_uid,cell_uid,commit_sequence,table_name,before_json,after_json) SELECT organ_uid,uid,(SELECT value FROM commit_sequence WHERE id = 1),'{name}',{before},{after} FROM record WHERE slug = 'local-cell'; END",
                schema::quoted(&format!("transfer_sync_{operation}_{name}")),
                schema::quoted(&name)
            );
            sqlx::query(&sql).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}

pub async fn owner(
    pool: &SqlitePool,
    table: &str,
    key: &[&str],
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT cell_uid FROM transfer_sync_owner WHERE table_name = ? AND row_key = ?",
    )
    .bind(table)
    .bind(serde_json::to_string(key).map_err(invalid)?)
    .fetch_optional(pool)
    .await
}

pub async fn head(
    connection: &mut SqliteConnection,
    organ: &str,
    cell: &str,
) -> Result<(i64, String), StoreError> {
    let row: Option<(i64, String)> = sqlx::query_as("SELECT sequence,payload FROM transfer_sync_message WHERE organ_uid = ? AND cell_uid = ? ORDER BY sequence DESC LIMIT 1")
        .bind(organ).bind(cell).fetch_optional(connection).await?;
    match row {
        Some((sequence, raw)) => Ok((
            sequence,
            serde_json::from_str::<Message>(&raw)
                .map_err(invalid)?
                .hash()?,
        )),
        None => Ok((0, String::new())),
    }
}

pub async fn pending(
    tx: &mut Transaction<'_, Sqlite>,
    organ: &str,
    cell: &str,
    limit: usize,
) -> Result<Vec<(i64, TransactionData)>, StoreError> {
    let end: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(journal_end),0) FROM transfer_sync_message WHERE organ_uid = ? AND cell_uid = ?")
        .bind(organ).bind(cell).fetch_one(&mut **tx).await?;
    let commits: Vec<i64> = sqlx::query_scalar("SELECT DISTINCT commit_sequence FROM transfer_sync_journal WHERE organ_uid = ? AND cell_uid = ? AND seq > ? ORDER BY commit_sequence LIMIT ?")
        .bind(organ).bind(cell).bind(end).bind(limit as i64).fetch_all(&mut **tx).await?;
    let mut result = Vec::new();
    for commit in commits {
        let rows = sqlx::query("SELECT seq,table_name,before_json,after_json FROM transfer_sync_journal WHERE organ_uid = ? AND cell_uid = ? AND commit_sequence = ? AND seq > ? ORDER BY seq")
            .bind(organ).bind(cell).bind(commit).bind(end).fetch_all(&mut **tx).await?;
        if rows.len() > MAX_CHANGES {
            return Err(invalid(
                "Transfer transaction exceeds the replication row budget",
            ));
        }
        let end = rows
            .last()
            .ok_or_else(|| invalid("empty Transfer journal transaction"))?
            .get("seq");
        let changes = rows
            .into_iter()
            .map(|row| {
                let parse = |raw: Option<String>| {
                    raw.map(|raw| serde_json::from_str(&raw).map_err(invalid))
                        .transpose()
                };
                Ok(Change {
                    table: row.get("table_name"),
                    before: parse(row.get("before_json"))?,
                    after: parse(row.get("after_json"))?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let mut ids: Vec<String> = sqlx::query_scalar("SELECT uid FROM fact WHERE commit_sequence = ? AND cause_kind != 'sync' ORDER BY rowid")
            .bind(commit).fetch_all(&mut **tx).await?;
        let mut included: BTreeSet<String> = ids.iter().cloned().collect();
        for change in &changes {
            let table = schema::Table::read(tx, &change.table).await?;
            for row in change.before.iter().chain(change.after.iter()) {
                for (column, target) in &table.references {
                    if target != "fact" {
                        continue;
                    }
                    if let Some(uid) = row[column].as_str() {
                        if included.insert(uid.to_owned()) {
                            let local: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fact WHERE uid = ? AND cause_kind != 'sync')")
                                .bind(uid).fetch_one(&mut **tx).await?;
                            if local {
                                ids.push(uid.to_owned());
                            }
                        }
                    }
                }
            }
        }
        if ids.len() > MAX_CHANGES {
            return Err(invalid("Transfer transaction exceeds the replication Fact budget"));
        }
        let mut facts = Vec::new();
        for uid in ids {
            facts.push(
                crate::facts::get_in_transaction(tx, &uid)
                    .await?
                    .ok_or_else(|| invalid("missing Transfer Fact"))?,
            );
        }
        let prerequisites = prerequisites(tx, &changes, &facts).await?;
        result.push((
            end,
            TransactionData {
                organ: organ.into(),
                cell: cell.into(),
                sequence: 0,
                previous: String::new(),
                changes,
                facts,
                prerequisites,
            },
        ));
    }
    Ok(result)
}

async fn prerequisites(
    connection: &mut SqliteConnection,
    changes: &[Change],
    facts: &[Fact],
) -> Result<Vec<Prerequisite>, StoreError> {
    let mut records = BTreeSet::new();
    let mut concepts = BTreeSet::new();
    let mut actors = BTreeSet::new();
    for change in changes {
        let table = schema::Table::read(connection, &change.table).await?;
        for row in change.before.iter().chain(change.after.iter()) {
            for (column, target) in &table.references {
                if let Some(uid) = row[column].as_str() {
                    match target.as_str() {
                        "record" => {
                            records.insert(uid.to_owned());
                        }
                        "concept" => {
                            concepts.insert(uid.to_owned());
                        }
                        "identity_key" => {
                            actors.insert(uid.to_owned());
                        }
                        _ => {}
                    }
                }
            }
            if change.table == "organ_contact" {
                if let Some(actor) = row["record_uid"].as_str() {
                    actors.insert(actor.to_owned());
                }
            }
            if change.table == "visibility_rule" {
                if let Some(target) = row["target_uid"].as_str() {
                    records.insert(target.to_owned());
                }
                if matches!(row["subject_kind"].as_str(), Some("organ" | "actor")) {
                    if let Some(subject) = row["subject_uid"].as_str() {
                        records.insert(subject.to_owned());
                    }
                }
            }
            if change.table == "signed_action_intent" {
                if let Some(actor) = row["actor_person_uid"].as_str() {
                    actors.insert(actor.to_owned());
                }
            }
        }
    }
    for fact in facts {
        records.insert(fact.record_uid.clone());
        if let Some(actor) = &fact.actor_uid {
            actors.insert(actor.clone());
            records.insert(actor.clone());
        }
    }
    let mut result = Vec::new();
    let table = schema::Table::read(connection, "record").await?;
    for uid in records {
        let sql = format!("SELECT {} FROM record WHERE uid = ?", table.json(""));
        let Some(raw) = sqlx::query_scalar::<_, String>(&sql)
            .bind(uid)
            .fetch_optional(&mut *connection)
            .await?
        else {
            continue;
        };
        let mut row: Value = serde_json::from_str(&raw).map_err(invalid)?;
        if let Some(unit) = row["unit_uid"].as_str() {
            concepts.insert(unit.to_owned());
        }
        row["quantity_mantissa"] = Value::String("0".into());
        row["quantity_scale"] = Value::from(0);
        row["slug"] = Value::Null;
        row["place_uid"] = Value::Null;
        result.push(Prerequisite {
            table: "record".into(),
            row,
        });
    }
    let table = schema::Table::read(connection, "concept").await?;
    for uid in concepts {
        let sql = format!("SELECT {} FROM concept WHERE uid = ?", table.json(""));
        if let Some(raw) = sqlx::query_scalar::<_, String>(&sql)
            .bind(uid)
            .fetch_optional(&mut *connection)
            .await?
        {
            result.insert(
                0,
                Prerequisite {
                    table: "concept".into(),
                    row: serde_json::from_str(&raw).map_err(invalid)?,
                },
            );
        }
    }
    let table = schema::Table::read(connection, "identity_key").await?;
    for actor in actors {
        let sql = format!(
            "SELECT {} FROM identity_key WHERE actor_uid = ? ORDER BY key_id",
            table.json("")
        );
        for raw in sqlx::query_scalar::<_, String>(&sql)
            .bind(actor)
            .fetch_all(&mut *connection)
            .await?
        {
            result.push(Prerequisite {
                table: "identity_key".into(),
                row: serde_json::from_str(&raw).map_err(invalid)?,
            });
        }
    }
    Ok(result)
}

pub async fn save(
    tx: &mut Transaction<'_, Sqlite>,
    message: &Message,
    journal_end: Option<i64>,
) -> Result<(), StoreError> {
    let raw = serde_json::to_string(message).map_err(invalid)?;
    if raw.len() > MAX_MESSAGE_BYTES {
        return Err(invalid(
            "Transfer transaction exceeds the replication byte budget",
        ));
    }
    let data = &message.transaction;
    sqlx::query("INSERT INTO transfer_sync_message(organ_uid,cell_uid,sequence,journal_end,payload) VALUES(?,?,?,?,?)")
        .bind(&data.organ).bind(&data.cell).bind(data.sequence).bind(journal_end).bind(raw).execute(&mut **tx).await?;
    Ok(())
}

pub async fn missing(
    pool: &SqlitePool,
    organ: &str,
    vector: &[crate::sync_ops::VectorEntry],
    limit: usize,
) -> Result<Vec<Message>, StoreError> {
    let cells: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT cell_uid FROM transfer_sync_message WHERE organ_uid = ? ORDER BY cell_uid",
    )
    .bind(organ)
    .fetch_all(pool)
    .await?;
    let mut result = Vec::new();
    let per_cell = limit.div_ceil(cells.len().max(1));
    for cell in cells {
        if result.len() >= limit {
            break;
        }
        let cursor = vector
            .iter()
            .find(|entry| entry.actor_cell == format!("{VECTOR_PREFIX}{cell}"))
            .map_or(0, |entry| entry.max_hlc);
        let rows: Vec<String> = sqlx::query_scalar("SELECT payload FROM transfer_sync_message WHERE organ_uid = ? AND cell_uid = ? AND sequence > ? ORDER BY sequence LIMIT ?")
            .bind(organ).bind(cell).bind(cursor).bind(per_cell.min(limit - result.len()) as i64).fetch_all(pool).await?;
        for raw in rows {
            result.push(serde_json::from_str(&raw).map_err(invalid)?);
        }
    }
    Ok(result)
}

pub async fn apply(
    tx: &mut Transaction<'_, Sqlite>,
    message: &Message,
) -> Result<Vec<Fact>, StoreError> {
    let data = &message.transaction;
    if data.changes.len() > MAX_CHANGES
        || data.facts.len() > MAX_CHANGES
        || data.prerequisites.len() > MAX_CHANGES
    {
        return Err(invalid(
            "Transfer replication transaction exceeds its row budget",
        ));
    }
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE transfer_sync_control SET importing = 1 WHERE id = 1")
        .execute(&mut **tx)
        .await?;
    let mut tables = BTreeMap::new();
    for prerequisite in &data.prerequisites {
        if !matches!(
            prerequisite.table.as_str(),
            "record" | "concept" | "identity_key"
        ) {
            return Err(invalid("invalid Transfer prerequisite"));
        }
        let table = schema::Table::read(tx, &prerequisite.table).await?;
        let current = table.current(tx, &prerequisite.row).await?;
        if current.is_none() {
            if table.name == "record"
                && (prerequisite.row["quantity_mantissa"] != "0"
                    || prerequisite.row["quantity_scale"] != 0
                    || !prerequisite.row["slug"].is_null()
                    || !prerequisite.row["place_uid"].is_null())
            {
                return Err(invalid(
                    "Transfer prerequisites cannot overwrite resource quantities or local identities",
                ));
            }
            table
                .change(tx, &None, &Some(prerequisite.row.clone()))
                .await?;
        } else if table.name == "record"
            && prerequisite.row["kind"] == "transfer"
            && data.changes.iter().any(|change| {
                change.table == "transfer"
                    && change
                        .after
                        .as_ref()
                        .is_some_and(|row| row["record_uid"] == prerequisite.row["uid"])
            })
        {
            let mut hydrated = current
                .clone()
                .ok_or_else(|| invalid("missing Transfer Record"))?;
            if !matches!(hydrated["kind"].as_str(), Some("plain" | "transfer"))
                || matches!(
                    hydrated["slug"].as_str(),
                    Some("local-organ" | "local-cell")
                )
            {
                return Err(invalid(
                    "Transfer metadata cannot replace another Record kind or a local identity",
                ));
            }
            if hydrated["organ_uid"] != prerequisite.row["organ_uid"]
                || hydrated["organ_uid"] != data.organ
            {
                return Err(invalid(
                    "Transfer Record ownership conflicts with its metadata",
                ));
            }
            for field in ["kind", "head", "body"] {
                let recorded: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sync_op WHERE tbl = 'record' AND uid = ? AND field = ?)").bind(prerequisite.row["uid"].as_str()).bind(field).fetch_one(&mut **tx).await?;
                if !recorded {
                    hydrated[field] = prerequisite.row[field].clone();
                }
            }
            if hydrated["kind"] != "transfer" {
                return Err(invalid(
                    "Transfer metadata conflicts with the recorded Record kind",
                ));
            }
            table.change(tx, &current, &Some(hydrated)).await?;
        } else if table.name == "identity_key"
            && current
                .as_ref()
                .is_some_and(|old| old["public_key"] != prerequisite.row["public_key"])
        {
            return Err(invalid(
                "Transfer prerequisite conflicts with an accepted identity key",
            ));
        }
    }
    let mut imported = Vec::new();
    for fact in &data.facts {
        if !nucleus::fact::verify_chain_step(fact) {
            return Err(invalid("Transfer Fact hash does not verify"));
        }
        if let Some(existing) = crate::facts::get_in_transaction(tx, &fact.uid).await? {
            if existing.cause.kind != CauseKind::Sync && existing.hash != fact.hash {
                return Err(invalid("Transfer Fact identity has conflicting evidence"));
            }
            let retained: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fact_origin WHERE fact_uid = ?)")
                    .bind(&fact.uid)
                    .fetch_one(&mut **tx)
                    .await?;
            if retained {
                crate::facts::retain_origin(tx, fact, &data.organ, &data.cell).await?;
            }
            continue;
        }
        let owner: Option<String> =
            sqlx::query_scalar("SELECT organ_uid FROM record WHERE uid = ?")
                .bind(&fact.record_uid)
                .fetch_one(&mut **tx)
                .await?;
        if !fact.delta.is_zero() && owner.as_deref() != Some(data.organ.as_str()) {
            return Err(invalid(
                "Transfer replication cannot change another Organ's resource quantities",
            ));
        }
        let prev = crate::facts::last_hash(tx).await?;
        let local = nucleus::fact::seal(
            NewFact {
                uid: Some(fact.uid.clone()),
                record_uid: fact.record_uid.clone(),
                delta: fact.delta,
                at: Some(fact.at),
                actor_uid: fact.actor_uid.clone(),
                cause: Cause {
                    kind: CauseKind::Sync,
                    uid: Some(data.organ.clone()),
                },
                payload: fact.payload.clone(),
            },
            &prev,
            nucleus::execution::now(),
        );
        crate::facts::insert(tx, &local).await?;
        crate::facts::retain_origin(tx, fact, &data.organ, &data.cell).await?;
        crate::records::bump_imported_quantity(
            tx,
            &local.record_uid,
            local.delta,
            &nucleus::execution::now().to_rfc3339(),
            &data.cell,
        )
        .await?;
        imported.push(local);
    }
    for change in &data.changes {
        if !schema::allowed(&change.table) {
            return Err(invalid("invalid Transfer replication table"));
        }
        if !tables.contains_key(&change.table) {
            tables.insert(
                change.table.clone(),
                schema::Table::read(tx, &change.table).await?,
            );
        }
        let table = &tables[&change.table];
        let row = change
            .after
            .as_ref()
            .or(change.before.as_ref())
            .ok_or_else(|| invalid("empty Transfer row"))?;
        let key = table.key(row)?;
        let owner: Option<String> = sqlx::query_scalar(
            "SELECT cell_uid FROM transfer_sync_owner WHERE table_name = ? AND row_key = ?",
        )
        .bind(&change.table)
        .bind(&key)
        .fetch_optional(&mut **tx)
        .await?;
        if owner.as_ref().is_some_and(|owner| *owner != data.cell) {
            return Err(invalid(
                "Transfer replication cannot replace another Cell's accepted state",
            ));
        }
        policy::prepare(tx, table, &owner, change).await?;
        table.change(tx, &change.before, &change.after).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO transfer_sync_owner(table_name,row_key,cell_uid) VALUES(?,?,?)",
        )
        .bind(&change.table)
        .bind(key)
        .bind(&data.cell)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query("UPDATE transfer_sync_control SET importing = 0 WHERE id = 1")
        .execute(&mut **tx)
        .await?;
    save(tx, message, None).await?;
    Ok(imported)
}

pub async fn require_private_writer(
    connection: &mut SqliteConnection,
    transfer: &str,
    person: &str,
) -> Result<(), StoreError> {
    let Some(local): Option<String> =
        sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-cell'")
            .fetch_optional(&mut *connection)
            .await?
    else {
        return Ok(());
    };
    let canonical: Option<String> = sqlx::query_scalar("SELECT cell_uid FROM transfer_sync_owner WHERE table_name = 'transfer' AND row_key = json_array(?)")
        .bind(transfer).fetch_optional(&mut *connection).await?;
    let writer = match canonical {
        Some(writer) => Some(writer),
        None => sqlx::query_scalar("SELECT s.actor_cell FROM sync_op s JOIN record own ON own.uid = s.organ_uid AND own.slug = 'local-organ' WHERE s.tbl = 'record' AND s.uid = ? AND s.kind = 'set' AND s.field = 'kind' ORDER BY s.hlc,s.actor_cell LIMIT 1")
            .bind(person).fetch_optional(&mut *connection).await?,
    };
    if writer.as_ref().is_some_and(|writer| writer != &local) {
        return Err(invalid(format!(
            "Submit private Transfer changes to the Person's writing Cell {}",
            writer.unwrap()
        )));
    }
    Ok(())
}
