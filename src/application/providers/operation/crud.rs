use std::collections::HashMap;

use crate::infrastructure::database::repositories::operation::{
    repository_operation_create, repository_operation_get_column_names,
};

pub async fn provider_operation_create(table: String, data: HashMap<String, String>) {
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

    repository_operation_create(query).await;
}

pub async fn provider_operation_get_column_names(table: String) -> Vec<String> {
    repository_operation_get_column_names(table).await
}
