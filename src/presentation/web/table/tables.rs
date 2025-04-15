use crate::application::providers::view::get_active_view_data::provider_view_get_active_view_data;
use maud::{Markup, html};

pub async fn presentation_web_tables() -> Markup {
    let tables = provider_view_get_active_view_data().await.unwrap();
    let tables_sorted: Vec<(
        String,
        Vec<std::collections::HashMap<String, String>>,
        Vec<String>,
    )> = tables
        .into_iter()
        .map(|(table_name, table)| {
            let mut headers: Vec<String> = table
                .first()
                .map(|row| row.keys().cloned().collect())
                .unwrap_or_default();
            headers.sort();
            (table_name, table, headers)
        })
        .collect();

    html! {
        main id="main" {
            @for (table_name, table, headers) in tables_sorted {
                div {
                    p { (table_name) }
                    table class="framed" {
                        @if !headers.is_empty() {
                            thead {
                                tr {
                                    @for key in &headers {
                                        th { (key) }
                                    }
                                }
                            }
                        }
                        tbody {
                            @for row in table {
                                tr {
                                    @for key in &headers {
                                        td
                                            hx-trigger="click" hx-swap="outerHTML" hx-get=(format!(
                                            "/table/{}/{}/{}/{}",
                                            table_name,
                                            row.get("id").unwrap(),
                                            key,
                                            match row.get(key).unwrap().as_str() {
                                                "" => "None",
                                                a => a,
                                            }
                                        )) {
                                            @if key == "id" {
                                                button
                                                    hx-delete=(format!("/table/{}/{}", table_name, row.get(key).unwrap_or(&"NULL".to_string())))
                                                    hx-target="#main"
                                                    hx-trigger="click"
                                                { "x" }
                                            }
                                            (row.get(key).unwrap_or(&"NULL".to_string()))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
