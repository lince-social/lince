//! Collaborative document membership, deltas, and cursor delivery.

use std::collections::{HashMap, HashSet};

use engine::{EngineError, performance::Span};
use nucleus::Fact;
use protein::authority::Property;
use tokio::sync::watch::Receiver;

use super::{Session, action_error, session_denied, session_expired};
use crate::protocol::{CollabCursor, ServerMessage};

const MAX_DOCUMENTS: usize = 64;
const MAX_CURSOR_BYTES: usize = 2048;

#[derive(Default)]
pub(super) struct Collaboration {
    records: HashSet<String>,
    versions: HashMap<String, String>,
    last_cursors: HashMap<String, Vec<CollabCursor>>,
}

impl Session {
    pub(super) fn has_joined_documents(&self) -> bool {
        !self.collaboration.records.is_empty()
    }

    pub(super) async fn join_document(
        &mut self,
        id: String,
        record_uid: String,
    ) -> Vec<ServerMessage> {
        if self.collaboration.records.len() >= MAX_DOCUMENTS
            && !self.collaboration.records.contains(&record_uid)
        {
            return vec![session_denied("document limit")];
        }
        if !self.may_read_document(&record_uid).await {
            return vec![not_visible(id)];
        }
        match self.engine.collab_state(&record_uid).await {
            Ok((snapshot_base64, version)) => {
                self.collaboration.records.insert(record_uid.clone());
                self.engine.presence.join(&record_uid, &self.connection_id);
                self.collaboration
                    .versions
                    .insert(record_uid.clone(), version.clone());
                let writable = self
                    .engine
                    .record_text_permissions(self.subject.as_deref(), &record_uid)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|property| match property {
                        Property::Head => Some("head".into()),
                        Property::Body => Some("body".into()),
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
            Err(error) => vec![action_error(id, error)],
        }
    }

    pub(super) fn leave_document(&mut self, record_uid: &str) -> Vec<ServerMessage> {
        self.engine
            .presence
            .leave_cursor(Some(record_uid), &self.connection_id);
        self.collaboration.last_cursors.remove(record_uid);
        self.collaboration.records.remove(record_uid);
        self.collaboration.versions.remove(record_uid);
        vec![]
    }

    pub(super) async fn publish_cursor(
        &self,
        record_uid: &str,
        property: String,
        anchor: String,
        focus: String,
    ) -> Vec<ServerMessage> {
        if self.collaboration.records.contains(record_uid)
            && matches!(property.as_str(), "head" | "body")
            && anchor.len() <= MAX_CURSOR_BYTES
            && focus.len() <= MAX_CURSOR_BYTES
            && self.may_read_document(record_uid).await
        {
            self.engine.presence.cursor(
                record_uid,
                CollabCursor {
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

    pub(super) async fn update_document(
        &self,
        id: String,
        record_uid: String,
        update_base64: &str,
    ) -> Vec<ServerMessage> {
        if self
            .engine
            .require_permission(self.subject.as_deref(), "record:update")
            .await
            .is_err()
        {
            return vec![session_denied("record:update")];
        }
        if !self.may_read_document(&record_uid).await {
            return vec![not_visible(id)];
        }
        match self
            .engine
            .apply_client_crdt_update_as(&record_uid, update_base64, self.subject.as_deref())
            .await
        {
            Ok(()) => vec![ServerMessage::CollabAck { id, record_uid }],
            Err(error) => vec![action_error(id, error)],
        }
    }

    /// Watches changes to live collaborative cursors.
    pub fn presence_changes(&self) -> Receiver<u64> {
        self.engine.presence.changes()
    }

    pub(super) async fn refresh_documents(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        let _collaboration = Span::new("cell-refresh-collaboration-await");
        let records: Vec<_> = self.collaboration.records.iter().cloned().collect();
        for record_uid in records {
            if self.may_read_document(&record_uid).await {
                match self.collab_delta(&record_uid).await {
                    Ok(Some(message)) => out.push(message),
                    Ok(None) => {}
                    Err(error) => out.push(action_error(record_uid, error)),
                }
            } else {
                self.collaboration.records.remove(&record_uid);
                self.collaboration.versions.remove(&record_uid);
                self.engine
                    .presence
                    .leave_cursor(Some(&record_uid), &self.connection_id);
                out.push(ServerMessage::Error {
                    id: record_uid,
                    message: "Record access was removed".into(),
                    code: Some("collab_not_visible".into()),
                });
            }
        }
        out
    }

    pub(super) async fn tick_cursors(&mut self) -> Vec<ServerMessage> {
        let _refresh = Span::new("cell-cursor-refresh-await");
        if !self.subject_may_act().await {
            self.clear_subscription_rows();
            self.engine.presence.leave_cursor(None, &self.connection_id);
            return vec![session_expired()];
        }
        let mut out = Vec::new();
        let records: Vec<_> = self.collaboration.records.iter().cloned().collect();
        let mut prepared = self
            .engine
            .presence
            .cursors_for(records.iter().map(String::as_str));
        for uid in &records {
            if !self.may_read_document(uid).await {
                self.engine
                    .presence
                    .leave_cursor(Some(uid), &self.connection_id);
                self.collaboration.records.remove(uid);
                self.collaboration.versions.remove(uid);
                self.collaboration.last_cursors.remove(uid);
                out.push(ServerMessage::CollabCursors {
                    record_uid: uid.clone(),
                    cursors: Vec::new(),
                });
                continue;
            }
            let mut cursors = Vec::new();
            for cursor in prepared.remove(uid.as_str()).unwrap_or_default() {
                if cursor.session != self.connection_id
                    && (cursor.organ.is_some()
                        || self
                            .engine
                            .may_read_record(cursor.person.as_deref(), uid)
                            .await
                            .unwrap_or(false))
                {
                    cursors.push(cursor);
                }
            }
            if self.collaboration.last_cursors.get(uid) != Some(&cursors) {
                self.collaboration
                    .last_cursors
                    .insert(uid.clone(), cursors.clone());
                out.push(ServerMessage::CollabCursors {
                    record_uid: uid.clone(),
                    cursors,
                });
            }
        }
        out
    }

    pub(super) async fn document_changes(&mut self, facts: &[Fact]) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        let _collaboration = Span::new("cell-refresh-collaboration-await");
        let mut refreshed = HashSet::new();
        for fact in facts {
            if !self.collaboration.records.contains(&fact.record_uid)
                || (facts.len() > 1 && !refreshed.insert(fact.record_uid.as_str()))
            {
                continue;
            }
            if self.may_read_document(&fact.record_uid).await
                && let Ok(Some(message)) = self.collab_delta(&fact.record_uid).await
            {
                out.push(message);
            }
        }
        out
    }

    async fn collab_delta(&mut self, uid: &str) -> Result<Option<ServerMessage>, EngineError> {
        let Some(previous) = self.collaboration.versions.get_mut(uid) else {
            return Ok(None);
        };
        let Some((update_base64, version)) = self.engine.collab_since(uid, previous).await? else {
            return Ok(None);
        };
        previous.clone_from(&version);
        Ok(Some(ServerMessage::CollabChange {
            record_uid: uid.into(),
            update_base64,
            version,
        }))
    }

    async fn may_read_document(&self, uid: &str) -> bool {
        self.engine
            .may_read_record(self.subject.as_deref(), uid)
            .await
            .unwrap_or_default()
    }
}

fn not_visible(id: String) -> ServerMessage {
    ServerMessage::Error {
        id,
        message: "record is not visible to you".into(),
        code: Some("collab_not_visible".into()),
    }
}
