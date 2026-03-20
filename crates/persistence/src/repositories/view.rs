use async_trait::async_trait;
use domain::clean::view::View;
use regex::Regex;
use serde::Serialize;
use sqlx::{Column, Pool, Row, Sqlite, TypeInfo};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Error, ErrorKind},
    sync::{Arc, OnceLock},
};

#[derive(Debug, Clone, Serialize)]
pub struct ViewSnapshot {
    pub view_id: u32,
    pub name: String,
    pub query: String,
    pub columns: Vec<String>,
    pub rows: Vec<BTreeMap<String, String>>,
}

#[async_trait]
pub trait ViewRepository: Send + Sync {
    async fn get_by_id(&self, view_id: u32) -> Result<View, Error>;
    async fn get_dependencies(&self, view_id: u32) -> Result<BTreeSet<String>, Error>;
    async fn read_snapshot(&self, view_id: u32) -> Result<ViewSnapshot, Error>;
}

pub struct ViewRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl ViewRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ViewRepository for ViewRepositoryImpl {
    async fn get_by_id(&self, view_id: u32) -> Result<View, Error> {
        sqlx::query_as::<_, View>("SELECT id, name, query FROM view WHERE id = ?")
            .bind(view_id)
            .fetch_one(&*self.pool)
            .await
            .map_err(|error| {
                if matches!(error, sqlx::Error::RowNotFound) {
                    Error::new(ErrorKind::NotFound, format!("No view with id {view_id}"))
                } else {
                    Error::new(ErrorKind::InvalidData, error)
                }
            })
    }

    async fn get_dependencies(&self, view_id: u32) -> Result<BTreeSet<String>, Error> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT table_name FROM view_dependency WHERE view_id = ? ORDER BY table_name",
        )
        .bind(view_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;

        let view = self.get_by_id(view_id).await?;
        let mut dependencies = rows.into_iter().collect::<BTreeSet<_>>();
        dependencies.extend(infer_view_dependencies(&view.query));
        dependencies.insert("view".to_string());
        dependencies.insert("view_dependency".to_string());
        Ok(dependencies)
    }

    async fn read_snapshot(&self, view_id: u32) -> Result<ViewSnapshot, Error> {
        let view = self.get_by_id(view_id).await?;
        if is_special_view_query(&view.query) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "View {} is not backed by SQL and cannot be streamed",
                    view.id
                ),
            ));
        }

        let rows = sqlx::query(&view.query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;

        let mut columns = Vec::new();
        let mut serialized_rows = Vec::with_capacity(rows.len());

        for row in rows {
            if columns.is_empty() {
                columns = row
                    .columns()
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect();
            }

            let mut serialized = BTreeMap::new();
            for (index, column) in row.columns().iter().enumerate() {
                let value = match column.type_info().name().to_uppercase().as_str() {
                    "INTEGER" => row
                        .try_get::<i64, _>(index)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|_| "NULL".to_string()),
                    "REAL" | "FLOAT" => row
                        .try_get::<f64, _>(index)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|_| "NULL".to_string()),
                    _ => row
                        .try_get::<String, _>(index)
                        .unwrap_or_else(|_| "NULL".to_string()),
                };
                serialized.insert(column.name().to_string(), value);
            }
            serialized_rows.push(serialized);
        }

        Ok(ViewSnapshot {
            view_id: view.id,
            name: view.name,
            query: view.query,
            columns,
            rows: serialized_rows,
        })
    }
}

pub fn is_special_view_query(query: &str) -> bool {
    if parse_creation_view_query(query).is_some() {
        return true;
    }

    matches!(
        query,
        "karma_orchestra" | "karma_view" | "testing" | "command_buffer"
    )
}

fn parse_creation_view_query(query: &str) -> Option<String> {
    let normalized = query.trim().to_lowercase().replace(['-', ' '], "_");
    normalized
        .strip_prefix("create_view_")
        .or_else(|| normalized.strip_prefix("creation_view_"))
        .or_else(|| normalized.strip_prefix("create_modal_"))
        .or_else(|| normalized.strip_prefix("creation_modal_"))
        .or_else(|| normalized.strip_prefix("cv_"))
        .map(str::to_string)
}

pub fn infer_view_dependencies(query: &str) -> BTreeSet<String> {
    if is_special_view_query(query) {
        return BTreeSet::new();
    }

    static DEPENDENCY_REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = DEPENDENCY_REGEX.get_or_init(|| {
        Regex::new(r#"(?i)\b(?:from|join)\s+["`[]?([a-zA-Z_][a-zA-Z0-9_]*)"#).unwrap()
    });

    regex
        .captures_iter(query)
        .filter_map(|capture| capture.get(1))
        .map(|matched| matched.as_str().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect()
}
