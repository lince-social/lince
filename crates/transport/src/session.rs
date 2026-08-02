//! The session state machine (blueprint VII.3). Transport-agnostic: a real
//! socket driver calls `handle` for inbound client messages and `on_fact` for
//! each fact off the engine's `fact_bus`, forwarding every returned
//! `ServerMessage` to the client. No socket is needed to test it.

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
    /// This connection's identity — used as the Protein visibility subject
    /// (None = the local Cell, sees everything) and the lane `from` label.
    subject: Option<String>,
    connection_id: String,
    /// Active subscriptions: subscription id -> the Protein to re-run.
    subscriptions: HashMap<String, Protein>,
    joined_rooms: Vec<String>,
    /// Records this connection collab-edits: a committed fact touching one of
    /// them pushes a fresh `CollabChange` snapshot (Ontology §11 "Collab").
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

    /// Initialize remote Action authentication and return the connection's
    /// first application frame. The mapped Person comes only from the engine's
    /// authenticated app_user lookup. Local Cell sessions remain an explicit
    /// trusted mode and may continue to use raw `Act` frames.
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

    /// Handle one inbound client message, producing the responses to send back.
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
                vec![] // the driver wires the room receiver into the outbound loop
            }
            ClientMessage::LaneLeave { room } => {
                self.joined_rooms.retain(|r| r != &room);
                self.hub.prune(&room);
                vec![]
            }
            ClientMessage::LaneSend { room, payload } => {
                if self.joined_rooms.contains(&room) {
                    self.hub.send(LaneEvent {
                        room,
                        from: self.connection_id.clone(),
                        payload,
                    });
                }
                vec![] // presence is fire-and-forget; senders don't echo to self
            }
            ClientMessage::CollabJoin { id, record_uid } => {
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
                // Success answers nothing here: the merge commits a refresh
                // fact, and `on_fact` echoes the merged doc back as a
                // `CollabChange` to every joined session (including this one —
                // Loro dedupes by version vector, so the echo is harmless).
                match self
                    .engine
                    .apply_client_crdt_update(&record_uid, &update_base64)
                    .await
                {
                    Ok(()) => vec![],
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
        }
    }

    async fn subscribe(&mut self, id: String, protein: Protein) -> Vec<ServerMessage> {
        let signer_actor = self.available_signer_actor().await;
        match protein::execute_for_with_signer(
            &self.engine.store,
            &protein,
            self.subject.as_deref(),
            signer_actor.as_deref(),
        )
        .await
        {
            Ok(rows) => {
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

    async fn subscribe_saved(&mut self, id: String, name: String) -> Vec<ServerMessage> {
        let signer_actor = self.available_signer_actor().await;
        match protein::execute_saved_with_signer(
            &self.engine.store,
            &name,
            self.subject.as_deref(),
            signer_actor.as_deref(),
        )
        .await
        {
            Ok(rows) => {
                // materialize the saved AST so future `on_fact` recomputes it
                match load_saved(&self.engine, &name).await {
                    Ok(protein) => {
                        self.subscriptions.insert(id.clone(), protein);
                    }
                    Err(e) => {
                        return vec![ServerMessage::Error {
                            id,
                            message: e,
                            code: None,
                        }];
                    }
                }
                vec![ServerMessage::Snapshot { id, rows }]
            }
            Err(e) => vec![ServerMessage::Error {
                id,
                message: e.to_string(),
                code: protein::error_code(&e),
            }],
        }
    }

    async fn act(&self, id: String, action: engine::actions::Action) -> ServerMessage {
        match self.engine.act(action, self.subject.clone()).await {
            Ok(outcome) => ServerMessage::ActionOk {
                id,
                created: outcome.created,
                facts: outcome.facts.len(),
                warnings: outcome.warnings,
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

    /// A fact was committed (off `fact_bus`): push an Update for every
    /// subscription it may have changed (blueprint VII.1 live subscriptions).
    /// v1 invalidation is coarse via `protein::affects`; re-execution is a full
    /// re-send, correct if not yet minimal.
    pub async fn on_fact(&self, fact: &Fact) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        // Live collab: any fact touching a joined record means its record-doc
        // may have changed (this client's own update, a sibling session, or a
        // peer Organ's sync import — all commit a refresh fact). Push the
        // merged doc; client-side Loro imports dedupe by version vector.
        if self.collab_records.contains(&fact.record_uid) {
            if let Ok(snapshot_base64) = self.engine.collab_snapshot(&fact.record_uid).await {
                out.push(ServerMessage::CollabChange {
                    record_uid: fact.record_uid.clone(),
                    snapshot_base64,
                });
            }
        }
        let signer_actor = self.available_signer_actor().await;
        for (id, protein) in &self.subscriptions {
            if !protein::affects(protein, fact) {
                continue;
            }
            match protein::execute_for_with_signer(
                &self.engine.store,
                protein,
                self.subject.as_deref(),
                signer_actor.as_deref(),
            )
            .await
            {
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
