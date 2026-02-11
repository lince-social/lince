use crate::{command::karma_execute_command, karma::karma_deliver};
use domain::dirty::operation::{OperationActions, OperationTables};
use injection::cross_cutting::InjectedServices;
use utils::logging::{LogEntry, log};

use regex::Regex;
use std::io::Error;

fn parse_id(part: &str) -> Option<&str> {
    let re = Regex::new(r"\d+").unwrap();

    if let Some(matched) = re.find(part) {
        return Some(matched.as_str());
    }

    None
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
                    if let Some(id) = parse_id(&operation)
                        && (karma_execute_command(services.clone(), id.parse::<u32>().unwrap_or(0))
                            .await)
                            .is_none()
                    {
                        let e = Error::other(format!("Failed to run command with id: {}", id));
                        log(LogEntry::Error(e.kind(), e.to_string()))
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
) -> Result<Vec<(OperationTables, OperationActions)>, Error> {
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

    // match parse_operation_and_execute(services.clone(), operation).await {
    //     None => presentation_html_section_body(services).await,
    //     Some(element) => element,
    // }
    Ok(vec![])
}

// Pega a operação que chegou e categoriza ela
// Se for Numbers zera rqNumbers, devolve: Enum
// Se for numeros e letras faz o parse e monta o Enum, devolve: Enum porque cada presentation tem que poder lidar de forma diferente com uma operação, a operação faz o parsing, e só
// a execução de mudança de coleção ativa, refetch dos dados, enviar comandos depende do frontend pq ele precisa poder lidar com o ato em si, não pegar um memorando dele.
