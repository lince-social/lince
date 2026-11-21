// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchFuture, Engine, EngineError, Utc,
    invalid_dispatch, validate_scope, widens_scope,
};

impl Engine {
    pub(super) fn dispatch_contacts(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::SetContactDelivery { target, mode } => {
                    let uid = self.resolve(&target).await?;
                    if !matches!(mode.as_str(), "direct" | "mailbox" | "auto") {
                        return Err(EngineError::Consequence(
                            "Choose direct, mailbox or auto delivery".into(),
                        ));
                    }
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "Choose an existing contact".into(),
                        ));
                    }
                    store::organs::set_mode(&self.store.pool, &uid, &mode).await?;
                    outcome.facts = self
                        .annotate(uid, actor, serde_json::json!({"delivery":mode}), now)
                        .await?;
                }
                Action::ReconnectContact { target } => {
                    let uid = self.resolve(&target).await?;
                    self.transport_for("reconnect a contact")?
                        .reconnect_contact(&uid)
                        .await?;
                    outcome.data = Some(
                        serde_json::json!({"status":"Device list refreshed. Queued work will recheck current permissions before delivery."}),
                    );
                }
                Action::RenameOrganContact { target, name } => {
                    let uid = self.resolve(&target).await?;
                    let name = name.trim();
                    if name.is_empty() {
                        return Err(EngineError::Consequence(
                            "give this contact a name you will recognise".into(),
                        ));
                    }
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ is renamed like any record"
                                .into(),
                        ));
                    }
                    store::organs::rename_contact(&self.store.pool, &uid, name).await?;
                }
                Action::SetSyncPolicy {
                    target,
                    sync_out,
                    sync_in,
                } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — there is no feed to open with this Cell's own Organ"
                                .into(),
                        ));
                    }
                    store::organs::set_sync_policy(&self.store.pool, &uid, sync_out, sync_in)
                        .await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "sync_out": sync_out, "sync_in": sync_in }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactAcceptScope { target, fields } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ sends us nothing to accept"
                                .into(),
                        ));
                    }
                    validate_scope(fields.as_deref())?;
                    store::organs::set_contact_accept_scope(
                        &self.store.pool,
                        &uid,
                        fields.as_deref(),
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "accept_fields": fields }),
                            now,
                        )
                        .await?;
                }
                Action::HideRecordFromContact {
                    target,
                    record,
                    hidden,
                } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ has no feed to hide from".into(),
                        ));
                    }
                    let record_uid = self.resolve(&record).await?;
                    if store::records::get(&self.store.pool, &record_uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence("no such record to hide".into()));
                    }
                    store::visibility::set_hidden_from_organ(
                        &self.store.pool,
                        &uid,
                        &record_uid,
                        hidden,
                    )
                    .await?;
                    if !hidden {
                        store::sync_ops::enqueue_record_for_contact(
                            &self.store.pool,
                            &uid,
                            &record_uid,
                        )
                        .await?;
                    }
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "hidden_record": record_uid, "hidden": hidden }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactScope { target, fields } => {
                    let uid = self.resolve(&target).await?;
                    let Some(contact) = store::organs::contact(&self.store.pool, &uid).await?
                    else {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ has no scope to narrow".into(),
                        ));
                    };
                    let before = contact.scope_fields;
                    validate_scope(fields.as_deref())?;
                    store::organs::set_contact_scope(&self.store.pool, &uid, fields.as_deref())
                        .await?;
                    if widens_scope(before.as_deref(), fields.as_deref()) {
                        store::sync_ops::enqueue_widened_for_contact(
                            &self.store.pool,
                            &uid,
                            before.as_deref(),
                            fields.as_deref(),
                        )
                        .await?;
                    }
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "scope_fields": fields }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactShare { target, protein } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ shares with nobody".into(),
                        ));
                    }
                    let raw = match &protein {
                        Some(value) => {
                            let raw = value.to_string();
                            let Some(parsed) = crate::share::parse(&raw) else {
                                return Err(EngineError::Consequence(
                                    "this selection is not a Protein query".into(),
                                ));
                            };
                            protein::matching_records(&self.store, &parsed, None).await?;
                            Some(raw)
                        }
                        None => None,
                    };
                    store::organs::set_contact_share_protein(
                        &self.store.pool,
                        &uid,
                        raw.as_deref(),
                    )
                    .await?;
                    if let Some(contact) = store::organs::contact(&self.store.pool, &uid).await? {
                        crate::share::reconcile_contact(self, &contact).await?;
                    }
                    outcome.facts = self
                        .annotate(uid, actor, serde_json::json!({ "share": protein }), now)
                        .await?;
                }
                Action::ForgetOrganContact { target } => {
                    let uid = self.resolve(&target).await?;
                    if store::organs::contact(&self.store.pool, &uid)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence(
                            "not a contact — this Cell's own Organ cannot be forgotten".into(),
                        ));
                    }
                    store::organs::forget_contact(&self.store.pool, &uid).await?;
                }
                Action::AddKnownOrgan { invite, name } => {
                    let invite = crate::pairing::PairingInvite::decode(&invite)?;
                    let name = name.trim();
                    if name.is_empty() {
                        return Err(EngineError::Consequence(
                            "give this contact a name you will recognise".into(),
                        ));
                    }
                    let existing =
                        store::organs::contact_by_node_id(&self.store.pool, &invite.node_id)
                            .await?;
                    if let Some(contact) = &existing {
                        if contact.trust == "blocked" {
                            return Err(EngineError::Consequence(
                                "this Organ is blocked. Unblock them first if that is what you \
                             meant — adding by code must not undo a block."
                                    .into(),
                            ));
                        }
                    }
                    let organ_uid = match &existing {
                        Some(contact) => contact.record_uid.clone(),
                        None => {
                            let organ_uid = format!("o-{}", &invite.node_id);
                            store::organs::add_contact(
                                &self.store.pool,
                                &organ_uid,
                                None,
                                name,
                                "",
                                1,
                            )
                            .await?;
                            store::organs::set_node_id(
                                &self.store.pool,
                                &organ_uid,
                                Some(&invite.node_id),
                            )
                            .await?;
                            organ_uid
                        }
                    };
                    if existing.is_some() {
                        store::organs::rename_contact(&self.store.pool, &organ_uid, name).await?;
                    }
                    if let Some(root_key) = &invite.root_key {
                        crate::trust::adopt_key(
                            &self.store,
                            &organ_uid,
                            crate::roster::ROOT_KEY_ID,
                            root_key,
                        )
                        .await?;
                    } else {
                        outcome.warnings.push(
                            "this code carried no identity key, so future key changes cannot be \
                         verified against it. Prefer a code that includes one."
                                .into(),
                        );
                    }
                    store::organs::set_trust(&self.store.pool, &organ_uid, "known").await?;
                    if existing.is_none() {
                        store::organs::set_pending_introduction(&self.store.pool, &organ_uid, true)
                            .await?;
                        outcome.warnings.push(
                            "added — but they are not reachable for sync until this Cell has \
                         connected to them once and learned their identity."
                                .into(),
                        );
                    }
                    outcome.created = Some(organ_uid);
                }
                Action::AuditContact { contact } => {
                    let contact_uid = self.resolve(&contact).await?;
                    match self.audit_contact(&contact_uid).await? {
                        Some(report) => {
                            outcome.data = Some(serde_json::json!({
                                "contact_organ": report.contact_organ,
                                "they_lack": report.they_lack,
                                "unknown_cells": report.unknown_cells,
                                "reached": true,
                            }));
                        }
                        None => {
                            outcome.data = Some(serde_json::json!({ "reached": false }));
                        }
                    }
                }
                Action::SetContactTrust { target, trust } => {
                    let uid = self.resolve(&target).await?;
                    if !matches!(trust.as_str(), "unknown" | "known" | "blocked") {
                        return Err(EngineError::Consequence(format!(
                            "invalid trust `{trust}`: must be unknown, known, or blocked"
                        )));
                    }
                    store::organs::set_trust(&self.store.pool, &uid, &trust).await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "contact_trust": trust }),
                            now,
                        )
                        .await?;
                }
                Action::SetContactProximity { target, proximity } => {
                    let uid = self.resolve(&target).await?;
                    store::organs::set_proximity(&self.store.pool, &uid, proximity).await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({ "contact_proximity": proximity }),
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
