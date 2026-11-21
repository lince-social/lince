// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DispatchFuture, Engine, EngineError, invalid_dispatch,
};

impl Engine {
    pub(super) fn dispatch_mailboxes(&self, action: Action) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::MailboxSavedStatus => {
                    outcome.data = Some(
                        serde_json::json!({"saved_mail":store::mailbox::delivery::inbox_status(&self.store.pool).await?}),
                    );
                }
                Action::MailboxSetCopies { copies } => {
                    if !(1..=2).contains(&copies) {
                        return Err(EngineError::Consequence(
                            "Choose one or two mailbox copies".into(),
                        ));
                    }
                    let mut config = store::cells::config(&self.store.pool, "lince.social")
                        .await?
                        .filter(serde_json::Value::is_object)
                        .unwrap_or_else(|| serde_json::json!({}));
                    config["mailbox_copies"] = copies.into();
                    store::cells::set_config(&self.store.pool, "lince.social", &config).await?;
                    outcome.data = Some(
                        serde_json::json!({"mailbox_copies":copies,"applies_to":"new outgoing envelopes"}),
                    );
                }
                Action::MailboxRetrySaved { uid } => {
                    if !store::mailbox::delivery::retry(&self.store.pool, &uid).await? {
                        return Err(EngineError::Consequence(
                            "There is no pending saved message to recover".into(),
                        ));
                    }
                    let imported = self.process_recovered_mail().await?;
                    outcome.data = Some(
                        serde_json::json!({"imported":imported,"saved_mail":store::mailbox::delivery::inbox_status(&self.store.pool).await?}),
                    );
                }
                Action::MailboxStatus => {
                    let swept = self.sweep_mailbox().await?;
                    let mut carrying = Vec::new();
                    for registration in store::mailbox::registrations(&self.store.pool).await? {
                        let held =
                            store::mailbox::carried_for(&self.store.pool, &registration.organ_uid)
                                .await?;
                        let known_as =
                            store::organs::contact(&self.store.pool, &registration.organ_uid)
                                .await?
                                .map(|contact| contact.head)
                                .filter(|name| !name.is_empty())
                                .unwrap_or_else(|| registration.label.clone());
                        carrying.push(serde_json::json!({
                            "organ_uid": registration.organ_uid,
                            "known_as": known_as,
                            "quota_bytes": registration.quota_bytes,
                            "held_bundles": held.bundles,
                            "held_bytes": held.bytes,
                            "registered_at": registration.registered_at,
                        }));
                    }
                    let pending = store::mailbox::pending_notices(&self.store.pool).await?;
                    outcome.data = Some(serde_json::json!({
                        "carrying_for": carrying,
                        "swept_just_now": swept,
                        "expiry_notices_pending": pending.len(),
                        "retention_days": crate::seal::RETENTION_DAYS,
                        "max_bundle_bytes": crate::mailbox::MAX_BUNDLE_BYTES,
                    }));
                }
                Action::MailboxCarryFor {
                    organ_uid,
                    label,
                    quota_bytes,
                } => {
                    let root_key = self.current_root_key(&organ_uid).await?.ok_or_else(|| {
                        EngineError::Consequence(
                        "no root key is held for that Organ: pair with them before offering to \
                         carry their mail"
                            .into(),
                    )
                    })?;
                    let quota = if quota_bytes > 0 {
                        quota_bytes
                    } else {
                        crate::mailbox::DEFAULT_QUOTA_BYTES
                    };
                    store::mailbox::register(
                        &self.store.pool,
                        &organ_uid,
                        &root_key,
                        &label,
                        quota,
                    )
                    .await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "quota_bytes": quota,
                    }));
                }
                Action::MailboxStopCarrying { organ_uid } => {
                    let held = store::mailbox::carried_for(&self.store.pool, &organ_uid).await?;
                    store::mailbox::deregister(&self.store.pool, &organ_uid).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "discarded_bundles": held.bundles,
                    }));
                }
                Action::MailboxPickupPoints => {
                    let points = self.own_pickup_points().await?;
                    let mut published = Vec::new();
                    for point in &points {
                        let probe = self.carrier_probe(&point.node_id).await;
                        let known_as = store::organs::contact(&self.store.pool, &point.organ_uid)
                            .await?
                            .map(|contact| contact.head)
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| point.label.clone());
                        let (state, waiting) = match &probe {
                            crate::wire::CarrierProbe::Carrying(waiting) => (
                                "carrying",
                                serde_json::json!({
                                    "bundles": waiting.bundles,
                                    "bytes": waiting.bytes,
                                    "oldest_expires_at": waiting.oldest_expires_at,
                                }),
                            ),
                            crate::wire::CarrierProbe::Refused => {
                                ("refused", serde_json::Value::Null)
                            }
                            crate::wire::CarrierProbe::Unreachable => {
                                ("unreachable", serde_json::Value::Null)
                            }
                        };
                        published.push(serde_json::json!({
                            "organ_uid": point.organ_uid,
                            "node_id": point.node_id,
                            "label": point.label,
                            "known_as": known_as,
                            "state": state,
                            "waiting": waiting,
                        }));
                    }
                    let candidates: Vec<serde_json::Value> =
                        store::organs::contacts(&self.store.pool)
                            .await?
                            .into_iter()
                            .filter(|contact| contact.trust == "known")
                            .filter(|contact| {
                                !points
                                    .iter()
                                    .any(|point| point.organ_uid == contact.record_uid)
                            })
                            .map(|contact| {
                                serde_json::json!({
                                    "organ_uid": contact.record_uid,
                                    "known_as": contact.head,
                                    "node_id": contact.node_id,
                                })
                            })
                            .collect();
                    outcome.data = Some(serde_json::json!({
                        "pickup": published,
                        "candidates": candidates,
                        "retention_days": crate::seal::RETENTION_DAYS,
                        "may_change": self.root_signer().await?.is_some(),
                    }));
                }
                Action::MailboxAddPickup {
                    organ_uid,
                    node_id,
                    label,
                } => {
                    let contact = store::organs::contact(&self.store.pool, &organ_uid).await?;
                    let node_id = if node_id.is_empty() {
                        contact
                            .as_ref()
                            .and_then(|contact| contact.node_id.clone())
                            .filter(|id| !id.is_empty())
                            .ok_or_else(|| {
                                EngineError::Consequence(
                                    "we have no address for that contact, so there is nothing to \
                                 publish."
                                        .into(),
                                )
                            })?
                    } else {
                        node_id
                    };
                    match self.carrier_probe(&node_id).await {
                        crate::wire::CarrierProbe::Carrying(_) => {}
                        crate::wire::CarrierProbe::Refused => {
                            return Err(EngineError::Consequence(
                                "they are not carrying mail for you. They have to add you first, \
                             and asking them from here is not built yet — nothing was \
                             published."
                                    .into(),
                            ));
                        }
                        crate::wire::CarrierProbe::Unreachable => {
                            return Err(EngineError::Consequence(
                                "they did not answer just now, so we cannot tell whether they \
                             carry mail for you. Nothing was published — try again."
                                    .into(),
                            ));
                        }
                    }
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "publishing a pickup point re-signs the roster, which needs the root \
                         key this Cell does not currently hold."
                                .into(),
                        )
                    })?;
                    let label = if label.is_empty() {
                        contact
                            .map(|contact| contact.head)
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| organ_uid.clone())
                    } else {
                        label
                    };
                    let mut points = self.own_pickup_points().await?;
                    points.retain(|point| point.organ_uid != organ_uid);
                    points.push(crate::roster::PickupPoint {
                        organ_uid: organ_uid.clone(),
                        node_id,
                        label,
                    });
                    let published = points.len();
                    self.set_pickup_points(&root, points).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "pickup_points": published,
                        "single_point_of_failure": published < 2,
                    }));
                }
                Action::MailboxRemovePickup { organ_uid } => {
                    let mut points = self.own_pickup_points().await?;
                    let Some(going) = points
                        .iter()
                        .find(|point| point.organ_uid == organ_uid)
                        .cloned()
                    else {
                        return Err(EngineError::Consequence(
                            "that is not one of your pickup points".into(),
                        ));
                    };
                    let stranded = match self.carrier_probe(&going.node_id).await {
                        crate::wire::CarrierProbe::Carrying(waiting) => Some(waiting.bundles),
                        _ => None,
                    };
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "removing a pickup point re-signs the roster, which needs the root \
                         key this Cell does not currently hold."
                                .into(),
                        )
                    })?;
                    points.retain(|point| point.organ_uid != organ_uid);
                    self.set_pickup_points(&root, points).await?;
                    outcome.data = Some(serde_json::json!({
                        "organ_uid": organ_uid,
                        "stranded_bundles": stranded,
                    }));
                }
                Action::MailboxCollectNow => {
                    let imported = self.collect_mail_now().await?;
                    outcome.data = Some(serde_json::json!({ "imported_ops": imported }));
                }
                Action::MailboxRequests => {
                    let pool = &self.store.pool;
                    let mut asks = Vec::new();
                    for row in store::mailbox::requests(pool).await? {
                        let known_as = store::organs::contact(pool, &row.organ_uid)
                            .await?
                            .and_then(|contact| contact.slug);
                        asks.push(serde_json::json!({
                            "organ_uid": row.organ_uid,
                            "known_as": known_as,
                            "claims_to_be": row.label,
                            "asked_at": row.asked_at,
                            "already_carried": store::mailbox::registration(pool, &row.organ_uid)
                                .await?
                                .is_some(),
                        }));
                    }
                    let invites: Vec<serde_json::Value> = store::mailbox::invites(pool)
                        .await?
                        .into_iter()
                        .map(|row| {
                            serde_json::json!({
                                "label": row.label,
                                "quota_bytes": row.quota_bytes,
                                "expires_at": row.expires_at,
                                "created_at": row.created_at,
                                "used_at": row.used_at,
                                "used_by": row.used_by,
                            })
                        })
                        .collect();
                    outcome.data = Some(serde_json::json!({
                        "requests": asks,
                        "invites": invites,
                        "invite_days": crate::mailbox::INVITE_TTL_DAYS,
                        "default_quota_bytes": crate::mailbox::DEFAULT_QUOTA_BYTES,
                    }));
                }
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
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
