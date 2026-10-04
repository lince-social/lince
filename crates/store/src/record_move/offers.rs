use crate::{
    StoreError,
    snapshot::{Table, Value},
};
use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::BTreeSet;

pub const MAX_RECORDS: usize = 128;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const TABLES: &[(&str, &str)] = &[
    ("concept", "uid"),
    ("concept_name", "concept_uid"),
    ("concept_parent", "concept_uid"),
    ("record", "uid"),
    ("record_doc", "record_uid"),
    ("record_extension", "record_uid"),
    ("record_revision", "record_uid"),
    ("record_assertion", "subject_uid"),
    ("recurrence", "record_uid"),
    ("recurrence_revision", "recurrence_uid"),
    ("karma_field", "uid"),
    ("karma_field_binding", "rule_uid"),
    ("karma_program_revision", "program_uid"),
    ("karma_program", "record_uid"),
    ("karma_frequency_revision", "frequency_uid"),
    ("karma_frequency_activation", "frequency_uid"),
    ("karma_frequency", "record_uid"),
    ("karma_rule_frequency", "recurrence_uid"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub uid: String,
    pub title: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub root: String,
    pub hash: String,
    pub records: Vec<Item>,
    pub assertions: usize,
    pub karma_rules: usize,
    pub bytes: usize,
    pub dependencies: Vec<Item>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub tables: Vec<Table>,
    pub facts: Vec<nucleus::Fact>,
    pub origins: Vec<Origin>,
    pub dependency_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Origin {
    pub fact_uid: String,
    pub organ_uid: String,
    pub cell_uid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub uid: String,
    pub root: String,
    pub peer: String,
    pub direction: String,
    pub state: String,
    pub preview: Preview,
    pub created_at: String,
    pub error: Option<String>,
    pub cancel_sent: bool,
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

pub async fn dependencies<'e, E>(executor: E) -> Result<Vec<(String, String)>, StoreError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT r.record_uid,json_object('condition',r.condition_src,'bindings',r.bindings_json,'consequences',r.consequences_json) FROM recurrence r JOIN record ON record.uid=r.record_uid WHERE record.deleted_at IS NULL UNION ALL SELECT p.program_uid,p.ast_json FROM karma_program_revision p JOIN record ON record.uid=p.program_uid WHERE record.deleted_at IS NULL UNION ALL SELECT f.frequency_uid,f.ast_json FROM karma_frequency_revision f JOIN record ON record.uid=f.frequency_uid WHERE record.deleted_at IS NULL UNION ALL SELECT a.subject_uid,json_object('object',a.object_uid,'predicate',a.predicate_uid) FROM record_assertion a JOIN record ON record.uid=a.subject_uid WHERE record.deleted_at IS NULL AND a.retracted_at IS NULL ORDER BY 1,2 LIMIT 20001")
        .fetch_all(executor).await?;
    if rows.len() > 20000
        || rows
            .iter()
            .map(|(uid, raw)| uid.len() + raw.len())
            .sum::<usize>()
            > 16 * 1024 * 1024
    {
        return Err(protocol(
            "The dependency index exceeds the move preview limit",
        ));
    }
    Ok(rows)
}

pub fn references(value: &serde_json::Value, found: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(s) => {
            found.insert(s.clone());
            if (s.starts_with('{') || s.starts_with('['))
                && let Ok(parsed) = serde_json::from_str(s)
            {
                references(&parsed, found);
            }
            for part in s.split('@').skip(1) {
                let id: String = part
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
                    .collect();
                if !id.is_empty() {
                    found.insert(id);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                references(value, found);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                references(value, found);
            }
        }
        _ => {}
    }
}

pub fn dependency_hash(rows: &[(String, String)], records: &Table) -> Result<String, StoreError> {
    let ids = ids(records, "uid");
    let mut tokens = ids.clone();
    tokens.extend(crate::record_move::offers::ids(records, "slug"));
    let mut selected = Vec::new();
    for (owner, raw) in rows {
        let mut found = BTreeSet::new();
        references(
            &serde_json::from_str(raw).map_err(|e| protocol(e.to_string()))?,
            &mut found,
        );
        if ids.contains(owner) || !found.is_disjoint(&tokens) {
            selected.push((owner, raw));
        }
    }
    Ok(nucleus::fact::sha256_hex(
        &serde_json::to_vec(&selected).map_err(|e| protocol(e.to_string()))?,
    ))
}

pub async fn table(
    pool: &SqlitePool,
    name: &str,
    key: &str,
    ids: &BTreeSet<String>,
) -> Result<Table, StoreError> {
    if !TABLES.contains(&(name, key)) {
        return Err(protocol("Unknown move table"));
    }
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
            .bind(name)
            .fetch_all(pool)
            .await?;
    let rows = sqlx::query(&format!(
        "SELECT * FROM \"{name}\" WHERE \"{key}\" IN (SELECT value FROM json_each(?)) LIMIT 20001"
    ))
    .bind(serde_json::to_string(ids).map_err(|e| protocol(e.to_string()))?)
    .fetch_all(pool)
    .await?;
    if rows.len() > 20000 {
        return Err(protocol("The move has too many dependency rows"));
    }
    let mut values = Vec::new();
    for row in rows {
        let mut out = Vec::new();
        for col in row.columns() {
            let raw = row.try_get_raw(col.ordinal())?;
            out.push(if raw.is_null() {
                Value::Null
            } else {
                match raw.type_info().name() {
                    "INTEGER" => Value::Integer(row.try_get(col.ordinal())?),
                    "REAL" => Value::RealBits(row.try_get::<f64, _>(col.ordinal())?.to_bits()),
                    "TEXT" => Value::Text(row.try_get(col.ordinal())?),
                    "BLOB" => Value::Blob(row.try_get(col.ordinal())?),
                    _ => return Err(protocol("Unsupported move value")),
                }
            });
        }
        values.push(out);
    }
    values.sort_by_cached_key(|row| serde_json::to_vec(row).unwrap_or_default());
    Ok(Table {
        name: name.into(),
        columns,
        rows: values,
    })
}

pub fn text<'a>(table: &Table, row: &'a [Value], column: &str) -> Option<&'a str> {
    table
        .columns
        .iter()
        .position(|c| c == column)
        .and_then(|i| row.get(i))
        .and_then(|v| match v {
            Value::Text(s) => Some(s.as_str()),
            _ => None,
        })
}

pub fn ids(table: &Table, column: &str) -> BTreeSet<String> {
    table
        .rows
        .iter()
        .filter_map(|row| text(table, row, column).map(str::to_owned))
        .collect()
}

pub fn manifest(bundle: &Bundle) -> Vec<Item> {
    let mut items = Vec::new();
    for table in &bundle.tables {
        if !matches!(
            table.name.as_str(),
            "record_assertion"
                | "recurrence"
                | "karma_field"
                | "concept"
                | "karma_program_revision"
                | "karma_frequency_revision"
        ) {
            continue;
        }
        for row in &table.rows {
            let uid = text(table, row, "uid")
                .or_else(|| text(table, row, "revision_hash"))
                .unwrap_or("");
            let title = match table.name.as_str() {
                "record_assertion" => format!(
                    "{} · {} · {}",
                    text(table, row, "subject_uid").unwrap_or(""),
                    text(table, row, "predicate_uid").unwrap_or(""),
                    text(table, row, "object_uid").unwrap_or("value")
                ),
                "recurrence" => format!(
                    "{} → {}",
                    text(table, row, "name")
                        .or_else(|| text(table, row, "note"))
                        .or_else(|| text(table, row, "condition_src"))
                        .unwrap_or("Recurring rule"),
                    text(table, row, "record_uid").unwrap_or("")
                ),
                "karma_field" => text(table, row, "source").unwrap_or("").into(),
                "concept" => text(table, row, "canonical_name").unwrap_or("").into(),
                _ => text(table, row, "program_uid")
                    .or_else(|| text(table, row, "frequency_uid"))
                    .unwrap_or("Revision")
                    .into(),
            };
            items.push(Item {
                uid: uid.into(),
                title: title.chars().take(256).collect(),
                kind: match table.name.as_str() {
                    "record_assertion" => "Assertion",
                    "recurrence" => "Karma rule",
                    "karma_field" => "Karma field",
                    "concept" => "Ontology term",
                    "karma_program_revision" => "Karma Program revision",
                    "karma_frequency_revision" => "Karma Frequency revision",
                    _ => unreachable!(),
                }
                .into(),
            });
        }
    }
    items.sort_by(|a, b| (&a.kind, &a.uid).cmp(&(&b.kind, &b.uid)));
    items
}

pub async fn save(
    pool: &SqlitePool,
    uid: &str,
    peer: &str,
    direction: &str,
    preview: &Preview,
    payload: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if direction == "incoming" {
        let cancelled: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM offer_local_outcome WHERE kind='record-move' AND subject_uid=? AND other_party=? AND outcome='cancelled')")
            .bind(uid).bind(peer).fetch_one(&mut *tx).await?;
        if cancelled {
            return Err(protocol("This move was cancelled before its offer arrived"));
        }
    }
    if direction == "outgoing" {
        for item in &preview.records {
            let busy: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_move_member m JOIN record_move_offer o ON o.uid=m.offer_uid WHERE m.record_uid=? AND o.direction='outgoing' AND o.state NOT IN ('complete','cancelled','declined'))")
                .bind(&item.uid).fetch_one(&mut *tx).await?;
            if busy {
                return Err(protocol(
                    "A dependency is already in another move; cancel it first",
                ));
            }
        }
    }
    let now = nucleus::execution::now().to_rfc3339();
    sqlx::query("INSERT INTO record_move_offer(uid,root,peer,direction,state,preview,payload,created_at,updated_at) VALUES (?,?,?,?,'offered',?,?,?,?)")
        .bind(uid).bind(&preview.root).bind(peer).bind(direction).bind(serde_json::to_string(preview).map_err(|e|protocol(e.to_string()))?).bind(payload).bind(&now).bind(&now).execute(&mut *tx).await?;
    for item in &preview.records {
        sqlx::query("INSERT INTO record_move_member(offer_uid,record_uid) VALUES (?,?)")
            .bind(uid)
            .bind(&item.uid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

fn map(row: sqlx::sqlite::SqliteRow) -> Result<Offer, StoreError> {
    Ok(Offer {
        uid: row.get("uid"),
        root: row.get("root"),
        peer: row.get("peer"),
        direction: row.get("direction"),
        state: row.get("state"),
        preview: serde_json::from_str(&row.get::<String, _>("preview"))
            .map_err(|e| protocol(e.to_string()))?,
        created_at: row.get("created_at"),
        error: row.get("error"),
        cancel_sent: row.get("cancel_sent"),
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Offer>, StoreError> {
    sqlx::query("SELECT * FROM record_move_offer WHERE uid=?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map)
        .transpose()
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Offer>, StoreError> {
    sqlx::query("SELECT * FROM record_move_offer ORDER BY state IN ('offered','accepted','transferring','changed') DESC,updated_at DESC,uid LIMIT 500")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect()
}

pub async fn transition(
    pool: &SqlitePool,
    uid: &str,
    from: &str,
    to: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query(
        "UPDATE record_move_offer SET state=?,updated_at=?,error=NULL,payload=CASE WHEN ?='cancelled' THEN NULL ELSE payload END WHERE uid=? AND state=?",
    )
    .bind(to)
    .bind(nucleus::execution::now().to_rfc3339())
    .bind(to)
    .bind(uid)
    .bind(from)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

pub async fn error(pool: &SqlitePool, uid: &str, reason: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record_move_offer SET error=? WHERE uid=?")
        .bind(reason)
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn cancellation_sent(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record_move_offer SET cancel_sent=1,error=NULL WHERE uid=? AND direction='outgoing' AND state='cancelled'")
        .bind(uid).execute(pool).await?;
    Ok(())
}

pub async fn payload(pool: &SqlitePool, uid: &str) -> Result<String, StoreError> {
    sqlx::query_scalar::<_, Option<String>>("SELECT payload FROM record_move_offer WHERE uid=?")
        .bind(uid)
        .fetch_one(pool)
        .await?
        .ok_or_else(|| protocol("No move payload retained"))
}

pub async fn insert_table(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &Table,
) -> Result<(), StoreError> {
    let (_, key) = TABLES
        .iter()
        .find(|(name, _)| *name == table.name)
        .ok_or_else(|| protocol("Unknown move table"))?;
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
            .bind(&table.name)
            .fetch_all(&mut **tx)
            .await?;
    if columns != table.columns || table.rows.len() > 20000 {
        return Err(protocol("Move schema does not match"));
    }
    let key_index = columns
        .iter()
        .position(|c| c == key)
        .ok_or_else(|| protocol("Move table has no identity"))?;
    let names = columns
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(",");
    let slots = vec!["?"; columns.len()].join(",");
    for row in &table.rows {
        if row.len() != columns.len() {
            return Err(protocol("Move row has wrong length"));
        }
        let statement = format!(
            "INSERT OR IGNORE INTO \"{}\" ({names}) VALUES ({slots})",
            table.name
        );
        let mut query = sqlx::query(&statement);
        for value in row {
            query = match value {
                Value::Null => query.bind(Option::<String>::None),
                Value::Integer(v) => query.bind(*v),
                Value::RealBits(v) => query.bind(f64::from_bits(*v)),
                Value::Text(v) => query.bind(v),
                Value::Blob(v) => query.bind(v),
            };
        }
        let inserted = query.execute(&mut **tx).await?.rows_affected();
        if inserted == 0 {
            let key = match &row[key_index] {
                Value::Text(v) => v,
                _ => return Err(protocol("Invalid move identity")),
            };
            let statement = format!(
                "SELECT EXISTS(SELECT 1 FROM \"{}\" WHERE \"{}\"=? AND {})",
                table.name,
                columns[key_index],
                columns
                    .iter()
                    .filter(|c| table.name != "concept" || c.as_str() != "created_at")
                    .map(|c| format!("\"{c}\" IS ?"))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            );
            let mut check = sqlx::query(&statement).bind(key);
            for (column, value) in columns.iter().zip(row) {
                if table.name == "concept" && column == "created_at" {
                    continue;
                }
                check = match value {
                    Value::Null => check.bind(Option::<String>::None),
                    Value::Integer(v) => check.bind(*v),
                    Value::RealBits(v) => check.bind(f64::from_bits(*v)),
                    Value::Text(v) => check.bind(v),
                    Value::Blob(v) => check.bind(v),
                };
            }
            let agrees: bool = check.fetch_one(&mut **tx).await?.get(0);
            if !agrees {
                return Err(protocol(format!(
                    "A destination {} identity ({key}) conflicts with the offered data; source retained",
                    table.name
                )));
            }
        }
    }
    Ok(())
}

pub async fn unchanged(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    bundle: &Bundle,
) -> Result<(), StoreError> {
    if dependency_hash(&dependencies(&mut **tx).await?, &bundle.tables[3])?
        != bundle.dependency_hash
    {
        return Err(protocol(
            "Record, Karma or Assertion dependencies changed; source retained",
        ));
    }
    let records = ids(&bundle.tables[3], "uid");
    let table = |name: &str| {
        bundle
            .tables
            .iter()
            .find(|table| table.name == name)
            .expect("validated table set")
    };
    let rules = ids(table("recurrence"), "uid");
    let fields = ids(table("karma_field_binding"), "field_uid");
    let concepts = ids(table("concept"), "uid");
    for (table, (_, key)) in bundle.tables.iter().zip(TABLES) {
        let ids = match table.name.as_str() {
            "concept" | "concept_name" | "concept_parent" => &concepts,
            "recurrence_revision" | "karma_field_binding" | "karma_rule_frequency" => &rules,
            "karma_field" => &fields,
            _ => &records,
        };
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM \"{}\" WHERE \"{key}\" IN (SELECT value FROM json_each(?))",
            table.name
        ))
        .bind(serde_json::to_string(&ids).map_err(|e| protocol(e.to_string()))?)
        .fetch_one(&mut **tx)
        .await?;
        if count as usize != table.rows.len() {
            return Err(protocol("Source dependencies changed; source retained"));
        }
        for row in &table.rows {
            let statement = format!(
                "SELECT EXISTS(SELECT 1 FROM \"{}\" WHERE {})",
                table.name,
                table
                    .columns
                    .iter()
                    .map(|c| format!("\"{c}\" IS ?"))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            );
            let mut query = sqlx::query(&statement);
            for value in row {
                query = match value {
                    Value::Null => query.bind(Option::<String>::None),
                    Value::Integer(v) => query.bind(*v),
                    Value::RealBits(v) => query.bind(f64::from_bits(*v)),
                    Value::Text(v) => query.bind(v),
                    Value::Blob(v) => query.bind(v),
                };
            }
            let matches: bool = query.fetch_one(&mut **tx).await?.get(0);
            if !matches {
                return Err(protocol("Source changed during delivery; source retained"));
            }
        }
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fact WHERE record_uid IN (SELECT value FROM json_each(?))",
    )
    .bind(serde_json::to_string(&records).map_err(|e| protocol(e.to_string()))?)
    .fetch_one(&mut **tx)
    .await?;
    if count as usize != bundle.facts.len() {
        return Err(protocol(
            "Source evidence changed during delivery; source retained",
        ));
    }
    Ok(())
}
