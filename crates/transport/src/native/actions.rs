//! Fiote action authorization, previewing, and receipt handling.

use std::sync::{Arc, atomic::Ordering};

use engine::{
    actions::Action,
    record_change::{Mutation, Request},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    NativeTool, State, bounded, error, json_argument,
    reads::{MAX_READS, insert_extension},
    receipts::{PreviewPermit, fingerprint},
    response,
};
use crate::ClientMessage;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionArguments {
    request_id: String,
    #[serde(deserialize_with = "json_argument")]
    pub(super) action: Action,
    read_ids: Vec<String>,
}

impl NativeTool {
    pub(super) async fn run_preview(
        &self,
        args: ActionArguments,
        arguments: Value,
    ) -> Result<Value, String> {
        let (mut session, mark) = {
            let mut state = self.native.state.lock().await;
            if state.closed
                || self.native.closed.load(Ordering::Acquire)
                || !state.session.subject_may_act().await
            {
                return Err("This agent connection is closed or its access was removed.".into());
            }
            if !args.read_ids.is_empty() {
                return Err(
                    "A proposal preview does not change live data. Use an empty read_ids list."
                        .into(),
                );
            }
            if let Some(previous) = state.receipts.previous(&args.request_id, &arguments)? {
                if let Some(source) = previous.pointer("/data/source").and_then(Value::as_str)
                    && self
                        .native
                        .engine
                        .store
                        .state_hash()
                        .await
                        .map_err(error)?
                        .as_str()
                        != source
                {
                    return Err(
                        "Source data changed. Request a fresh preview to check current access."
                            .into(),
                    );
                }
                return Ok(previous);
            }
            let mark = state.receipts.begin_preview(&args.request_id, &arguments)?;
            (state.session.fork_call(), mark)
        };
        let permit = PreviewPermit::new(&self.native.state, args.request_id.clone(), mark);
        let result = response(
            session
                .handle(ClientMessage::Act {
                    id: args.request_id.clone(),
                    action: args.action,
                })
                .await,
        )
        .and_then(bounded);
        let mut state = self.native.state.lock().await;
        state.receipts.end_preview(&args.request_id);
        let result = if state.closed
            || self.native.closed.load(Ordering::Acquire)
            || !state.session.subject_may_act().await
        {
            Err("This agent connection is closed or its access was removed.".into())
        } else {
            result
        };
        state
            .receipts
            .remember(args.request_id, mark, result.clone());
        permit.finish();
        result
    }
}

impl State {
    pub(super) async fn action(&mut self, arguments: &Value) -> Result<Value, String> {
        let args = ActionArguments::deserialize(arguments).map_err(error)?;
        if let Action::ReportFioteChild { parent, .. } = &args.action
            && parent != &self.context.thread
        {
            return Err("Report children only for this connection's parent thread".into());
        }
        if matches!(&args.action, Action::ConfigureFiote { .. })
            || matches!(&args.action, Action::SetExtension { namespace, .. } if namespace.starts_with("lince.fiote"))
        {
            return Err("Fiote identity, session instructions and dispatch settings are changed by the operator in Fiote settings.".into());
        }
        if let Some(previous) = self.receipts.previous(&args.request_id, arguments)? {
            return Ok(previous);
        }
        if matches!(
            args.action,
            Action::EditRecordText { .. }
                | Action::ChangeRecord {
                    request: Request {
                        mutation: Mutation::Text { .. },
                        ..
                    }
                }
        ) {
            return Err(
                "Use lince_read_record and lince_edit_text for collaborative text edits.".into(),
            );
        }
        if args.read_ids.len() > MAX_READS {
            return Err("Too many read_ids.".into());
        }
        let mut observed = Vec::with_capacity(args.read_ids.len());
        for id in &args.read_ids {
            let read = self
                .reads
                .get(id)
                .ok_or("Unknown read_id. Read the Record in this turn first.")?;
            observed.push(Arc::clone(&read.record));
        }
        let replaces_text = matches!(
            args.action,
            Action::DeleteRecord { .. }
                | Action::ReviseMessage { .. }
                | Action::ReviseMessageDraft { .. }
                | Action::SaveProtein { .. }
        );
        for record in &observed {
            let uid = record
                .get("uid")
                .and_then(Value::as_str)
                .ok_or("Missing Record UID.")?;
            let mut current = self.record(uid).await?;
            if let Some(extensions) = record.get("extensions").and_then(Value::as_object) {
                for namespace in extensions.keys() {
                    let value = self.extension(uid, namespace).await?;
                    insert_extension(&mut current, namespace.clone(), value)?;
                }
            }
            if !same_properties(record, &current, replaces_text) {
                return Err("Record properties changed since this read. Read it again and reconsider the operation.".into());
            }
        }
        if let Some(target) = record_target(&args.action)
            && !observed
                .iter()
                .any(|record| record.get("uid").and_then(Value::as_str) == Some(target))
        {
            return Err("Supply a read_id for the Record being changed and use its stable UID as the target.".into());
        }
        if let Action::RetractAssertion { assertion } = &args.action
            && !observed.iter().any(|record| {
                record
                    .get("assertions")
                    .and_then(Value::as_array)
                    .is_some_and(|assertions| {
                        assertions.iter().any(|value| {
                            value.get("uid").and_then(Value::as_str) == Some(assertion)
                        })
                    })
            })
        {
            return Err(
                "Read the assertion's subject Record and supply its read_id before retracting it."
                    .into(),
            );
        }
        if let Action::SetExtension {
            target, namespace, ..
        } = &args.action
            && !observed.iter().any(|record| {
                record.get("uid").and_then(Value::as_str) == Some(target)
                    && record
                        .get("extensions")
                        .and_then(|value| value.get(namespace))
                        .is_some()
            })
        {
            return Err(
                "Read this extension namespace with lince_read_record before changing it.".into(),
            );
        }
        if let Action::SaveProtein { slug, .. } = &args.action {
            let existing = self.query(json!({"protein":{"source":"record","where":[{"slug_eq":slug}],"fields":["uid"],"limit":1}})).await?;
            if let Some(record) = existing
                .get("rows")
                .and_then(Value::as_array)
                .and_then(|rows| rows.first())
                && !observed.iter().any(|read| {
                    read.get("uid").unwrap_or(&Value::Null)
                        == record.get("uid").unwrap_or(&Value::Null)
                        && read.pointer("/extensions/lince.protein").is_some()
                })
            {
                return Err("Read the existing Protein Record and its lince.protein extension before replacing its query.".into());
            }
        }
        let mark = fingerprint(arguments)?;
        let result = response(
            self.session
                .handle(ClientMessage::Act {
                    id: args.request_id.clone(),
                    action: args.action,
                })
                .await,
        );
        self.receipts
            .remember(args.request_id, mark, result.clone());
        result
    }
}

fn same_properties(previous: &Value, current: &Value, include_text: bool) -> bool {
    let (Some(left), Some(right)) = (previous.as_object(), current.as_object()) else {
        return previous == current;
    };
    let relevant = |key: &str| {
        !matches!(key, "updated_at" | "spent_seconds")
            && (include_text || !matches!(key, "head" | "body"))
    };
    let mut entries = left.iter().filter(|(key, _)| relevant(key));
    entries.clone().count() == right.keys().filter(|key| relevant(key)).count()
        && entries.all(|(key, value)| right.get(key) == Some(value))
}

fn record_target(action: &Action) -> Option<&str> {
    match action {
        Action::ChangeRecord { request } => Some(&request.record_uid),
        Action::ReviseMessage { message, .. } => Some(message),
        Action::ReviseMessageDraft { draft, .. } => Some(draft),
        Action::SetQuantity { target, .. }
        | Action::SetQuantityExact { target, .. }
        | Action::AddQuantity { target, .. }
        | Action::Activate { target }
        | Action::Deactivate { target }
        | Action::DeleteRecord { target }
        | Action::SetSlug { target, .. }
        | Action::SetUnit { target, .. }
        | Action::SetExtension { target, .. }
        | Action::SetPlace { target, .. } => Some(target),
        Action::AssertRecord { subject, .. }
        | Action::RetractRecord { subject, .. }
        | Action::RefineAssertion { subject, .. }
        | Action::SetIdentity { subject, .. }
        | Action::TransitionRecord { subject, .. } => Some(subject),
        _ => None,
    }
}
