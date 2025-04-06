use maud::{Markup, html};
use serde::Serialize;
use serde_json::{Value, to_value};
use std::collections::HashMap;

use crate::application::providers::record::fetch_all::record_providers_fetch_all;

fn to_table_data<T: Serialize>(records: &[T]) -> (Vec<String>, Vec<HashMap<String, String>>) {
    let mut rows = vec![];
    let mut headers = vec![];

    for record in records {
        let val = to_value(record).unwrap();
        if let Value::Object(map) = val {
            let mut row = HashMap::new();
            for (key, value) in map {
                if !headers.contains(&key) {
                    headers.push(key.clone());
                }
                row.insert(key, value_to_string(&value));
            }
            rows.push(row);
        }
    }

    (headers, rows)
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => format!("{:?}", v),
    }
}

pub fn render_table(headers: &[String], rows: &[HashMap<String, String>]) -> Markup {
    html! {
        main id="main" {

        table border="1" {
            thead {
                tr {
                    @for header in headers {
                        th { (header) }
                    }
                }
            }
            tbody {
                @for row in rows {
                    tr {
                        @for header in headers {
                            @if header == "id" {
                                td {
                                    (row.get(header).unwrap_or(&"".to_string()))
                    button
                    hx-delete=(format!("/record/{}", row.get(header).unwrap()))
                        hx-swap="outerHTML"
                        hx-target="#main"
                    hx-trigger="click"
                    { "x" }
                                    }
                            } @else {
                                td { (row.get(header).unwrap_or(&"".to_string())) }
                            }
                            }
                    }
                }
            }
        }
        }
    }
}
pub async fn get_records_component() -> Markup {
    let records = record_providers_fetch_all().await.unwrap();
    let (headers, rows) = to_table_data(&records);
    render_table(&headers, &rows)
    // let records = record_providers_fetch_all().await;
    // let records = records.unwrap();
    // println!("{:?}", records);

    // html! {
    // div id="main" {
    //         @for record in &records {
    //             div id=(format!("{}", record.id)) class="row" {
    //             p { (record.quantity) }
    //                 p { (record.head) }

    //             }
    //         }
    //     }
    // }
}
