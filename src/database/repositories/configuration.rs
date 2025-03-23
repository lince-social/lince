// use std::{
//     collections::HashMap,
//     io::{Error, ErrorKind},
// };

// use rusqlite::types::Value;

// use crate::infrastructure::database::connection::connection;

// pub async fn get_data(query: String) -> Result<Vec<HashMap<String, String>>, Error> {
//     let conn = connection().await;
//     if conn.is_err() {
//         return Err(Error::new(
//             ErrorKind::ConnectionAborted,
//             "Error when connecting to database",
//         ));
//     }
//     let conn = conn.unwrap();
//     let stmt = conn.prepare(&query).unwrap();

//     let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

//     let rows = stmt
//         .query_map([], |row| {
//             let mut data = HashMap::new();
//             for (index, name) in column_names.iter().enumerate() {
//                 let value: Value = row.get(index).unwrap_or(Value::Null);
//                 let value_str = match value {
//                     Value::Null => "NULL".to_string(),
//                     Value::Integer(i) => i.to_string(),
//                     Value::Real(f) => f.to_string(),
//                     Value::Text(s) => s,
//                     Value::Blob(b) => format!("[BLOB: {} bytes", b.len()),
//                 };
//                 data.insert(name.clone(), value_str);
//             }
//             Ok(data)
//         })
//         .map_err(|e| {
//             Error::new(
//                 ErrorKind::InvalidData,
//                 format!("Failed to execute query: {}", e),
//             )
//         })?;
//     let mut table_data = Vec::new();
//     for row in rows {
//         table_data.push(row.map_err(|e| {
//             Error::new(
//                 ErrorKind::InvalidData,
//                 format!("Failed to process row: {}", e),
//             )
//         })?);
//     }

//     Ok(table_data)
//     // let mut table_data = Vec::new();
//     // for row in rows {
//     //     table_data.push(row);
//     // }
//     // Ok(table_data)
// }
