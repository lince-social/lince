use crate::{command::karma_execute_command, karma::karma_deliver};
use injection::cross_cutting::InjectedServices;
use regex::Regex;
use std::{collections::HashMap, io::Error};
use utils::logging::{LogEntry, log};

pub fn operation_tables() -> Vec<(&'static str, &'static str)> {
    vec![
        ("0", "Configuration"),
        ("1", "Collection"),
        ("2", "View"),
        ("3", "collection_View"),
        ("4", "Record"),
        ("5", "Karma_Condition"),
        ("6", "Karma_Consequence"),
        ("7", "Karma"),
        ("8", "Command"),
        ("9", "Frequency"),
        ("10", "Sum"),
        ("11", "History"),
        ("12", "DNA"),
        ("13", "Transfer"),
    ]
}

pub fn operation_actions() -> Vec<(&'static str, &'static str)> {
    vec![
        ("c", "Create"),
        ("q", "SQL Query"),
        ("k", "Karma"),
        ("s", "Shell Command"),
        ("a", "Activate Configuration"),
    ]
}

fn parse_table(operation: String) -> String {
    let re = Regex::new(r"\d+").unwrap();
    let operation_parts: Vec<&str> = operation.split_whitespace().collect();
    for part in operation_parts {
        if let Some(matched) = re.find(part) {
            match matched.as_str() {
                "0" => return "configuration".to_string(),
                "1" => return "collection".to_string(),
                "2" => return "view".to_string(),
                "3" => return "collection_view".to_string(),
                "4" => return "record".to_string(),
                "5" => return "karma_condition".to_string(),
                "6" => return "karma_consequence".to_string(),
                "7" => return "karma".to_string(),
                "8" => return "command".to_string(),
                "9" => return "frequency".to_string(),
                "10" => return "sum".to_string(),
                "11" => return "history".to_string(),
                "12" => return "dna".to_string(),
                "13" => return "transfer".to_string(),
                "14" => return "query".to_string(),
                _ => continue,
            }
        }
    }

    let re = Regex::new(r"\w+").unwrap();
    let operation_parts: Vec<&str> = operation.split_whitespace().collect();
    for part in operation_parts {
        if let Some(matched) = re.find(part) {
            match matched.as_str() {
                "configuration" => return "configuration".to_string(),
                "collection" => return "collection".to_string(),
                "view" => return "view".to_string(),
                "collection_view" => return "collection_view".to_string(),
                "record" => return "record".to_string(),
                "karma_condition" => return "karma_condition".to_string(),
                "karma_consequence" => return "karma_consequence".to_string(),
                "karma" => return "karma".to_string(),
                "command" => return "command".to_string(),
                "frequency" => return "frequency".to_string(),
                "sum" => return "sum".to_string(),
                "history" => return "history".to_string(),
                "dna" => return "dna".to_string(),
                "transfer" => return "transfer".to_string(),
                "query" => return "query".to_string(),
                _ => continue,
            }
        }
    }

    "record".to_string()
}

fn parse_id(part: &str) -> Option<&str> {
    let re = Regex::new(r"\d+").unwrap();

    if let Some(matched) = re.find(part) {
        return Some(matched.as_str());
    }

    None
}

pub async fn parse_operation_and_execute(
    services: InjectedServices,
    operation: String,
) -> Option<String> {
    let re = Regex::new(r"[a-zA-Z]+").unwrap();

    let operation_parts: Vec<&str> = operation.split_whitespace().collect();

    for part in operation_parts {
        if let Some(matched) = re.find(part) {
            match matched.as_str() {
                "q" | "query" => {
                    return Some("query modal".to_string());
                }
                "c" | "create" => {
                    let _ =
                        operation_create_component(services, parse_table(operation.clone())).await;
                    return Some("create modal".to_string());
                }
                "k" | "collection" => {
                    if let Some(id) = parse_id(&operation)
                        && let Err(e) = services.repository.collection.set_active(id).await
                    {
                        log(LogEntry::Error(e.kind(), e.to_string()))
                    }
                    return Some("record body".to_string());
                }
                "a" | "configuration" => {
                    if let Some(id) = parse_id(&operation)
                        && let Err(e) = services.repository.configuration.set_active(id).await
                    {
                        log(LogEntry::Error(e.kind(), e.to_string()))
                    }
                    return Some("configuration body".to_string());
                }
                "s" | "command" | "shell" | "shell command" => {
                    if let Some(id) = parse_id(&operation)
                        && (karma_execute_command(services.clone(), id.parse::<u32>().unwrap_or(0))
                            .await)
                            .is_none()
                    {
                        let e = Error::other(format!("Failed to run command with id: {}", id));
                        log(LogEntry::Error(e.kind(), e.to_string()))
                    }

                    return Some("collection body".to_string());
                }

                _ => continue,
            }
        }
    }

    None
}

pub async fn operation_execute(services: InjectedServices, operation: String) -> String {
    let only_digits_regex = Regex::new(r"^\d+$").unwrap();
    if only_digits_regex.is_match(&operation) {
        let id = operation.parse::<u32>().unwrap();
        let _ = services
            .repository
            .record
            .set_quantity(id, 0.0)
            .await
            .inspect_err(|e| log(LogEntry::Error(e.kind(), e.to_string())));

        let vec_karma = services.repository.karma.get_active(Some(id)).await;
        if let Err(e) = vec_karma {
            log(LogEntry::Error(e.kind(), e.to_string()))
        } else {
            let _ = karma_deliver(services.clone(), vec_karma.unwrap())
                .await
                .inspect_err(|e| log(LogEntry::Error(e.kind(), e.to_string())));
        }

        return "main body".to_string();
    }

    match parse_operation_and_execute(services.clone(), operation).await {
        None => "main body".to_string(),
        Some(element) => element,
    }
}

pub async fn operation_create_component(services: InjectedServices, table: String) -> String {
    let _column_names = services
        .repository
        .operation
        .get_column_names(table.clone())
        .await
        .unwrap_or_default();

    "create form".to_string()
}

pub async fn operation_create_persist(
    services: InjectedServices,
    table: String,
    data: HashMap<String, String>,
) -> String {
    if let Err(e) = services.repository.operation.create(table, data).await {
        println!("Error creating operation: {}", e);
    }
    "main section".to_string()
}
