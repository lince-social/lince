// use super::{
//     command::use_case_karma_execute_command, engine::return_engine, get::use_case_karma_get,
// };
// use crate::{
//     application::{
//         providers::record::{
//             get_quantity_by_id::provider_record_get_quantity_by_id_sync,
//             set_quantity::provider_record_set_quantity_sync,
//         },
//         use_cases::{
//             frequency::{check::use_case_frequency_check, update::use_case_frequency_update},
//             query::execute::use_case_query_execute,
//         },
//     },
//     domain::entities::frequency::Frequency,
// };
// use regex::Regex;
// use std::{collections::HashMap, io::Error};

// pub async fn use_case_karma_deliver() -> Result<(), Error> {
//     let engine = return_engine();
//     let vec_karma = use_case_karma_get().unwrap();
//     let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
//     let regex_frequency = Regex::new(r"f(\d+)").unwrap();
//     let regex_command = Regex::new(r"c(\d+)").unwrap();
//     let regex_query = Regex::new(r"sql(\d+)").unwrap();
//     let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();

//     for karma in vec_karma {
//         println!("{karma:#?}");
//         let condition = replace_record_quantities(regex_record_quantity.clone(), karma.condition)
//             .and_then(|condition| {
//                 replace_frequencies(
//                     regex_frequency.clone(),
//                     condition,
//                     &mut frequencies_to_update,
//                 )
//             })
//             .and_then(|condition| replace_commands(regex_command.clone(), condition));

//         if condition.is_err() {
//             continue;
//         }

//         let condition = format!("({}) * 1.0", condition.unwrap());
//         println!("condition: {condition}");
//         let condition: f64 = engine.eval(&condition).unwrap();

//         let operator = karma.operator.as_str();
//         if !((operator == "=" && condition != 0.0) || operator == "=*") {
//             continue;
//         }

//         regex_record_quantity
//             .captures(&karma.consequence)
//             .and_then(|caps| caps[1].parse::<u32>().ok())
//             .and_then(|id| provider_record_set_quantity_sync(id, condition).err())
//             .map(|e| println!("Error when zeroing record: {e}"));

//         regex_command
//             .captures(&karma.consequence)
//             .and_then(|caps| caps[1].parse::<u32>().ok())
//             .and_then(|id| use_case_karma_execute_command(id))
//             .map(|e| println!("Error when executing command: {e}"));

//         regex_query
//             .captures(&karma.consequence)
//             .and_then(|caps| caps[1].parse::<u32>().ok())
//             .and_then(|id| use_case_query_execute(id).err())
//             .map(|e| println!("Error when executing query: {e}"));
//     }

//     use_case_frequency_update(frequencies_to_update.into_values().collect());
//     Ok(())
// }

// fn replace_record_quantities(regex: Regex, condition: String) -> Result<String, Error> {
//     let mut karma_condition = condition;
//     for caps in regex.captures_iter(&karma_condition.clone()) {
//         let id = caps[1].parse::<u32>().unwrap();
//         let replacement = provider_record_get_quantity_by_id_sync(id)?.to_string();
//         let range = caps.get(0).unwrap().range();
//         if range.end <= karma_condition.len() {
//             karma_condition.replace_range(range, &replacement);
//         }
//     }
//     println!("{karma_condition}");
//     Ok(karma_condition)
// }

// fn replace_frequencies(
//     regex: Regex,
//     karma_condition: String,
//     frequencies_to_update: &mut HashMap<u32, Frequency>,
// ) -> Result<String, Error> {
//     let mut karma_condition = karma_condition;
//     for caps in regex.captures_iter(&karma_condition.clone()) {
//         let id = caps[1].parse::<u32>().unwrap();
//         let (replacement, frequency_to_update) = use_case_frequency_check(id);

//         if let Some(frequency_to_update) = frequency_to_update {
//             frequencies_to_update.insert(frequency_to_update.id, frequency_to_update);
//         }

//         let range = caps.get(0).unwrap().range();
//         if range.end <= karma_condition.len() {
//             karma_condition.replace_range(range, &replacement.to_string());
//         }
//     }
//     Ok(karma_condition)
// }

// fn replace_commands(regex: Regex, karma_condition: String) -> Result<String, Error> {
//     let mut karma_condition = karma_condition;
//     for caps in regex.captures_iter(&karma_condition.clone()) {
//         let id = caps[1].parse::<u32>().unwrap();
//         let replacement = use_case_karma_execute_command(id);
//         karma_condition.replace_range(
//             caps.get(0).unwrap().range(),
//             &replacement.unwrap_or(0).to_string(),
//         );
//     }
//     Ok(karma_condition)
// }
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
        println!("{karma:#?}");
        let condition = replace_record_quantities(&regex_record_quantity, karma.condition)
            .and_then(|c| replace_frequencies(&regex_frequency, c, &mut frequencies_to_update))
            .and_then(|c| replace_commands(&regex_command, c));

        if condition.is_err() {
            continue;
        }

        let condition = format!("({}) * 1.0", condition.unwrap());
        println!("condition: {condition}");
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
    let replaced = regex.replace_all(&condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        provider_record_get_quantity_by_id_sync(id)
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "0".to_string())
    });
    Ok(replaced.into_owned())
}

fn replace_frequencies(
    regex: &Regex,
    karma_condition: String,
    frequencies_to_update: &mut HashMap<u32, Frequency>,
) -> Result<String, Error> {
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
    let replaced = regex.replace_all(&karma_condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        use_case_karma_execute_command(id)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string())
    });
    Ok(replaced.into_owned())
}
