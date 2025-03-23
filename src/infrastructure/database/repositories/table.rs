use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

use crate::infrastructure::database::connection::connection;

pub async fn get_data(query: String) -> Result<Vec<HashMap<String, String>>, Error> {
    let conn = connection().await;
    if conn.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when connecting to database",
        ));
    }
    let conn = conn.unwrap();
    let stmt = conn.prepare("SELECT * FROM record").unwrap();

    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    let rows = stmt
        .query_map([], |row| {
            let mut data = HashMap::new();
            for (index, name) in column_names.iter().enumerate() {
                let value: String = match row.get(index) {
                    Ok(v) => v.to_string(),
                    Err(_) => "NULL".to_string(),
                };
                data.insert(name.clone(), value);
            }
            Ok(data)
        })
        .unwrap();
    let mut table_data = Vec::new();
    for row in rows {
        table_data.push(row.unwrap());
    }
    Ok(table_data)
}
