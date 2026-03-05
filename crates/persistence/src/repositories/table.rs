use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait TableRepository: Send + Sync {
    async fn delete_by_id(&self, table: String, id: String) -> Result<(), Error>;
    async fn get_columns(&self, table: String) -> Result<Vec<String>, Error>;
    async fn insert_row(&self, table: String, values: HashMap<String, String>)
    -> Result<(), Error>;
}

pub struct TableRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl TableRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TableRepository for TableRepositoryImpl {
    async fn delete_by_id(&self, table: String, id: String) -> Result<(), Error> {
        let query = format!("DELETE FROM {} WHERE id = {}", table, id);

        sqlx::query(&query)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(())
    }

    async fn get_columns(&self, table: String) -> Result<Vec<String>, Error> {
        let rows = sqlx::query_as::<_, (String,)>("SELECT name FROM pragma_table_info(?)")
            .bind(table)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    async fn insert_row(
        &self,
        table: String,
        values: HashMap<String, String>,
    ) -> Result<(), Error> {
        if values.is_empty() {
            let query = format!("INSERT INTO {table} DEFAULT VALUES");
            sqlx::query(&query)
                .execute(&*self.pool)
                .await
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
            return Ok(());
        }

        let mut entries = values.into_iter().collect::<Vec<_>>();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let columns = entries
            .iter()
            .map(|(column, _)| column.clone())
            .collect::<Vec<_>>();
        let placeholders = vec!["?"; columns.len()].join(", ");
        let query = format!(
            "INSERT INTO {table} ({}) VALUES ({placeholders})",
            columns.join(", ")
        );
        let mut built_query = sqlx::query(&query);
        for (_, value) in entries {
            built_query = built_query.bind(value);
        }
        built_query
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(())
    }
}
