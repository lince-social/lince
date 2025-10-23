use crate::domain::clean::table::{SortedTables, Table};
use serde::Serialize;
use std::collections::HashMap;

pub fn to_named_sorted_table<T: Serialize>(name: impl Into<String>, rows: Vec<T>) -> SortedTables {
    let mut headers: Vec<String> = vec![];
    let mut table: Table = vec![];

    for row in rows {
        let json = serde_json::to_value(row).unwrap();
        if let serde_json::Value::Object(obj) = json {
            if headers.is_empty() {
                headers = obj.keys().cloned().collect();
                headers.sort();
            }

            let mut map = HashMap::new();
            for key in &headers {
                let val = obj.get(key).cloned().unwrap_or(serde_json::Value::Null);
                map.insert(key.clone(), v_to_string(val));
            }
            table.push(map);
        }
    }

    vec![(name.into(), table, headers)]
}

fn v_to_string(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s,
        _ => serde_json::to_string(&v).unwrap(), // For arrays/objects
    }
}
