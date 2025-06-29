use super::{command::use_case_karma_execute_command, engine::return_engine};
use crate::{
    application::use_cases::{
        frequency::{check::use_case_frequency_check, update::use_case_frequency_update},
        query::execute::use_case_query_execute,
    },
    domain::entities::frequency::Frequency,
    infrastructure::{
        cross_cutting::InjectedServices,
        utils::log::{LogEntry, log},
    },
};
use regex::Regex;
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub async fn use_case_karma_deliver(services: InjectedServices) -> Result<(), Error> {
    let engine = return_engine();
    let vec_karma = futures::executor::block_on(async { services.providers.karma.get().await })?;
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_frequency = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    let regex_query = Regex::new(r"sql(\d+)").unwrap();
    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();

    for karma in vec_karma {
        dbg!(&karma.id);
        let condition = match replace_record_quantities(
            services.clone(),
            &regex_record_quantity,
            karma.clone().condition,
        ) {
            Ok(c) => c,
            Err(e) => {
                log(LogEntry::Error(
                    e.kind(),
                    format!("record quantity error on karma id {}", karma.id),
                ));
                continue;
            }
        };

        let condition = match replace_frequencies(
            services.clone(),
            &regex_frequency,
            condition,
            &mut frequencies_to_update,
        ) {
            Ok(c) => c,
            Err(e) => {
                log(LogEntry::Error(
                    e.kind(),
                    format!("frequency error on karma id {}: {}", karma.id, e),
                ));
                continue;
            }
        };

        let condition = match replace_commands(services.clone(), &regex_command, condition) {
            Ok(c) => c,
            Err(e) => {
                log(LogEntry::Error(
                    e.kind(),
                    format!("command error on karma id {}: {}", karma.id, e),
                ));
                continue;
            }
        };

        let condition = format!("({}) * 1.0", condition);
        let condition: f64 = engine.eval(&condition).unwrap();

        let operator = karma.operator.as_str();
        if !((operator == "=" && condition != 0.0) || operator == "=*") {
            continue;
        }
        dbg!(&condition);

        // Handle `record.set_quantity(...)`
        if let Some(caps) = regex_record_quantity.captures(&karma.consequence) {
            let id_str = &caps[1];
            match id_str.parse::<u32>() {
                Ok(id) => {
                    if let Err(e) = futures::executor::block_on(async {
                        services.providers.record.set_quantity(id, condition).await
                    }) {
                        log(LogEntry::Error(
                            ErrorKind::Other,
                            format!("Error when zeroing record on karma id {}: {e}", karma.id),
                        ));
                    }
                }
                Err(e) => log(LogEntry::Error(
                    ErrorKind::Other,
                    format!(
                        "Failed to parse record id '{}' on karma id {}: {e}",
                        id_str, karma.id
                    ),
                )),
            }
        }

        // Handle `use_case_karma_execute_command(...)`
        if let Some(caps) = regex_command.captures(&karma.consequence) {
            let id_str = &caps[1];
            match id_str.parse::<u32>() {
                Ok(id) => {
                    println!("Chegou no comando com id: {}", id);
                    let result = futures::executor::block_on(async {
                        use_case_karma_execute_command(services.clone(), id).await
                    });
                    if result.is_none() {
                        log(LogEntry::Error(
                            ErrorKind::Other,
                            format!("Command returned None for karma id {}", karma.id),
                        ));
                    }
                }
                Err(e) => log(LogEntry::Error(
                    ErrorKind::Other,
                    format!(
                        "Failed to parse command id '{}' on karma id {}: {e}",
                        id_str, karma.id
                    ),
                )),
            }
        }

        // Handle `use_case_query_execute(...)`
        if let Some(caps) = regex_query.captures(&karma.consequence) {
            let id_str = &caps[1];
            match id_str.parse::<u32>() {
                Ok(id) => {
                    if let Err(e) = futures::executor::block_on(async {
                        use_case_query_execute(services.clone(), id).await
                    }) {
                        log(LogEntry::Error(
                            ErrorKind::Other,
                            format!("Error when executing query on karma id {}: {e}", karma.id),
                        ));
                    }
                }
                Err(e) => log(LogEntry::Error(
                    ErrorKind::Other,
                    format!(
                        "Failed to parse query id '{}' on karma id {}: {e}",
                        id_str, karma.id
                    ),
                )),
            }
        }
        // if let Some(caps) = regex_record_quantity.captures(&karma.consequence) {
        //     let id_str = &caps[1];
        //     match id_str.parse::<u32>() {
        //         Ok(id) => {
        //             if let Err(e) = futures::executor::block_on(async {
        //                 services.providers.record.set_quantity(id, condition).await
        //             }) {
        //                 println!("Error when zeroing record on karma id {}: {e}", karma.id);
        //             }
        //         }
        //         Err(e) => println!(
        //             "Failed to parse record id '{}' on karma id {}: {e}",
        //             id_str, karma.id
        //         ),
        //     }
        // }

        // if let Some(caps) = regex_command.captures(&karma.consequence) {
        //     let id_str = &caps[1];
        //     match id_str.parse::<u32>() {
        //         Ok(id) => {
        //             println!("Chegou no comando com id: {}", id);
        //             let result = futures::executor::block_on(async {
        //                 use_case_karma_execute_command(services.clone(), id).await
        //             });
        //             if result.is_none() {
        //                 log(LogEntry::Error(
        //                     ErrorKind::Other,
        //                     format!("Command returned None for karma id {}", karma.id),
        //                 ));
        //             }
        //         }
        //         Err(e) => log(LogEntry::Error(
        //             ErrorKind::Other,
        //             format!(
        //                 "Failed to parse command id '{}' on karma id {}: {e}",
        //                 id_str, karma.id
        //             ),
        //         )),
        //     }
        // }

        // if let Some(caps) = regex_query.captures(&karma.consequence) {
        //     let id_str = &caps[1];
        //     match id_str.parse::<u32>() {
        //         Ok(id) => {
        //             if let Err(e) = futures::executor::block_on(async {
        //                 use_case_query_execute(services.clone(), id).await
        //             }) {
        //                 println!("Error when executing query on karma id {}: {e}", karma.id);
        //             }
        //         }
        //         Err(e) => println!(
        //             "Failed to parse query id '{}' on karma id {}: {e}",
        //             id_str, karma.id
        //         ),
        //     }
        // }
        // if let Some(caps) = regex_record_quantity.captures(&karma.consequence) {
        //     let id_str = &caps[1];
        //     match id_str.parse::<u32>() {
        //         Ok(id) => {
        //             if let Err(e) = futures::executor::block_on(async {
        //                 services.providers.record.set_quantity(id, condition).await
        //             }) {
        //                 println!("Error when zeroing record on karma id {}: {e}", karma.id);
        //             }
        //         }
        //         Err(e) => println!(
        //             "Failed to parse record id '{}' on karma id {}: {e}",
        //             id_str, karma.id
        //         ),
        //     }
        // }

        // if let Some(caps) = regex_command.captures(&karma.consequence) {
        //     let id_str = &caps[1];
        //     match id_str.parse::<u32>() {
        //         Ok(id) => {
        //             println!("Chegou no comando com id: {}", id);
        //             if let Err(e) = futures::executor::block_on(async {
        //                 use_case_karma_execute_command(services.clone(), id).await
        //             }) {
        //                 println!("Error when executing command on karma id {}: {e}", karma.id);
        //             }
        //         }
        //         Err(e) => println!(
        //             "Failed to parse command id '{}' on karma id {}: {e}",
        //             id_str, karma.id
        //         ),
        //     }
        // }

        // if let Some(caps) = regex_query.captures(&karma.consequence) {
        //     let id_str = &caps[1];
        //     match id_str.parse::<u32>() {
        //         Ok(id) => {
        //             if let Err(e) = futures::executor::block_on(async {
        //                 use_case_query_execute(services.clone(), id).await
        //             }) {
        //                 println!("Error when executing query on karma id {}: {e}", karma.id);
        //             }
        //         }
        //         Err(e) => println!(
        //             "Failed to parse query id '{}' on karma id {}: {e}",
        //             id_str, karma.id
        //         ),
        //     }
        // }

        // if let Some(e) = regex_record_quantity
        //     .captures(&karma.consequence)
        //     .and_then(|caps| caps[1].parse::<u32>().ok())
        //     .and_then(|id| {
        //         futures::executor::block_on(async {
        //             services
        //                 .providers
        //                 .record
        //                 .set_quantity(id, condition)
        //                 .await
        //                 .err()
        //         })
        //     })
        // {
        //     println!("Error when zeroing record: {e}")
        // }

        // if let Some(e) = regex_command
        //     .captures(&karma.consequence)
        //     .and_then(|caps| caps[1].parse::<u32>().ok())
        //     .and_then(|id| {
        //         println!("Chegou no comando com id: {}", id);
        //         futures::executor::block_on(async {
        //             use_case_karma_execute_command(services.clone(), id).await
        //         })
        //     })
        // {
        //     println!("Error when executing command: {e}")
        // }

        // if let Some(e) = regex_query
        //     .captures(&karma.consequence)
        //     .and_then(|caps| caps[1].parse::<u32>().ok())
        //     .and_then(|id| {
        //         futures::executor::block_on(async {
        //             use_case_query_execute(services.clone(), id).await
        //         })
        //         .err()
        //     })
        // {
        //     println!("Error when executing query: {e}")
        // }
    }

    futures::executor::block_on(async {
        use_case_frequency_update(services, frequencies_to_update.into_values().collect()).await
    });
    Ok(())
}

fn replace_record_quantities(
    services: InjectedServices,
    regex: &Regex,
    condition: String,
) -> Result<String, Error> {
    if !regex.is_match(&condition) {
        return Ok(condition);
    }

    let replaced = regex.replace_all(&condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();

        match futures::executor::block_on(async { services.providers.record.get_by_id(id).await }) {
            Ok(record) => record.quantity.to_string(),
            Err(_) => "0.0".to_string(),
        }
    });

    Ok(replaced.into_owned())
}

fn replace_frequencies(
    services: InjectedServices,
    regex: &Regex,
    karma_condition: String,
    frequencies_to_update: &mut HashMap<u32, Frequency>,
) -> Result<String, Error> {
    if !regex.is_match(&karma_condition) {
        return Ok(karma_condition);
    }

    let replaced = regex.replace_all(&karma_condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        let (replacement, freq_opt) = futures::executor::block_on(async {
            use_case_frequency_check(services.clone(), id).await
        });
        if let Some(f) = freq_opt {
            frequencies_to_update.insert(f.id, f);
        }
        replacement.to_string()
    });
    Ok(replaced.into_owned())
}

fn replace_commands(
    services: InjectedServices,
    regex: &Regex,
    karma_condition: String,
) -> Result<String, Error> {
    if !regex.is_match(&karma_condition) {
        return Ok(karma_condition);
    }
    dbg!(&karma_condition);

    let replaced = regex.replace_all(&karma_condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        futures::executor::block_on(async {
            use_case_karma_execute_command(services.clone(), id).await
        })
        .map(|v| v.to_string())
        .unwrap_or_else(|| "0".to_string())
    });
    Ok(replaced.into_owned())
}
