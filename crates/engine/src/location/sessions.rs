use super::*;

impl Engine {
    pub(super) async fn location_validate_configuration(
        &self,
        settings: &Settings,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        settings.validate().map_err(invalid)?;
        self.location_manage(&settings.controller_uid, &settings.record_uid, actor)
            .await?;
        self.require_permission(Some(&settings.controller_uid), "record:update")
            .await?;
        self.refuse_unreadable(
            Some(&settings.controller_uid),
            std::slice::from_ref(&settings.record_uid),
        )
        .await?;
        if self.location_authority(&settings.record_uid).await? != self.location_node_id()? {
            return Err(denied("Location authority changed"));
        }
        let (_, devices) = self.location_devices().await?;
        if !devices.iter().any(|device| {
            device.uid == settings.source_cell_uid
                && device.node_id.as_deref() == Some(settings.source_node_id.as_str())
        }) {
            return Err(denied("Choose an authorized device in this Organ"));
        }
        let mut connection = self.store.pool.acquire().await?;
        let revoked = store::session_access::device_on(
            &mut connection,
            &settings.controller_uid,
            &settings.source_node_id,
        )
        .await?
        .is_some_and(|device| device.revoked);
        drop(connection);
        if revoked {
            return Err(denied("The selected Person's source device is revoked"));
        }
        for recipient in &settings.recipients {
            self.location_identity(recipient, None).await?;
            self.refuse_unreadable(actor, std::slice::from_ref(recipient))
                .await?;
        }
        if let Some(transfer) = &settings.transfer_uid {
            self.refuse_unreadable(actor, std::slice::from_ref(transfer))
                .await?;
            if store::transfers::get(&self.store.pool, transfer)
                .await?
                .is_none()
            {
                return Err(invalid("Choose a Transfer for the end condition"));
            }
        }
        Ok(())
    }

    pub(super) async fn location_begin(
        &self,
        settings: Settings,
        replace: Option<&str>,
    ) -> Result<Value, EngineError> {
        if let Some(transfer) = &settings.transfer_uid {
            if store::location::transfer_ended(&self.store.pool, transfer).await? {
                return Err(invalid("The linked Transfer has ended"));
            }
        }
        let node = self.location_node_id()?;
        let record = settings.record_uid.clone();
        let person = settings.controller_uid.clone();
        let lease = SourceLease {
            authority_node_id: node.clone(),
            session_uid: nucleus::new_uid("location"),
            expires_at_ms: now_ms() + i64::from(settings.duration_seconds) * 1000,
            approved: false,
            challenge_uid: nucleus::new_uid("lc"),
            lease_millis: 30_000,
            settings,
        };
        let mut tx = if replace.is_some() {
            Some(self.store.pool.begin().await?)
        } else {
            None
        };
        let previous = {
            let mut state = self.location.state.lock().await;
            let previous = match (replace, state.sessions.get(&record)) {
                (None, None) => None,
                (Some(expected), Some(session))
                    if session.lease.session_uid == expected
                        && session.lease.settings.controller_uid == person =>
                {
                    Some(session.lease.clone())
                }
                _ => {
                    return Err(denied(
                        "Location session changed; review the current source before continuing",
                    ));
                }
            };
            if previous.is_none() && state.sessions.len() >= MAX_SESSIONS {
                return Err(invalid("Too many active location sessions"));
            }
            if lease.settings.source_node_id == node
                && !state.sources.contains_key(&record)
                && state.sources.len() >= MAX_SESSIONS
            {
                return Err(invalid("Too many source requests"));
            }
            if let Some(mut tx) = tx.take() {
                store::location::save_on(&mut tx, &lease.settings).await?;
                tx.commit().await?;
            }
            state.sources.remove(&record);
            state.sessions.insert(
                record.clone(),
                Session {
                    deadline: Instant::now()
                        + Duration::from_secs(u64::from(lease.settings.duration_seconds)),
                    lease: lease.clone(),
                    latest: None,
                    sequence: 0,
                    unavailable: false,
                    approved_at: None,
                    challenges: VecDeque::new(),
                },
            );
            if lease.settings.source_node_id == node {
                state.sources.insert(
                    record.clone(),
                    Source {
                        lease: lease.clone(),
                        renewed: Instant::now(),
                    },
                );
            }
            previous
        };
        if let Some(previous) = previous {
            self.location_end_remote_source(&previous).await;
        }
        if !self
            .location_session_matches(&record, &lease.session_uid)
            .await
        {
            return Err(denied("Location session ended while handing over"));
        }
        if lease.settings.source_node_id != node {
            let requested = match self.location_network() {
                Ok(network) => tokio::time::timeout(
                    Duration::from_secs(5),
                    network.location_request(
                        &lease.settings.source_node_id,
                        PeerRequest::RequestSource {
                            lease: lease.clone(),
                        },
                    ),
                )
                .await
                .map_err(|_| invalid("The replacement source did not answer"))
                .and_then(|result| result),
                Err(error) => Err(error),
            };
            if let Err(error) = requested {
                self.location_stop_record_if(&record, Some(&lease.session_uid))
                    .await;
                self.location_end_remote_source(&lease).await;
                return Err(error);
            }
        }
        if !self
            .location_session_matches(&record, &lease.session_uid)
            .await
        {
            self.location_end_remote_source(&lease).await;
            return Err(denied(
                "Location session ended before the replacement was ready",
            ));
        }
        let view = self.location_view(&person, &record).await?;
        if view.session_uid.as_deref() != Some(lease.session_uid.as_str()) {
            return Err(denied("Location session changed before confirmation"));
        }
        Ok(serde_json::to_value(view)?)
    }

    async fn location_session_matches(&self, record: &str, session: &str) -> bool {
        self.location
            .state
            .lock()
            .await
            .sessions
            .get(record)
            .is_some_and(|active| active.lease.session_uid == session)
    }

    pub(super) async fn location_end_remote_source(&self, lease: &SourceLease) {
        if self
            .location_node_id()
            .is_ok_and(|node| node != lease.settings.source_node_id)
        {
            if let Ok(network) = self.location_network() {
                let _ = tokio::time::timeout(
                    Duration::from_secs(5),
                    network.location_request(
                        &lease.settings.source_node_id,
                        PeerRequest::StopSource {
                            record_uid: lease.settings.record_uid.clone(),
                            session_uid: lease.session_uid.clone(),
                        },
                    ),
                )
                .await;
            }
        }
    }
}
