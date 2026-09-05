use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use engine::action_intent::ActionIntentSession;
use engine::{Engine, EngineError};
use nucleus::Fact;
use nucleus::action_intent::{ActionIntentSessionProof, SignedActionIntent};
use protein::Protein;

use crate::lane::{LaneEvent, LaneHub};
use crate::protocol::{ClientMessage, ServerMessage};

pub struct Session {
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    subject: Option<String>,
    connection_id: String,
    subscriptions: HashMap<String, Protein>,
    last_ephemeral: HashMap<String, Vec<serde_json::Value>>,
    joined_rooms: Vec<String>,
    collab_records: HashSet<String>,
    action_intent: Option<ActionIntentSession>,
    action_intent_initialization_error: Option<(String, Option<String>)>,
    action_intent_initialized: bool,
}

impl Session {
    pub fn new(
        engine: Arc<Engine>,
        hub: Arc<LaneHub>,
        connection_id: impl Into<String>,
        subject: Option<String>,
    ) -> Session {
        Session {
            engine,
            hub,
            subject,
            connection_id: connection_id.into(),
            subscriptions: HashMap::new(),
            last_ephemeral: HashMap::new(),
            joined_rooms: Vec::new(),
            collab_records: HashSet::new(),
            action_intent: None,
            action_intent_initialization_error: None,
            action_intent_initialized: false,
        }
    }

    pub fn joined_rooms(&self) -> &[String] {
        &self.joined_rooms
    }

    pub async fn subject_may_act(&self) -> bool {
        let Some(subject) = self.subject.as_deref() else {
            return true;
        };
        store::people::is_active(&self.engine.store.pool, subject)
            .await
            .unwrap_or(true)
    }

    pub async fn initialize_action_intent(&mut self) -> ServerMessage {
        if !self.action_intent_initialized {
            self.action_intent_initialized = true;
            if let Some(subject) = self.subject.as_deref() {
                match self.engine.begin_action_intent_session(subject).await {
                    Ok(session) => self.action_intent = Some(session),
                    Err(error) => {
                        let (message, code) = engine_error(&error);
                        self.action_intent_initialization_error = Some((
                            message,
                            code.or(Some("action_intent_session_unavailable".into())),
                        ));
                    }
                }
            }
        }

        let person = self
            .action_intent
            .as_ref()
            .map(|session| session.person_uid().to_string());
        let session_id = self
            .action_intent
            .as_ref()
            .map(|session| session.session_id().to_string())
            .unwrap_or_else(|| nucleus::new_uid("unavailable-action-intent"));
        let challenge = self
            .action_intent
            .as_ref()
            .map(|session| session.challenge().to_string())
            .unwrap_or_else(|| nucleus::new_uid("unavailable-action-challenge"));
        ServerMessage::SessionChallenge {
            session_id,
            challenge,
            algorithm: "ed25519".into(),
            person,
            signing_required: self.subject.is_some(),
        }
    }

    pub async fn handle(&mut self, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::Subscribe { id, protein } => self.subscribe(id, protein).await,
            ClientMessage::SubscribeSaved { id, name } => self.subscribe_saved(id, name).await,
            ClientMessage::Unsubscribe { id } => {
                self.subscriptions.remove(&id);
                vec![]
            }
            ClientMessage::Act { id, action } => {
                if self.subject.is_some() {
                    vec![ServerMessage::Error {
                        id,
                        message: "authenticated WebSocket Actions require a signed intent".into(),
                        code: Some("action_intent_required".into()),
                    }]
                } else {
                    vec![self.act(id, action).await]
                }
            }
            ClientMessage::SessionAuthenticate {
                id,
                session_id,
                session_challenge,
                person_uid,
                key_id,
                public_key_base64,
                signature,
            } => {
                vec![
                    self.authenticate_action_intent(
                        id,
                        ActionIntentSessionProof {
                            session_id,
                            session_challenge,
                            person_uid,
                            key_id,
                            public_key_base64,
                            signature,
                        },
                    )
                    .await,
                ]
            }
            ClientMessage::SignedAct {
                id,
                session_id,
                session_challenge,
                sequence,
                action_base64,
                signature,
            } => {
                vec![
                    self.signed_act(SignedActionIntent {
                        session_id,
                        session_challenge,
                        sequence,
                        message_id: id,
                        action_base64,
                        signature,
                    })
                    .await,
                ]
            }
            ClientMessage::LaneJoin { room } => {
                if !self.joined_rooms.contains(&room) {
                    self.joined_rooms.push(room);
                }
                vec![]
            }
            ClientMessage::LaneLeave { room } => {
                self.joined_rooms.retain(|r| r != &room);
                self.hub.prune(&room);
                vec![]
            }
            ClientMessage::LaneSend {
                room,
                payload,
                organ,
            } => {
                if self.joined_rooms.contains(&room) {
                    self.hub.send(LaneEvent {
                        room,
                        from: self.connection_id.clone(),
                        payload,
                        from_subject: self.subject.clone(),
                        organ,
                    });
                }
                vec![]
            }
            ClientMessage::CollabJoin { id, record_uid } => {
                if !self
                    .engine
                    .may_read_record(self.subject.as_deref(), &record_uid)
                    .await
                    .unwrap_or(false)
                {
                    return vec![ServerMessage::Error {
                        id,
                        message: "record is not visible to you".into(),
                        code: Some("collab_not_visible".into()),
                    }];
                }
                match self.engine.collab_snapshot(&record_uid).await {
                    Ok(snapshot_base64) => {
                        self.collab_records.insert(record_uid.clone());
                        vec![ServerMessage::CollabState {
                            id,
                            record_uid,
                            snapshot_base64,
                        }]
                    }
                    Err(e) => {
                        let code = e.code().map(str::to_string);
                        vec![ServerMessage::Error {
                            id,
                            message: e.to_string(),
                            code,
                        }]
                    }
                }
            }
            ClientMessage::CollabLeave { record_uid } => {
                self.collab_records.remove(&record_uid);
                vec![]
            }
            ClientMessage::CollabUpdate {
                id,
                record_uid,
                update_base64,
            } => {
                if !self
                    .engine
                    .may_read_record(self.subject.as_deref(), &record_uid)
                    .await
                    .unwrap_or(false)
                {
                    return vec![ServerMessage::Error {
                        id,
                        message: "record is not visible to you".into(),
                        code: Some("collab_not_visible".into()),
                    }];
                }
                match self
                    .engine
                    .apply_client_crdt_update(&record_uid, &update_base64)
                    .await
                {
                    Ok(()) => vec![ServerMessage::CollabAck {
                        id: id.clone(),
                        record_uid: record_uid.clone(),
                    }],
                    Err(e) => {
                        let code = e.code().map(str::to_string);
                        vec![ServerMessage::Error {
                            id,
                            message: e.to_string(),
                            code,
                        }]
                    }
                }
            }
            ClientMessage::TerminalOpen { id, .. }
            | ClientMessage::TerminalInput { id, .. }
            | ClientMessage::TerminalResize { id, .. }
            | ClientMessage::TerminalClose { id } => vec![ServerMessage::Error {
                id,
                message: "terminal capability requires a host transport driver".into(),
                code: None,
            }],
            ClientMessage::LiveLogin { .. } => vec![ServerMessage::Error {
                id: "-".into(),
                message: "this session is already authenticated".into(),
                code: Some("already_authenticated".into()),
            }],
        }
    }

    async fn execute(
        &self,
        protein: &Protein,
    ) -> Result<Vec<serde_json::Value>, protein::ProteinError> {
        let signer_actor = self.available_signer_actor().await;
        let nearby = protein::is_ephemeral(protein).then(|| self.engine.nearby_peers());
        protein::execute_for_with_context(
            &self.engine.store,
            protein,
            self.subject.as_deref(),
            signer_actor.as_deref(),
            protein::Context {
                nearby: nearby.as_deref(),
            },
        )
        .await
    }

    async fn subscribe(&mut self, id: String, protein: Protein) -> Vec<ServerMessage> {
        match self.execute(&protein).await {
            Ok(rows) => {
                if protein::is_ephemeral(&protein) {
                    self.last_ephemeral.insert(id.clone(), rows.clone());
                }
                self.subscriptions.insert(id.clone(), protein);
                vec![ServerMessage::Snapshot { id, rows }]
            }
            Err(e) => vec![ServerMessage::Error {
                id,
                message: e.to_string(),
                code: protein::error_code(&e),
            }],
        }
    }

    pub fn has_ephemeral_subscriptions(&self) -> bool {
        self.subscriptions.values().any(protein::is_ephemeral)
    }

    pub async fn tick_ephemeral(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        let ephemeral: Vec<(String, Protein)> = self
            .subscriptions
            .iter()
            .filter(|(_, protein)| protein::is_ephemeral(protein))
            .map(|(id, protein)| (id.clone(), protein.clone()))
            .collect();
        for (id, protein) in ephemeral {
            match self.execute(&protein).await {
                Ok(rows) => {
                    if self.last_ephemeral.get(&id) == Some(&rows) {
                        continue;
                    }
                    self.last_ephemeral.insert(id.clone(), rows.clone());
                    out.push(ServerMessage::Update { id, rows });
                }
                Err(e) => out.push(ServerMessage::Error {
                    id,
                    message: e.to_string(),
                    code: protein::error_code(&e),
                }),
            }
        }
        out
    }

    async fn subscribe_saved(&mut self, id: String, name: String) -> Vec<ServerMessage> {
        let protein = match load_saved(&self.engine, &name).await {
            Ok(protein) => protein,
            Err(e) => {
                return vec![ServerMessage::Error {
                    id,
                    message: e,
                    code: None,
                }];
            }
        };
        self.subscribe(id, protein).await
    }

    async fn act(&self, id: String, action: engine::actions::Action) -> ServerMessage {
        match self.engine.act(action, self.subject.clone()).await {
            Ok(outcome) => ServerMessage::ActionOk {
                id,
                created: outcome.created,
                facts: outcome.facts.len(),
                warnings: outcome.warnings,
                data: outcome.data,
            },
            Err(e) => {
                let code = e.code().map(str::to_string);
                ServerMessage::Error {
                    id,
                    message: e.to_string(),
                    code,
                }
            }
        }
    }

    async fn authenticate_action_intent(
        &mut self,
        id: String,
        proof: ActionIntentSessionProof,
    ) -> ServerMessage {
        if self.subject.is_none() {
            return ServerMessage::Error {
                id,
                message: "trusted local sessions do not register remote Action keys".into(),
                code: Some("action_intent_not_required".into()),
            };
        }
        let Some(session) = self.action_intent.as_mut() else {
            return self.action_intent_unavailable(id);
        };
        let key_id = proof.key_id.clone();
        match self
            .engine
            .authenticate_action_intent_session(session, proof)
            .await
        {
            Ok(()) => ServerMessage::SessionAuthenticated {
                id,
                session_id: session.session_id().to_string(),
                person: session.person_uid().to_string(),
                key_id,
            },
            Err(error) => action_error(id, error),
        }
    }

    async fn signed_act(&mut self, intent: SignedActionIntent) -> ServerMessage {
        let id = intent.message_id.clone();
        if self.subject.is_none() {
            return ServerMessage::Error {
                id,
                message: "signed Action intents require an authenticated app user".into(),
                code: Some("action_intent_subject_required".into()),
            };
        }
        let Some(session) = self.action_intent.as_mut() else {
            return self.action_intent_unavailable(id);
        };
        let verified = match self.engine.verify_action_intent(session, intent).await {
            Ok(verified) => verified,
            Err(error) => return action_error(id, error),
        };
        match self.engine.act_verified_intent(verified).await {
            Ok(outcome) => ServerMessage::ActionOk {
                id,
                created: outcome.created,
                facts: outcome.facts.len(),
                warnings: outcome.warnings,
                data: outcome.data,
            },
            Err(error) => action_error(id, error),
        }
    }

    fn action_intent_unavailable(&self, id: String) -> ServerMessage {
        let (message, code) = self
            .action_intent_initialization_error
            .clone()
            .unwrap_or_else(|| {
                (
                    "signed Action intent session was not initialized".into(),
                    Some("action_intent_session_unavailable".into()),
                )
            });
        ServerMessage::Error { id, message, code }
    }

    async fn available_signer_actor(&self) -> Option<String> {
        if self.subject.is_some() {
            return self
                .action_intent
                .as_ref()
                .filter(|session| session.bound_key_id().is_some())
                .map(|session| session.person_uid().to_string());
        }
        self.engine.signer_actor_uid().await
    }

    pub async fn on_fact(&self, fact: &Fact) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        if self.collab_records.contains(&fact.record_uid) {
            if let Ok(snapshot_base64) = self.engine.collab_snapshot(&fact.record_uid).await {
                out.push(ServerMessage::CollabChange {
                    record_uid: fact.record_uid.clone(),
                    snapshot_base64,
                });
            }
        }
        for (id, protein) in &self.subscriptions {
            if !protein::affects(protein, fact) {
                continue;
            }
            match self.execute(protein).await {
                Ok(rows) => out.push(ServerMessage::Update {
                    id: id.clone(),
                    rows,
                }),
                Err(e) => out.push(ServerMessage::Error {
                    id: id.clone(),
                    message: e.to_string(),
                    code: protein::error_code(&e),
                }),
            }
        }
        out
    }
}

fn engine_error(error: &EngineError) -> (String, Option<String>) {
    (error.to_string(), error.code().map(str::to_string))
}

fn action_error(id: String, error: EngineError) -> ServerMessage {
    let (message, code) = engine_error(&error);
    ServerMessage::Error { id, message, code }
}

async fn load_saved(engine: &Engine, name: &str) -> Result<Protein, String> {
    let record = store::records::resolve(&engine.store.pool, name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no saved protein {name}"))?;
    let ast = store::records::get_extension(&engine.store.pool, &record.uid, "lince.protein")
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{name} has no protein AST"))?;
    serde_json::from_value(ast).map_err(|e| e.to_string())
}
