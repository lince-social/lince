use crate::{
    command::{CommandOrigin, spawn_command_buffer_session_by_id},
    karma::karma_deliver,
};
use domain::dirty::operation::{DatabaseTable, OperationActions, ParsedOperation};
use injection::cross_cutting::InjectedServices;
use utils::logging::{LogEntry, log};

use regex::Regex;
use std::io::Error;
use std::str::FromStr;

fn parse_id(part: &str) -> Option<&str> {
    let re = Regex::new(r"\d+").unwrap();

    if let Some(matched) = re.find(part) {
        return Some(matched.as_str());
    }

    None
}

fn parse_operation_action(operation: &str) -> Option<OperationActions> {
    let compact = operation
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_lowercase();

    let short_pattern = Regex::new(r"^(\d+c|c\d+)$").unwrap();
    if short_pattern.is_match(&compact) {
        return Some(OperationActions::Create);
    }

    for part in operation.split_whitespace() {
        let normalized = part.to_lowercase();
        if matches!(normalized.as_str(), "criar" | "novo") {
            return Some(OperationActions::Create);
        }
        if let Ok(action) = OperationActions::from_str(&normalized) {
            return Some(action);
        }
    }

    None
}

fn parse_operation_table(operation: &str) -> Option<DatabaseTable> {
    if let Some(id) = parse_id(operation)
        && let Ok(parsed_id) = id.parse::<u32>()
        && let Some(table) = DatabaseTable::from_id(parsed_id)
    {
        return Some(table);
    }

    let word_regex = Regex::new(r"[a-zA-Z_]+").unwrap();
    for matched in word_regex.find_iter(operation) {
        if let Ok(table) = DatabaseTable::from_str(matched.as_str()) {
            return Some(table);
        }
    }

    None
}

fn parse_operation_result(operation: &str) -> Vec<ParsedOperation> {
    let mut results = Vec::new();

    if let Some(action) = parse_operation_action(operation)
        && action == OperationActions::Create
    {
        results.push(ParsedOperation::new(action, parse_operation_table(operation)));
    }

    results
}

pub async fn parse_operation_and_execute(services: InjectedServices, operation: String) {
    let re = Regex::new(r"[a-zA-Z]+").unwrap();

    let operation_parts: Vec<&str> = operation.split_whitespace().collect();

    for part in operation_parts {
        if let Some(matched) = re.find(part) {
            match matched.as_str() {
                // "q" | "query" => {
                //     return Some(
                //         presentation_html_section_body_home_modal(
                //             presentation_html_operation_query().await,
                //         )
                //         .await,
                //     );
                // }
                // "c" | "create" => {
                //     return Some(
                //         presentation_html_section_body_home_modal(
                //             operation_create_component(services, parse_table(operation.clone()))
                //                 .await,
                //         )
                //         .await,
                //     );
                // }
                "k" | "collection" => {
                    if let Some(id) = parse_id(&operation)
                        && let Err(e) = services.repository.collection.set_active(id).await
                    {
                        log(LogEntry::Error(e.kind(), e.to_string()))
                    }
                }
                "a" | "configuration" => {
                    if let Some(id) = parse_id(&operation)
                        && let Err(e) = services.repository.configuration.set_active(id).await
                    {
                        log(LogEntry::Error(e.kind(), e.to_string()))
                    }
                }
                "s" | "command" | "shell" | "shell command" => {
                    if let Some(id) = parse_id(&operation) {
                        let parsed_id = id.parse::<u32>().unwrap_or(0);
                        if let Err(e) = spawn_command_buffer_session_by_id(
                            services.clone(),
                            parsed_id,
                            CommandOrigin::Operation,
                        )
                        .await
                        {
                            log(LogEntry::Error(e.kind(), e.to_string()))
                        }
                    }
                }

                _ => continue,
            }
        }
    }
}

pub async fn operation_execute(
    services: InjectedServices,
    operation: String,
) -> Result<Vec<ParsedOperation>, Error> {
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
    }

    parse_operation_and_execute(services.clone(), operation.clone()).await;

    Ok(parse_operation_result(&operation))
}
