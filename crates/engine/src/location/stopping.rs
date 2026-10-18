use super::*;
use futures::StreamExt;

impl State {
    pub(super) fn stop(&mut self, record: &str, expected: Option<&str>) -> Option<SourceLease> {
        if expected.is_some_and(|expected| {
            self.sessions
                .get(record)
                .is_none_or(|session| session.lease.session_uid != expected)
        }) {
            return None;
        }
        self.invitations
            .retain(|_, lease| lease.invitation.record_uid != record);
        self.sources.remove(record);
        self.sessions.remove(record).map(|session| session.lease)
    }
}

impl Engine {
    pub(crate) async fn location_bind_journey_stop(&self, command: &str, person: &str, transfer: &str) {
        let mut state = self.location.state.lock().await;
        if state.journey_stops.contains_key(command) || state.journey_stops.len() >= 256 {
            return;
        }
        let leases = state.sessions.iter().filter(|(_, session)| {
            session.lease.settings.controller_uid == person
                && session.lease.settings.transfer_uid.as_deref() == Some(transfer)
        }).map(|(record, session)| (record.clone(), session.lease.session_uid.clone())).collect();
        state.journey_stops.insert(command.into(), leases);
    }

    pub async fn complete_journey_location_stop(&self, command: &str, accepted: bool) {
        let leases = {
            let mut state = self.location.state.lock().await;
            state.journey_stops.remove(command).unwrap_or_default().into_iter()
                .filter_map(|(record, epoch)| accepted.then(|| state.stop(&record, Some(&epoch))).flatten())
                .collect::<Vec<_>>()
        };
        futures::stream::iter(leases).for_each_concurrent(4, |lease| async move {
            self.location_end_remote_source(&lease).await;
        }).await;
    }

    pub async fn stop_transfer_location(
        &self,
        person: &str,
        transfer: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if self.location_network().is_err() {
            self.location_identity(person, actor).await?;
            self.location_stop_transfer_sessions(person, transfer).await?;
            return Ok(());
        }
        self.location_request(
            Command::StopTransfer {
                person: person.into(),
                transfer_uid: transfer.into(),
            },
            actor,
        )
        .await
        .map(|_| ())
    }

    pub(super) async fn location_stop_transfer_sessions(
        &self,
        person: &str,
        transfer: &str,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(transfer, "r") {
            return Err(invalid("Choose a valid linked Transfer"));
        }
        Ok(json!({"stopped":self.location_stop_transfer_scope(Some(person),transfer).await}))
    }

    pub(crate) async fn location_stop_ended_transfer(
        &self,
        transfer: &str,
    ) -> Result<(), EngineError> {
        if !store::location::transfer_ended(&self.store.pool, transfer).await? {
            return Ok(());
        }
        self.location_stop_transfer_scope(None, transfer).await;
        Ok(())
    }

    async fn location_stop_transfer_scope(&self, person: Option<&str>, transfer: &str) -> usize {
        let leases = {
            let mut state = self.location.state.lock().await;
            let sessions = state
                .sessions
                .iter()
                .filter(|(_, session)| {
                    person.is_none_or(|person| session.lease.settings.controller_uid == person)
                        && session.lease.settings.transfer_uid.as_deref() == Some(transfer)
                })
                .map(|(record, session)| (record.clone(), session.lease.session_uid.clone()))
                .collect::<Vec<_>>();
            sessions
                .into_iter()
                .filter_map(|(record, epoch)| state.stop(&record, Some(&epoch)))
                .collect::<Vec<_>>()
        };
        let count = leases.len();
        futures::stream::iter(leases)
            .for_each_concurrent(4, |lease| async move {
                self.location_end_remote_source(&lease).await;
            })
            .await;
        count
    }
}
