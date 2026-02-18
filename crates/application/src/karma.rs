use domain::clean::{frequency::Frequency, karma::Karma};
use injection::cross_cutting::InjectedServices;
use regex::Regex;
use utils::info;
use utils::logging::{LogEntry, log};

use crate::{
    command::karma_execute_command, engine::return_engine, frequency::frequency_check,
    query::query_execute,
};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub async fn karma_deliver(services: InjectedServices, vec_karma: Vec<Karma>) -> Result<(), Error> {
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_frequency = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    let regex_query = Regex::new(r"sql(\d+)").unwrap();

    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();

    let mut prepared = Vec::with_capacity(vec_karma.len());

    for karma in vec_karma {
        let mut condition = karma.condition.clone();

        condition = replace_record_quantities(&services, &regex_record_quantity, condition).await?;

        condition = replace_frequencies(
            &services,
            &regex_frequency,
            condition,
            &mut frequencies_to_update,
        )
        .await?;

        condition = replace_commands(&services, &regex_command, condition).await?;

        prepared.push((karma, format!("({}) * 1.0", condition)));
    }

    // ─────────────────────────────
    // 2. Blocking Rhai evaluation
    // ─────────────────────────────
    let evaluated = tokio::task::spawn_blocking(move || {
        let engine = return_engine();

        prepared
            .into_iter()
            .map(|(karma, expr)| {
                let value = engine.eval::<f64>(&expr).unwrap_or(0.0);
                (karma, value)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(Error::other)?;

    // ─────────────────────────────
    // 3. Async execution phase
    // ─────────────────────────────
    for (karma, condition) in evaluated {
        let operator = karma.operator.as_str();
        if !((operator == "=" && condition != 0.0) || operator == "=*") {
            continue;
        }

        if let Some(caps) = regex_record_quantity.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                if let Err(e) = services.repository.record.set_quantity(id, condition).await {
                    log(LogEntry::Error(
                        e.kind(),
                        format!("record quantity error on karma id {}", karma.id),
                    ));
                }
            }
        }

        if let Some(caps) = regex_command.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                info!("Running command {}", id);
                let services = services.clone();
                let karma_id = karma.id;
                tokio::spawn(async move {
                    if karma_execute_command(services, id).await.is_none() {
                        log(LogEntry::Error(
                            ErrorKind::Other,
                            format!("Command returned None for karma id {}", karma_id),
                        ));
                    }
                });
            }
        }

        if let Some(caps) = regex_query.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                if let Err(e) = query_execute(services.clone(), id).await {
                    log(LogEntry::Error(
                        ErrorKind::Other,
                        format!("Query error on karma id {}: {e}", karma.id),
                    ));
                }
            }
        }
    }

    for (_, frequency) in frequencies_to_update {
        let _ = services.repository.frequency.update(frequency).await;
    }

    Ok(())
}

async fn replace_record_quantities(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let id = caps[1].parse::<u32>().unwrap();
        let value = services
            .repository
            .record
            .get_by_id(id)
            .await
            .map(|r| r.quantity)
            .unwrap_or(0.0);

        output = output.replace(&caps[0], &value.to_string());
    }

    Ok(output)
}

async fn replace_frequencies(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
    frequencies_to_update: &mut HashMap<u32, Frequency>,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let id = caps[1].parse::<u32>().unwrap();
        let (replacement, freq_opt) = frequency_check(services.clone(), id).await;

        if let Some(f) = freq_opt {
            frequencies_to_update.insert(f.id, f);
        }

        output = output.replace(&caps[0], &replacement.to_string());
    }

    Ok(output)
}

async fn replace_commands(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let id = caps[1].parse::<u32>().unwrap();
        let value = karma_execute_command(services.clone(), id)
            .await
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string());

        output = output.replace(&caps[0], &value);
    }

    Ok(output)
}
