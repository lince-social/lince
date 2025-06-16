use crate::domain::repositories::operation::OperationRepository;
use std::{collections::HashMap, io::Error};

pub struct OperationProvider {
    pub repository: std::sync::Arc<dyn OperationRepository>,
}

impl OperationProvider {
    pub async fn create(&self, table: String, data: HashMap<String, String>) -> Result<(), Error> {
        let mut columns = Vec::new();
        let mut values = Vec::new();

        for (k, v) in data {
            if v.is_empty() {
                continue;
            }
            columns.push(k);
            values.push(if v.parse::<i64>().is_ok() || v.parse::<f64>().is_ok() {
                v
            } else {
                format!("'{}'", v)
            });
        }

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            values.join(", ")
        );

        self.repository.create(query).await
    }

    pub async fn get_column_names(&self, table: String) -> Result<Vec<String>, Error> {
        self.repository.get_column_names(table).await
    }
}
