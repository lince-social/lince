use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::location::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

#[async_trait::async_trait]
pub trait Network: Send + Sync {
    fn node_id(&self) -> String;
    async fn location_request(
        &self,
        node: &str,
        request: PeerRequest,
    ) -> Result<Value, EngineError>;
}

#[derive(Default)]
pub(crate) struct Runtime {
    network: Mutex<Option<Weak<dyn Network>>>,
    state: tokio::sync::Mutex<State>,
    mutation: tokio::sync::Mutex<()>,
    observers: Mutex<HashMap<(String, String), String>>,
}

#[derive(Default)]
struct State {
    sessions: HashMap<String, Session>,
    sources: HashMap<String, Source>,
}

struct Session {
    lease: SourceLease,
    deadline: Instant,
    latest: Option<Observation>,
    sequence: u64,
    unavailable: bool,
    approved_at_ms: Option<i64>,
}

struct Source {
    lease: SourceLease,
    renewed: Instant,
}

struct Observation {
    fix: Fix,
    received: Instant,
    initial_age_ms: u64,
}

impl Observation {
    fn age_ms(&self) -> u64 {
        self.initial_age_ms
            .saturating_add(self.received.elapsed().as_millis() as u64)
    }
}

fn denied(message: &str) -> EngineError {
    EngineError::Forbidden(message.into())
}
fn invalid(message: &str) -> EngineError {
    EngineError::Consequence(message.into())
}
fn now_ms() -> i64 {
    nucleus::execution::now().timestamp_millis()
}

impl Engine {
    pub fn attach_location_network(&self, network: Arc<dyn Network>) {
        *self.location.network.lock().expect("location network") = Some(Arc::downgrade(&network));
    }

    pub(crate) fn location_network(&self) -> Result<Arc<dyn Network>, EngineError> {
        self.location
            .network
            .lock()
            .expect("location network")
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| invalid("The device connection is unavailable"))
    }

    pub fn location_node_id(&self) -> Result<String, EngineError> {
        Ok(self.location_network()?.node_id())
    }

    pub fn bind_location_observer(
        &self,
        node: &str,
        local_person: &str,
        remote_person: &str,
    ) -> Result<(), EngineError> {
        let mut observers = self.location.observers.lock().expect("location observers");
        let key = (node.into(), local_person.into());
        if observers.len() >= 256 && !observers.contains_key(&key) {
            return Err(invalid("Too many location viewing identities"));
        }
        observers.insert(key, remote_person.into());
        Ok(())
    }

    async fn location_identity(
        &self,
        person: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if actor.is_some_and(|actor| actor != person) {
            return Err(denied("Use your signed-in Person for location sharing"));
        }
        if !store::people::is_active(&self.store.pool, person).await? {
            return Err(denied("Choose an active Person for location sharing"));
        }
        Ok(())
    }

    async fn location_manage(
        &self,
        person: &str,
        record: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.location_identity(person, actor).await?;
        let mut tx = self.store.pool.begin().await?;
        self.require_permission_on(&mut tx, actor, "record:update")
            .await?;
        tx.commit().await?;
        self.refuse_unreadable(actor, &[record.into()]).await?;
        Ok(())
    }

    async fn location_devices(&self) -> Result<(String, Vec<Choice>), EngineError> {
        let local = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("This device has no Cell identity"))?;
        let node = self.location_node_id()?;
        let mut devices = vec![Choice {
            uid: local.uid,
            label: format!("{} (this device)", local.label),
            node_id: Some(node.clone()),
        }];
        if let Some(roster) = self.roster_of(&local.organ_uid).await? {
            if !crate::roster::roster_signature_is_valid(&roster)
                || chrono::DateTime::parse_from_rfc3339(&roster.roster.not_after)
                    .map_or(true, |at| at.timestamp_millis() <= now_ms())
            {
                return Err(denied("Refresh this Organ's device authorization"));
            }
            devices = roster
                .roster
                .cells
                .into_iter()
                .filter(|cell| cell.may(crate::roster::CAP_WRITE))
                .map(|cell| Choice {
                    uid: cell.cell_uid,
                    label: if cell.node_id == node {
                        format!("{} (this device)", cell.label)
                    } else {
                        cell.label
                    },
                    node_id: Some(cell.node_id),
                })
                .collect();
        }
        Ok((node, devices))
    }

    pub async fn location_authority(&self, record: &str) -> Result<String, EngineError> {
        let row = store::records::get(&self.store.pool, record)
            .await?
            .ok_or_else(|| denied("Location Record is unavailable"))?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Local Organ is unavailable"))?;
        if row.organ_uid.as_deref().is_some_and(|uid| uid != organ.uid) {
            return Err(denied("Choose a Record owned by this Organ"));
        }
        let (_, devices) = self.location_devices().await?;
        devices
            .into_iter()
            .filter_map(|device| device.node_id)
            .min()
            .ok_or_else(|| invalid("No authorized location authority"))
    }

    async fn location_controller(
        &self,
        person: &str,
        record: &str,
    ) -> Result<Settings, EngineError> {
        let settings = store::location::settings(&self.store.pool, record)
            .await?
            .ok_or_else(|| invalid("Configure this Record's location first"))?;
        if settings.controller_uid != person {
            return Err(denied("Only the starting Person controls this location"));
        }
        Ok(settings)
    }

    pub async fn location_request(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        let bootstrap = matches!(&command, Command::Context { person, .. } if person.is_empty() && actor.is_none());
        let admitted_observer = match &command {
            Command::Observe {
                person, node_id, ..
            } if actor.is_none() => self
                .location
                .observers
                .lock()
                .expect("location observers")
                .contains_key(&(node_id.clone(), person.clone())),
            _ => false,
        };
        let organ_observer = matches!(&command, Command::Observe { person, .. } if person.is_empty() && actor.is_none());
        if !bootstrap && !admitted_observer && !organ_observer {
            self.location_identity(command.person(), actor).await?;
        }
        match &command {
            Command::Stop { person, record_uid } => {
                self.location
                    .state
                    .lock()
                    .await
                    .sources
                    .retain(|record, source| {
                        record != record_uid || source.lease.settings.controller_uid != *person
                    });
            }
            Command::StopAll { person } => {
                self.location
                    .state
                    .lock()
                    .await
                    .sources
                    .retain(|_, source| source.lease.settings.controller_uid != *person);
            }
            _ => {}
        }
        let record = match &command {
            Command::Configure { settings } => Some(settings.record_uid.as_str()),
            Command::Context { record_uid, .. }
            | Command::Start { record_uid, .. }
            | Command::View { record_uid, .. }
            | Command::Stop { record_uid, .. } => Some(record_uid.as_str()),
            _ => None,
        };
        let data = if bootstrap {
            self.location_local_command(command, actor).await?
        } else if matches!(command, Command::StopAll { .. }) {
            let (node, devices) = self.location_devices().await?;
            let authority = devices
                .into_iter()
                .filter_map(|device| device.node_id)
                .min()
                .ok_or_else(|| invalid("No location authority is available"))?;
            if authority == node {
                self.location_local_command(command, actor).await?
            } else {
                self.location_network()?
                    .location_request(&authority, PeerRequest::Control { command })
                    .await?
            }
        } else if let Command::Observe {
            person,
            record_uid,
            node_id,
        } = command
        {
            if let Ok(view) = self
                .location_network()?
                .location_request(
                    &node_id,
                    PeerRequest::Read {
                        record_uid: record_uid.clone(),
                        data: nucleus::visibility::Data::LiveLocation,
                    },
                )
                .await
            {
                return Ok(ActionOutcome {
                    data: Some(view),
                    ..Default::default()
                });
            }
            if person.is_empty() {
                return Err(denied(
                    "This Organ has no location access. Authenticate a named recipient to continue",
                ));
            }
            let person = self
                .location
                .observers
                .lock()
                .expect("location observers")
                .get(&(node_id.clone(), person.clone()))
                .cloned()
                .unwrap_or(person);
            self.location_network()?
                .location_request(
                    &node_id,
                    PeerRequest::Control {
                        command: Command::View { person, record_uid },
                    },
                )
                .await?
        } else if let Some(record) = record {
            let authority = self.location_authority(record).await?;
            if authority != self.location_node_id()? {
                let context_request = matches!(command, Command::Context { .. });
                let mut data = self
                    .location_network()?
                    .location_request(&authority, PeerRequest::Control { command })
                    .await?;
                if context_request {
                    let (node, devices) = self.location_devices().await?;
                    let cell = store::cells::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("Device identity unavailable"))?;
                    data["current_node_id"] = json!(node);
                    data["current_cell_uid"] = json!(cell.uid);
                    data["devices"] = serde_json::to_value(devices)?;
                }
                data
            } else {
                self.location_local_command(command, actor).await?
            }
        } else {
            self.location_local_command(command, actor).await?
        };
        Ok(ActionOutcome {
            data: Some(data),
            ..Default::default()
        })
    }

    async fn location_local_command(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.location_expire().await?;
        match command {
            Command::Context { person, record_uid } => {
                self.refuse_unreadable(actor, std::slice::from_ref(&record_uid))
                    .await?;
                let (current_node_id, devices) = self.location_devices().await?;
                let current_cell_uid = store::cells::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| invalid("Device identity unavailable"))?
                    .uid;
                let settings = store::location::settings(&self.store.pool, &record_uid)
                    .await?
                    .filter(|settings| settings.controller_uid == person);
                let mut people = Vec::new();
                for choice in store::location::people(&self.store.pool).await? {
                    if self.may_read_record(actor, &choice.uid).await? {
                        people.push(choice);
                    }
                }
                let view = if person.is_empty() {
                    stopped(&record_uid)
                } else {
                    self.location_view(&person, &record_uid).await?
                };
                let authority_node_id = self.location_authority(&record_uid).await?;
                Ok(serde_json::to_value(Context {
                    authority_node_id,
                    settings,
                    people,
                    devices,
                    current_cell_uid,
                    current_node_id,
                    view,
                })?)
            }
            Command::Configure { settings } => {
                let _mutation = self.location.mutation.lock().await;
                settings.validate().map_err(invalid)?;
                self.location_manage(&settings.controller_uid, &settings.record_uid, actor)
                    .await?;
                if self.location_authority(&settings.record_uid).await?
                    != self.location_node_id()?
                {
                    return Err(denied("Location authority changed"));
                }
                let (_, devices) = self.location_devices().await?;
                if !devices.iter().any(|device| {
                    device.uid == settings.source_cell_uid
                        && device.node_id.as_deref() == Some(settings.source_node_id.as_str())
                }) {
                    return Err(denied("Choose an authorized device in this Organ"));
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
                let mut tx = self.store.pool.begin().await?;
                store::location::save_on(&mut tx, &settings).await?;
                tx.commit().await?;
                self.location_stop_record(&settings.record_uid).await;
                Ok(json!({"configured":true,"view":stopped(&settings.record_uid)}))
            }
            Command::Start { person, record_uid } => {
                let _mutation = self.location.mutation.lock().await;
                if self.location_authority(&record_uid).await? != self.location_node_id()? {
                    return Err(denied("Location authority changed"));
                }
                self.location_manage(&person, &record_uid, actor).await?;
                let settings = self.location_controller(&person, &record_uid).await?;
                if let Some(transfer) = &settings.transfer_uid {
                    if store::location::transfer_ended(&self.store.pool, transfer).await? {
                        return Err(invalid("The linked Transfer has ended"));
                    }
                }
                let node = self.location_node_id()?;
                let lease = SourceLease {
                    authority_node_id: node.clone(),
                    session_uid: nucleus::new_uid("location"),
                    expires_at_ms: now_ms() + i64::from(settings.duration_seconds) * 1000,
                    approved: false,
                    settings,
                };
                {
                    let mut state = self.location.state.lock().await;
                    if state.sessions.contains_key(&record_uid) {
                        return Err(invalid(
                            "Stop the existing location session before starting another",
                        ));
                    }
                    if state.sessions.len() >= MAX_SESSIONS {
                        return Err(invalid("Too many active location sessions"));
                    }
                    state.sessions.insert(
                        record_uid.clone(),
                        Session {
                            deadline: Instant::now()
                                + Duration::from_secs(u64::from(lease.settings.duration_seconds)),
                            lease: lease.clone(),
                            latest: None,
                            sequence: 0,
                            unavailable: false,
                            approved_at_ms: None,
                        },
                    );
                }
                if lease.settings.source_node_id == node {
                    self.location.state.lock().await.sources.insert(
                        record_uid.clone(),
                        Source {
                            lease,
                            renewed: Instant::now(),
                        },
                    );
                } else if let Err(error) = self
                    .location_network()?
                    .location_request(
                        &lease.settings.source_node_id,
                        PeerRequest::RequestSource {
                            lease: lease.clone(),
                        },
                    )
                    .await
                {
                    self.location_stop_record(&record_uid).await;
                    return Err(error);
                }
                Ok(serde_json::to_value(
                    self.location_view(&person, &record_uid).await?,
                )?)
            }
            Command::Approve { person, record_uid } => {
                self.location_identity(&person, actor).await?;
                let lease = self
                    .location
                    .state
                    .lock()
                    .await
                    .sources
                    .get(&record_uid)
                    .map(|source| source.lease.clone())
                    .ok_or_else(|| invalid("No location request is waiting on this device"))?;
                if lease.settings.controller_uid != person {
                    return Err(denied("Only the starting Person can approve this device"));
                }
                let data = if lease.authority_node_id == self.location_node_id()? {
                    self.location_approve(
                        &record_uid,
                        &lease.session_uid,
                        &person,
                        &lease.settings.source_node_id,
                    )
                    .await?
                } else {
                    self.location_network()?
                        .location_request(
                            &lease.authority_node_id,
                            PeerRequest::ApproveSource {
                                person,
                                record_uid: record_uid.clone(),
                                session_uid: lease.session_uid.clone(),
                            },
                        )
                        .await?
                };
                let next: SourceLease = serde_json::from_value(data.clone())?;
                if let Some(source) = self
                    .location
                    .state
                    .lock()
                    .await
                    .sources
                    .get_mut(&record_uid)
                {
                    if source.lease.session_uid == next.session_uid {
                        source.lease = next;
                        source.renewed = Instant::now();
                    }
                }
                Ok(data)
            }
            Command::View { person, record_uid } => Ok(serde_json::to_value(
                self.location_view(&person, &record_uid).await?,
            )?),
            Command::Stop { person, record_uid } => {
                self.location_controller(&person, &record_uid).await?;
                self.location_stop_record(&record_uid).await;
                Ok(serde_json::to_value(stopped(&record_uid))?)
            }
            Command::StopAll { person } => {
                let sources: Vec<_> = self
                    .location
                    .state
                    .lock()
                    .await
                    .sources
                    .values()
                    .filter(|source| source.lease.settings.controller_uid == person)
                    .map(|source| source.lease.clone())
                    .collect();
                for lease in sources {
                    self.location
                        .state
                        .lock()
                        .await
                        .sources
                        .remove(&lease.settings.record_uid);
                    if lease.authority_node_id == self.location_node_id()? {
                        self.location_stop_record(&lease.settings.record_uid).await;
                    } else {
                        let _ = self
                            .location_network()?
                            .location_request(
                                &lease.authority_node_id,
                                PeerRequest::Control {
                                    command: Command::Stop {
                                        person: person.clone(),
                                        record_uid: lease.settings.record_uid,
                                    },
                                },
                            )
                            .await;
                    }
                }
                let records: Vec<_> = self
                    .location
                    .state
                    .lock()
                    .await
                    .sessions
                    .iter()
                    .filter(|(_, session)| session.lease.settings.controller_uid == person)
                    .map(|(record, _)| record.clone())
                    .collect();
                for record in records {
                    self.location_stop_record(&record).await;
                }
                for settings in store::location::retained_for(&self.store.pool, &person).await? {
                    let authority = self.location_authority(&settings.record_uid).await?;
                    if authority != self.location_node_id()? {
                        let _ = self
                            .location_network()?
                            .location_request(
                                &authority,
                                PeerRequest::Control {
                                    command: Command::Stop {
                                        person: person.clone(),
                                        record_uid: settings.record_uid,
                                    },
                                },
                            )
                            .await;
                    }
                }
                Ok(json!({"stopped":true}))
            }
            Command::Publish {
                person,
                record_uid,
                session_uid,
                fix,
            } => {
                let source = self
                    .location
                    .state
                    .lock()
                    .await
                    .sources
                    .get(&record_uid)
                    .map(|source| source.lease.clone())
                    .ok_or_else(|| denied("Approve location on this source device first"))?;
                if source.settings.controller_uid != person
                    || source.settings.source_kind != SourceKind::Manual
                    || source.session_uid != session_uid
                {
                    return Err(denied(
                        "Manual location requires the selected manual source session",
                    ));
                }
                self.location_publish_source(&source, fix).await?;
                Ok(json!({"published":true}))
            }
            Command::Observe { .. } => Err(invalid("Use the selected observer endpoint")),
        }
    }

    async fn location_approve(
        &self,
        record: &str,
        session: &str,
        person: &str,
        peer: &str,
    ) -> Result<Value, EngineError> {
        let mut state = self.location.state.lock().await;
        let active = state
            .sessions
            .get_mut(record)
            .ok_or_else(|| invalid("The location request has ended"))?;
        if active.lease.session_uid != session
            || active.lease.settings.controller_uid != person
            || active.lease.settings.source_node_id != peer
        {
            return Err(denied("This device cannot approve that location source"));
        }
        active.lease.approved = true;
        active.approved_at_ms = Some(now_ms());
        Ok(serde_json::to_value(&active.lease)?)
    }

    async fn location_view(&self, person: &str, record: &str) -> Result<View, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .unwrap_or_default();
        self.location_person_view(person, record, &organ).await
    }

    async fn location_person_view(
        &self,
        person: &str,
        record: &str,
        organ: &str,
    ) -> Result<View, EngineError> {
        self.location_identity(person, Some(person)).await?;
        let selected = self
            .location
            .state
            .lock()
            .await
            .sessions
            .get(record)
            .map(|session| session.lease.clone());
        let Some(lease) = selected else {
            return Ok(stopped(record));
        };
        let controller = lease.settings.controller_uid == person;
        let decision = self
            .organ_visibility(record, nucleus::visibility::Data::LiveLocation, organ)
            .await?;
        let blocked = store::organs::contact(&self.store.pool, organ)
            .await?
            .is_some_and(|contact| contact.trust == "blocked");
        let allowed = controller
            || (!blocked
                && decision.excluded_by.is_empty()
                && (decision.allowed
                    || lease
                        .settings
                        .recipients
                        .iter()
                        .any(|recipient| recipient == person)));
        self.location_permitted_view(record, &lease.session_uid, allowed, controller)
            .await
    }

    pub(crate) async fn location_organ_view(
        &self,
        organ: &str,
        record: &str,
    ) -> Result<Value, EngineError> {
        let decision = self
            .organ_visibility(record, nucleus::visibility::Data::LiveLocation, organ)
            .await?;
        let session = self
            .location
            .state
            .lock()
            .await
            .sessions
            .get(record)
            .map(|session| session.lease.session_uid.clone());
        let view = if let Some(session) = session {
            self.location_permitted_view(record, &session, decision.allowed, false)
                .await?
        } else {
            stopped(record)
        };
        Ok(serde_json::to_value(view)?)
    }

    async fn location_permitted_view(
        &self,
        record: &str,
        session_uid: &str,
        allowed: bool,
        controller: bool,
    ) -> Result<View, EngineError> {
        let state = self.location.state.lock().await;
        let Some(session) = state.sessions.get(record) else {
            return Ok(stopped(record));
        };
        if !allowed || session.lease.session_uid != session_uid {
            return Ok(stopped(record));
        }
        let age = session.latest.as_ref().map(Observation::age_ms);
        let fresh = age.is_some_and(|age| age < MAX_FIX_AGE_MS as u64);
        Ok(View {
            record_uid: record.into(),
            status: if !session.lease.approved {
                Status::AwaitingApproval
            } else if session.unavailable {
                Status::Unavailable
            } else if !fresh {
                Status::Acquiring
            } else if age.is_some_and(|age| age > 15_000) {
                Status::Stale
            } else {
                Status::Live
            },
            session_uid: controller.then(|| session.lease.session_uid.clone()),
            expires_at_ms: Some(session.lease.expires_at_ms),
            source_kind: Some(session.lease.settings.source_kind),
            fix: session
                .latest
                .as_ref()
                .filter(|_| fresh)
                .map(|observation| observation.fix.clone()),
            age_ms: age.filter(|_| fresh),
        })
    }

    async fn location_accept_fix(
        &self,
        peer: &str,
        record: &str,
        session_uid: &str,
        fix: Fix,
    ) -> Result<Value, EngineError> {
        let timestamp = now_ms();
        fix.validate(timestamp).map_err(invalid)?;
        let mut state = self.location.state.lock().await;
        let session = state
            .sessions
            .get_mut(record)
            .ok_or_else(|| denied("Location session has ended"))?;
        if !session.lease.approved
            || session.lease.session_uid != session_uid
            || session.lease.settings.source_node_id != peer
            || session.deadline <= Instant::now()
            || fix.sequence <= session.sequence
            || session
                .approved_at_ms
                .is_none_or(|approved| fix.captured_at_ms < approved)
        {
            return Err(denied(
                "Expired, replayed, or unauthorized location observation",
            ));
        }
        if session
            .latest
            .as_ref()
            .is_some_and(|old| fix.captured_at_ms < old.fix.captured_at_ms)
        {
            return Err(invalid("Location capture time moved backwards"));
        }
        session.sequence = fix.sequence;
        session.latest = Some(Observation {
            initial_age_ms: timestamp.saturating_sub(fix.captured_at_ms) as u64,
            fix,
            received: Instant::now(),
        });
        session.unavailable = false;
        Ok(serde_json::to_value(&session.lease)?)
    }

    pub async fn location_peer(
        &self,
        organ: &str,
        peer: &str,
        request: PeerRequest,
    ) -> Result<Value, EngineError> {
        let write = matches!(&request, PeerRequest::Visibility { .. })
            || matches!(&request, PeerRequest::Control { command } if !matches!(command, Command::View { .. }));
        self.access_scope(write, self.location_peer_inner(organ, peer, request))
            .await
    }

    async fn location_peer_inner(
        &self,
        organ: &str,
        peer: &str,
        request: PeerRequest,
    ) -> Result<Value, EngineError> {
        if serde_json::to_vec(&request)?.len() > 16 * 1024 {
            return Err(invalid("Location request exceeds its limit"));
        }
        self.location_expire().await?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Local Organ unavailable"))?;
        match request {
            PeerRequest::Visibility { command, person } => {
                if organ != local.uid {
                    return Err(denied(
                        "Visibility control requires this Organ's authorized device",
                    ));
                }
                let (_, devices) = self.location_devices().await?;
                if !devices
                    .iter()
                    .any(|device| device.node_id.as_deref() == Some(peer))
                {
                    return Err(denied("Refresh this Organ's device authorization"));
                }
                if let Some(person) = &person {
                    let mut connection = self.store.pool.acquire().await?;
                    let device =
                        store::session_access::device_on(&mut connection, person, peer).await?;
                    if !device.is_some_and(|device| !device.revoked) {
                        return Err(denied(
                            "Authenticate this Person on the controlling device first",
                        ));
                    }
                }
                let data = Box::pin(self.visibility_request(command, person.as_deref()))
                    .await?
                    .data;
                Ok(data.unwrap_or_default())
            }
            PeerRequest::Read { record_uid, data } => {
                self.visibility_read(organ, &record_uid, data).await
            }
            PeerRequest::Control { command } => {
                if !matches!(
                    command,
                    Command::Context { .. }
                        | Command::Configure { .. }
                        | Command::Start { .. }
                        | Command::Stop { .. }
                        | Command::StopAll { .. }
                        | Command::View { .. }
                ) {
                    return Err(denied(
                        "Source approval and acquisition require a local device action",
                    ));
                }
                let person = command.person().to_string();
                let mut connection = self.store.pool.acquire().await?;
                let device =
                    store::session_access::device_on(&mut connection, &person, peer).await?;
                if !device.is_some_and(|device| !device.revoked) {
                    return Err(denied(
                        "Authenticate this Person on the viewing device first",
                    ));
                }
                drop(connection);
                self.location_identity(&person, Some(&person)).await?;
                if let Command::View { record_uid, .. } = command {
                    return Ok(serde_json::to_value(
                        self.location_person_view(&person, &record_uid, organ)
                            .await?,
                    )?);
                }
                if !matches!(command, Command::View { .. }) && organ != local.uid {
                    return Err(denied("Location control requires a device in this Organ"));
                }
                self.location_local_command(command, Some(&person)).await
            }
            PeerRequest::RequestSource { lease } => {
                lease.settings.validate().map_err(invalid)?;
                let cell = store::cells::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| invalid("Local Cell unavailable"))?;
                if organ != local.uid
                    || lease.authority_node_id != peer
                    || self.location_authority(&lease.settings.record_uid).await? != peer
                    || lease.settings.source_cell_uid != cell.uid
                    || lease.settings.source_node_id != self.location_node_id()?
                    || lease.approved
                    || lease.expires_at_ms <= now_ms()
                    || lease.expires_at_ms > now_ms() + i64::from(MAX_DURATION_SECONDS) * 1000
                {
                    return Err(denied("Invalid location source request"));
                }
                self.location_identity(&lease.settings.controller_uid, None)
                    .await?;
                let mut state = self.location.state.lock().await;
                if state.sources.len() >= MAX_SESSIONS {
                    return Err(invalid("Too many source requests"));
                }
                if state.sources.contains_key(&lease.settings.record_uid) {
                    return Err(invalid("This Record already has a source request"));
                }
                state.sources.insert(
                    lease.settings.record_uid.clone(),
                    Source {
                        lease,
                        renewed: Instant::now(),
                    },
                );
                Ok(json!({"awaiting_local_approval":true}))
            }
            PeerRequest::ApproveSource {
                person,
                record_uid,
                session_uid,
            } => {
                if organ != local.uid {
                    return Err(denied("Source belongs to another Organ"));
                }
                let mut connection = self.store.pool.acquire().await?;
                if !store::session_access::device_on(&mut connection, &person, peer)
                    .await?
                    .is_some_and(|device| !device.revoked)
                {
                    return Err(denied(
                        "Authenticate the starting Person on the source device first",
                    ));
                }
                drop(connection);
                self.location_identity(&person, Some(&person)).await?;
                self.location_approve(&record_uid, &session_uid, &person, peer)
                    .await
            }
            PeerRequest::SourceLease {
                record_uid,
                session_uid,
            } => {
                if organ != local.uid {
                    return Err(denied("Source belongs to another Organ"));
                }
                let state = self.location.state.lock().await;
                let lease = state
                    .sessions
                    .get(&record_uid)
                    .filter(|session| {
                        session.lease.session_uid == session_uid
                            && session.lease.settings.source_node_id == peer
                    })
                    .map(|session| &session.lease);
                Ok(serde_json::to_value(lease)?)
            }
            PeerRequest::StopSource {
                record_uid,
                session_uid,
            } => {
                let mut state = self.location.state.lock().await;
                if state.sources.get(&record_uid).is_some_and(|source| {
                    source.lease.session_uid == session_uid
                        && source.lease.authority_node_id == peer
                        && organ == local.uid
                }) {
                    state.sources.remove(&record_uid);
                }
                Ok(json!({"stopped":true}))
            }
            PeerRequest::Publish {
                record_uid,
                session_uid,
                fix,
            } => {
                if organ != local.uid {
                    return Err(denied("Source belongs to another Organ"));
                }
                self.location_accept_fix(peer, &record_uid, &session_uid, fix)
                    .await
            }
            PeerRequest::Unavailable {
                record_uid,
                session_uid,
            } => {
                let mut state = self.location.state.lock().await;
                if let Some(session) = state.sessions.get_mut(&record_uid) {
                    if session.lease.session_uid == session_uid
                        && session.lease.settings.source_node_id == peer
                        && organ == local.uid
                    {
                        session.latest = None;
                        session.unavailable = true;
                    }
                }
                Ok(json!({"unavailable":true}))
            }
        }
    }

    async fn location_stop_record(&self, record: &str) {
        let lease = {
            let mut state = self.location.state.lock().await;
            state.sources.remove(record);
            state.sessions.remove(record).map(|session| session.lease)
        };
        if let Some(lease) = lease {
            if self
                .location_node_id()
                .is_ok_and(|node| node != lease.settings.source_node_id)
            {
                if let Ok(network) = self.location_network() {
                    let _ = network
                        .location_request(
                            &lease.settings.source_node_id,
                            PeerRequest::StopSource {
                                record_uid: record.into(),
                                session_uid: lease.session_uid,
                            },
                        )
                        .await;
                }
            }
        }
    }

    async fn location_expire(&self) -> Result<(), EngineError> {
        let sessions: Vec<_> = self
            .location
            .state
            .lock()
            .await
            .sessions
            .iter()
            .map(|(record, session)| {
                (
                    record.clone(),
                    session.deadline,
                    session.lease.settings.clone(),
                    session.lease.approved,
                )
            })
            .collect();
        for (record, deadline, settings, approved) in sessions {
            let live = deadline > Instant::now()
                && now_ms()
                    < self
                        .location
                        .state
                        .lock()
                        .await
                        .sessions
                        .get(&record)
                        .map_or(0, |session| session.lease.expires_at_ms)
                && store::people::is_active(&self.store.pool, &settings.controller_uid).await?
                && self
                    .require_permission(Some(&settings.controller_uid), "record:update")
                    .await
                    .is_ok()
                && self
                    .may_read_record(Some(&settings.controller_uid), &record)
                    .await?;
            let ended = match &settings.transfer_uid {
                Some(transfer) => {
                    store::location::transfer_ended(&self.store.pool, transfer).await?
                }
                None => false,
            };
            let authority_valid = self
                .location_authority(&record)
                .await
                .is_ok_and(|authority| self.location_node_id().is_ok_and(|node| authority == node));
            let source_valid = self.location_devices().await.is_ok_and(|(_, devices)| {
                devices.iter().any(|device| {
                    device.uid == settings.source_cell_uid
                        && device.node_id.as_deref() == Some(settings.source_node_id.as_str())
                })
            });
            let source_admitted = if !approved
                || self
                    .location_node_id()
                    .is_ok_and(|node| node == settings.source_node_id)
            {
                true
            } else {
                let mut connection = self.store.pool.acquire().await?;
                store::session_access::device_on(
                    &mut connection,
                    &settings.controller_uid,
                    &settings.source_node_id,
                )
                .await?
                .is_some_and(|device| !device.revoked)
            };
            if !live || ended || !authority_valid || !source_valid || !source_admitted {
                self.location_stop_record(&record).await;
            }
        }
        let mut state = self.location.state.lock().await;
        for session in state.sessions.values_mut() {
            if session
                .latest
                .as_ref()
                .is_some_and(|observation| observation.age_ms() >= MAX_FIX_AGE_MS as u64)
            {
                session.latest = None;
            }
        }
        state.sources.retain(|_, source| {
            source.lease.expires_at_ms > now_ms()
                && source.renewed.elapsed() < Duration::from_secs(30)
        });
        Ok(())
    }

    pub async fn location_tick(&self) -> Result<(), EngineError> {
        self.location_expire().await?;
        let sources: Vec<_> = self
            .location
            .state
            .lock()
            .await
            .sources
            .values()
            .filter(|source| source.renewed.elapsed() >= Duration::from_secs(5))
            .map(|source| source.lease.clone())
            .collect();
        for lease in sources {
            let data = if lease.authority_node_id == self.location_node_id()? {
                serde_json::to_value(
                    self.location
                        .state
                        .lock()
                        .await
                        .sessions
                        .get(&lease.settings.record_uid)
                        .map(|session| &session.lease),
                )?
            } else {
                match self
                    .location_network()?
                    .location_request(
                        &lease.authority_node_id,
                        PeerRequest::SourceLease {
                            record_uid: lease.settings.record_uid.clone(),
                            session_uid: lease.session_uid.clone(),
                        },
                    )
                    .await
                {
                    Ok(data) => data,
                    Err(_) => continue,
                }
            };
            let renewed: Option<SourceLease> = serde_json::from_value(data)?;
            let mut state = self.location.state.lock().await;
            if let Some(source) = state.sources.get_mut(&lease.settings.record_uid) {
                if source.lease.session_uid != lease.session_uid {
                    continue;
                }
                if let Some(renewed) = renewed {
                    source.lease = renewed;
                    source.renewed = Instant::now();
                } else {
                    state.sources.remove(&lease.settings.record_uid);
                }
            }
        }
        Ok(())
    }

    pub async fn location_sources(&self) -> Vec<SourceLease> {
        self.location
            .state
            .lock()
            .await
            .sources
            .values()
            .filter(|source| {
                source.lease.approved
                    && source.lease.expires_at_ms > now_ms()
                    && source.renewed.elapsed() < Duration::from_secs(30)
            })
            .map(|source| source.lease.clone())
            .collect()
    }

    pub async fn location_pending_sources(&self) -> Vec<SourceLease> {
        self.location
            .state
            .lock()
            .await
            .sources
            .values()
            .filter(|source| !source.lease.approved)
            .map(|source| source.lease.clone())
            .collect()
    }

    async fn location_publish_source(
        &self,
        source: &SourceLease,
        fix: Fix,
    ) -> Result<(), EngineError> {
        if !source.approved {
            return Err(denied("Approve location on this device first"));
        }
        fix.validate(now_ms()).map_err(invalid)?;
        let data = if source.authority_node_id == self.location_node_id()? {
            self.location_accept_fix(
                &source.settings.source_node_id,
                &source.settings.record_uid,
                &source.session_uid,
                fix,
            )
            .await?
        } else {
            self.location_network()?
                .location_request(
                    &source.authority_node_id,
                    PeerRequest::Publish {
                        record_uid: source.settings.record_uid.clone(),
                        session_uid: source.session_uid.clone(),
                        fix,
                    },
                )
                .await?
        };
        let renewed: SourceLease = serde_json::from_value(data)?;
        if let Some(active) = self
            .location
            .state
            .lock()
            .await
            .sources
            .get_mut(&source.settings.record_uid)
        {
            if active.lease.session_uid == renewed.session_uid {
                active.renewed = Instant::now();
            }
        }
        Ok(())
    }

    pub async fn publish_device_location(
        &self,
        latitude: f64,
        longitude: f64,
        accuracy_metres: Option<f64>,
        captured_at_ms: i64,
        sequence: u64,
    ) -> Result<(), EngineError> {
        self.location_expire().await?;
        for source in self
            .location_sources()
            .await
            .into_iter()
            .filter(|source| source.settings.source_kind == SourceKind::Device)
        {
            self.location_publish_source(
                &source,
                Fix {
                    latitude,
                    longitude,
                    accuracy_metres,
                    captured_at_ms,
                    sequence,
                },
            )
            .await?;
        }
        Ok(())
    }

    pub async fn location_device_unavailable(&self) {
        for source in self
            .location_sources()
            .await
            .into_iter()
            .filter(|source| source.settings.source_kind == SourceKind::Device)
        {
            if self
                .location_node_id()
                .is_ok_and(|node| node == source.authority_node_id)
            {
                if let Some(session) = self
                    .location
                    .state
                    .lock()
                    .await
                    .sessions
                    .get_mut(&source.settings.record_uid)
                {
                    session.latest = None;
                    session.unavailable = true;
                }
            } else if let Ok(network) = self.location_network() {
                let _ = network
                    .location_request(
                        &source.authority_node_id,
                        PeerRequest::Unavailable {
                            record_uid: source.settings.record_uid,
                            session_uid: source.session_uid,
                        },
                    )
                    .await;
            }
        }
    }
}

fn stopped(record: &str) -> View {
    View {
        record_uid: record.into(),
        status: Status::Stopped,
        session_uid: None,
        expires_at_ms: None,
        source_kind: None,
        fix: None,
        age_ms: None,
    }
}

#[cfg(test)]
mod tests;
