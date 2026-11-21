// non-lean-yet
// Scope: struct Subscription, struct QueryResult, enum SubscriptionChange, struct Session.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 43, 139, 140,
//   142, 144, 145, 151, 233, 234, 246, 247, 366, 497, 499, 527, 532, 594, 596, 640, 641, 687, 688,
//   746, 793, 806, 824, 827, 828, 838, 922, 945, 956, 957, 1004, 1019, 1029, 1072, 1091, 1093,
//   1165, 1199, 1216, 1246, 1328.
// - [review: §3 Ownership] Check that these inputs can be borrowed without changing ownership
//   semantics. Sites subscribe_saved:name → &str (1114).
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 704, 799, 905.
// - [Clippy probe: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones.
//   Sites 43, 139, 140, 151, 233, 246, 687, 746, 793, 806, 824, 828, 945, 957.
// - [Clippy probe: clippy::collapsible_if] this `if` statement can be collapsed. Sites 1293.
// - [Clippy probe: clippy::expect_used] Check the stated invariant and handle reachable failures.
//   Sites 531, 571, 816, 1316, 1327.
// - [Clippy probe: clippy::indexing_slicing] Verify bounds or use checked access on fallible
//   inputs. Sites 921, 1416, 1419, 1420.
// - [Clippy probe: clippy::map_unwrap_or] called `map(<f>).unwrap_or_else(<g>)` on an `Option`
//   value. Sites 203, 208, 941.
// - [Clippy probe: clippy::needless_pass_by_value] Borrow the input after checking the caller and
//   framework contract (Clippy suggests &). Sites 1374.
// - [Clippy probe: clippy::rc_buffer] Check whether a shared slice or str suffices. Sites 15, 19,
//   24, 731.
// - [rustc probe: missing_debug_implementations] type does not implement `std::fmt::Debug`;
//   consider adding `#[derive(Debug)]` or a manual implementation. Sites 59.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   59, 88, 116, 122, 133, 137, 150, 154, 156, 165, 169, 182, 222, 680, 981, 986, 990, 1101, 1267,
//   1336.

use std::collections::HashMap;
use std::sync::Arc;

use engine::{Engine, EngineError};
use nucleus::Fact;
use nucleus::action_intent::ActionIntentSessionProof;
#[cfg(test)]
use protein::Protein;

mod actions;
mod collaboration;
mod subscriptions;
mod workspace;

use self::{
    actions::ActionIntents,
    collaboration::Collaboration,
    subscriptions::{Subscription, SubscriptionChange},
    workspace::WorkspaceSubscription,
};

use crate::lane::{LaneEvent, LaneHub};
use crate::protocol::{ClientMessage, ServerMessage};

pub struct Session {
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    subject: Option<String>,
    login: Option<engine::login::LoginSession>,
    connection_id: String,
    workspace_participant: String,
    subscriptions: HashMap<String, Subscription>,
    workspace_subscriptions: HashMap<String, WorkspaceSubscription>,
    joined_rooms: Vec<String>,
    collaboration: Collaboration,
    action_intent: ActionIntents,
    local_sync: bool,
    fiote: Option<Arc<dyn crate::fiote::Service>>,
    speech: Option<Arc<dyn crate::speech::Service>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.engine.presence.leave_cursor(None, &self.connection_id);
        self.leave_workspaces();
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
            workspace_participant: nucleus::new_uid("participant"),
            subscriptions: HashMap::new(),
            workspace_subscriptions: HashMap::new(),
            joined_rooms: Vec::new(),
            collaboration: Collaboration::default(),
            action_intent: ActionIntents::default(),
            local_sync: false,
            fiote: None,
            speech: None,
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

    pub fn fork_call(&self) -> Self {
        let mut session = Self::new(
            self.engine.clone(),
            self.hub.clone(),
            format!("call:{}", self.connection_id),
            self.subject.clone(),
        );
        session.speech = self.speech.clone();
        session.login = self.login.clone();
        session.local_sync = self.local_sync;
        session
    }

    pub fn into_native_tools(self, context: crate::native::Context) -> crate::native::NativeTools {
        crate::native::NativeTools::new(self.engine.clone(), self, context)
    }

    pub fn with_speech(mut self, service: Arc<dyn crate::speech::Service>) -> Self {
        self.speech = Some(service);
        self
    }

    pub fn with_fiote(mut self, service: Arc<dyn crate::fiote::Service>) -> Self {
        self.fiote = Some(service);
        self
    }

    pub(crate) fn local_authority(&self) -> bool {
        self.local_sync && self.subject.is_none()
    }

    pub fn joined_rooms(&self) -> &[String] {
        &self.joined_rooms
    }

    pub async fn subject_may_act(&self) -> bool {
        let _subject = engine::performance::Span::new("cell-subject-validation-await");
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

    pub async fn handle(&mut self, msg: ClientMessage) -> Vec<ServerMessage> {
        if !self.subject_may_act().await {
            self.clear_subscription_rows();
            self.clear_workspace_subscriptions();
            return vec![session_expired()];
        }
        if let Some(login) = &self.login {
            login.touch();
        }
        if msg.runs_karma_preview() {
            let engine = self.engine.clone();
            if let Some(login) = self.login.clone() {
                return login
                    .run_unlocked(&engine, async { Ok(self.handle_inner(msg).await) })
                    .await
                    .unwrap_or_else(|_| {
                        self.clear_subscription_rows();
                        vec![session_expired()]
                    });
            }
            return self.handle_inner(msg).await;
        }
        if self.local_sync
            && self.login.is_none()
            && matches!(
                msg,
                ClientMessage::Fiote { .. }
                    | ClientMessage::FioteTerminal { .. }
                    | ClientMessage::Speech { .. }
            )
        {
            return self.handle_inner(msg).await;
        }
        let engine = self.engine.clone();
        let login = self.login.clone();
        let write = match &msg {
            ClientMessage::Act { action, .. } => !action.is_read_only(),
            _ => matches!(
                &msg,
                ClientMessage::SignedAct { .. }
                    | ClientMessage::DescriptionAsset {
                        request: nucleus::description_asset::Request::Put { .. },
                        ..
                    }
                    | ClientMessage::CollabUpdate { .. }
                    | ClientMessage::SessionAuthenticate { .. }
                    | ClientMessage::Fiote { .. }
                    | ClientMessage::FioteTerminal { .. }
                    | ClientMessage::Speech { .. }
            ),
        };
        let work = self.handle_inner(msg);
        let operation = async { Ok(work.await) };
        let result = if let Some(login) = login {
            login.run(&engine, write, operation).await
        } else {
            engine.access_scope(write, operation).await
        };
        result.unwrap_or_else(|_| {
            self.clear_subscription_rows();
            vec![session_expired()]
        })
    }

    fn handle_inner(
        &mut self,
        msg: ClientMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<ServerMessage>> + Send + '_>> {
        if matches!(
            msg,
            ClientMessage::Act { .. } | ClientMessage::SignedAct { .. }
        ) {
            Box::pin(self.handle_action(msg))
        } else {
            Box::pin(self.handle_protocol(msg))
        }
    }

    async fn handle_protocol(&mut self, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::WorkspaceSubscribe {
                id,
                workspace,
                client,
            } => self.subscribe_workspace(id, workspace, client).await,
            ClientMessage::DescriptionAsset { id, request } => {
                vec![match self
                    .engine
                    .description_asset(self.subject.as_deref(), request)
                    .await
                {
                    Ok(response) => ServerMessage::DescriptionAsset { id, response },
                    Err(error) => ServerMessage::Error {
                        id,
                        message: error.to_string(),
                        code: Some("description_asset".into()),
                    },
                }]
            }
            ClientMessage::CallContext { id, thread } => {
                vec![match self
                    .engine
                    .call_context(&thread, self.subject.as_deref())
                    .await
                {
                    Ok(context) => ServerMessage::CallContext { id, context },
                    Err(error) => ServerMessage::Error {
                        id,
                        message: error.to_string(),
                        code: Some("call".into()),
                    },
                }]
            }
            ClientMessage::Call {
                id,
                thread,
                person,
                operation,
            } => {
                vec![match self
                    .engine
                    .call_for_session(
                        thread,
                        person,
                        self.subject.as_deref(),
                        &self.connection_id,
                        operation,
                    )
                    .await
                {
                    Ok(snapshot) => ServerMessage::Call {
                        id,
                        device: self.connection_id.clone(),
                        snapshot,
                    },
                    Err(error) => ServerMessage::Error {
                        id,
                        message: error.to_string(),
                        code: Some("call".into()),
                    },
                }]
            }
            ClientMessage::Speech { id, request } => {
                let result = match &self.speech {
                    Some(service) if self.local_sync => service.handle(request).await,
                    _ => Err("Speech settings are available only on the local Cell.".into()),
                };
                vec![match result {
                    Ok(status) => ServerMessage::Speech { id, status },
                    Err(message) => ServerMessage::Error {
                        id,
                        message,
                        code: Some("speech".into()),
                    },
                }]
            }
            ClientMessage::FioteTerminal { id, request } => {
                let result = match &self.fiote {
                    Some(service) if self.local_sync => service.terminal(request).await,
                    _ => Err("Terminal login is available only on the local Cell.".into()),
                };
                vec![match result {
                    Ok(frame) => ServerMessage::FioteTerminal { id, frame },
                    Err(message) => ServerMessage::Error {
                        id,
                        message,
                        code: Some("fiote_terminal".into()),
                    },
                }]
            }
            ClientMessage::Fiote { id, request } => {
                let result = match &self.fiote {
                    Some(service) if self.local_sync => service.handle(request).await,
                    _ => Err(
                        "Fiote settings and execution are available only on the local Cell.".into(),
                    ),
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
                match self.engine.set_sync_history_policy(retention).await {
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
            ClientMessage::SubscribeSaved { id, name } => self.subscribe_saved(id, &name).await,
            ClientMessage::Unsubscribe { id } => {
                self.subscriptions.remove(&id);
                self.unsubscribe_workspace(&id);
                vec![]
            }
            message @ (ClientMessage::Act { .. } | ClientMessage::SignedAct { .. }) => {
                self.handle_action(message).await
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
                self.join_document(id, record_uid).await
            }
            ClientMessage::CollabLeave { record_uid } => self.leave_document(&record_uid),
            ClientMessage::CollabPresence {
                record_uid,
                property,
                anchor,
                focus,
            } => {
                self.publish_cursor(&record_uid, property, anchor, focus)
                    .await
            }
            ClientMessage::CollabUpdate {
                id,
                record_uid,
                update_base64,
            } => self.update_document(id, record_uid, &update_base64).await,
            ClientMessage::Command { id, .. }
            | ClientMessage::TerminalOpen { id, .. }
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
            self.clear_subscription_rows();
            self.clear_workspace_subscriptions();
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
        result.unwrap_or_else(|_| {
            self.clear_subscription_rows();
            vec![session_expired()]
        })
    }

    async fn sync_event_inner(&mut self, event: crate::SyncEvent) -> Vec<ServerMessage> {
        let (counter, count) = match &event {
            crate::SyncEvent::Fact(_) => ("cell-refresh-fact-events", 1),
            crate::SyncEvent::Facts(facts) => ("cell-refresh-fact-events", facts.len() as u64),
            crate::SyncEvent::Refresh => ("cell-refresh-global-events", 1),
            crate::SyncEvent::Projection => ("cell-refresh-projection-events", 1),
            crate::SyncEvent::Ephemeral => ("cell-refresh-ephemeral-events", 1),
            crate::SyncEvent::Presence => ("cell-refresh-presence-events", 1),
        };
        engine::performance::add(counter, count);
        match event {
            crate::SyncEvent::Fact(fact) => self.on_fact(&fact).await,
            crate::SyncEvent::Facts(facts) => {
                engine::performance::add("cell-refresh-fact-batches", 1);
                self.on_facts(&facts).await
            }
            crate::SyncEvent::Refresh => self.refresh_inner(SubscriptionChange::All, false).await,
            crate::SyncEvent::Projection => {
                self.refresh_inner(SubscriptionChange::Projection, false)
                    .await
            }
            crate::SyncEvent::Ephemeral => self.tick_ephemeral().await,
            crate::SyncEvent::Presence => self.tick_cursors().await,
        }
    }

    pub async fn refresh(&mut self) -> Vec<ServerMessage> {
        self.refresh_inner(SubscriptionChange::All, true).await
    }

    async fn refresh_inner(
        &mut self,
        change: SubscriptionChange<'_>,
        full_snapshot: bool,
    ) -> Vec<ServerMessage> {
        let _refresh = engine::performance::Span::new("cell-refresh-await");
        if !self.subject_may_act().await {
            self.clear_subscription_rows();
            self.clear_workspace_subscriptions();
            return vec![session_expired()];
        }
        let mut out = if matches!(change, SubscriptionChange::All) {
            self.refresh_workspaces().await
        } else {
            Vec::new()
        };
        for message in self.update_subscriptions(change, full_snapshot).await {
            if let ServerMessage::Update { id, rows } = message {
                out.push(ServerMessage::Snapshot { id, rows });
            } else {
                out.push(message);
            }
        }
        if matches!(change, SubscriptionChange::Projection) {
            return out;
        }
        out.extend(self.refresh_documents().await);
        out
    }

    pub async fn tick_ephemeral(&mut self) -> Vec<ServerMessage> {
        let mut out = self.tick_cursors().await;
        if !self.subject_may_act().await {
            self.clear_subscription_rows();
            return out;
        }
        out.extend(
            self.update_subscriptions(SubscriptionChange::Ephemeral, false)
                .await,
        );
        out
    }

    pub async fn on_fact(&mut self, fact: &Fact) -> Vec<ServerMessage> {
        self.on_facts(std::slice::from_ref(fact)).await
    }

    async fn on_facts(&mut self, facts: &[Fact]) -> Vec<ServerMessage> {
        let _refresh = engine::performance::Span::new("cell-refresh-await");
        if !self.subject_may_act().await {
            self.clear_subscription_rows();
            self.leave_workspaces();
            return vec![session_expired()];
        }
        let mut out = self.document_changes(facts).await;
        out.extend(
            self.update_subscriptions(SubscriptionChange::Facts(facts), false)
                .await,
        );
        out
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

fn action_error(id: String, error: EngineError) -> ServerMessage {
    let (message, code) = engine_error(&error);
    ServerMessage::Error { id, message, code }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscription_aliases_share_exact_retained_rows_and_release_them_on_unsubscribe() {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Private shared result".into(),
                    body: "Shared retained body ".repeat(4096),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let query: Protein = serde_json::from_value(serde_json::json!({
            "source": "record", "where": [{"uid_eq": uid}],
            "fields": ["uid", "head", "body"]
        }))
        .unwrap();
        let mut session = Session::local(engine, Arc::new(LaneHub::new()), "retention");
        for id in ["a", "b"] {
            let response = session
                .handle(ClientMessage::Subscribe {
                    id: id.into(),
                    protein: query.clone(),
                })
                .await;
            assert!(
                matches!(response.as_slice(), [ServerMessage::Snapshot { rows, .. }] if rows.len() == 1)
            );
        }
        let retained = Arc::downgrade(session.subscriptions["a"].rows.as_ref().unwrap());
        assert_eq!(retained.strong_count(), 2);
        assert!(Arc::ptr_eq(
            session.subscriptions["a"].rows.as_ref().unwrap(),
            session.subscriptions["b"].rows.as_ref().unwrap(),
        ));
        for _ in 0..3 {
            assert!(
                session
                    .on_sync_event(crate::SyncEvent::Refresh)
                    .await
                    .is_empty()
            );
            assert_eq!(retained.strong_count(), 2);
        }
        session
            .handle(ClientMessage::Unsubscribe { id: "a".into() })
            .await;
        assert_eq!(retained.strong_count(), 1);
        session
            .handle(ClientMessage::Unsubscribe { id: "b".into() })
            .await;
        assert!(retained.upgrade().is_none());
    }
}
