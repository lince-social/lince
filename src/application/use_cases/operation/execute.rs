use super::crud::use_case_operation_create_component;
use crate::{
    application::use_cases::record::set_quantity::use_case_record_set_quantity,
    presentation::web::{
        operation::{
            get::presentation_web_operation_get_nested_body,
            query::presentation_web_operation_query,
        },
        section::body::presentation_web_section_body,
    },
};
use regex::Regex;

fn parse_table(operation: String) -> String {
    let re = Regex::new(r"\d+").unwrap();
    let operation_parts: Vec<&str> = operation.split_whitespace().collect();
    for part in operation_parts {
        if let Some(matched) = re.find(part) {
            match matched.as_str() {
                "0" => return "configuration".to_string(),
                "1" => return "view".to_string(),
                "2" => return "configuration_view".to_string(),
                "3" => return "record".to_string(),
                "4" => return "karma_condition".to_string(),
                "5" => return "karma_consequence".to_string(),
                "6" => return "karma".to_string(),
                "7" => return "command".to_string(),
                "8" => return "frequency".to_string(),
                "9" => return "sum".to_string(),
                "10" => return "history".to_string(),
                "11" => return "dna".to_string(),
                "12" => return "transfer".to_string(),
                "13" => return "query".to_string(),
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
                "view" => return "view".to_string(),
                "configuration_view" => return "configuration_view".to_string(),
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

pub async fn parse_operation_and_execute(operation: String) -> Option<String> {
    let re = Regex::new(r"[a-zA-Z]+").unwrap();

    let operation_parts: Vec<&str> = operation.split_whitespace().collect();

    for part in operation_parts {
        if let Some(matched) = re.find(part) {
            match matched.as_str() {
                "q" | "query" => {
                    return Some(
                        presentation_web_operation_get_nested_body(
                            presentation_web_operation_query().await,
                        )
                        .await,
                    );
                }
                "c" | "create" => {
                    return Some(
                        presentation_web_operation_get_nested_body(
                            use_case_operation_create_component(parse_table(operation.clone()))
                                .await,
                        )
                        .await,
                    );
                }
                _ => continue,
            }
        }
    }

    None
}

pub async fn use_case_operation_execute(operation: String) -> String {
    let only_digits = Regex::new(r"^\d+$").unwrap();
    if only_digits.is_match(&operation) {
        return use_case_record_set_quantity(operation.parse::<u32>().unwrap(), 0.0).await;
    }

    match parse_operation_and_execute(operation).await {
        None => presentation_web_section_body().await,
        Some(element) => element,
    }
}
