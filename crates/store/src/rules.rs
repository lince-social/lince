//! Rule/consequence repository (blueprint VI.1). Rules are records with the
//! `rule` sidecar; activation is the record's quantity.

use nucleus::{ConsequenceKind, ConsequenceSpec, RecordKind};
use sqlx::{Row, SqlitePool};

use crate::records::{self, NewRecord};
use crate::StoreError;

#[derive(Debug, Clone)]
pub struct RuleRow {
    pub record_uid: String,
    pub slug: Option<String>,
    pub active: bool,
    pub condition: String,
    pub gate: String,
    pub carry: String,
    pub consequences: Vec<ConsequenceSpec>,
}

pub struct NewRule<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub condition: &'a str,
    pub gate: &'a str,
    pub carry: &'a str,
    pub consequences: Vec<(ConsequenceKind, Option<String>, Option<serde_json::Value>)>,
}

pub async fn create(pool: &SqlitePool, new: NewRule<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: Some(new.slug),
            kind: RecordKind::Rule,
            head: new.head,
            body: "",
            quantity: 1.0, // active by default: quantity is the universal enable
        },
    )
    .await?;
    sqlx::query("INSERT INTO rule (record_uid, condition, gate, carry) VALUES (?, ?, ?, ?)")
        .bind(&rec.uid)
        .bind(new.condition)
        .bind(new.gate)
        .bind(new.carry)
        .execute(pool)
        .await?;
    for (i, (kind, target, params)) in new.consequences.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO rule_consequence (uid, rule_uid, position, kind, target, params)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(nucleus::new_uid("q"))
        .bind(&rec.uid)
        .bind(i as i64)
        .bind(kind.as_str())
        .bind(target)
        .bind(params.map(|p| p.to_string()))
        .execute(pool)
        .await?;
    }
    Ok(rec.uid)
}

pub async fn load_all(pool: &SqlitePool) -> Result<Vec<RuleRow>, StoreError> {
    let rows = sqlx::query(
        "SELECT r.uid, r.slug, r.quantity, ru.condition, ru.gate, ru.carry
         FROM rule ru JOIN record r ON r.uid = ru.record_uid",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let record_uid: String = row.get("uid");
        let cons = sqlx::query(
            "SELECT kind, target, params, position FROM rule_consequence
             WHERE rule_uid = ? ORDER BY position",
        )
        .bind(&record_uid)
        .fetch_all(pool)
        .await?;
        let consequences = cons
            .into_iter()
            .filter_map(|c| {
                let kind: String = c.get("kind");
                Some(ConsequenceSpec {
                    kind: ConsequenceKind::parse(&kind)?,
                    target: c.get("target"),
                    params: c
                        .get::<Option<String>, _>("params")
                        .and_then(|p| serde_json::from_str(&p).ok()),
                    position: c.get("position"),
                })
            })
            .collect();
        out.push(RuleRow {
            record_uid,
            slug: row.get("slug"),
            active: row.get::<f64, _>("quantity") != 0.0,
            condition: row.get("condition"),
            gate: row.get("gate"),
            carry: row.get("carry"),
            consequences,
        });
    }
    Ok(out)
}
