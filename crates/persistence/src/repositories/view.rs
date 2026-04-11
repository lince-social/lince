use async_trait::async_trait;
use domain::clean::view::View;
use regex::Regex;
use serde::Serialize;
use sqlx::{Column, Executor, Pool, Row, Sqlite, SqliteConnection};
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

        let mut dependencies = rows.into_iter().collect::<BTreeSet<_>>();
        dependencies.insert("view".to_string());
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

        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
        let described = connection
            .describe(&view.query)
            .await
            .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
        let columns = described
            .columns()
            .iter()
            .map(|column| column.name().to_string())
            .collect::<Vec<_>>();

        let rows = sqlx::query(&view.query)
            .fetch_all(&mut *connection)
            .await
            .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;

        let mut serialized_rows = Vec::with_capacity(rows.len());

        for row in rows {
            let mut serialized = BTreeMap::new();
            for (index, column) in row.columns().iter().enumerate() {
                let value = serialize_sqlite_value(&row, index);
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

fn serialize_sqlite_value(row: &sqlx::sqlite::SqliteRow, index: usize) -> String {
    if let Ok(Some(value)) = row.try_get::<Option<i64>, _>(index) {
        return value.to_string();
    }
    if let Ok(Some(value)) = row.try_get::<Option<f64>, _>(index) {
        return value.to_string();
    }
    if let Ok(Some(value)) = row.try_get::<Option<String>, _>(index) {
        return value;
    }
    "NULL".to_string()
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
        Regex::new(r#"(?i)\b(?:from|join)\s+(?:"|`|\[)?([a-zA-Z_][a-zA-Z0-9_]*)"#)
            .expect("valid dependency regex")
    });

    regex
        .captures_iter(query)
        .filter_map(|capture| capture.get(1))
        .map(|matched| matched.as_str().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect()
}

pub async fn sync_all_view_dependencies_in_pool(db: &Pool<Sqlite>) -> Result<(), Error> {
    let views = sqlx::query_as::<_, (i64, String)>("SELECT id, query FROM view ORDER BY id")
        .fetch_all(db)
        .await
        .map_err(Error::other)?;

    apply_view_dependency_plan_to_pool(db, build_view_dependency_plan(views)).await
}

pub async fn sync_all_view_dependencies_in_connection(
    connection: &mut SqliteConnection,
) -> Result<(), Error> {
    let views = sqlx::query_as::<_, (i64, String)>("SELECT id, query FROM view ORDER BY id")
        .fetch_all(&mut *connection)
        .await
        .map_err(Error::other)?;

    apply_view_dependency_plan_to_connection(connection, build_view_dependency_plan(views)).await
}

pub async fn sync_view_dependencies_for_view_in_connection(
    connection: &mut SqliteConnection,
    view_id: i64,
) -> Result<(), Error> {
    let query = sqlx::query_scalar::<_, String>("SELECT query FROM view WHERE id = ?")
        .bind(view_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(Error::other)?;

    let dependencies = query
        .map(|query| infer_view_dependencies(&query))
        .unwrap_or_default();

    replace_view_dependencies_in_connection(connection, view_id, dependencies).await
}

pub async fn delete_view_dependencies_in_connection(
    connection: &mut SqliteConnection,
    view_id: i64,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM view_dependency WHERE view_id = ?")
        .bind(view_id)
        .execute(&mut *connection)
        .await
        .map_err(Error::other)?;

    Ok(())
}

fn build_view_dependency_plan(views: Vec<(i64, String)>) -> Vec<(i64, BTreeSet<String>)> {
    views
        .into_iter()
        .map(|(view_id, query)| (view_id, infer_view_dependencies(&query)))
        .collect()
}

async fn apply_view_dependency_plan_to_pool(
    db: &Pool<Sqlite>,
    plan: Vec<(i64, BTreeSet<String>)>,
) -> Result<(), Error> {
    for (view_id, dependencies) in plan {
        replace_view_dependencies_in_pool(db, view_id, dependencies).await?;
    }

    Ok(())
}

async fn apply_view_dependency_plan_to_connection(
    connection: &mut SqliteConnection,
    plan: Vec<(i64, BTreeSet<String>)>,
) -> Result<(), Error> {
    for (view_id, dependencies) in plan {
        replace_view_dependencies_in_connection(connection, view_id, dependencies).await?;
    }

    Ok(())
}

async fn replace_view_dependencies_in_pool(
    db: &Pool<Sqlite>,
    view_id: i64,
    dependencies: BTreeSet<String>,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM view_dependency WHERE view_id = ?")
        .bind(view_id)
        .execute(db)
        .await
        .map_err(Error::other)?;

    for table_name in dependencies {
        sqlx::query("INSERT OR IGNORE INTO view_dependency(view_id, table_name) VALUES (?, ?)")
            .bind(view_id)
            .bind(table_name)
            .execute(db)
            .await
            .map_err(Error::other)?;
    }

    Ok(())
}

async fn replace_view_dependencies_in_connection(
    connection: &mut SqliteConnection,
    view_id: i64,
    dependencies: BTreeSet<String>,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM view_dependency WHERE view_id = ?")
        .bind(view_id)
        .execute(&mut *connection)
        .await
        .map_err(Error::other)?;

    for table_name in dependencies {
        sqlx::query("INSERT OR IGNORE INTO view_dependency(view_id, table_name) VALUES (?, ?)")
            .bind(view_id)
            .bind(table_name)
            .execute(&mut *connection)
            .await
            .map_err(Error::other)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::infer_view_dependencies;
    use std::collections::BTreeSet;

    #[test]
    fn infers_dependencies_from_basic_selects() {
        let dependencies = infer_view_dependencies(
            "SELECT * FROM record JOIN frequency ON frequency.id = record.id",
        );

        assert_eq!(
            dependencies,
            BTreeSet::from(["frequency".to_string(), "record".to_string()])
        );
    }

    #[test]
    fn infers_dependencies_from_bracketed_and_quoted_tables() {
        let dependencies = infer_view_dependencies(
            "SELECT * FROM [view] JOIN `collection_view` ON `collection_view`.view_id = [view].id",
        );

        assert_eq!(
            dependencies,
            BTreeSet::from(["collection_view".to_string(), "view".to_string()])
        );
    }

    #[test]
    fn ignores_special_views() {
        assert!(infer_view_dependencies("karma_orchestra").is_empty());
    }
}
