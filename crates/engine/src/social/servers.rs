use super::*;

const NAMESPACE: &str = "lince.social.servers";

impl Engine {
    pub(super) async fn social_servers(&self) -> Result<Vec<ServerChoice>, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let choices: Vec<ServerChoice> =
            store::records::get_extension(&self.store.pool, &cell.uid, NAMESPACE)
                .await?
                .map(|value| serde_json::from_value(value["choices"].clone()))
                .transpose()?
                .unwrap_or_default();
        validate(&choices)?;
        Ok(choices)
    }

    pub(super) async fn social_save_server(
        &self,
        choice: Option<ServerChoice>,
        endpoint: &str,
    ) -> Result<Value, EngineError> {
        let endpoint = endpoint
            .parse::<iroh::EndpointId>()
            .map_err(|_| invalid("Choose a valid pinned server endpoint ID"))?
            .to_string();
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut tx = self.social_write_tx().await?;
        let held: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(&cell.uid)
        .bind(NAMESPACE)
        .fetch_optional(&mut *tx)
        .await?;
        let mut choices: Vec<ServerChoice> = held
            .map(|body| {
                serde_json::from_str::<Value>(&body)
                    .and_then(|value| serde_json::from_value(value["choices"].clone()))
            })
            .transpose()?
            .unwrap_or_default();
        validate(&choices)?;
        choices.retain(|entry| entry.endpoint != endpoint);
        if let Some(mut choice) = choice {
            choice.endpoint = endpoint;
            choices.push(choice);
        }
        choices.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
        validate(&choices)?;
        store::records::set_extension_on(
            &mut tx,
            &cell.uid,
            NAMESPACE,
            &json!({"choices":choices}),
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"servers":choices,"can_manage_services":true,"status":"Server choices saved for this device. Existing posts and encrypted delivery keep their previously reviewed destinations; withdraw posts or replace reply routes separately"}),
        )
    }
}

fn validate(choices: &[ServerChoice]) -> Result<(), EngineError> {
    if choices.len() > 16
        || choices.iter().filter(|c| c.publication).count() > 8
        || choices.iter().filter(|c| c.query).count() > 8
        || choices.iter().filter(|c| c.mailbox).count() > 8
    {
        return Err(invalid(
            "Remember at most sixteen servers, with at most eight chosen for each role",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for choice in choices {
        text(&choice.label, 80, true).map_err(invalid)?;
        text(&choice.operator, 160, true).map_err(invalid)?;
        let endpoint = choice
            .endpoint
            .parse::<iroh::EndpointId>()
            .map_err(|_| invalid("Choose a valid pinned server endpoint ID"))?
            .to_string();
        if endpoint != choice.endpoint || !seen.insert(endpoint) {
            return Err(invalid("Remember each canonical server endpoint once"));
        }
    }
    Ok(())
}
