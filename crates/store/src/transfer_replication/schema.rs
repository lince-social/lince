use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::StoreError;

pub fn allowed(table: &str) -> bool {
    (table == "transfer" || table.starts_with("transfer_"))
        && !table.starts_with("transfer_sync_")
        && !matches!(
            table,
            "transfer_stock_roster_history"
                | "transfer_delivery_outbox"
                | "transfer_delivery_retry_event"
                | "transfer_delivery_pull_request"
                | "transfer_application_attestation_outbox"
                | "transfer_open_owner_backfill_guard"
        )
        || matches!(
            table,
            "promise"
                | "signed_action_intent"
                | "fact_action_intent"
                | "fact_remote_command"
                | "visibility_rule"
                | "organ_contact"
        )
}

pub fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[derive(Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<String>,
    real_columns: std::collections::BTreeSet<String>,
    pub keys: Vec<String>,
    pub references: Vec<(String, String)>,
}

impl Table {
    pub async fn read(connection: &mut SqliteConnection, name: &str) -> Result<Self, StoreError> {
        if !allowed(name) && !matches!(name, "record" | "concept" | "identity_key") {
            return Err(super::invalid("table is not part of Transfer replication"));
        }
        let rows = sqlx::query("SELECT name, pk, type FROM pragma_table_info(?) ORDER BY cid")
            .bind(name)
            .fetch_all(&mut *connection)
            .await?;
        let columns = rows
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .filter(|column| {
                name != "organ_contact"
                    || matches!(
                        column.as_str(),
                        "record_uid"
                            | "trust"
                            | "share_protein"
                            | "scope_fields"
                            | "scope_version"
                            | "accept_fields"
                            | "accept_version"
                            | "closed_by_default"
                    )
            })
            .collect();
        let real_columns = rows
            .iter()
            .filter(|row| row.get::<String, _>("type").eq_ignore_ascii_case("REAL"))
            .map(|row| row.get("name"))
            .collect();
        let mut keys: Vec<(i64, String)> = rows
            .iter()
            .filter_map(|row| {
                let position: i64 = row.get("pk");
                (position > 0).then(|| (position, row.get("name")))
            })
            .collect();
        if name == "identity_key" {
            keys = vec![(1, "actor_uid".into()), (2, "key_id".into())];
        }
        keys.sort();
        if keys.is_empty() {
            return Err(super::invalid(format!(
                "Transfer table {name} has no primary key"
            )));
        }
        let references = sqlx::query("SELECT \"from\", \"table\" FROM pragma_foreign_key_list(?)")
            .bind(name)
            .fetch_all(&mut *connection)
            .await?
            .iter()
            .map(|row| (row.get("from"), row.get("table")))
            .collect();
        Ok(Self {
            name: name.into(),
            columns,
            real_columns,
            keys: keys.into_iter().map(|(_, name)| name).collect(),
            references,
        })
    }

    pub fn json(&self, prefix: &str) -> String {
        let columns: Vec<_> = self
            .columns
            .iter()
            .flat_map(|name| {
                [
                    format!("'{}'", name.replace('\'', "''")),
                    if self.real_columns.contains(name) {
                        let field = format!("{prefix}{}", quoted(name));
                        format!(
                            "CASE WHEN {field} IS NULL THEN NULL ELSE printf('%!.17g', {field}) END"
                        )
                    } else {
                        format!("{prefix}{}", quoted(name))
                    },
                ]
            })
            .collect();
        format!("json_object({})", columns.join(","))
    }

    pub fn key(&self, row: &Value) -> Result<String, StoreError> {
        self.validate(row)?;
        serde_json::to_string(&self.keys.iter().map(|key| &row[key]).collect::<Vec<_>>())
            .map_err(super::invalid)
    }

    pub fn validate(&self, row: &Value) -> Result<(), StoreError> {
        let Some(object) = row.as_object() else {
            return Err(super::invalid("Transfer row must be an object"));
        };
        if object.len() != self.columns.len()
            || self.columns.iter().any(|name| !object.contains_key(name))
            || object
                .values()
                .any(|value| !matches!(value, Value::Null | Value::String(_) | Value::Number(_)))
        {
            return Err(super::invalid(
                "Transfer row does not match the local schema",
            ));
        }
        if self.keys.iter().any(|key| row[key].is_null()) {
            return Err(super::invalid("Transfer row identities cannot be null"));
        }
        if self.name == "promise" && row["transfer_uid"].is_null() {
            return Err(super::invalid(
                "ordinary promises are not Transfer metadata",
            ));
        }
        Ok(())
    }

    pub async fn current(
        &self,
        connection: &mut SqliteConnection,
        row: &Value,
    ) -> Result<Option<Value>, StoreError> {
        self.validate(row)?;
        let condition = self
            .keys
            .iter()
            .map(|key| format!("{} IS json_extract(?, '$.{key}')", quoted(key)))
            .collect::<Vec<_>>()
            .join(" AND ");
        let sql = format!(
            "SELECT {} FROM {} WHERE {condition}",
            self.json(""),
            quoted(&self.name)
        );
        let json = row.to_string();
        let mut query = sqlx::query_scalar::<_, String>(&sql);
        for _ in &self.keys {
            query = query.bind(&json);
        }
        query
            .fetch_optional(connection)
            .await?
            .map(|raw| serde_json::from_str(&raw).map_err(super::invalid))
            .transpose()
    }

    pub async fn change(
        &self,
        connection: &mut SqliteConnection,
        before: &Option<Value>,
        after: &Option<Value>,
    ) -> Result<(), StoreError> {
        let row = after
            .as_ref()
            .or(before.as_ref())
            .ok_or_else(|| super::invalid("empty Transfer change"))?;
        self.validate(row)?;
        if let Some(before) = before {
            self.validate(before)?;
        }
        let current = self.current(connection, row).await?;
        if current == *after {
            return Ok(());
        }
        if current != *before {
            return Err(super::invalid(format!(
                "Transfer replication conflict in {}",
                self.name
            )));
        }
        let json = row.to_string();
        match (before, after) {
            (None, Some(_)) => {
                let columns = self
                    .columns
                    .iter()
                    .map(|name| quoted(name))
                    .collect::<Vec<_>>()
                    .join(",");
                let values = self
                    .columns
                    .iter()
                    .map(|name| format!("json_extract(?, '$.{name}')"))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "INSERT INTO {} ({columns}) VALUES ({values})",
                    quoted(&self.name)
                );
                let mut query = sqlx::query(&sql);
                for _ in &self.columns {
                    query = query.bind(&json);
                }
                query.execute(connection).await?;
            }
            (Some(old), Some(_)) => {
                if self.key(old)? != self.key(row)? {
                    return Err(super::invalid("Transfer row identity cannot change"));
                }
                let columns = self
                    .columns
                    .iter()
                    .map(|name| format!("{} = json_extract(?, '$.{name}')", quoted(name)))
                    .collect::<Vec<_>>()
                    .join(",");
                let condition = self
                    .keys
                    .iter()
                    .map(|name| format!("{} IS json_extract(?, '$.{name}')", quoted(name)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let sql = format!(
                    "UPDATE {} SET {columns} WHERE {condition}",
                    quoted(&self.name)
                );
                let mut query = sqlx::query(&sql);
                for _ in self.columns.iter().chain(self.keys.iter()) {
                    query = query.bind(&json);
                }
                query.execute(connection).await?;
            }
            (Some(_), None) => {
                let condition = self
                    .keys
                    .iter()
                    .map(|name| format!("{} IS json_extract(?, '$.{name}')", quoted(name)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let sql = format!("DELETE FROM {} WHERE {condition}", quoted(&self.name));
                let mut query = sqlx::query(&sql);
                for _ in &self.keys {
                    query = query.bind(&json);
                }
                query.execute(connection).await?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}
