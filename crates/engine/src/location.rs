use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::location::*;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

pub mod clock;
mod sessions;
mod invitations;
mod stopping;
mod capture;
use clock::Instant;

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
    invitations: HashMap<String, invitations::Lease>,
    journey_stops: HashMap<String, Vec<(String, String)>>,
}

struct Session {
    lease: SourceLease,
    deadline: Instant,
    latest: Option<Observation>,
    sequence: u64,
    unavailable: bool,
    approved_at: Option<Instant>,
    challenges: VecDeque<(String, Instant)>,
}

struct Source {
    lease: SourceLease,
    renewed: Instant,
}

impl Source {
    fn renew(&mut self, lease: SourceLease, started: Instant, explicit_approval: bool) -> Result<(), EngineError> {
        if lease.session_uid != self.lease.session_uid || lease.authority_node_id != self.lease.authority_node_id || lease.settings != self.lease.settings {
            return Err(denied("Location renewal changed its reviewed scope"));
        }
        if !explicit_approval && (started < self.renewed || (self.lease.approved && !lease.approved)) {return Ok(());}
        if !(1..=30_000).contains(&lease.lease_millis) || !nucleus::valid_uid(&lease.challenge_uid,"lc") || started.elapsed() >= Duration::from_millis(u64::from(lease.lease_millis)) || (lease.approved && !self.lease.approved && !explicit_approval) {
            return Err(denied("Location renewal changed its reviewed scope, approval or elapsed lease"));
        }
        self.lease = lease;
        self.renewed = started;
        Ok(())
    }
}

struct Observation {
    fix: Fix,
    received: Instant,
    initial_age_ms: u64,
}

impl Session {
    fn renew(&mut self) -> SourceLease {
        let now = Instant::now();
        self.lease.lease_millis = (self.deadline - now).as_millis().min(30_000) as u32;
        self.challenges.retain(|(_, issued)| issued.elapsed() < Duration::from_secs(30));
        self.lease.challenge_uid = nucleus::new_uid("lc");
        if self.lease.approved {
            self.challenges.push_back((self.lease.challenge_uid.clone(), now));
            while self.challenges.len() > 8 { self.challenges.pop_front(); }
        }
        self.lease.clone()
    }
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

    pub(crate) fn location_observer_person(&self, node: &str, person: &str) -> String {
        self.location
            .observers
            .lock()
            .expect("location observers")
            .get(&(node.into(), person.into()))
            .cloned()
            .unwrap_or_else(|| person.into())
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
            Command::ObserveInvitation {person,invitation} if actor.is_none() => self.location.observers.lock().expect("location observers").contains_key(&(invitation.authority_node_id.clone(),person.clone())),
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
            Command::StopTransfer {person,transfer_uid} => {
                self.location.state.lock().await.sources.retain(|_,source|source.lease.settings.controller_uid != *person || source.lease.settings.transfer_uid.as_ref() != Some(transfer_uid));
            }
            _ => {}
        }
        let record = match &command {
            Command::Configure { settings } | Command::Handover { settings, .. } => Some(settings.record_uid.as_str()),
            Command::Context { record_uid, .. }
            | Command::Start { record_uid, .. }
            | Command::View { record_uid, .. }
            | Command::Stop { record_uid, .. } => Some(record_uid.as_str()),
            Command::PrepareInvitation {record_uid,..} => Some(record_uid.as_str()),
            _ => None,
        };
        let data = if bootstrap {
            self.location_local_command(command, actor).await?
        } else if matches!(command, Command::StopAll { .. } | Command::StopTransfer {..}) {
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
        } else if let Command::ObserveInvitation {person,invitation} = command {
            invitation.validate().map_err(invalid)?;
            let node = invitation.authority_node_id.clone();
            let person = self.location.observers.lock().expect("location observers").get(&(node.clone(),person.clone())).cloned().unwrap_or(person);
            let started = Instant::now();
            let response = if node == self.location_node_id()? {
                let organ = store::organs::local(&self.store.pool).await?.ok_or_else(||invalid("Local Organ unavailable"))?.uid;
                self.location_read_invitation(&person,&organ,&invitation).await?
            } else {
                self.location_network()?.location_request(&node,PeerRequest::Control {command:Command::ReadInvitation {person,invitation}}).await?
            };
            age_response(response,started.elapsed())
        } else if let Command::Observe {
            person,
            record_uid,
            node_id,
        } = command
        {
            let started = Instant::now();
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
                    data: Some(age_response(view, started.elapsed())),
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
            let started = Instant::now();
            let result = self.location_network()?
                .location_request(
                    &node_id,
                    PeerRequest::Control {
                        command: Command::View { person, record_uid },
                    },
                )
                .await?;
            age_response(result, started.elapsed())
        } else if let Some(record) = record {
            let authority = self.location_authority(record).await?;
            if authority != self.location_node_id()? {
                let context_request = matches!(command, Command::Context { .. });
                let started = Instant::now();
                let mut data = self
                    .location_network()?
                    .location_request(&authority, PeerRequest::Control { command })
                    .await?;
                data = age_response(data, started.elapsed());
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
            Command::PrepareInvitation {person,record_uid,recipient,duration_seconds,radius_metres} => self.location_prepare_invitation(&person,&record_uid,&recipient,duration_seconds,radius_metres,actor).await,
            Command::ReadInvitation {person,invitation} => {
                let organ = store::organs::local(&self.store.pool).await?.ok_or_else(||invalid("Local Organ unavailable"))?.uid;
                self.location_read_invitation(&person,&organ,&invitation).await
            }
            Command::SendInvitation {person,conversation,invitation} => {
                invitation.validate().map_err(invalid)?;
                let grant = invitation.grant.as_ref().ok_or_else(||invalid("Review a recipient-scoped invitation first"))?;
                if grant.issuer != person {return Err(denied("Only the issuing Person can send this reviewed invitation"));}
                let precision = grant.radius_metres.map_or("permitted precise position".into(),|radius|format!("an area of at least {radius} metres radius"));
                let text = format!("Private location invitation for {}: {precision}. Opening requires current access and recipient authentication. The source enforces its expiry.\n{}",grant.recipient,invitation.reference().map_err(invalid)?);
                let result = self.social_command(nucleus::social::Command::SendPrivate {conversation,text},actor,nucleus::execution::now()).await?;
                Ok(json!({"invitation_message":result.data.unwrap_or_default()}))
            }
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
                self.location_validate_configuration(&settings,actor).await?;
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
                self.location_validate_configuration(&settings, actor).await?;
                self.location_begin(settings,None).await
            }
            Command::Handover { settings, expected_session_uid } => {
                let _mutation = self.location.mutation.lock().await;
                self.location_validate_configuration(&settings,actor).await?;
                self.location_begin(settings,Some(&expected_session_uid)).await
            }
            Command::Approve { person, record_uid, expected_session_uid } => {
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
                if lease.session_uid != expected_session_uid {return Err(denied("The source session changed; review its approval again"));}
                let started = Instant::now();
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
                let mut state = self
                    .location
                    .state
                    .lock()
                    .await;
                let source = state.sources.get_mut(&record_uid).ok_or_else(|| denied("Location source request ended before approval"))?;
                source.renew(next,started,true)?;
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
            Command::StopTransfer {person,transfer_uid} => self.location_stop_transfer_sessions(&person,&transfer_uid).await,
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
            Command::Observe { .. } | Command::ObserveInvitation {..} => Err(invalid("Use the selected observer endpoint")),
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
        active.approved_at = Some(Instant::now());
        Ok(serde_json::to_value(active.renew())?)
    }

    async fn location_view(&self, person: &str, record: &str) -> Result<View, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .unwrap_or_default();
        self.location_person_view(person, record, &organ).await
    }

    pub(crate) async fn location_person_view(
        &self,
        person: &str,
        record: &str,
        organ: &str,
    ) -> Result<View, EngineError> {
        self.location_person_view_scoped(person,record,organ,None).await
    }

    async fn location_person_view_scoped(&self,person:&str,record:&str,organ:&str,scope:Option<&InvitationGrant>) -> Result<View,EngineError> {
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
        if scope.is_some_and(|scope|scope.issuer != lease.settings.controller_uid) {return Ok(stopped(record));}
        let controller = lease.settings.controller_uid == person;
        let decision = self
            .principal_visibility(
                record,
                nucleus::visibility::Data::LiveLocation,
                organ,
                Some(person),
            )
            .await?;
        let record_allowed = store::data_visibility::policy(
            &self.store.pool,
            record,
            nucleus::visibility::Data::Record,
        )
        .await?
        .is_none()
            || self
                .principal_visibility(
                    record,
                    nucleus::visibility::Data::Record,
                    organ,
                    Some(person),
                )
                .await?
                .allowed;
        let blocked = store::organs::contact(&self.store.pool, organ)
            .await?
            .is_some_and(|contact| contact.trust == "blocked");
        let allowed = controller
            || (!blocked
                && record_allowed
                && decision.excluded_by.is_empty()
                && (decision.allowed
                    || lease
                        .settings
                        .recipients
                        .iter()
                        .any(|recipient| recipient == person)));
        self.location_permitted_view(
            record,
            &lease.session_uid,
            allowed,
            controller && scope.is_none(),
            [decision.radius_metres,scope.and_then(|scope|scope.radius_metres)].into_iter().flatten().max(),
        )
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
        let record_allowed = store::data_visibility::policy(
            &self.store.pool,
            record,
            nucleus::visibility::Data::Record,
        )
        .await?
        .is_none()
            || self
                .organ_visibility(record, nucleus::visibility::Data::Record, organ)
                .await?
                .allowed;
        let session = self
            .location
            .state
            .lock()
            .await
            .sessions
            .get(record)
            .map(|session| session.lease.session_uid.clone());
        let view = if let Some(session) = session {
            self.location_permitted_view(
                record,
                &session,
                decision.allowed && record_allowed,
                false,
                decision.radius_metres,
            )
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
        radius_metres: Option<u32>,
    ) -> Result<View, EngineError> {
        let state = self.location.state.lock().await;
        let Some(session) = state.sessions.get(record) else {
            return Ok(stopped(record));
        };
        if !allowed || session.lease.session_uid != session_uid || session.deadline <= Instant::now() {
            return Ok(stopped(record));
        }
        let age = session.latest.as_ref().map(Observation::age_ms);
        let fresh = age.is_some_and(|age| age < MAX_FIX_AGE_MS as u64);
        let mut view = View {
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
            remaining_millis: Some((session.deadline - Instant::now()).as_millis() as u64),
            next_sequence: controller.then(||session.sequence.saturating_add(1)),
            radius_metres: None,
        };
        if !controller
            && let Some(radius) = radius_metres
            && let Some(fix) = &mut view.fix
        {
            let center = nucleus::place::approximate(
                nucleus::place::Place {
                    lat: fix.latitude,
                    lon: fix.longitude,
                },
                radius,
            )
            .ok_or_else(|| invalid("Invalid approximate location area"))?;
            fix.latitude = center.lat;
            fix.longitude = center.lon;
            fix.accuracy_metres = None;
            view.radius_metres = Some(radius);
        }
        Ok(view)
    }

    #[cfg(test)]
    async fn location_accept_fix(&self, peer: &str, record: &str, session_uid: &str, fix: Fix) -> Result<Value, EngineError> {
        let challenge = self.location.state.lock().await.sessions.get(record).map(|session|session.lease.challenge_uid.clone()).unwrap_or_default();
        self.location_accept_challenged_fix(peer,record,session_uid,&challenge,fix).await
    }

    async fn location_accept_challenged_fix(
        &self,
        peer: &str,
        record: &str,
        session_uid: &str,
        challenge_uid: &str,
        fix: Fix,
    ) -> Result<Value, EngineError> {
        fix.validate().map_err(invalid)?;
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
            || session.approved_at.is_none_or(|approved| fix.capture_age_ms > approved.elapsed().as_millis() as u64)
        {
            return Err(denied(
                "Expired, replayed, or unauthorized location observation",
            ));
        }
        let issued = session.challenges.iter().find(|(challenge, issued)| challenge == challenge_uid && issued.elapsed() < Duration::from_secs(30)).map(|(_,issued)|*issued).ok_or_else(|| denied("Location challenge expired or belongs to another session"))?;
        let initial_age_ms = fix.capture_age_ms.saturating_add(issued.elapsed().as_millis() as u64);
        if initial_age_ms >= MAX_FIX_AGE_MS as u64 {return Err(denied("Location freshness cannot be established"));}
        session.sequence = fix.sequence;
        session.latest = Some(Observation {
            initial_age_ms,
            fix,
            received: Instant::now(),
        });
        session.unavailable = false;
        Ok(serde_json::to_value(session.renew())?)
    }

    pub async fn location_peer(
        &self,
        organ: &str,
        peer: &str,
        request: PeerRequest,
    ) -> Result<Value, EngineError> {
        let write = matches!(&request, PeerRequest::Visibility { .. })
            || matches!(&request, PeerRequest::Control { command } if !matches!(command, Command::View { .. } | Command::ReadInvitation {..}));
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
            PeerRequest::ReadPerson {
                record_uid,
                data,
                person,
            } => {
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
                self.visibility_read_principal(organ, Some(&person), &record_uid, data)
                    .await
            }
            PeerRequest::Control { command } => {
                if !matches!(
                    command,
                    Command::Context { .. }
                        | Command::Configure { .. }
                        | Command::Handover { .. }
                        | Command::Start { .. }
                        | Command::Stop { .. }
                        | Command::StopAll { .. }
                        | Command::StopTransfer {..}
                        | Command::View { .. }
                        | Command::PrepareInvitation {..}
                        | Command::ReadInvitation {..}
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
                if let Command::ReadInvitation {person,invitation} = command {return self.location_read_invitation(&person,organ,&invitation).await;}
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
                    || lease.lease_millis == 0
                    || lease.lease_millis > 30_000
                    || !nucleus::valid_uid(&lease.challenge_uid,"lc")
                    || !nucleus::valid_uid(&lease.session_uid,"location")
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
                let mut state = self.location.state.lock().await;
                let lease = state
                    .sessions
                    .get_mut(&record_uid)
                    .filter(|session| {
                        session.lease.session_uid == session_uid
                            && session.lease.settings.source_node_id == peer
                    })
                    .map(Session::renew);
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
                challenge_uid,
                fix,
            } => {
                if organ != local.uid {
                    return Err(denied("Source belongs to another Organ"));
                }
                self.location_accept_challenged_fix(peer, &record_uid, &session_uid, &challenge_uid, fix)
                    .await
            }
            PeerRequest::Unavailable {
                record_uid,
                session_uid,
            } => {
                let permitted = self.location.state.lock().await.sessions.get(&record_uid).is_some_and(|session|session.lease.session_uid == session_uid
                        && session.lease.settings.source_node_id == peer
                        && organ == local.uid
                );
                if permitted {self.location_stop_record_if(&record_uid,Some(&session_uid)).await;}
                Ok(json!({"stopped":permitted}))
            }
        }
    }

    async fn location_stop_record(&self, record: &str) {
        self.location_stop_record_if(record,None).await;
    }

    async fn location_stop_record_if(&self, record: &str, expected: Option<&str>) {
        let lease = self.location.state.lock().await.stop(record,expected);
        if let Some(lease) = lease {self.location_end_remote_source(&lease).await;}
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
                    session.lease.session_uid.clone(),
                    !session.lease.approved || session.challenges.back().is_some_and(|(_,issued)|issued.elapsed() < Duration::from_secs(30)),
                )
            })
            .collect();
        for (record, deadline, settings, approved, session_uid, source_live) in sessions {
            let live = deadline > Instant::now()
                && source_live
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
            let source_admitted = {
                let mut connection = self.store.pool.acquire().await?;
                let device=store::session_access::device_on(
                    &mut connection,
                    &settings.controller_uid,
                    &settings.source_node_id,
                )
                .await?;
                device.map_or(!approved || self.location_node_id().is_ok_and(|node|node==settings.source_node_id),|device|!device.revoked)
            };
            if !live || ended || !authority_valid || !source_valid || !source_admitted {
                self.location_stop_record_if(&record,Some(&session_uid)).await;
            }
        }
        let mut state = self.location.state.lock().await;
        state.invitations.retain(|_,invitation|invitation.deadline > Instant::now());
        for session in state.sessions.values_mut() {
            if session
                .latest
                .as_ref()
                .is_some_and(|observation| observation.age_ms() >= MAX_FIX_AGE_MS as u64)
            {
                session.latest = None;
            }
        }
        state.sources.retain(|_, source| source.renewed.elapsed() < Duration::from_millis(u64::from(source.lease.lease_millis)));
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
            let started = Instant::now();
            let data = if lease.authority_node_id == self.location_node_id()? {
                serde_json::to_value(
                    self.location
                        .state
                        .lock()
                        .await
                        .sessions
                        .get_mut(&lease.settings.record_uid)
                        .map(Session::renew),
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
                    if source.renew(renewed,started,false).is_err() {state.sources.remove(&lease.settings.record_uid);}
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
                    && source.renewed.elapsed() < Duration::from_millis(u64::from(source.lease.lease_millis))
            })
            .map(|source| {
                let mut lease = source.lease.clone();
                lease.lease_millis = lease.lease_millis.saturating_sub(source.renewed.elapsed().as_millis().min(u128::from(u32::MAX)) as u32);
                lease
            })
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
        fix.validate().map_err(invalid)?;
        let started = Instant::now();
        let data = if source.authority_node_id == self.location_node_id()? {
            self.location_accept_challenged_fix(
                &source.settings.source_node_id,
                &source.settings.record_uid,
                &source.session_uid,
                &source.challenge_uid,
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
                        challenge_uid: source.challenge_uid.clone(),
                        fix,
                    },
                )
                .await?
        };
        let renewed: SourceLease = serde_json::from_value(data)?;
        let mut state = self
            .location
            .state
            .lock()
            .await;
        let active = state.sources.get_mut(&source.settings.record_uid).ok_or_else(|| denied("Location source ended before acknowledgment"))?;
        active.renew(renewed,started,false)?;
        Ok(())
    }

    pub async fn publish_device_location(
        &self,
        latitude: f64,
        longitude: f64,
        accuracy_metres: Option<f64>,
        captured_at_ms: i64,
        capture_age_ms: u64,
        sequence: u64,
    ) -> Result<(), EngineError> {
        let sources=self.location_sources().await;
        self.publish_device_fix_for(&sources,Fix {latitude,longitude,accuracy_metres,captured_at_ms,capture_age_ms,sequence}).await
    }

    pub async fn location_device_unavailable(&self) {
        let sources=self.location_sources().await;
        self.location_device_unavailable_for(&sources).await;
    }
}

fn age_response(mut value: Value, elapsed: Duration) -> Value {
    let target = if value.get("view").is_some() {&mut value["view"]} else {&mut value};
    if let Ok(mut view) = serde_json::from_value::<View>(target.clone()) {
        view.elapse(elapsed.as_millis() as u64);
        *target = serde_json::to_value(view).expect("Location view serialization");
    }
    value
}

fn stopped(record: &str) -> View {
    View {
        record_uid: record.into(),
        status: Status::Stopped,
        session_uid: None,
        expires_at_ms: None,
        source_kind: None,
        fix: None,
        radius_metres: None,
        age_ms: None,
        remaining_millis: None,
        next_sequence: None,
    }
}

#[cfg(test)]
mod tests;
