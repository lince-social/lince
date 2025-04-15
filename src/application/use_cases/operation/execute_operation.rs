use regex::Regex;

use crate::{
    application::use_cases::record::set_quantity::use_case_record_set_quantity,
    presentation::web::{
        operation::get::presentation_web_operation_get_nested_body,
        section::body::presentation_web_section_body,
    },
};

use super::crud::use_case_operation_create_component;

fn parse_table(operation: String) -> String {
    let re = Regex::new(r"\d+").unwrap();

    let table = if let Some(matched) = re.find(&operation) {
        match matched.as_str() {
            "0" => "configuration",
            "1" => "view",
            "2" => "configuration_view",
            "3" => "record",
            "4" => "karma_condition",
            "5" => "karma_consequence",
            "6" => "karma",
            "7" => "command",
            "8" => "frequency",
            "9" => "sum",
            "10" => "history",
            "11" => "dna",
            "12" => "transfer",
            _ => "record",
        }
    } else {
        "record"
    };
    table.to_string()
}

pub async fn execute_operation(operation: String) -> String {
    let only_digits = Regex::new(r"^\d+$").unwrap();
    if only_digits.is_match(&operation) {
        return use_case_record_set_quantity(operation, 0.0).await;
    }

    let table = parse_table(operation.clone());
    let re = Regex::new(r"[a-z]+").unwrap();

    let _ = match re.find(&operation) {
        None => return presentation_web_section_body().await,
        Some(operation) => {
            let element = match operation.as_str() {
                "c" => use_case_operation_create_component(table).await,
                _ => "".to_string(),
            };
            return presentation_web_operation_get_nested_body(element).await;
        }
    };
}
