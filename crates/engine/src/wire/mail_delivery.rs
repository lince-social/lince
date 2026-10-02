use super::*;

impl Wire {
    pub(super) async fn try_direct_conversation(
        &self,
        contact: &store::organs::Contact,
        request: &WireRequest,
        live: Option<Connection>,
        connections: &tokio::sync::Mutex<HashMap<String, Option<Connection>>>,
    ) -> Result<Option<WireResponse>, EngineError> {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            if let Some(live) = live {
                if let Ok(response) = self.exchange(&live, request).await {
                    return Ok(Some(response));
                }
            }
            let connection = {
                let mut held = connections.lock().await;
                if let Some(connection) = held.get(&contact.record_uid) {
                    connection.clone()
                } else {
                    let connection = self.dial(contact).await;
                    held.insert(contact.record_uid.clone(), connection.clone());
                    connection
                }
            };
            match connection {
                Some(connection) => self.exchange(&connection, request).await.map(Some),
                None => Ok(None),
            }
        })
        .await
        .map_err(|_| EngineError::Consequence("Direct conversation delivery timed out".into()))?
    }

    pub(super) async fn is_conversation_delivery(&self, root: Option<&str>) -> bool {
        match root {
            Some(root) => store::records::get(&self.engine.store.pool, root)
                .await
                .ok()
                .flatten()
                .is_some_and(|record| record.kind == "conversation"),
            None => false,
        }
    }

    pub async fn retry_saved_mail_once(&self) -> Result<usize, EngineError> {
        let mut accepted = 0;
        for job in store::mailbox::outbox::pending(&self.engine.store.pool, 8).await? {
            match self.deliver_saved_mail(&job).await {
                Ok(MailLeft::Left {
                    copies,
                    requested_copies,
                    ..
                }) if copies >= requested_copies => accepted += 1,
                Ok(_) => {}
                Err(error) => {
                    store::mailbox::outbox::attempted(
                        &self.engine.store.pool,
                        &job.uid,
                        Some(&error.to_string()),
                    )
                    .await?;
                }
            }
        }
        Ok(accepted)
    }

    pub(super) async fn deliver_saved_mail(
        &self,
        queued: &store::mailbox::outbox::Envelope,
    ) -> Result<MailLeft, EngineError> {
        if store::organs::contact(&self.engine.store.pool, &queued.to_organ)
            .await?
            .is_some_and(|contact| {
                contact.trust == "blocked" || contact.delivery() == store::organs::Delivery::Direct
            })
        {
            return Err(EngineError::Forbidden(
                "Saved mail is held because this contact is blocked or now uses direct delivery"
                    .into(),
            ));
        }
        let Some(roster) = self.engine.roster_of(&queued.to_organ).await? else {
            return Ok(MailLeft::NoRoster);
        };
        if !crate::roster::roster_signature_is_valid(&roster) {
            return Err(EngineError::Forbidden(
                "Saved mail is held until the recipient's device authority is refreshed".into(),
            ));
        }
        if roster.roster.pickup.is_empty() {
            return Ok(MailLeft::NoPickupPoints);
        }
        let envelope: crate::seal::SealedBundle = serde_json::from_str(&queued.body)?;
        crate::seal::validate_envelope(&envelope, nucleus::execution::now().timestamp())
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        if envelope.to_organ != queued.to_organ || crate::seal::delivery_id(&envelope) != queued.uid
        {
            return Err(EngineError::Forbidden(
                "The saved mail identity changed".into(),
            ));
        }
        let keys: std::collections::HashSet<&str> = roster
            .roster
            .cells
            .iter()
            .filter(|cell| cell.may(crate::roster::CAP_WRITE))
            .filter_map(|cell| cell.sealing_key.as_ref())
            .filter(|key| {
                chrono::DateTime::parse_from_rfc3339(&key.not_after).is_ok_and(|expiry| {
                    expiry.with_timezone(&chrono::Utc) > nucleus::execution::now()
                })
            })
            .map(|key| key.key_id.as_str())
            .collect();
        if envelope
            .recipients
            .iter()
            .any(|recipient| !keys.contains(recipient.key_id.as_str()))
        {
            return Err(EngineError::Forbidden("Recipient keys or device permissions changed. This older envelope stays held; resend retained content under current authority".into()));
        }
        let mut receipts =
            store::mailbox::outbox::receipts(&self.engine.store.pool, &queued.uid).await?;
        if queued.next_attempt > nucleus::execution::now().timestamp()
            && receipts.len() < queued.requested_copies as usize
        {
            return Ok(if let Some((carrier, _)) = receipts.first() {
                MailLeft::Left {
                    carrier: carrier.clone(),
                    uid: queued.uid.clone(),
                    copies: receipts.len(),
                    requested_copies: queued.requested_copies as usize,
                }
            } else {
                MailLeft::NoneAccepted {
                    refusals: vec![(queued.to_organ.clone(), "retry_backoff".into())],
                }
            });
        }
        let mut refusals = Vec::new();
        for point in roster.roster.pickup.iter().take(8) {
            if receipts.len() >= queued.requested_copies as usize {
                break;
            }
            if receipts.iter().any(|(_, node)| node == &point.node_id) {
                continue;
            }
            let Ok(id) = point.node_id.parse::<EndpointId>() else {
                refusals.push((point.organ_uid.clone(), "unusable_node_id".into()));
                continue;
            };
            match self
                .deposit_bundle(EndpointAddr::new(id), &queued.body)
                .await
            {
                Ok(Ok(uid)) if uid == queued.uid => {
                    store::mailbox::outbox::accepted(
                        &self.engine.store.pool,
                        &uid,
                        &point.organ_uid,
                        &point.node_id,
                    )
                    .await?;
                    receipts.push((point.organ_uid.clone(), point.node_id.clone()));
                }
                Ok(Ok(_)) => refusals.push((point.organ_uid.clone(), "invalid_receipt".into())),
                Ok(Err(code)) => refusals.push((point.organ_uid.clone(), code)),
                Err(_) => refusals.push((point.organ_uid.clone(), "unreachable".into())),
            }
        }
        let error = (receipts.len() < queued.requested_copies as usize).then(|| {
            format!(
                "{} of {} servers accepted this envelope; {} refusal(s)",
                receipts.len(),
                queued.requested_copies,
                refusals.len()
            )
        });
        store::mailbox::outbox::attempted(&self.engine.store.pool, &queued.uid, error.as_deref())
            .await?;
        Ok(if let Some((carrier, _)) = receipts.first() {
            MailLeft::Left {
                carrier: carrier.clone(),
                uid: queued.uid.clone(),
                copies: receipts.len(),
                requested_copies: queued.requested_copies as usize,
            }
        } else {
            MailLeft::NoneAccepted { refusals }
        })
    }
}
