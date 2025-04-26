use crate::{
    domain::entities::table::{SortedTables, Table},
    presentation::web::table::add_row::presentation_web_table_add_row,
};
use maud::{Markup, html};

pub async fn presentation_web_tables(page: String, tables: Vec<(String, Table)>) -> Markup {
    let sorted_tables: SortedTables = tables
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
            @for (table_name, table, headers) in sorted_tables {
                div {
                    div class="row middle_y" {p { (table_name) } (presentation_web_table_add_row(table_name.clone()))}
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
                                                some => some,
                                            }))
                                            {
                                            @if key == "id" {
                                                button
                                                    hx-delete=(format!("/table/{}/{}/page/{}", table_name, row.get(key).unwrap_or(&"NULL".to_string()), page))
                                                    hx-target="#main"
                                                    class="delete_row"
                                                    hx-trigger="click"
                                                    onclick="event.stopPropagation()"
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
