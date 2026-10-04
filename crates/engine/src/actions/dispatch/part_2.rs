use super::*;

impl Engine {
    pub(super) fn dispatch_part_2(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        _verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::MailboxAnswerRequest {
                    organ_uid,
                    accept,
                    quota_bytes,
                } => {
                    let pool = &self.store.pool;
                    let ask = store::mailbox::request(pool, &organ_uid)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no such request".into()))?;
                    if accept {
                        let quota = if quota_bytes > 0 {
                            quota_bytes
                        } else {
                            crate::mailbox::DEFAULT_QUOTA_BYTES
                        };
                        store::mailbox::register(
                            pool,
                            &ask.organ_uid,
                            &ask.root_key,
                            &ask.label,
                            quota,
                        )
                        .await?;
                    }
                    store::mailbox::answer_request(pool, &organ_uid).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "accepted": accept,
                    }));
                }
                Action::MailboxIssueInvite { label, quota_bytes } => {
                    let token = self.issue_mailbox_invite(&label, quota_bytes).await?;
                    let node_id = self.own_node_id().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "this Cell has published no address, so an invite would have nowhere \
                         to point. Publish a device list first."
                                .into(),
                        )
                    })?;
                    let code = crate::pairing::MailboxInviteCode { node_id, token }.encode();
                    outcome.data = Some(serde_json::json!({
                        "code": code,
                        "expires_in_days": crate::mailbox::INVITE_TTL_DAYS,
                    }));
                }
                Action::MailboxAskCarry { organ_uid } => {
                    let contact = store::organs::contact(&self.store.pool, &organ_uid)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no such contact".into()))?;
                    let node_id = contact.node_id.clone().or_else(|| None).ok_or_else(|| {
                        EngineError::Consequence(
                            "we hold no address for them, so there is nobody to ask".into(),
                        )
                    })?;
                    self.ask_carrier(&node_id).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "asked": true,
                    }));
                }
                Action::MailboxUseInvite { code } => {
                    let (label, quota_bytes) = self.redeem_carry_code(&code).await?;
                    outcome.data = Some(serde_json::json!({
                        "label": label,
                        "quota_bytes": quota_bytes,
                    }));
                }
                Action::SetFileSyncEnabled { enabled } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can configure directory sync".into(),
                        ));
                    }
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence("Local Organ is unavailable".into())
                        })?;
                    let mut config = store::records::get_extension(
                        &self.store.pool,
                        &organ.uid,
                        "lince.file_sync",
                    )
                    .await?
                    .filter(serde_json::Value::is_object)
                    .unwrap_or_else(|| serde_json::json!({"format":"lingua"}));
                    config["enabled"] = enabled.into();
                    store::records::set_extension(
                        &self.store.pool,
                        &organ.uid,
                        "lince.file_sync",
                        &config,
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            organ.uid,
                            actor,
                            serde_json::json!({"extension":"lince.file_sync"}),
                            now,
                        )
                        .await?;
                }
                Action::ConfigureFileSync {
                    protein,
                    path,
                    format,
                    enabled,
                } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can configure directory sync".into(),
                        ));
                    }
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence("Local Organ is unavailable".into())
                        })?;
                    let config = if enabled {
                        let path = path.trim();
                        if !std::path::Path::new(path).is_absolute() {
                            return Err(EngineError::Consequence(
                                "Choose an absolute directory path".into(),
                            ));
                        }
                        if std::path::Path::new(path).exists()
                            && !std::path::Path::new(path).is_dir()
                        {
                            return Err(EngineError::Consequence(
                                "The sync path must be a directory".into(),
                            ));
                        }
                        let mut config =
                            serde_json::json!({"enabled": true, "path": path, "format": format});
                        if !protein.trim().is_empty() {
                            let uid = self.resolve(&protein).await?;
                            self.file_sync_protein(&uid).await?;
                            config["protein"] = uid.into();
                        }
                        config
                    } else {
                        let mut config = store::records::get_extension(
                            &self.store.pool,
                            &organ.uid,
                            "lince.file_sync",
                        )
                        .await?
                        .filter(serde_json::Value::is_object)
                        .unwrap_or_else(|| serde_json::json!({}));
                        config["enabled"] = false.into();
                        config
                    };
                    store::records::set_extension(
                        &self.store.pool,
                        &organ.uid,
                        "lince.file_sync",
                        &config,
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            organ.uid,
                            actor,
                            serde_json::json!({"extension": "lince.file_sync"}),
                            now,
                        )
                        .await?;
                }
                Action::FileSyncStatus { organ } => {
                    let organ_uid = self.resolve(&organ).await?;
                    outcome.data = Some(serde_json::json!({
                        "checked": self.file_sync_has_ticked(&organ_uid),
                        "conflicts": self
                            .file_sync_conflicts(&organ_uid)
                            .into_iter()
                            .map(|c| serde_json::json!({ "path": c.path, "reason": c.reason }))
                            .collect::<Vec<_>>(),
                    }));
                }
                Action::MailboxOutbound => {
                    let pool = &self.store.pool;
                    let queued = store::sync_ops::outbox_due(pool).await?;
                    let now = nucleus::execution::now();
                    let minutes = |stamp: &Option<String>| -> Option<i64> {
                        stamp
                            .as_deref()
                            .and_then(|when| chrono::DateTime::parse_from_rfc3339(when).ok())
                            .map(|when| (now - when.with_timezone(&chrono::Utc)).num_minutes())
                    };
                    let mut rows = Vec::new();
                    for contact in store::organs::contacts(pool).await? {
                        let Some(waiting) = minutes(&contact.unreachable_since) else {
                            continue;
                        };
                        let publishes = self
                            .roster_of(&contact.record_uid)
                            .await?
                            .map(|signed| !signed.roster.pickup.is_empty())
                            .unwrap_or(false);
                        let ops = queued
                            .iter()
                            .filter(|row| row.contact_organ == contact.record_uid)
                            .count();
                        rows.push(serde_json::json!({
                            "organ_uid": contact.record_uid,
                            "known_as": contact.slug,
                            "unreachable_minutes": waiting,
                            "queued_ops": ops,
                            "mailed_minutes_ago": minutes(&contact.mailed_at),
                            "can_be_mailed": publishes,
                        }));
                    }
                    let mut never_picked_up = Vec::new();
                    for gone in store::mail_left::expired(pool, 20).await? {
                        let known_as = store::organs::contact(pool, &gone.to_organ)
                            .await?
                            .map(|contact| contact.head)
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| gone.to_organ.clone());
                        never_picked_up.push(serde_json::json!({
                            "organ_uid": gone.to_organ,
                            "known_as": known_as,
                            "carrier": gone.carrier_organ,
                            "left_at": gone.left_at,
                            "expired_at": gone.expired_at,
                        }));
                    }
                    let mail_key_published =
                        self.own_sealing_key_is_published().await.unwrap_or(true);
                    outcome.data = Some(serde_json::json!({
                        "contacts": rows,
                        "window_minutes": crate::wire::Wire::MAIL_AFTER.num_minutes(),
                        "never_picked_up": never_picked_up,
                        "outstanding_deposits": store::mail_left::outstanding(pool).await?,
                        "saved_outgoing": store::mailbox::outbox::status(pool).await?,
                        "mailbox_copies": store::cells::config(pool, "lince.social").await?
                            .and_then(|value| value["mailbox_copies"].as_i64()).unwrap_or(2),
                        "mail_key_published": mail_key_published,
                    }));
                }
                Action::MailboxMailNow { organ_uid } => {
                    if store::organs::contact(&self.store.pool, organ_uid.as_str())
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Consequence("no such contact".into()));
                    }
                    let past =
                        (nucleus::execution::now() - crate::wire::Wire::MAIL_AFTER).to_rfc3339();
                    store::organs::backdate_unreachable(
                        &self.store.pool,
                        organ_uid.as_str(),
                        &past,
                    )
                    .await?;
                    store::organs::mark_mailed_clear(&self.store.pool, organ_uid.as_str()).await?;
                    store::mailbox::outbox::retry_recipient(&self.store.pool, organ_uid.as_str())
                        .await?;
                    let moved = self.sync_now().await?;
                    let after =
                        store::organs::contact(&self.store.pool, organ_uid.as_str()).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "batches": moved,
                        "reached": after
                            .as_ref()
                            .map(|c| c.unreachable_since.is_none())
                            .unwrap_or(false),
                        "mailed": after.and_then(|c| c.mailed_at).is_some(),
                    }));
                }
                Action::ReadMessageAttachment { message, index } => {
                    let uid = self.resolve(&message).await?;
                    if !self.may_read_record(actor.as_deref(), &uid).await? {
                        return Err(EngineError::Forbidden(
                            "This attachment is not available to this Person".into(),
                        ));
                    }
                    let parts = store::message_content::load(&self.store.pool, &uid).await?;
                    let part = parts.get(index).ok_or_else(|| {
                        EngineError::Consequence("Attachment is unavailable".into())
                    })?;
                    if !matches!(part, nucleus::message::MessagePart::Attachment { .. }) {
                        return Err(EngineError::Consequence(
                            "This message part is not an attachment".into(),
                        ));
                    }
                    outcome.data = Some(serde_json::to_value(part).map_err(EngineError::Json)?);
                }
                Action::SyncNow => {
                    let moved = self.sync_now().await?;
                    let status = self.cell_delivery_status().await?;
                    let cells = status["cells"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    let failed = cells
                        .iter()
                        .filter(|cell| {
                            cell["delivery"].is_null() || !cell["delivery"]["error"].is_null()
                        })
                        .count();
                    let pending: i64 = cells
                        .iter()
                        .filter_map(|cell| cell["delivery"]["pending"].as_i64())
                        .sum();
                    let message = if let Some(recovery) = status["recovery"].as_str() {
                        recovery.to_owned()
                    } else if failed > 0 {
                        format!(
                            "{failed} device(s) could not confirm delivery. Your changes are saved here. Open device status for the reason."
                        )
                    } else if cells.is_empty() {
                        format!(
                            "{moved} batch(es) exchanged. No other device is enrolled in this Organ."
                        )
                    } else if pending > 0 {
                        format!(
                            "{moved} batch(es) exchanged; {pending} operation confirmations still pending across your devices."
                        )
                    } else {
                        "Your enrolled devices confirmed all current Organ operations. No pending delivery at this check.".into()
                    };
                    outcome.warnings.push(message);
                    outcome.data = Some(status);
                }
                Action::RosterStatus => {
                    let held = store::door::held(&self.store.pool, 50).await?;
                    let waiting: Vec<serde_json::Value> = held
                        .into_iter()
                        .map(|row| {
                            let claimed = serde_json::from_str::<serde_json::Value>(&row.intro)
                                .ok()
                                .and_then(|intro| {
                                    intro
                                        .get("display_name")
                                        .and_then(serde_json::Value::as_str)
                                        .map(str::to_string)
                                })
                                .unwrap_or_default();
                            serde_json::json!({
                                "uid": row.uid,
                                "node_id": row.node_id,
                                "organ_uid": row.organ_uid,
                                "claimed_name": claimed,
                                "received_at": row.received_at,
                            })
                        })
                        .collect();
                    let stale: Vec<serde_json::Value> = self
                        .stale_siblings
                        .lock()
                        .expect("stale siblings")
                        .iter()
                        .map(|cell| {
                            serde_json::json!({
                                "cell_uid": cell.cell_uid,
                                "label": cell.label,
                                "node_id": cell.node_id,
                                "their_epoch": cell.their_epoch,
                                "our_epoch": cell.our_epoch,
                            })
                        })
                        .collect();
                    let this_cell = store::cells::local(&self.store.pool).await?;
                    let held = match store::organs::local(&self.store.pool).await? {
                        Some(organ) => self.roster_of(&organ.uid).await?,
                        None => None,
                    };
                    let has_roster = held.is_some();
                    let can_manage = self.root_signer().await?.is_some();
                    let enrolment_error =
                        self.may_enrol().await.err().map(|error| error.to_string());
                    let discovery =
                        store::cells::config(&self.store.pool, "lince.discovery").await?;
                    let network = store::cells::config(&self.store.pool, "lince.network").await?;
                    let peer_port = crate::wire::configured_peer_port(network.as_ref())?;
                    let peer_network = self.peer_network_status();
                    let capabilities: Vec<String> = match (&this_cell, held) {
                        (Some(cell), Some(signed)) => signed
                            .roster
                            .cells
                            .into_iter()
                            .find(|member| member.cell_uid == cell.uid)
                            .map(|member| member.capabilities)
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    outcome.data = Some(serde_json::json!({
                        "waiting_at_the_door": waiting,
                        "devices_needing_update": stale,
                        "this_cell": this_cell.as_ref().map(|cell| cell.uid.clone()),
                        "capabilities": capabilities,
                        "has_roster": has_roster,
                        "karma": self.karma_device_execution().await?,
                        "can_manage": can_manage,
                        "enrolment_error": enrolment_error,
                        "discovery": discovery,
                        "peer_port": peer_port,
                        "peer_network": peer_network,
                        "sync": self.cell_delivery_status().await?,
                    }));
                }
                Action::SetCellConfig { namespace, fds } => {
                    outcome = self.set_cell_config_action(&namespace, fds).await?;
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
                Action::RosterJoinOrgan { code } => {
                    let roster = self.join_from_code(code.trim()).await?;
                    outcome.created = Some(roster.roster.organ_uid.clone());
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": roster.roster.organ_uid,
                        "version": roster.roster.version,
                        "cells": roster.roster.cells.len(),
                    }));
                    outcome.warnings.push(format!(
                        "this device is now part of that Organ, alongside {} other device(s). \
                     Its own previous identity is gone.",
                        roster.roster.cells.len().saturating_sub(1)
                    ));
                }
                Action::RosterRevokeCell { cell_uid } => {
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "the root key is not on this Cell — bring it back to revoke a device"
                                .into(),
                        )
                    })?;
                    let roster = self.revoke_cell(&root, &cell_uid).await?;
                    outcome.warnings.push(format!(
                        "roster v{} published without {cell_uid}",
                        roster.roster.version
                    ));
                }
                Action::RootKeyExport { destination } => {
                    let path = self
                        .root_key_path
                        .lock()
                        .expect("root key path")
                        .clone()
                        .ok_or_else(|| {
                            EngineError::Consequence("this Cell has no root key path".into())
                        })?;
                    crate::roster::export_root_key(&path, std::path::Path::new(&destination))?;
                    outcome.warnings.push(format!(
                        "root key copied to {destination}. Keep it offline; this Cell can now be \
                     detached from it."
                    ));
                }
                Action::RootKeyDetach { copy_at } => {
                    let path = self
                        .root_key_path
                        .lock()
                        .expect("root key path")
                        .clone()
                        .ok_or_else(|| {
                            EngineError::Consequence("this Cell has no root key path".into())
                        })?;
                    crate::roster::detach_root_key(&path, std::path::Path::new(&copy_at))?;
                    outcome.warnings.push(
                    "root key removed from this Cell. Enrolling or revoking a device now needs \
                     it back; everything else keeps working."
                        .into(),
                );
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
                Action::Compensate { fact } => {
                    let original = store::facts::get(&self.store.pool, &fact)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(fact.clone()))?;
                    if store::transfer_effects::protected(&self.store.pool, &original.uid).await?
                        || store::transfers::occurrence_settlement_for_application_fact(
                            &self.store.pool,
                            &original.uid,
                        )
                        .await?
                        .is_some()
                        || store::transfers::occurrence_settlement_compensation_for_fact(
                            &self.store.pool,
                            &original.uid,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(EngineError::Conflict {
                        code: "typed_transfer_settlement_compensation_required",
                        message: "Transfer settlement applications and their corrections cannot be changed through generic compensation"
                            .into(),
                    });
                    }
                    if store::entries::for_fact(&self.store.pool, &original.uid)
                        .await?
                        .is_some()
                    {
                        return Err(EngineError::Conflict {
                            code: "entry_void_required",
                            message: "this Fact belongs to an Entry; use void-entry".into(),
                        });
                    }
                    if !original.delta.is_zero() {
                        outcome.facts = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: original.record_uid,
                                    delta: store::exact::negate(original.delta)?,
                                    at: None,
                                    actor_uid: actor,
                                    cause: Cause {
                                        kind: CauseKind::Compensation,
                                        uid: Some(original.uid),
                                    },
                                    payload: None,
                                },
                                now,
                            )
                            .await?;
                    }
                }
                Action::CreateLingua { name, visibility } => {
                    outcome.created = Some(
                        store::linguas::create(&self.store.pool, &name, None, &visibility).await?,
                    );
                }
                Action::RenameLingua { lingua, name } => {
                    let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                    store::linguas::rename(&self.store.pool, &lingua_uid, &name).await?;
                }
                Action::DeleteLingua { lingua } => {
                    let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                    if lingua_uid == store::linguas::LOCAL_UID {
                        return Err(EngineError::Consequence(
                            "the local Lingua is the ontology's permanent private home".into(),
                        ));
                    }
                    store::linguas::delete(&self.store.pool, &lingua_uid).await?;
                }
                Action::CreateConcept {
                    lingua,
                    name,
                    parents,
                } => {
                    let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                    let mut parent_uids = Vec::new();
                    for p in &parents {
                        parent_uids.push(
                            store::concepts::resolve(&self.store.pool, p)
                                .await?
                                .ok_or_else(|| EngineError::UnknownRecord(p.clone()))?,
                        );
                    }
                    let refs: Vec<&str> = parent_uids.iter().map(String::as_str).collect();
                    outcome.created = Some(
                        store::concepts::create_in(&self.store.pool, &lingua_uid, &name, &refs)
                            .await?,
                    );
                }
                Action::RenameConcept { concept, name } => {
                    let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    store::concepts::rename(&self.store.pool, &concept_uid, &name).await?;
                }
                Action::DeleteConcept { concept } => {
                    let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    match store::concepts::delete(&self.store.pool, &concept_uid).await {
                        Err(error)
                            if error
                                .as_database_error()
                                .is_some_and(|error| error.is_foreign_key_violation()) =>
                        {
                            return Err(EngineError::Conflict {
                            code: "concept_in_use",
                            message: "This concept is still used by Records, assertion history or other saved data. Remove it from a Lingua instead of deleting it."
                                .into(),
                        });
                        }
                        result => {
                            result?;
                        }
                    }
                }
                Action::AdoptConcept { lingua, concept } => {
                    let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                    let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    store::linguas::adopt(&self.store.pool, &lingua_uid, &concept_uid).await?;
                }
                Action::RemoveConceptFromLingua { lingua, concept } => {
                    let lingua_uid = store::linguas::resolve(&self.store.pool, &lingua)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(lingua))?;
                    let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    store::linguas::remove_concept(&self.store.pool, &lingua_uid, &concept_uid)
                        .await?;
                }
                Action::AddConceptParent { concept, parent } => {
                    let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    let parent_uid = store::concepts::resolve(&self.store.pool, &parent)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(parent))?;
                    store::concepts::add_parent(&self.store.pool, &concept_uid, &parent_uid)
                        .await?;
                }
                Action::RemoveConceptParent { concept, parent } => {
                    let concept_uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    let parent_uid = store::concepts::resolve(&self.store.pool, &parent)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(parent))?;
                    store::concepts::remove_parent(&self.store.pool, &concept_uid, &parent_uid)
                        .await?;
                }
                Action::AssertRecord {
                    subject,
                    predicate,
                    object,
                    quantity,
                    unit,
                } => {
                    let subject_uid = self.resolve(&subject).await?;
                    let object_uid = match object {
                        Some(object) => Some(self.resolve(&object).await?),
                        None => None,
                    };
                    let predicate_uid = store::concepts::resolve(&self.store.pool, &predicate)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(predicate))?;
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: subject_uid.clone(),
                                mutation: crate::record_change::Mutation::Assertion {
                                    predicate: predicate_uid.clone(),
                                    object: object_uid.clone(),
                                    quantity,
                                    unit,
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                    if object_uid.is_some()
                        && is_order_like(&self.store.pool, &predicate_uid).await?
                    {
                        for cycle in kind_cycles(&self.store.pool, &predicate_uid).await? {
                            if cycle.contains(&subject_uid)
                                || object_uid
                                    .as_ref()
                                    .is_some_and(|object| cycle.contains(object))
                            {
                                outcome.warnings.push(format!(
                                    "these {} records form a loop: {}",
                                    cycle.len(),
                                    cycle.join(" -> ")
                                ));
                            }
                        }
                    }
                }
                Action::PreviewInstinct => {
                    outcome.data = Some(serde_json::to_value(self.preview_instinct(actor.as_deref()).await?)?);
                }
                Action::ImportInstinct { fingerprint } => {
                    outcome = self.import_instinct(&fingerprint, actor.as_deref(), now).await?;
                }
                Action::ConfigureFiote {
                    target,
                    prompt_parent,
                    run_assigned,
                } => {
                    let uid = self.resolve(&target).await?;
                    self.configure_fiote(
                        &uid,
                        prompt_parent.as_deref(),
                        run_assigned,
                        actor.as_deref(),
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            uid,
                            actor,
                            serde_json::json!({"fiote_configured":true}),
                            now,
                        )
                        .await?;
                }
                Action::CreateAgent { head, operated_by } => {
                    let head = head.trim();
                    if head.is_empty() {
                        return Err(EngineError::Consequence(
                            "give the Agent a name you will recognise in an assignee list".into(),
                        ));
                    }
                    let operator = match &operated_by {
                        Some(person) => {
                            let uid = self.resolve(person).await?;
                            let row = store::records::get(&self.store.pool, &uid)
                                .await?
                                .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                            if row.kind != RecordKind::Person.as_str() {
                                return Err(EngineError::Consequence(
                                    "an Agent is operated by a Person".into(),
                                ));
                            }
                            Some(uid)
                        }
                        None => None,
                    };
                    let actor_concept = store::concepts::ensure(&self.store.pool, "actor").await?;
                    for child in ["person", "agent"] {
                        let uid = store::concepts::ensure(&self.store.pool, child).await?;
                        store::concepts::add_parent(&self.store.pool, &uid, &actor_concept).await?;
                    }
                    let created = self
                        .act(
                            Action::CreateRecord {
                                slug: None,
                                kind: RecordKind::Person,
                                head: head.into(),
                                body: String::new(),
                                quantity: 0.0,
                            },
                            actor.clone(),
                        )
                        .await?;
                    let uid = created
                        .created
                        .ok_or_else(|| EngineError::Consequence("Agent was not created".into()))?;
                    outcome.facts.extend(created.facts);
                    self.act(
                        Action::AssertRecord {
                            subject: uid.clone(),
                            predicate: "agent".into(),
                            object: None,
                            quantity: None,
                            unit: None,
                        },
                        actor.clone(),
                    )
                    .await?;
                    self.act(
                        Action::SetIdentity {
                            subject: uid.clone(),
                            predicate: Some("agent".into()),
                        },
                        actor.clone(),
                    )
                    .await?;
                    if let Some(operator) = operator {
                        store::concepts::ensure(&self.store.pool, "operated-by").await?;
                        self.act(
                            Action::AssertRecord {
                                subject: uid.clone(),
                                predicate: "operated-by".into(),
                                object: Some(operator),
                                quantity: None,
                                unit: None,
                            },
                            actor.clone(),
                        )
                        .await?;
                    }
                    outcome.created = Some(uid);
                }
                Action::RetractAssertion { assertion } => {
                    let row = store::assertions::get(&self.store.pool, &assertion)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(assertion.clone()))?;
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: row.subject_uid,
                                mutation: crate::record_change::Mutation::RetractAssertion {
                                    assertion,
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                }
                action @ Action::RefineAssertion { .. } => {
                    outcome = self.edit_record_relations_as(action, actor.as_deref(), now).await?;
                }
                action @ Action::RetractRecord { .. } => {
                    outcome = self.edit_record_relations_as(action, actor.as_deref(), now).await?;
                }
                action @ Action::SetIdentity { .. } => {
                    outcome = self.edit_record_relations_as(action, actor.as_deref(), now).await?;
                }
                _ => {
                    return Err(EngineError::Consequence(
                        "Invalid internal action dispatch".into(),
                    ));
                }
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
