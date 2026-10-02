use super::*;

pub(super) fn validate_settings(settings: &ServiceSettings) -> Result<(), EngineError> {
    text(&settings.contact, 160, true).map_err(invalid)?;
    text(&settings.policy, 1000, true).map_err(invalid)?;
    if settings.mailbox && (settings.cache_entries < 32 || settings.storage_bytes < 4 * 1024 * 1024)
        || !(1..=100_000).contains(&settings.cache_entries)
        || !(1024 * 1024..=16 * 1024 * 1024 * 1024).contains(&settings.storage_bytes)
        || !(4 * MAX_FRAME_BYTES as u64..=1024 * 1024 * 1024)
            .contains(&settings.incoming_bytes_per_minute)
        || !(4 * MAX_FRAME_BYTES as u64..=1024 * 1024 * 1024)
            .contains(&settings.outgoing_bytes_per_minute)
    {
        return Err(invalid(
            "Choose valid bounded service capacity and traffic limits",
        ));
    }
    Ok(())
}

pub(super) fn describe(
    endpoint: &str,
    settings: &ServiceSettings,
    now: i64,
) -> Result<ServiceDescriptor, EngineError> {
    endpoint
        .parse::<iroh::EndpointId>()
        .map_err(|_| invalid("Invalid service endpoint identity"))?;
    validate_settings(settings)?;
    let roles = [
        ("directory", settings.directory),
        ("townsquare", settings.townsquare),
        ("mailbox", settings.mailbox),
        ("reports", settings.directory || settings.townsquare),
    ]
    .into_iter()
    .filter(|(_, enabled)| *enabled)
    .map(|(role, _)| role.to_owned())
    .collect();
    let mut effective = settings.clone();
    effective.relay = false;
    effective.gossip = false;
    Ok(ServiceDescriptor {
        protocol: "lince.social-service.1".into(),
        endpoint: endpoint.into(),
        roles,
        settings: effective,
        frame_bytes: MAX_PRIVATE_FRAME_BYTES as u32,
        envelope_bytes: nucleus::social::requests::MAX_ATTACHMENT_ENVELOPE_BYTES as u32,
        mailbox_retention_days: 30,
        issued_at: now,
        expires_at: now
            .checked_add(300)
            .ok_or_else(|| invalid("Invalid service time"))?,
    })
}

pub(super) fn validate(
    descriptor: &ServiceDescriptor,
    endpoint: &str,
    now: i64,
) -> Result<(), EngineError> {
    validate_settings(&descriptor.settings)?;
    let expected = describe(endpoint, &descriptor.settings, descriptor.issued_at)?;
    if descriptor.protocol != expected.protocol
        || descriptor.endpoint != endpoint
        || descriptor.roles != expected.roles
        || descriptor.settings.relay
        || descriptor.settings.gossip
        || descriptor.frame_bytes != expected.frame_bytes
        || descriptor.envelope_bytes != expected.envelope_bytes
        || descriptor.mailbox_retention_days != expected.mailbox_retention_days
        || descriptor.issued_at <= 0
        || descriptor.issued_at > now.saturating_add(300)
        || descriptor.expires_at <= now
        || descriptor.expires_at != expected.expires_at
    {
        return Err(invalid(
            "This service returned a stale, incompatible or conflicting descriptor",
        ));
    }
    Ok(())
}

impl Engine {
    pub(super) async fn social_inspect_service(
        &self,
        endpoint: &str,
    ) -> Result<Value, EngineError> {
        let endpoint = endpoint
            .parse::<iroh::EndpointId>()
            .map_err(|_| invalid("Choose a valid service endpoint ID"))?
            .to_string();
        let response = self
            .social_network()?
            .request(&endpoint, PublicRequest::DescribeService)
            .await?;
        if serde_json::to_vec(&response)?.len() > 8 * 1024 {
            return Err(invalid("The service descriptor exceeds its bound"));
        }
        let descriptor: ServiceDescriptor = serde_json::from_value(response["descriptor"].clone())?;
        validate(
            &descriptor,
            &endpoint,
            nucleus::execution::now().timestamp(),
        )?;
        Ok(
            json!({"descriptor":descriptor,"status":"Verified through the selected endpoint. Roles and policy are the operator's current statement; inspection does not select any role for your data"}),
        )
    }
}
