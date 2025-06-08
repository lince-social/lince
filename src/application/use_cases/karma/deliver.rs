use super::{
    command::use_case_karma_execute_command, engine::return_engine, get::use_case_karma_get,
};
use crate::{
    domain::entities::frequency::Frequency, infrastructure::cross_cutting::InjectedServices,
};
use regex::Regex;
use std::{collections::HashMap, io::Error};

pub async fn use_case_karma_deliver(services: InjectedServices) -> Result<(), Error> {
    let engine = return_engine();
    let vec_karma = use_case_karma_get(services.clone()).await?;
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
