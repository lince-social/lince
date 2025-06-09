use super::{command::use_case_karma_execute_command, engine::return_engine};
use crate::{
    application::use_cases::{
        frequency::update::use_case_frequency_update, query::execute::use_case_query_execute,
    },
    domain::entities::frequency::Frequency,
    infrastructure::cross_cutting::InjectedServices,
};
use regex::Regex;
use std::{collections::HashMap, io::Error};

pub async fn use_case_karma_deliver(services: InjectedServices) -> Result<(), Error> {
    let engine = return_engine();
    let vec_karma = services.providers.karma.get().await?;
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
        if let Some(e) = regex_record_quantity
            .captures(&karma.consequence)
            .and_then(|caps| caps[1].parse::<u32>().ok())
            .and_then(|id| {
                services
                    .providers
                    .record
                    .set_quantity_sync(id, condition)
                    .err()
            })
        {
            println!("Error when zeroing record: {e}")
        }

        if let Some(e) = regex_command
            .captures(&karma.consequence)
            .and_then(|caps| caps[1].parse::<u32>().ok())
            .and_then(|id| {
                futures::executor::block_on(async {
                    use_case_karma_execute_command(services, &id.to_string()).await
                })
            })
        {
            println!("Error when executing command: {e}")
        }

        if let Some(e) = regex_query
            .captures(&karma.consequence)
            .and_then(|caps| caps[1].parse::<u32>().ok())
            .and_then(|id| {
                futures::executor::block_on(async { use_case_query_execute(services, id).await })
                    .err()
            })
        {
            println!("Error when executing query: {e}")
        }
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

        services
            .provider
            .record
            .get_quantity_by_id_sync(id)
            .map_err(|_| "0.0")
            .to_string()
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
        let (replacement, freq_opt) = use_case_frequency_check(services, id);
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

    let replaced = regex.replace_all(&karma_condition, |caps: &regex::Captures| {
        let id = caps[1].parse::<u32>().unwrap();
        use_case_karma_execute_command(id)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string())
    });
    Ok(replaced.into_owned())
}

pub async fn use_case_karma_deliver(services: InjectedServices) -> Result<(), Error> {
    let engine = return_engine();
    let vec_karma = services.providers.karma.get().await?;
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_frequency = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    let regex_query = Regex::new(r"sql(\d+)").unwrap();
    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();

    for karma in vec_karma {
        let mut should_execute = true;

        for condition in karma.condition {
            let record_quantity = regex_record_quantity
                .captures(&condition.condition)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok());

            if let Some(record_id) = record_quantity {
                let quantity = services
                    .providers
                    .record
                    .get_quantity_by_id(record_id)
                    .await;
                if quantity < condition.quantity {
                    should_execute = false;
                    break;
                }
            }
        }

        if !should_execute {
            continue;
        }

        for consequence in karma.consequences {
            let record_quantity = regex_record_quantity
                .captures(&consequence.consequence)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok());

            if let Some(record_id) = record_quantity {
                let quantity = services
                    .providers
                    .record
                    .get_quantity_by_id(record_id)
                    .await;
                let new_quantity = quantity + consequence.quantity;
                services
                    .providers
                    .record
                    .set_quantity(record_id, new_quantity)
                    .await;
            }

            let frequency = regex_frequency
                .captures(&consequence.consequence)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok());

            if let Some(frequency_id) = frequency {
                let (_, frequency) = services.providers.frequency.check(frequency_id).await;
                if let Some(frequency) = frequency {
                    frequencies_to_update.insert(frequency_id, frequency);
                }
            }

            let command = regex_command
                .captures(&consequence.consequence)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok());

            if let Some(command_id) = command {
                use_case_karma_execute_command(services.clone(), command_id).await;
            }

            let query = regex_query
                .captures(&consequence.consequence)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok());

            if let Some(query_id) = query {
                services.providers.query.execute(query_id).await?;
            }
        }
    }

    for (_, frequency) in frequencies_to_update {
        services.providers.frequency.update(frequency).await;
    }

    Ok(())
}
