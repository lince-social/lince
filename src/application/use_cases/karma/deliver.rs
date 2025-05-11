use super::{
    command::use_case_karma_execute_command, engine::return_engine, get::use_case_karma_get,
};
use crate::{
    application::{
        providers::record::{
            get_quantity_by_id::provider_record_get_quantity_by_id_sync,
            set_quantity::provider_record_set_quantity_sync,
        },
        use_cases::{
            frequency::{check::use_case_frequency_check, update::use_case_frequency_update},
            query::execute::use_case_query_execute,
        },
    },
    domain::entities::frequency::Frequency,
};
use regex::Regex;
use std::{collections::HashMap, io::Error};

pub async fn use_case_karma_deliver() -> Result<(), Error> {
    let engine = return_engine();
    let vec_karma = use_case_karma_get().unwrap();
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_frequency = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    let regex_query = Regex::new(r"sql(\d+)").unwrap();
    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();

    for karma in vec_karma {
        let condition =
            match replace_record_quantities(&regex_record_quantity, karma.clone().condition) {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("record quantity error on karma id {}", karma.id);
                    continue;
                }
            };

        let condition =
            match replace_frequencies(&regex_frequency, condition, &mut frequencies_to_update) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("frequency error on karma id {}: {}", karma.id, e);
                    continue;
                }
            };

        let condition = match replace_commands(&regex_command, condition) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("command error on karma id {}: {}", karma.id, e);
                continue;
            }
        };

        let condition = format!("({}) * 1.0", condition);
        let condition: f64 = engine.eval(&condition).unwrap();

        let operator = karma.operator.as_str();
        if !((operator == "=" && condition != 0.0) || operator == "=*") {
            continue;
        }

        regex_record_quantity
            .captures(&karma.consequence)
            .and_then(|caps| caps[1].parse::<u32>().ok())
            .and_then(|id| provider_record_set_quantity_sync(id, condition).err())
            .map(|e| println!("Error when zeroing record: {e}"));

        regex_command
            .captures(&karma.consequence)
            .and_then(|caps| caps[1].parse::<u32>().ok())
            .and_then(|id| use_case_karma_execute_command(id))
            .map(|e| println!("Error when executing command: {e}"));

        regex_query
            .captures(&karma.consequence)
            .and_then(|caps| caps[1].parse::<u32>().ok())
            .and_then(|id| use_case_query_execute(id).err())
            .map(|e| println!("Error when executing query: {e}"));
    }

    use_case_frequency_update(frequencies_to_update.into_values().collect());
    Ok(())
}

fn replace_record_quantities(regex: &Regex, condition: String) -> Result<String, Error> {
    if !regex.is_match(&condition) {
        return Ok(condition);
    }

    let replaced = regex.replace_all(&condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        let quantity = match provider_record_get_quantity_by_id_sync(id) {
            Ok(quantity) => quantity.to_string(),
            Err(_) => "0.0".to_string(),
        };
        quantity
    });
    Ok(replaced.into_owned())
}

fn replace_frequencies(
    regex: &Regex,
    karma_condition: String,
    frequencies_to_update: &mut HashMap<u32, Frequency>,
) -> Result<String, Error> {
    if !regex.is_match(&karma_condition) {
        return Ok(karma_condition);
    }

    let replaced = regex.replace_all(&karma_condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        let (replacement, freq_opt) = use_case_frequency_check(id);
        if let Some(f) = freq_opt {
            frequencies_to_update.insert(f.id, f);
        }
        replacement.to_string()
    });
    Ok(replaced.into_owned())
}

fn replace_commands(regex: &Regex, karma_condition: String) -> Result<String, Error> {
    if !regex.is_match(&karma_condition) {
        return Ok(karma_condition);
    }

    let replaced = regex.replace_all(&karma_condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        use_case_karma_execute_command(id)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string())
    });
    Ok(replaced.into_owned())
}
