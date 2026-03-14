use crate::table::{add_row::presentation_html_table_add_row, cell_id};
use domain::clean::table::{SortedTables, Table};
use injection::cross_cutting::InjectedServices;
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
        @for (table_name, table, headers) in sorted_tables {
            div {
                row.middle_y.s_gap {p { (table_name) } (presentation_html_table_add_row(table_name.clone()))}
                table class="rounded-table" {
                    @if !headers.is_empty() {
                        thead {
                            tr {
                                @for (i, key) in headers.iter().enumerate() {
                                    @let class = match i {
                                        0 => "top-left",
                                        _ if i == headers.len() - 1 => "top-right",
                                        _ => "",
                                    };
                                    th class=(class) { (key) }
                                }
                            }
                        }
                    }
                    tbody {
                        @for (row_i, row) in table.iter().enumerate() {
                            @let last_row = row_i == table.len() - 1;
                            tr {
                                @for (col_i, key) in headers.iter().enumerate() {
                                    @let class = match (row_i, col_i) {
                                       (_, 0) if last_row => "bottom-left breakword",
                                        (_, x) if last_row && x == headers.len() - 1 => "bottom-right breakword",
                                        _ => "breakword",
                                    };
                                    @let cell_id = cell_id(&table_name, row.get("id").unwrap(), key);
                                    td id=(cell_id) class=(class) data-signals="{editing: false}" {
                                        div data-show="!$editing" data-on:click="$editing = true" {
                                            @if key == "id" {
                                                button
                                                    type="button"
                                                    data-on:click__stop=(format!("@delete('/table/{}/{}')", table_name, row.get(key).unwrap_or(&"NULL".to_string())))
                                                    class="delete_row"
                                                { "x" }
                                            }
                                            button type="button" class="plain-button" {
                                                (row.get(key).unwrap_or(&"NULL".to_string()))
                                            }
                                        }
                                        form
                                            data-show="$editing"
                                            data-on:submit__prevent=(format!(
                                                "@patch('/table/{}/{}/{}', {{contentType: 'form'}})",
                                                table_name,
                                                row.get("id").unwrap(),
                                                key
                                            ))
                                        {
                                            input
                                                name="value"
                                                value=(row.get(key).unwrap_or(&"".to_string())) {}
                                            button type="submit" { "Save" }
                                            button type="button" data-on:click="$editing = false" { "Cancel" }
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
        @for (table_name, table, headers) in tables {
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
                                    @let cell_id = cell_id(&table_name, row.get("id").unwrap(), key);
                                    td id=(cell_id) data-signals="{editing: false}" {
                                        div data-show="!$editing" data-on:click="$editing = true" {
                                            @if key == "id" {
                                                button
                                                    type="button"
                                                    data-on:click__stop=(format!("@delete('/table/{}/{}')", table_name, row.get(key).unwrap_or(&"NULL".to_string())))
                                                    class="delete_row"
                                                { "x" }
                                                (row.get(key).unwrap_or(&"NULL".to_string()))
                                            } @else if key == "condition" || key == "consequence" {
                                                (presentation_html_tables_karma_replacer(
                                                    services.clone(),
                                                    row.get(key).unwrap_or(&"NULL".to_string()).clone()).await)
                                            } @else {
                                                (row.get(key).unwrap_or(&"NULL".to_string()))
                                            }
                                        }
                                        form
                                            data-show="$editing"
                                            data-on:submit__prevent=(format!(
                                                "@patch('/table/{}/{}/{}', {{contentType: 'form'}})",
                                                table_name,
                                                row.get("id").unwrap(),
                                                key
                                            ))
                                        {
                                            @if key == "condition" || key == "consequence" {
                                                textarea name="value" class="autosize-textarea" { (row.get(key).unwrap_or(&"".to_string())) }
                                            } @else {
                                                input name="value" value=(row.get(key).unwrap_or(&"".to_string())) {}
                                            }
                                            button type="submit" { "Save" }
                                            button type="button" data-on:click="$editing = false" { "Cancel" }
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
        let res = services.repository.record.get_by_id(id).await;
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
        let replacement_f = services.repository.frequency.get(id).await;
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
        let replacement_command = services.repository.command.get_by_id(id).await;
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
