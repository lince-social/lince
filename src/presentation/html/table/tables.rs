use crate::{
    domain::entities::table::{SortedTables, Table},
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::table::add_row::presentation_html_table_add_row,
};
use maud::{Markup, html};
use regex::Regex;

pub async fn presentation_html_tables(tables: Vec<(String, Table)>) -> Markup {
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
                    row.middle_y.s_gap {p { (table_name) } (presentation_html_table_add_row(table_name.clone()))}
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

pub async fn presentation_html_tables_karma(
    services: InjectedServices,
    tables: SortedTables,
) -> Markup {
    html! {
        main id="main" {
            @for (table_name, table, headers) in tables{
                div {
                    div class="row middle_y" {p { (table_name) } (presentation_html_table_add_row(table_name.clone()))}
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
                                                    (presentation_html_tables_karma_replacer(
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
pub async fn presentation_html_tables_karma_replacer(
    services: InjectedServices,
    og_row: String,
) -> Markup {
    let regex_rq = Regex::new(r"rq(\d+)").unwrap();
    let regex_f = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    // let mut row = og_row.clone();
    let mut row = og_row;

    let mut replacements_rq = Vec::new();
    for caps in regex_rq.captures_iter(&row) {
        let id = caps[1].parse::<u32>().unwrap();
        replacements_rq.push((caps.get(0).unwrap().range(), id));
    }
    for (range, id) in replacements_rq.into_iter().rev() {
        let res = services.providers.record.get_by_id(id).await;
        let replacement_rq = match res {
            Ok(record) => record.head.to_string(),
            Err(error) => {
                println!("Error at record with id: {id}: {}", error);
                "Error".to_string()
            }
        };
        row.replace_range(range, &replacement_rq);
    }

    let mut replacements_f = Vec::new();
    for caps in regex_f.captures_iter(&row) {
        let id = caps[1].parse::<u32>().unwrap();
        replacements_f.push((caps.get(0).unwrap().range(), id));
    }
    for (range, id) in replacements_f.into_iter().rev() {
        let replacement_f = services.providers.frequency.get(id).await;
        match replacement_f {
            Ok(opt) => match opt {
                None => println!("Empty frequency name with id: {id}"),
                Some(frequency) => row.replace_range(range, &frequency.name),
            },
            Err(e) => println!(
                "Error when fetching frequency with id: {}. Error: {}",
                id, e
            ),
        }
    }

    let mut replacements_command = Vec::new();
    for caps in regex_command.captures_iter(&row) {
        let id = caps[1].parse::<u32>().unwrap();
        replacements_command.push((caps.get(0).unwrap().range(), id));
    }
    for (range, id) in replacements_command.into_iter().rev() {
        let replacement_command = services.providers.command.get_by_id(id).await;
        match replacement_command {
            Ok(opt) => match opt {
                None => println!("Empty command name with id: {id}"),
                Some(command) => row.replace_range(range, &command.name),
            },
            Err(e) => println!("Error when fetching command with id: {}. Error: {}", id, e),
        }
    }

    html!((row))
}
