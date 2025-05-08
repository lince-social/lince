use crate::{
    application::{
        providers::record::get_name_by_id::provider_record_get_head_by_id,
        use_cases::{
            frequency::get_name::use_case_frequency_get_name,
            karma::command::use_case_command_get_name,
        },
    },
    domain::entities::table::{SortedTables, Table},
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
                                                    hx-delete=(format!("/table/{}/{}",
                                                        table_name, row.get(key).unwrap_or(&"NULL".to_string())))
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
                                                (presentation_web_tables_karma_replacer(
                                                    row.get(key).unwrap_or(&"NULL".to_string()).clone()).await)}
                                            } @else {
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
                                            (row.get(key).unwrap_or(&"NULL".to_string()))}
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

pub async fn presentation_web_tables_karma_replacer(og_row: String) -> Markup {
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
        let res = provider_record_get_head_by_id(id).await;
        let replacement_rq = match res {
            Ok(quantity) => quantity.to_string(),
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
        let replacement_f = use_case_frequency_get_name(id).await;
        match replacement_f {
            None => println!("Empty frequency name with id: {id}"),
            Some(name) => row.replace_range(range, &name),
        }
    }

    let mut replacements_command = Vec::new();
    for caps in regex_command.captures_iter(&row) {
        let id = caps[1].parse::<u32>().unwrap();
        replacements_command.push((caps.get(0).unwrap().range(), id));
    }
    for (range, id) in replacements_command.into_iter().rev() {
        let replacement_command = use_case_command_get_name(id).await;
        match replacement_command {
            None => println!("Empty command name with id: {id}"),
            Some(name) => row.replace_range(range, &name),
        }
    }

    html!((row))
}
