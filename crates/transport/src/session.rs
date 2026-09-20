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
    login: Option<engine::login::LoginSession>,
    connection_id: String,
    subscriptions: HashMap<String, Protein>,
    last_ephemeral: HashMap<String, Vec<serde_json::Value>>,
    joined_rooms: Vec<String>,
    collab_records: HashSet<String>,
    collab_versions: std::sync::Mutex<HashMap<String, String>>,
    last_cursors: HashMap<String, Vec<crate::protocol::CollabCursor>>,
    action_intent: Option<ActionIntentSession>,
    action_intent_initialization_error: Option<(String, Option<String>)>,
    action_intent_initialized: bool,
    local_sync: bool,
    fiote: Option<Arc<dyn crate::fiote::Service>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.engine.presence.leave_cursor(None, &self.connection_id);
    }
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
            login: None,
            connection_id: connection_id.into(),
            subscriptions: HashMap::new(),
            last_ephemeral: HashMap::new(),
            joined_rooms: Vec::new(),
            collab_records: HashSet::new(),
            collab_versions: std::sync::Mutex::new(HashMap::new()),
            last_cursors: HashMap::new(),
            action_intent: None,
            action_intent_initialization_error: None,
            action_intent_initialized: false,
            local_sync: false,
            fiote: None,
        }
    }

    pub fn local(engine: Arc<Engine>, hub: Arc<LaneHub>, connection_id: impl Into<String>) -> Self {
        let mut session = Self::new(engine, hub, connection_id, None);
        session.local_sync = true;
        session
    }

    pub fn authenticated(
        engine: Arc<Engine>,
        hub: Arc<LaneHub>,
        connection_id: impl Into<String>,
        login: engine::login::LoginSession,
    ) -> Self {
        let mut session = Self::new(engine, hub, connection_id, Some(login.person_uid().into()));
        session.login = Some(login);
        session
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn into_native_tools(self, context: crate::native::Context) -> crate::native::NativeTools {
        crate::native::NativeTools::new(self.engine.clone(), self, context)
    }

    pub fn with_fiote(mut self, service: Arc<dyn crate::fiote::Service>) -> Self {
        self.fiote = Some(service);
        self
    }

    pub fn joined_rooms(&self) -> &[String] {
        &self.joined_rooms
    }

    pub async fn subject_may_act(&self) -> bool {
        if let Some(login) = &self.login {
            return login.require(&self.engine).await.is_ok();
        }
        let Some(subject) = self.subject.as_deref() else {
            return true;
        };
        store::people::is_active(&self.engine.store.pool, subject)
            .await
            .unwrap_or(false)
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
        if !self.subject_may_act().await {
            return vec![session_expired()];
        }
        if let Some(login) = &self.login {
            login.touch();
        }
        if self.local_sync && self.login.is_none() && matches!(msg, ClientMessage::Fiote { .. }) {
            return self.handle_inner(msg).await;
        }
        let engine = self.engine.clone();
        let login = self.login.clone();
        let write = matches!(
            msg,
            ClientMessage::Act { .. }
                | ClientMessage::SignedAct { .. }
                | ClientMessage::CollabUpdate { .. }
                | ClientMessage::SessionAuthenticate { .. }
                | ClientMessage::Fiote { .. }
        );
        let work = Box::pin(self.handle_inner(msg));
        let operation = async { Ok(work.await) };
        let result = if let Some(login) = login {
            login.run(&engine, write, operation).await
        } else {
            engine.access_scope(write, operation).await
        };
        result.unwrap_or_else(|_| vec![session_expired()])
    }

    async fn handle_inner(&mut self, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::Fiote { id, request } => {
                let result = match &self.fiote {
                    Some(service) if self.local_sync => service.handle(request).await,
                    _ => Err("Fiote settings and execution are available only on the local Cell.".into()),
                };
                vec![match result {
                    Ok(status) => ServerMessage::Fiote { id, status },
                    Err(message) => ServerMessage::Error {
                        id,
                        message,
                        code: Some("fiote".into()),
                    },
                }]
            }
            ClientMessage::SyncInspect { id, before } => self.sync_status(id, before).await,
            ClientMessage::SyncHistoryPolicy { id, retention } => {
                if !self.local_sync {
                    return vec![sync_denied(id)];
                }
                match store::sync_activity::set_retention(
                    &self.engine.store.pool,
                    retention,
                    sync_now(),
                )
                .await
                {
                    Ok(()) => self.sync_status(id, None).await,
                    Err(error) => vec![ServerMessage::Error {
                        id,
                        message: error.to_string(),
                        code: Some("sync_history_policy".into()),
                    }],
                }
            }
            ClientMessage::SyncForgetHistory { id } => {
                if !self.local_sync {
                    return vec![sync_denied(id)];
                }
                match store::sync_activity::clear(&self.engine.store.pool).await {
                    Ok(()) => self.sync_status(id, None).await,
                    Err(error) => vec![ServerMessage::Error {
                        id,
                        message: error.to_string(),
                        code: Some("sync_history_clear".into()),
                    }],
                }
            }
            ClientMessage::Subscribe { id, protein } => self.subscribe(id, protein).await,
            ClientMessage::SubscribeSaved { id, name } => self.subscribe_saved(id, name).await,
            ClientMessage::Unsubscribe { id } => {
                self.subscriptions.remove(&id);
                self.last_ephemeral.remove(&id);
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
                if room.len() > 512
                    || (self.joined_rooms.len() >= 64 && !self.joined_rooms.contains(&room))
                {
                    return vec![session_denied("stream limit")];
                }
                if !self.may_use_lane().await {
                    return vec![session_denied("view:stream")];
                }
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
                if !self.may_use_lane().await {
                    return vec![session_denied("view:stream")];
                }
                if self.joined_rooms.contains(&room) {
                    self.hub.send(LaneEvent {
                        room,
                        from: self.connection_id.clone(),
                        payload,
                        from_subject: self.subject.clone(),
                        organ: self
                            .login
                            .as_ref()
                            .map(|login| login.organ_uid().to_string())
                            .or(organ.filter(|_| self.subject.is_none())),
                    });
                }
                vec![]
            }
            ClientMessage::CollabJoin { id, record_uid } => {
                if self.collab_records.len() >= 64 && !self.collab_records.contains(&record_uid) {
                    return vec![session_denied("document limit")];
                }
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
                match self.engine.collab_state(&record_uid).await {
                    Ok((snapshot_base64, version)) => {
                        self.collab_records.insert(record_uid.clone());
                        self.engine.presence.join(&record_uid, &self.connection_id);
                        self.collab_versions
                            .lock()
                            .expect("document versions")
                            .insert(record_uid.clone(), version.clone());
                        let writable = self
                            .engine
                            .record_text_permissions(self.subject.as_deref(), &record_uid)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|property| match property {
                                protein::authority::Property::Head => Some("head".into()),
                                protein::authority::Property::Body => Some("body".into()),
                                _ => None,
                            })
                            .collect();
                        vec![ServerMessage::CollabState {
                            id,
                            record_uid,
                            snapshot_base64,
                            version,
                            writable,
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
                self.engine.presence
                    .leave_cursor(Some(&record_uid), &self.connection_id);
                self.last_cursors.remove(&record_uid);
                self.collab_records.remove(&record_uid);
                self.collab_versions
                    .lock()
                    .expect("document versions")
                    .remove(&record_uid);
                vec![]
            }
            ClientMessage::CollabPresence {
                record_uid,
                property,
                anchor,
                focus,
            } => {
                if self.collab_records.contains(&record_uid)
                    && matches!(property.as_str(), "head" | "body")
                    && anchor.len() <= 2048
                    && focus.len() <= 2048
                    && self
                        .engine
                        .may_read_record(self.subject.as_deref(), &record_uid)
                        .await
                        .unwrap_or(false)
                {
                    self.engine.presence.cursor(
                        &record_uid,
                        crate::protocol::CollabCursor {
                            session: self.connection_id.clone(),
                            organ: None,
                            person: self.subject.clone(),
                            property,
                            anchor,
                            focus,
                        },
                    );
                }
                vec![]
            }
            ClientMessage::CollabUpdate {
                id,
                record_uid,
                update_base64,
            } => {
                if self
                    .engine
                    .require_permission(self.subject.as_deref(), "record:update")
                    .await
                    .is_err()
                {
                    return vec![session_denied("record:update")];
                }
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
                    .apply_client_crdt_update_as(
                        &record_uid,
                        &update_base64,
                        self.subject.as_deref(),
                    )
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

    async fn sync_status(&self, id: String, before: Option<i64>) -> Vec<ServerMessage> {
        if !self.local_sync {
            return vec![sync_denied(id)];
        }
        match self.engine.sync_overview(before).await {
            Ok(overview) => vec![ServerMessage::SyncStatus { id, overview }],
            Err(error) => vec![action_error(id, error)],
        }
    }

    pub async fn on_sync_event(&mut self, event: crate::SyncEvent) -> Vec<ServerMessage> {
        if !self.subject_may_act().await {
            return vec![session_expired()];
        }
        let engine = self.engine.clone();
        let login = self.login.clone();
        let operation = async { Ok(self.sync_event_inner(event).await) };
        let result = if let Some(login) = login {
            login.run(&engine, false, operation).await
        } else {
            engine.access_scope(false, operation).await
        };
        result.unwrap_or_else(|_| vec![session_expired()])
    }

    async fn sync_event_inner(&mut self, event: crate::SyncEvent) -> Vec<ServerMessage> {
        match event {
            crate::SyncEvent::Fact(fact) => self.on_fact(&fact).await,
            crate::SyncEvent::Refresh => self.refresh().await,
            crate::SyncEvent::Ephemeral => self.tick_ephemeral().await,
            crate::SyncEvent::Presence => self.tick_cursors().await,
        }
    }

    async fn execute_sync(
        &self,
        id: &str,
        protein: &Protein,
    ) -> Result<Vec<serde_json::Value>, protein::ProteinError> {
        use nucleus::sync::{Activity, Direction, Instance, Outcome, Summary, Update};
        self.engine
            .sync_service
            .run(
                &self.engine.store,
                Activity {
                    instance: Instance::interface(&self.connection_id, id),
                    direction: Direction::Outgoing,
                    update: Update::Full,
                },
                self.execute(protein),
                |rows| {
                    let mut summary = Summary::new(Outcome::Refreshed, rows.len());
                    summary.subjects = rows
                        .iter()
                        .filter_map(|row| row["uid"].as_str().map(str::to_owned))
                        .take(32)
                        .collect();
                    summary
                },
            )
            .await
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
        if id.len() > 512
            || (self.subscriptions.len() >= 64 && !self.subscriptions.contains_key(&id))
        {
            return vec![session_denied("subscription limit")];
        }
        match self.execute_sync(&id, &protein).await {
            Ok(rows) => {
                if protein::is_ephemeral(&protein) {
                    self.last_ephemeral.insert(id.clone(), rows.clone());
                } else {
                    self.last_ephemeral.remove(&id);
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
        !self.collab_records.is_empty() || self.subscriptions.values().any(protein::is_ephemeral)
    }

    pub fn presence_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.engine.presence.changes()
    }

    pub async fn refresh(&mut self) -> Vec<ServerMessage> {
        if !self.subject_may_act().await {
            return vec![session_expired()];
        }
        let mut out = Vec::new();
        for (id, protein) in self.subscriptions.clone() {
            out.extend(self.subscribe(id, protein).await);
        }
        for record_uid in self.collab_records.clone() {
            if self
                .engine
                .may_read_record(self.subject.as_deref(), &record_uid)
                .await
                .unwrap_or(false)
            {
                match self.collab_delta(&record_uid).await {
                    Ok(Some(message)) => out.push(message),
                    Ok(None) => {}
                    Err(error) => out.push(action_error(record_uid.clone(), error)),
                }
            } else {
                self.collab_records.remove(&record_uid);
                self.engine.presence.leave_cursor(Some(&record_uid), &self.connection_id);
                out.push(ServerMessage::Error {
                    id: record_uid,
                    message: "Record access was removed".into(),
                    code: Some("collab_not_visible".into()),
                });
            }
        }
        out
    }

    async fn tick_cursors(&mut self) -> Vec<ServerMessage> {
        if !self.subject_may_act().await {
            self.engine.presence.leave_cursor(None, &self.connection_id);
            return vec![session_expired()];
        }
        let mut out = Vec::new();
        for uid in self.collab_records.clone() {
            if !self
                .engine
                .may_read_record(self.subject.as_deref(), &uid)
                .await
                .unwrap_or(false)
            {
                self.engine.presence.leave_cursor(Some(&uid), &self.connection_id);
                self.collab_records.remove(&uid);
                self.last_cursors.remove(&uid);
                out.push(ServerMessage::CollabCursors {
                    record_uid: uid,
                    cursors: Vec::new(),
                });
                continue;
            }
            let mut cursors = Vec::new();
            for cursor in self.engine.presence.cursors(&uid) {
                if cursor.session != self.connection_id
                    && (cursor.organ.is_some() || self.engine.may_read_record(cursor.person.as_deref(), &uid).await.unwrap_or(false))
                {
                    cursors.push(cursor);
                }
            }
            if self.last_cursors.get(&uid) != Some(&cursors) {
                self.last_cursors.insert(uid.clone(), cursors.clone());
                out.push(ServerMessage::CollabCursors {
                    record_uid: uid,
                    cursors,
                });
            }
        }
        out
    }

    pub async fn tick_ephemeral(&mut self) -> Vec<ServerMessage> {
        let mut out = self.tick_cursors().await;
        if !self.subject_may_act().await {
            return out;
        }
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
        if self.local_sync
            && let Some(service) = &self.fiote
            && let engine::actions::Action::CreateMessage {
                thread, body, author: None, state: nucleus::MessageState::Finished,
                parent: None, references,
            } = &action
            && references.is_empty()
        {
            match service.send(thread, body).await {
                Ok(Some(outcome)) => return ServerMessage::ActionOk {
                    id,
                    created: outcome.created,
                    facts: outcome.facts.len(),
                    warnings: outcome.warnings,
                    data: outcome.data,
                },
                Ok(None) => {},
                Err(message) => return ServerMessage::Error {
                    id,
                    message,
                    code: Some("fiote".into()),
                },
            }
        }
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
        if !self.subject_may_act().await {
            return vec![session_expired()];
        }
        let mut out = Vec::new();
        if self.collab_records.contains(&fact.record_uid)
            && self
                .engine
                .may_read_record(self.subject.as_deref(), &fact.record_uid)
                .await
                .unwrap_or(false)
        {
            if let Ok(Some(message)) = self.collab_delta(&fact.record_uid).await {
                out.push(message);
            }
        }
        for (id, protein) in &self.subscriptions {
            if !protein::affects(protein, fact) {
                continue;
            }
            match self.execute_sync(id, protein).await {
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

    async fn collab_delta(&self, uid: &str) -> Result<Option<ServerMessage>, EngineError> {
        let version = self
            .collab_versions
            .lock()
            .expect("document versions")
            .get(uid)
            .cloned();
        let Some(version) = version else {
            return Ok(None);
        };
        let Some((update_base64, version)) = self.engine.collab_since(uid, &version).await? else {
            return Ok(None);
        };
        self.collab_versions
            .lock()
            .expect("document versions")
            .insert(uid.into(), version.clone());
        Ok(Some(ServerMessage::CollabChange {
            record_uid: uid.into(),
            update_base64,
            version,
        }))
    }

    pub async fn may_use_lane(&self) -> bool {
        self.subject_may_act().await
            && self
                .engine
                .require_permission(self.subject.as_deref(), "view:stream")
                .await
                .is_ok()
    }
}

fn session_expired() -> ServerMessage {
    ServerMessage::Error {
        id: "-".into(),
        message: "Please log in again".into(),
        code: Some("session_expired".into()),
    }
}

fn session_denied(permission: &str) -> ServerMessage {
    ServerMessage::Error {
        id: "-".into(),
        message: format!("Missing {permission} permission"),
        code: Some("forbidden".into()),
    }
}

fn engine_error(error: &EngineError) -> (String, Option<String>) {
    (error.to_string(), error.code().map(str::to_string))
}

fn sync_denied(id: String) -> ServerMessage {
    ServerMessage::Error {
        id,
        message: "Sync administration is available in the local interface".into(),
        code: Some("sync_local_only".into()),
    }
}

fn sync_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
