use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError};

pub const LEASE: Duration = Duration::from_secs(30);
const MAX_DEVICES: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub organ: String,
    pub person: String,
    pub device: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracks {
    pub microphone: bool,
    pub camera: bool,
    pub screen: bool,
    pub shared_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Signal {
    Offer(String),
    Answer(String),
    Candidate {
        mid: String,
        line: i32,
        candidate: String,
    },
}

impl Signal {
    fn len(&self) -> usize {
        match self {
            Self::Offer(value) | Self::Answer(value) => value.len(),
            Self::Candidate { mid, candidate, .. } => mid.len() + candidate.len(),
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Offer(sdp) | Self::Answer(sdp) => {
                sdp.len() <= 65_536 && sdp.matches("m=").count() <= 4
            }
            Self::Candidate {
                mid,
                line,
                candidate,
            } => mid.len() <= 64 && (0..4).contains(line) && candidate.len() <= 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Inspect,
    Start,
    Join {
        call: String,
    },
    Poll {
        call: String,
    },
    Tracks {
        call: String,
        tracks: Tracks,
    },
    Signal {
        call: String,
        to: Identity,
        signal: Signal,
    },
    Leave {
        call: String,
    },
    End {
        call: String,
    },
}

impl Operation {
    fn call(&self) -> Option<&str> {
        match self {
            Self::Inspect | Self::Start => None,
            Self::Join { call }
            | Self::Poll { call }
            | Self::Tracks { call, .. }
            | Self::Signal { call, .. }
            | Self::Leave { call }
            | Self::End { call } => Some(call),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub thread: String,
    pub person: Option<String>,
    pub device: String,
    pub name: String,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub identity: Identity,
    pub name: String,
    pub organ_name: String,
    pub tracks: Tracks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub from: Identity,
    pub signal: Signal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub call: Option<String>,
    pub started_at: Option<String>,
    pub participants: Vec<Participant>,
    pub signals: Vec<Envelope>,
    pub redirect: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub root: String,
    pub people: Vec<(String, String)>,
    pub organs: Vec<(String, String)>,
    pub current_organs: Vec<String>,
    pub admitted: Vec<String>,
    pub group: Option<crate::groups::SignedMembership>,
}

struct Occupant {
    participant: Participant,
    seen: Instant,
    inbox: VecDeque<Envelope>,
}

struct Active {
    id: String,
    root: String,
    started_at: String,
    started: Instant,
    starter: Participant,
    occupants: Vec<Occupant>,
}

#[derive(Default)]
pub(crate) struct Calls {
    active: BTreeMap<String, Active>,
}

fn denied(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

impl Engine {
    pub async fn call_context(
        &self,
        thread: &str,
        actor: Option<&str>,
    ) -> Result<Context, EngineError> {
        if !self.may_read_record(actor, thread).await? {
            return Err(denied("This thread is unavailable"));
        }
        let (root, _) = self.call_root(thread).await?;
        let people = if let Some(actor) = actor {
            store::records::get(&self.store.pool, actor)
                .await?
                .into_iter()
                .map(|person| (person.uid, person.head))
                .collect()
        } else {
            store::sqlx::query_as::<_, (String, String)>("SELECT uid, head FROM record WHERE kind = 'person' AND deleted_at IS NULL ORDER BY head LIMIT 256").fetch_all(&self.store.pool).await?
        };
        let organs = if self
            .require_permission(actor, "record:create")
            .await
            .is_ok()
        {
            store::sqlx::query_as::<_, (String, String)>("SELECT c.record_uid, r.head FROM organ_contact c JOIN record r ON r.uid = c.record_uid WHERE c.trust != 'blocked' AND c.node_id IS NOT NULL ORDER BY r.head LIMIT 256").fetch_all(&self.store.pool).await?
        } else {
            Vec::new()
        };
        let mut admitted = Vec::new();
        for (person, _) in &people {
            if self.group_person_admitted(&root, person).await? {
                admitted.push(person.clone());
            }
        }
        let group = self.group(&root).await?;
        let current_organs = if let Some(group) = &group {
            group
                .membership
                .members
                .iter()
                .filter(|member| !member.removed)
                .map(|member| member.organ.clone())
                .collect()
        } else {
            store::sqlx::query_scalar("SELECT contact_organ FROM replica_grant WHERE root_record = ? ORDER BY contact_organ LIMIT 6")
                .bind(&root).fetch_all(&self.store.pool).await?
        };
        Ok(Context {
            root,
            people,
            organs,
            current_organs,
            admitted,
            group,
        })
    }
    fn call_transport(
        &self,
    ) -> Result<std::sync::Arc<dyn crate::enrolment::CellTransport>, EngineError> {
        self.enroller
            .lock()
            .map_err(|_| denied("Network unavailable"))?
            .as_ref()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| denied("Start this Cell's network to make calls"))
    }

    async fn call_root(&self, thread: &str) -> Result<(String, String), EngineError> {
        let row = store::records::get(&self.store.pool, thread)
            .await?
            .ok_or_else(|| denied("Thread unavailable"))?;
        if row.kind != nucleus::RecordKind::Thread.as_str() || !row.is_active() {
            return Err(denied("Thread unavailable"));
        }
        let root = store::replica::root_of(&self.store.pool, thread)
            .await?
            .unwrap_or_else(|| thread.to_owned());
        let owner = store::records::get(&self.store.pool, &root)
            .await?
            .and_then(|record| record.organ_uid)
            .ok_or_else(|| denied("Thread owner unavailable"))?;
        Ok((root, owner))
    }

    pub async fn call_for_session(
        &self,
        thread: String,
        selected_person: Option<String>,
        actor: Option<&str>,
        device: &str,
        operation: Operation,
    ) -> Result<Snapshot, EngineError> {
        let person = match (actor, selected_person) {
            (Some(actor), Some(person)) if actor != person => {
                return Err(denied("A call cannot use another person's identity"));
            }
            (Some(actor), _) => Some(actor.to_owned()),
            (None, person) => person,
        };
        if !self
            .may_read_record(person.as_deref().or(actor), &thread)
            .await?
        {
            return Err(denied("This thread is unavailable"));
        }
        let (root, owner) = self.call_root(&thread).await?;
        let name = if let Some(person) = &person {
            if !store::people::is_active(&self.store.pool, person).await? {
                return Err(denied("Choose an active person"));
            }
            if self.group(&root).await?.is_some()
                && !self.group_person_admitted(&root, person).await?
            {
                return Err(denied("Your Organ must admit this person to the group"));
            }
            store::records::get(&self.store.pool, person)
                .await?
                .ok_or_else(|| denied("Person unavailable"))?
                .head
        } else if matches!(operation, Operation::Inspect) {
            String::new()
        } else {
            return Err(denied("Choose the person who is joining"));
        };
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| denied("No local Organ"))?;
        let request = Request {
            thread,
            person,
            device: device.into(),
            name,
            operation,
        };
        let transport = self.call_transport()?;
        let node = self.call_coordinator(&root, &owner).await?;
        let local_node = transport.local_node_id();
        if Some(&node) == local_node.as_ref() {
            self.coordinate_call(&local.uid, request).await
        } else {
            transport.call(&node, request).await
        }
    }

    async fn call_coordinator(&self, root: &str, owner: &str) -> Result<String, EngineError> {
        if let Some(group) = self.group(root).await? {
            let node = group
                .membership
                .members
                .into_iter()
                .find(|member| member.organ == owner && member.accepted && !member.removed)
                .map(|member| member.node_id)
                .ok_or_else(|| denied("Group coordinator unavailable"))?;
            if let Some(roster) = self.roster_of(owner).await?
                && (!crate::roster::roster_signature_is_valid(&roster)
                    || !roster
                        .roster
                        .cells
                        .iter()
                        .any(|cell| cell.node_id == node && cell.may(crate::roster::CAP_REPRESENT)))
            {
                return Err(denied(
                    "The group coordinating Cell is no longer authorized",
                ));
            }
            return Ok(node);
        }
        if let Some(roster) = self.roster_of(owner).await? {
            if !crate::roster::roster_signature_is_valid(&roster) {
                return Err(denied("The coordinating Cell's authorization has expired"));
            }
            let creator: Option<String> = store::sqlx::query_scalar("SELECT actor_cell FROM sync_op WHERE tbl = 'record' AND uid = ? AND organ_uid = ? ORDER BY hlc LIMIT 1")
                .bind(root).bind(owner).fetch_optional(&self.store.pool).await?;
            if let Some(cell) = roster.roster.cells.iter().find(|cell| {
                Some(&cell.cell_uid) == creator.as_ref() && cell.may(crate::roster::CAP_REPRESENT)
            }) {
                return Ok(cell.node_id.clone());
            }
            return Err(denied("The owning Organ has no coordinating Cell"));
        }
        if store::organs::local(&self.store.pool)
            .await?
            .is_some_and(|local| local.uid == owner)
        {
            return self
                .call_transport()?
                .local_node_id()
                .ok_or_else(|| denied("Coordinator unavailable"));
        }
        store::organs::contact(&self.store.pool, owner)
            .await?
            .filter(|contact| contact.trust != "blocked")
            .and_then(|contact| contact.node_id)
            .ok_or_else(|| denied("The owning Organ is unavailable"))
    }

    async fn call_organ_allowed(&self, root: &str, organ: &str) -> Result<bool, EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| denied("No local Organ"))?;
        if organ != local.uid
            && store::organs::contact(&self.store.pool, organ)
                .await?
                .is_none_or(|contact| contact.trust == "blocked")
        {
            return Ok(false);
        }
        if let Some(group) = self.group(root).await? {
            return Ok(group
                .membership
                .members
                .iter()
                .any(|member| member.organ == organ && member.accepted && !member.removed));
        }
        Ok(
            organ == local.uid
                || store::replica::is_accepted(&self.store.pool, root, organ).await?,
        )
    }

    async fn call_person_allowed(
        &self,
        root: &str,
        thread: &str,
        identity: &Identity,
    ) -> Result<bool, EngineError> {
        if !self.call_organ_allowed(root, &identity.organ).await? {
            return Ok(false);
        }
        if store::organs::local(&self.store.pool)
            .await?
            .is_some_and(|local| local.uid == identity.organ)
        {
            return Ok(
                store::people::is_active(&self.store.pool, &identity.person).await?
                    && self.may_read_record(Some(&identity.person), thread).await?
                    && (self.group(root).await?.is_none()
                        || self.group_person_admitted(root, &identity.person).await?),
            );
        }
        Ok(true)
    }

    pub async fn coordinate_call(
        &self,
        organ: &str,
        request: Request,
    ) -> Result<Snapshot, EngineError> {
        if request.device.is_empty()
            || request.device.len() > 128
            || request.name.len() > 512
            || request
                .person
                .as_ref()
                .is_some_and(|person| !nucleus::valid_uid(person, "r"))
        {
            return Err(denied("Invalid call identity"));
        }
        let (root, owner) = self.call_root(&request.thread).await?;
        if !self.call_organ_allowed(&root, organ).await? {
            return Err(denied("This Organ cannot enter this call"));
        }
        let node = self.call_coordinator(&root, &owner).await?;
        if self.call_transport()?.local_node_id().as_ref() != Some(&node) {
            return Ok(Snapshot {
                redirect: Some(node),
                ..Default::default()
            });
        }
        let identity = Identity {
            organ: organ.into(),
            person: request.person.clone().unwrap_or_default(),
            device: request.device.clone(),
        };
        if !matches!(request.operation, Operation::Inspect)
            && !self
                .call_person_allowed(&root, &request.thread, &identity)
                .await?
        {
            return Err(denied("This person cannot enter this call"));
        }
        let mut calls = self.calls.lock().await;
        self.prune_call(&mut calls, &request.thread).await?;
        if matches!(request.operation, Operation::Start)
            && !calls.active.contains_key(&request.thread)
        {
            if calls.active.len() >= 32 {
                return Err(denied("This Cell has too many active calls"));
            }
            let organ_name = store::records::get(&self.store.pool, organ)
                .await?
                .map_or_else(|| organ.into(), |row| row.head);
            let starter = Participant {
                identity: identity.clone(),
                name: request.name.clone(),
                organ_name,
                tracks: Tracks::default(),
            };
            calls.active.insert(
                request.thread.clone(),
                Active {
                    id: nucleus::new_uid("r"),
                    root: root.clone(),
                    started_at: chrono::Utc::now().to_rfc3339(),
                    started: Instant::now(),
                    starter,
                    occupants: Vec::new(),
                },
            );
        }
        let Some(active) = calls.active.get_mut(&request.thread) else {
            return if matches!(
                request.operation,
                Operation::Inspect | Operation::Leave { .. } | Operation::Poll { .. }
            ) {
                Ok(Snapshot::default())
            } else {
                Err(denied("The call has ended"))
            };
        };
        if request.operation.call().is_some_and(|id| id != active.id) {
            return Err(denied(
                "This call has ended; join the current call explicitly",
            ));
        }
        let mut index = active
            .occupants
            .iter()
            .position(|occupant| occupant.participant.identity == identity);
        if matches!(request.operation, Operation::Start | Operation::Join { .. }) && index.is_none()
        {
            if active.occupants.len() >= MAX_DEVICES {
                return Err(denied("This call already has six devices"));
            }
            if active.occupants.iter().any(|occupant| {
                occupant.participant.identity.organ == identity.organ
                    && occupant.participant.identity.person == identity.person
            }) {
                return Err(denied("This person is already using another device"));
            }
            let organ_name = store::records::get(&self.store.pool, organ)
                .await?
                .map_or_else(|| organ.into(), |row| row.head);
            index = Some(active.occupants.len());
            active.occupants.push(Occupant {
                participant: Participant {
                    identity: identity.clone(),
                    name: request.name,
                    organ_name,
                    tracks: Tracks::default(),
                },
                seen: Instant::now(),
                inbox: VecDeque::new(),
            });
        }
        if !matches!(request.operation, Operation::Inspect) && index.is_none() {
            return Err(denied("Join this call before sending controls"));
        }
        if let Some(index) = index {
            active.occupants[index].seen = Instant::now();
        }
        let polling = matches!(request.operation, Operation::Poll { .. });
        match request.operation {
            Operation::Tracks { tracks, .. } => {
                if tracks.screen
                    && active.occupants.iter().any(|occupant| {
                        occupant.participant.identity != identity
                            && occupant.participant.tracks.screen
                    })
                {
                    return Err(denied("Someone is already sharing a screen"));
                }
                active.occupants[index.unwrap()].participant.tracks = tracks;
            }
            Operation::Signal { to, signal, .. } => {
                if !signal.valid() || to == identity {
                    return Err(denied("Invalid call signal"));
                }
                let recipient = active
                    .occupants
                    .iter_mut()
                    .find(|occupant| occupant.participant.identity == to)
                    .ok_or_else(|| denied("The recipient has left"))?;
                if recipient.inbox.len() >= 64
                    || recipient
                        .inbox
                        .iter()
                        .map(|envelope| envelope.signal.len())
                        .sum::<usize>()
                        + signal.len()
                        > 262_144
                {
                    return Err(denied("The recipient is not receiving call signals"));
                }
                recipient.inbox.push_back(Envelope {
                    from: identity.clone(),
                    signal,
                });
            }
            Operation::Leave { .. } => {
                active.occupants.remove(index.unwrap());
                index = None;
            }
            Operation::End { .. } => {
                if active.starter.identity != identity {
                    return Err(denied(
                        "Only the person who started this call can end it for everyone",
                    ));
                }
                active.occupants.clear();
                index = None;
            }
            _ => {}
        }
        if active.occupants.is_empty() {
            let ended = calls.active.remove(&request.thread).unwrap();
            if let Err(error) = self.complete_call(&request.thread, &ended).await {
                calls.active.insert(request.thread, ended);
                return Err(error);
            }
            return Ok(Snapshot::default());
        }
        let signals = if polling {
            index
                .map(|index| active.occupants[index].inbox.drain(..).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Snapshot {
            call: Some(active.id.clone()),
            started_at: Some(active.started_at.clone()),
            participants: active
                .occupants
                .iter()
                .map(|occupant| occupant.participant.clone())
                .collect(),
            signals,
            redirect: None,
        })
    }

    async fn prune_call(&self, calls: &mut Calls, thread: &str) -> Result<(), EngineError> {
        if let Some(active) = calls.active.get_mut(thread) {
            let mut retained = Vec::new();
            for occupant in active.occupants.drain(..) {
                if occupant.seen.elapsed() < LEASE
                    && self
                        .call_person_allowed(&active.root, thread, &occupant.participant.identity)
                        .await?
                {
                    retained.push(occupant);
                }
            }
            active.occupants = retained;
            let present: Vec<_> = active
                .occupants
                .iter()
                .map(|occupant| occupant.participant.identity.clone())
                .collect();
            for occupant in &mut active.occupants {
                occupant
                    .inbox
                    .retain(|envelope| present.contains(&envelope.from));
            }
            if active.occupants.is_empty() {
                let ended = calls.active.remove(thread).unwrap();
                if let Err(error) = self.complete_call(thread, &ended).await {
                    calls.active.insert(thread.into(), ended);
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub async fn sweep_calls(&self) -> Result<(), EngineError> {
        let mut calls = self.calls.lock().await;
        let threads: Vec<_> = calls.active.keys().cloned().collect();
        for thread in threads {
            self.prune_call(&mut calls, &thread).await?;
        }
        Ok(())
    }

    async fn complete_call(&self, thread: &str, call: &Active) -> Result<(), EngineError> {
        let root = store::replica::root_of(&self.store.pool, thread).await?;
        if store::records::get(&self.store.pool, &call.id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let body = serde_json::json!({ "started_by": call.starter.name, "started_by_person": call.starter.identity.person, "started_by_organ": call.starter.identity.organ, "organ": call.starter.organ_name, "started_at": call.started_at, "duration_seconds": call.started.elapsed().as_secs() });
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| denied("No local Organ"))?;
        let predicate = store::concepts::ensure(&self.store.pool, "call-in").await?;
        let mut transaction = store::write_tx(&self.store.pool).await?;
        let record = store::records::create_with_uid_on(
            &mut transaction,
            &call.id,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::CallSession,
                head: "Call",
                body: &body.to_string(),
                quantity: store::exact::one(),
            },
            &local.uid,
            root.as_deref(),
        )
        .await?;
        store::assertions::insert_tx(
            &mut transaction,
            &nucleus::new_uid("a"),
            store::assertions::NewAssertion {
                subject_uid: &record.uid,
                predicate_uid: &predicate,
                object_uid: Some(thread),
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await?;
        transaction.commit().await?;
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(())
    }
}
