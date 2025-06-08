use crate::{
    application::use_cases::frequency::get_name::use_case_frequency_get_name,
    domain::entities::table::{SortedTables, Table},
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::table::add_row::presentation_web_table_add_row,
};
use maud::{Markup, html};
use regex::Regex;

pub async fn presentation_web_tables(tables: Vec<(String, Table)>) -> Markup {
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
                    row.middle_y.s_gap {p { (table_name) } (presentation_web_table_add_row(table_name.clone()))}
                    table {
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
                                        td {
                                            form
                                                hx-post=(format!("/table/{}/{}/{}", table_name, row.get("id").unwrap(), key))
                                                hx-swap="outerHTML"
                                                hx-target="closest td"
                                                hx-trigger="click"
                                            {
                                                @if key == "id" {
                                                    button
                                                        hx-delete=(format!("/table/{}/{}",
                                                            table_name, row.get(key).unwrap_or(&"NULL".to_string())))
                                                        hx-target="#main"
                                                        class="delete_row"
                                                        hx-trigger="click"
                                                        onclick="event.stopPropagation()"
                                                    { "x" }
                                                }
                                                input type="hidden" name="value"
                                                    value=(row.get(key).unwrap_or(&"".to_string())) {}
                                                button type="submit" style="all: unset; cursor: pointer;" {
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
    }
}

pub async fn presentation_web_tables_karma(tables: Vec<(String, Table)>) -> Markup {
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
                    table {
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
                                        @if key == "id" {
                                            td
                                                hx-trigger="click" hx-swap="outerHTML"
                                                hx-get=(format!(
                                                "/table/{}/{}/{}/{}",
                                                table_name,
                                                row.get("id").unwrap(),
                                                key,
                                                match row.get(key).unwrap().as_str() {
                                                    "" => "None",
                                                    some => some,
                                                }))
                                        {
                                                button
                                                    hx-delete=(format!("/table/{}/{}",
                                                        table_name,
                                                        row.get(key).unwrap_or(&"NULL".to_string())))
                                                    hx-target="#body"
                                                    class="delete_row"
                                                    hx-trigger="click"
                                                    onclick="event.stopPropagation()"
                                                { "x" }
                                                (row.get(key).unwrap_or(&"NULL".to_string()))}
                                                } @else if key == "condition" || key == "consequence" {
                                                    td
                                                    hx-trigger="click"
                                                    hx-swap="outerHTML"
                                                    hx-post="/table/editable"
                                                    hx-vals=(format!(
                                                        r#"{{ "table": "{}", "id": "{}", "column": "{}", "value": "{}" }}"#,
                                                        table_name,
                                                        row.get("id").unwrap(),
                                                        key,
                                                        row.get(key).unwrap().replace('"', "\\\"")
                                                    ))
                                                        {
                                                    (presentation_web_tables_karma_replacer(
                                                        services.clone(),
                                                        row.get(key).unwrap_or(&"NULL".to_string()).clone()).await)}
                                                } @else {
                                                    td
                                                        hx-trigger="click"
                                                        hx-swap="outerHTML"
                                                        hx-post="/table/editable"
                                                        hx-vals=(format!(
                                                            r#"{{ "table": "{}", "id": "{}", "column": "{}", "value": "{}" }}"#,
                                                            table_name,
                                                            row.get("id").unwrap(),
                                                            key,
                                                            row.get(key).unwrap().replace('"', "\\\"")
                                                        ))
                                                        {
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
}

pub async fn presentation_web_tables_karma_replacer(
    services: InjectedServices,
    id: u32,
    key: String,
) -> Markup {
    let res = services.providers.record.get_head_by_id(id).await;
    match res {
        Ok(head) => {
            let replacement_f = services
                .providers
                .frequency
                .get(head)
                .execute(services.clone(), head.frequency_id)
                .await;
            let replacement_command = services
                .providers
                .command
                .get_by_id(id)
                .get
                .get_command_name(services.clone(), head.command_id)
                .await;

            html! {
                td {
                    (replacement_f)
                }
                td {
                    (replacement_command)
                }
            }
        }
        Err(_) => html! {
            td { "Error" }
            td { "Error" }
        },
    }
}
