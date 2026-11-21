// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchFuture, Engine, EngineError, Utc,
    invalid_dispatch,
};

impl Engine {
    pub(super) fn dispatch_record_extensions(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::SetExtension {
                    target,
                    namespace,
                    fds,
                } => {
                    if namespace.starts_with("lince.demeter")
                        || namespace.starts_with("lince.optimization")
                    {
                        return Err(EngineError::Forbidden("Use the Demeter or optimization commands to change managed planning data".into()));
                    }
                    if namespace == store::people::NAMESPACE {
                        return Err(EngineError::Forbidden("Use Person access controls to change standing or authentication metadata".into()));
                    }
                    if namespace == nucleus::trust_analysis::contacts::NAMESPACE {
                        return Err(EngineError::Forbidden("Use reviewed signed contact disclosures instead of editing their evidence".into()));
                    }
                    if namespace == nucleus::transfer::outcome::NAMESPACE
                        || namespace == nucleus::transfer::receipt::NAMESPACE
                    {
                        return Err(EngineError::Forbidden(
                            "Use signed outcome reports to change Transfer evidence".into(),
                        ));
                    }
                    if matches!(
                        namespace.as_str(),
                        nucleus::record_extension::SCHEMA_NAMESPACE
                            | nucleus::record_extension::VALUES_NAMESPACE
                    ) {
                        return Err(EngineError::Forbidden(
                            "Use the extension editor to change schemas or field values".into(),
                        ));
                    }
                    if namespace == "lince.command"
                        && !crate::commands::COMMAND_AUTHORING
                            .try_with(|allowed| *allowed)
                            .unwrap_or(false)
                    {
                        return Err(EngineError::Forbidden(
                            "Use command authoring to change executable configuration".into(),
                        ));
                    }
                    if namespace.starts_with("lince.social.") {
                        return Err(EngineError::Forbidden("Use social controls to change publication or private conversation state".into()));
                    }
                    if namespace == crate::groups::NAMESPACE
                        || namespace == crate::groups::ADMISSION_NAMESPACE
                    {
                        return Err(EngineError::Forbidden(
                            "Use the group membership controls".into(),
                        ));
                    }
                    if namespace == "lince.file_sync" && actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can configure directory sync".into(),
                        ));
                    }
                    let uid = self.resolve(&target).await?;
                    if namespace == "lince.file_sync" {
                        self.check_legacy_sync_path(&fds).await?;
                    }
                    if namespace == "lince.message" {
                        return Err(EngineError::Forbidden(
                            "Use message actions to change authorship or lifecycle.".into(),
                        ));
                    }
                    let mut expected_content = None;
                    if let Some(revision) = nucleus::window_opening::revision(&namespace, &fds)
                        .map_err(EngineError::Consequence)?
                    {
                        let current =
                            store::records::get_extension(&self.store.pool, &uid, &namespace)
                                .await?;
                        let previous = current
                            .as_ref()
                            .and_then(|value| value["revision"].as_u64())
                            .unwrap_or(0);
                        if previous.checked_add(1) != Some(revision) {
                            return Err(EngineError::Consequence(
                                "Opening preferences changed. Reload before saving.".into(),
                            ));
                        }
                        expected_content = Some(current.unwrap_or_default());
                    }
                    if namespace == nucleus::shader_effect::NAMESPACE {
                        if nucleus::json::encoded_len(&fds)? > 196_608 {
                            return Err(EngineError::Consequence(
                                "WGSL extension is limited to 192 KB.".into(),
                            ));
                        }
                        let extension: nucleus::shader_effect::Extension =
                            serde_json::from_value(fds.clone())
                                .map_err(|error| EngineError::Consequence(error.to_string()))?;
                        extension.validate().map_err(EngineError::Consequence)?;
                        let current =
                            store::records::get_extension(&self.store.pool, &uid, &namespace)
                                .await?;
                        let revision = current
                            .as_ref()
                            .and_then(|value| value["revision"].as_u64())
                            .unwrap_or(0);
                        if revision.checked_add(1) != Some(extension.revision) {
                            return Err(EngineError::Consequence(
                                "The WGSL extension changed. Reload before saving.".into(),
                            ));
                        }
                        expected_content = Some(current.unwrap_or_default());
                    }
                    if namespace == "lince.message-content" {
                        let metadata =
                            store::records::get_extension(&self.store.pool, &uid, "lince.message")
                                .await?
                                .ok_or_else(|| {
                                    EngineError::Consequence(
                                        "Content must belong to a message.".into(),
                                    )
                                })?;
                        let (_, operator) = self.message_authorship(None, actor.as_deref()).await?;
                        if metadata["state"] != "writing" {
                            let before =
                                store::records::get_extension(&self.store.pool, &uid, &namespace)
                                    .await?
                                    .unwrap_or_default();
                            let old: Vec<nucleus::message::StoredPart> =
                                serde_json::from_value(before["parts"].clone())
                                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                            let new: Vec<nucleus::message::StoredPart> =
                                serde_json::from_value(fds["parts"].clone())
                                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                            if old == new {
                                return Err(EngineError::Consequence(
                                    "These message contents have already been saved.".into(),
                                ));
                            }
                            if old.len() != new.len() {
                                return Err(EngineError::Consequence(
                                    "Only existing steps and questions can be edited here.".into(),
                                ));
                            }
                            for (old, new) in old.iter().zip(&new) {
                                match (old, new) {
                                _ if old == new => {},
                                (nucleus::message::StoredPart::Steps { .. }, nucleus::message::StoredPart::Steps { steps }) => {
                                    if metadata["operator"] != operator || metadata["author"] != operator { return Err(EngineError::Forbidden("Only the author can edit their step list.".into())); }
                                    nucleus::operation::validate_steps(steps).map_err(EngineError::Consequence)?;
                                }
                                (nucleus::message::StoredPart::Question { question: old }, nucleus::message::StoredPart::Question { question: new }) => old.response(new, &operator, metadata["operator"].as_str().unwrap_or_default(), now.timestamp_millis().max(0) as u64).map_err(EngineError::Forbidden)?,
                                _ => return Err(EngineError::Consequence("Message attachments cannot be changed through form or step controls.".into())),
                            }
                            }
                            expected_content = Some(before);
                        } else if metadata["operator"] != operator {
                            return Err(EngineError::Forbidden(
                                "Only the message operator may change its contents.".into(),
                            ));
                        }
                    }
                    if namespace == "lince.fiote" {
                        return Err(EngineError::Consequence("Use configure-fiote to change prompt ancestry and assignment behavior.".into()));
                    }
                    if namespace == "work" {
                        return self
                            .change_record(
                                crate::record_change::Request {
                                    id: nucleus::new_uid("op"),
                                    record_uid: uid,
                                    mutation: crate::record_change::Mutation::WorkMetadata {
                                        value: fds,
                                    },
                                },
                                actor.as_deref(),
                            )
                            .await
                            .map(ControlFlow::Break);
                    }
                    outcome.facts = self
                        .change_record_scalar_as(
                            &uid,
                            crate::record_policy::ScalarChange::Extension {
                                namespace: &namespace,
                                value: &fds,
                                expected: expected_content.as_ref(),
                            },
                            actor.as_deref(),
                            now,
                        )
                        .await?;
                }
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
