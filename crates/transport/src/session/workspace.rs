//! Hosted workspace subscriptions and their presence lifecycle.

use engine::{EngineError, workspace_sync::Client};
use serde_json::{Value, json};

use super::{Session, action_error, session_denied, subscriptions::MAX_SUBSCRIPTION_ID_BYTES};
use crate::ServerMessage;

const MAX_WORKSPACE_SUBSCRIPTIONS: usize = 8;

pub(super) struct WorkspaceSubscription {
    workspace: String,
    client: Client,
}

impl Session {
    pub(super) async fn subscribe_workspace(
        &mut self,
        id: String,
        workspace: String,
        client: Client,
    ) -> Vec<ServerMessage> {
        if id.len() > MAX_SUBSCRIPTION_ID_BYTES
            || self.subscriptions.contains_key(&id)
            || (self.workspace_subscriptions.len() >= MAX_WORKSPACE_SUBSCRIPTIONS
                && !self.workspace_subscriptions.contains_key(&id))
        {
            return vec![session_denied("Workspace subscription limit")];
        }
        let snapshot = match self.workspace_view(&workspace, &client).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.unsubscribe_workspace(&id);
                return vec![action_error(id, error)];
            }
        };
        if let Err(error) = self.join_workspace(&workspace) {
            return vec![action_error(id, error)];
        }
        if let Some(old) = self.workspace_subscriptions.get(&id)
            && old.workspace != workspace
        {
            self.leave_unused_workspace(&old.workspace, Some(&id));
        }
        let message = self.workspace_message(id.clone(), &workspace, snapshot);
        self.workspace_subscriptions
            .insert(id, WorkspaceSubscription { workspace, client });
        vec![message]
    }

    pub(super) async fn refresh_workspaces(&mut self) -> Vec<ServerMessage> {
        let ids: Vec<_> = self.workspace_subscriptions.keys().cloned().collect();
        let mut messages = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(subscription) = self.workspace_subscriptions.get(&id) else {
                continue;
            };
            let message = match self
                .workspace_view(&subscription.workspace, &subscription.client)
                .await
            {
                Ok(snapshot) => match self.join_workspace(&subscription.workspace) {
                    Ok(()) => self.workspace_message(id, &subscription.workspace, snapshot),
                    Err(error) => action_error(id, error),
                },
                Err(error) => {
                    self.unsubscribe_workspace(&id);
                    action_error(id, error)
                }
            };
            messages.push(message);
        }
        messages
    }

    pub(super) fn unsubscribe_workspace(&mut self, id: &str) {
        if let Some(old) = self.workspace_subscriptions.remove(id) {
            self.leave_unused_workspace(&old.workspace, None);
        }
    }

    pub(super) fn clear_workspace_subscriptions(&mut self) {
        self.workspace_subscriptions.clear();
        self.leave_workspaces();
    }

    pub(super) fn leave_workspaces(&self) {
        if self
            .engine
            .workspace_presence
            .leave(None, &self.workspace_participant)
        {
            self.engine.notify_query_changed();
        }
    }

    fn leave_unused_workspace(&self, workspace: &str, except: Option<&str>) {
        if !self
            .workspace_subscriptions
            .iter()
            .any(|(id, subscription)| {
                Some(id.as_str()) != except && subscription.workspace == workspace
            })
            && self
                .engine
                .workspace_presence
                .leave(Some(workspace), &self.workspace_participant)
        {
            self.engine.notify_query_changed();
        }
    }

    async fn workspace_view(&self, workspace: &str, client: &Client) -> Result<Value, EngineError> {
        self.engine
            .workspace_view(self.subject.as_deref(), workspace, client, false)
            .await
    }

    fn join_workspace(&self, workspace: &str) -> Result<(), EngineError> {
        if self
            .engine
            .workspace_presence
            .join(workspace, &self.workspace_participant)?
        {
            self.engine.notify_query_changed();
        }
        Ok(())
    }

    fn workspace_message(&self, id: String, workspace: &str, snapshot: Value) -> ServerMessage {
        let Value::Object(mut view) = snapshot else {
            return action_error(
                id,
                EngineError::Consequence("Workspace view is not an object".into()),
            );
        };
        view.insert(
            "participants".into(),
            json!(self.engine.workspace_presence.participants(workspace)),
        );
        ServerMessage::Workspace {
            id,
            workspace: Value::Object(view),
        }
    }
}
