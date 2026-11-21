// non-lean-yet

//! Dispatch for authenticated peer requests and their replies.

use super::{
    ExpiredMail, HeldIntroduction, MailboxBundle, RevocationCert, SuccessionCert, Wire,
    WireRequest, WireResponse,
};
use crate::{error::EngineError, sync::Introduction};

impl Wire {
    pub(super) async fn handle(
        &self,
        authenticated: &str,
        peer: &str,
        request: WireRequest,
    ) -> WireResponse {
        if matches!(
            &request,
            WireRequest::MoveOffer { .. }
                | WireRequest::MoveBundle { .. }
                | WireRequest::MoveCancel { .. }
        ) && !self.may_represent().await
        {
            return WireResponse::Refused {
                code: "owner_device_required".into(),
                message: "Move offers require a device authorized to represent this Organ".into(),
            };
        }
        if matches!(
            &request,
            WireRequest::MoveOffer { .. }
                | WireRequest::MoveBundle { .. }
                | WireRequest::MoveCancel { .. }
        ) {
            let authorized = self
                .engine
                .roster_of(authenticated)
                .await
                .ok()
                .flatten()
                .is_some_and(|r| {
                    crate::roster::roster_signature_is_valid(&r)
                        && r.roster
                            .cells
                            .iter()
                            .any(|c| c.node_id == peer && c.may(crate::roster::CAP_WRITE))
                });
            if !authorized {
                return WireResponse::Refused {
                    code: "move_device_required".into(),
                    message: "Reconnect to refresh the sender's authorized device list".into(),
                };
            }
        }
        match request {
            WireRequest::Location { request } => {
                match self
                    .engine
                    .location_peer(authenticated, peer, request)
                    .await
                {
                    Ok(data) => WireResponse::Location { data },
                    Err(error) => WireResponse::Refused {
                        code: "location_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::SandPackages { query } => {
                match self.engine.public_sand_packages(authenticated, query).await {
                    Ok(response) => WireResponse::SandPackages { response },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MoveOffer { uid, preview } => {
                match self
                    .engine
                    .receive_move_offer(authenticated, &uid, preview)
                    .await
                {
                    Ok(state) => {
                        let receipt = if state == "received" {
                            store::record_move::offers::get(&self.engine.store.pool, &uid)
                                .await
                                .ok()
                                .flatten()
                                .map(|o| o.preview.hash)
                        } else {
                            None
                        };
                        WireResponse::MoveStatus { state, receipt }
                    }
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MoveBundle { uid, bundle } => {
                match self
                    .engine
                    .receive_move_bundle(authenticated, &uid, bundle)
                    .await
                {
                    Ok(hash) => WireResponse::MoveStatus {
                        state: "received".into(),
                        receipt: Some(hash),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MoveCancel { uid } => {
                match self.engine.receive_move_cancel(authenticated, &uid).await {
                    Ok(()) => WireResponse::Applied { applied: 0 },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::Presence { records, entries } => {
                match self
                    .engine
                    .exchange_presence(authenticated, &records, entries)
                    .await
                {
                    Ok(entries) => WireResponse::Presence { entries },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::Introduction | WireRequest::Introduce { .. } => {
                match self.engine.introduction().await {
                    Ok(intro) => WireResponse::Introduction { intro },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::OrganAccessStatus => {
                match store::organ_access::for_peer(&self.engine.store.pool, authenticated).await {
                    Ok(status) => WireResponse::OrganAccessStatus { status },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::PushOps { batch } => {
                if batch.from_organ != authenticated {
                    tracing::warn!(
                        claimed = %batch.from_organ,
                        proven = %authenticated,
                        "peer pushed a batch attributed to another Organ"
                    );
                    return WireResponse::Refused {
                        code: "batch_peer_mismatch".into(),
                        message: "the batch does not belong to the Organ on this connection".into(),
                    };
                }
                match self.engine.receive_sync_batch(authenticated, &batch).await {
                    Ok(applied) => WireResponse::BatchSaved {
                        applied,
                        complete: self.engine.batch_is_saved(&batch).await.unwrap_or(false),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchRoster => {
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(Some(organ)) => organ.uid,
                    Ok(None) => {
                        return WireResponse::Roster { roster: None };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self.engine.roster_of(&local).await {
                    Ok(roster) => WireResponse::Roster { roster },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchRevocations => {
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(Some(organ)) => organ.uid,
                    _ => return WireResponse::Revocations { certs: Vec::new() },
                };
                match self.engine.published_revocations(&local).await {
                    Ok(rows) => WireResponse::Revocations {
                        certs: rows
                            .into_iter()
                            .map(|(revoked_key, signature)| RevocationCert {
                                organ_uid: local.clone(),
                                revoked_key,
                                signature,
                            })
                            .collect(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::TransferPost { verb, body } => {
                let handler = self.transfer.lock().expect("transfer handler").clone();
                let Some(handler) = handler else {
                    return WireResponse::Error {
                        message: "this Cell does not deliver Transfers".into(),
                    };
                };
                match handler.handle(verb, body).await {
                    Ok(body) => WireResponse::Transfer { body },
                    Err(message) => WireResponse::Error { message },
                }
            }
            WireRequest::FetchSuccessions => {
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(Some(organ)) => organ.uid,
                    _ => return WireResponse::Successions { certs: Vec::new() },
                };
                match self.engine.published_successions(&local).await {
                    Ok(rows) => WireResponse::Successions {
                        certs: rows
                            .into_iter()
                            .map(|row| SuccessionCert {
                                organ_uid: local.clone(),
                                old_key: row.old_key,
                                new_key: row.new_key,
                                created_at: row.created_at,
                                signature: row.signature,
                            })
                            .collect(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::Enrol {
                token,
                cell_uid,
                node_id,
                label,
                operational_key,
                sealing_key,
            } => {
                let root = match self.engine.root_signer().await {
                    Ok(Some(root)) => root,
                    Ok(None) => {
                        return WireResponse::Refused {
                            code: "root_key_absent".into(),
                            message: "this Cell cannot enrol devices without its root key".into(),
                        };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self
                    .engine
                    .redeem_enrolment(
                        &root,
                        &token,
                        crate::roster::CellEntry {
                            cell_uid,
                            node_id,
                            label,
                            operational_key,
                            sealing_key,
                            front_door: false,
                            capabilities: crate::roster::full_capabilities(),
                        },
                    )
                    .await
                {
                    Ok(roster) => {
                        tracing::info!(version = roster.roster.version, "device enrolled");
                        WireResponse::Roster {
                            roster: Some(roster),
                        }
                    }
                    Err(error) => WireResponse::Refused {
                        code: "enrolment_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::GroupOffer { group } => {
                match self.engine.receive_group(authenticated, &group).await {
                    Ok(()) => WireResponse::Applied { applied: 1 },
                    Err(error) => WireResponse::Refused {
                        code: "group_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::Call { request } => {
                match self.engine.coordinate_call(authenticated, request).await {
                    Ok(snapshot) => WireResponse::Call { snapshot },
                    Err(error) => WireResponse::Refused {
                        code: "call_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::GroupAccept { root } => {
                match self.engine.admit_group_organ(authenticated, &root).await {
                    Ok(group) => WireResponse::Group { group },
                    Err(error) => WireResponse::Refused {
                        code: "group_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::OfferGrant {
                root,
                title,
                intro: _,
            } => {
                if store::offers::standing_refusal(
                    &self.engine.store.pool,
                    store::offers::OfferKind::ThreadInvite,
                    &root,
                    authenticated,
                )
                .await
                .unwrap_or(false)
                {
                    tracing::info!(
                        from = %authenticated,
                        "offer absorbed: this conversation was declined and the refusal still stands"
                    );
                    return WireResponse::Applied { applied: 0 };
                }
                match store::replica::offer(&self.engine.store.pool, &root, authenticated).await {
                    Ok(()) => {
                        match store::invites::put(
                            &self.engine.store.pool,
                            authenticated,
                            &root,
                            &title,
                        )
                        .await
                        {
                            Ok(Some(_)) => {
                                tracing::info!(%root, from = %authenticated, %title, "conversation offered");
                                self.engine.notify_notifications_changed();
                            }
                            Ok(None) => {
                                tracing::info!(
                                    from = %authenticated,
                                    "offer ignored: this Organ already has an invite pending"
                                );
                            }
                            Err(error) => {
                                return WireResponse::Error {
                                    message: error.to_string(),
                                };
                            }
                        }
                        WireResponse::Applied { applied: 0 }
                    }
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::AcceptGrant { root } => {
                match store::replica::state(&self.engine.store.pool, &root, authenticated).await {
                    Ok(Some(_)) => {
                        match store::replica::accept(&self.engine.store.pool, &root, authenticated)
                            .await
                        {
                            Ok(()) => WireResponse::Applied { applied: 0 },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Ok(None) => WireResponse::Refused {
                        code: "no_such_offer".into(),
                        message: "no grant was offered to you on that root".into(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::DeclineGrant { root } => {
                match store::replica::state(&self.engine.store.pool, &root, authenticated).await {
                    Ok(Some(_)) => {
                        let _ = store::offers::refuse(
                            &self.engine.store.pool,
                            store::offers::OfferKind::ReplicaGrant,
                            &root,
                            authenticated,
                        )
                        .await;
                        match store::replica::revoke(&self.engine.store.pool, &root, authenticated)
                            .await
                        {
                            Ok(()) => WireResponse::Applied { applied: 0 },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Ok(None) => WireResponse::Applied { applied: 0 },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::PushGrantOps { root, batch } => {
                if batch.from_organ != authenticated {
                    tracing::warn!(
                        claimed = %batch.from_organ,
                        proven = %authenticated,
                        "peer pushed a grant batch attributed to another Organ"
                    );
                    return WireResponse::Refused {
                        code: "batch_peer_mismatch".into(),
                        message: "the batch does not belong to the Organ on this connection".into(),
                    };
                }
                match self.engine.import_grant_batch(&root, &batch).await {
                    Ok(applied) => WireResponse::BatchSaved {
                        applied,
                        complete: self.engine.batch_is_saved(&batch).await.unwrap_or(false),
                    },
                    Err(error) => WireResponse::Refused {
                        code: "grant_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchGrantOpsSince {
                root,
                vector,
                limit,
            } => {
                match self
                    .engine
                    .export_conversation_page(authenticated, &root, &vector, limit)
                    .await
                {
                    Ok(page) => WireResponse::Ops {
                        from_organ: page.batch.from_organ,
                        ops: page.batch.ops,
                        head: page.head,
                    },
                    Err(EngineError::Forbidden(message)) => WireResponse::Refused {
                        code: "grant_denied".into(),
                        message,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchOpsSince { vector, limit } => {
                match self
                    .engine
                    .export_sync_page(authenticated, &vector, limit)
                    .await
                {
                    Ok(page) => WireResponse::Ops {
                        from_organ: page.batch.from_organ,
                        ops: page.batch.ops,
                        head: page.head,
                    },
                    Err(EngineError::Conflict { code, message }) => WireResponse::Refused {
                        code: code.into(),
                        message,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchReference { root, record } => {
                match store::replica::is_accepted(&self.engine.store.pool, &root, authenticated)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        return WireResponse::Refused {
                            code: "no_grant".into(),
                            message: "no accepted grant on that conversation".into(),
                        };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                match store::replica::root_references_record(
                    &self.engine.store.pool,
                    &root,
                    &record,
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => return reference_gone(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                let contact = store::organs::contact(&self.engine.store.pool, authenticated)
                    .await
                    .ok()
                    .flatten();
                match self.engine.outgoing_hidden_from_organ(authenticated).await {
                    Ok(hidden) if hidden.contains(&record) => return reference_gone(),
                    Ok(_) => {}
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                let protein = protein::Protein {
                    source: protein::Source::Record,
                    filter: vec![protein::Predicate::UidEq(record.clone())],
                    fields: contact.and_then(|contact| contact.scope_fields),
                    include: Default::default(),
                    aggregate: None,
                    order: Vec::new(),
                    limit: Some(1),
                };
                match protein::execute(&self.engine.store, &protein).await {
                    Ok(rows) => match rows.into_iter().next() {
                        Some(row) => {
                            if let Err(error) = store::replica::note_reference_read(
                                &self.engine.store.pool,
                                authenticated,
                                &record,
                                &root,
                            )
                            .await
                            {
                                tracing::warn!(%error, "could not record a reference read");
                            }
                            WireResponse::Reference { row }
                        }
                        None => reference_gone(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchVector { organ_uid } => {
                let local = store::organs::local(&self.engine.store.pool)
                    .await
                    .ok()
                    .flatten()
                    .map(|organ| organ.uid)
                    .unwrap_or_default();
                if organ_uid != authenticated && organ_uid != local {
                    return WireResponse::Refused {
                        code: "not_your_vector".into(),
                        message: "a vector is served only for your Organ or ours".into(),
                    };
                }
                match store::sync_ops::version_vector_for_organ(&self.engine.store.pool, &organ_uid)
                    .await
                {
                    Ok(mut vector) => {
                        if authenticated != local {
                            vector.retain(|entry| {
                                !entry
                                    .actor_cell
                                    .starts_with(nucleus::social::CONVERSATION_VECTOR_PREFIX)
                            });
                        }
                        WireResponse::Vector { vector }
                    }
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchDoorRequests { limit } => {
                if !self.is_sibling(authenticated).await {
                    return WireResponse::Refused {
                        code: "not_a_cell_of_this_organ".into(),
                        message: "only this Organ's own Cells may read the door".into(),
                    };
                }
                match store::door::held(&self.engine.store.pool, limit.clamp(1, 500)).await {
                    Ok(rows) => WireResponse::DoorRequests {
                        requests: rows
                            .into_iter()
                            .filter_map(|row| {
                                serde_json::from_str::<Introduction>(&row.intro)
                                    .ok()
                                    .map(|intro| HeldIntroduction {
                                        uid: row.uid,
                                        node_id: row.node_id,
                                        intro,
                                        received_at: row.received_at,
                                    })
                            })
                            .collect(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::ReleaseDoorRequests { uids } => {
                if !self.is_sibling(authenticated).await {
                    return WireResponse::Refused {
                        code: "not_a_cell_of_this_organ".into(),
                        message: "only this Organ's own Cells may clear the door".into(),
                    };
                }
                match store::door::release(&self.engine.store.pool, &uids).await {
                    Ok(()) => WireResponse::Applied {
                        applied: uids.len(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxDeposit { body } => {
                match self.engine.accept_bundle(&body, peer).await {
                    Ok(uid) => WireResponse::MailboxAccepted { uid },
                    Err(refusal) => WireResponse::Refused {
                        code: refusal.code().into(),
                        message: refusal.to_string(),
                    },
                }
            }
            WireRequest::MailboxWaiting { organ_uid, roster } => {
                match self.engine.may_collect(&organ_uid, peer, &roster).await {
                    Ok(true) => {
                        match store::mailbox::waiting(&self.engine.store.pool, &organ_uid).await {
                            Ok(waiting) => WireResponse::MailboxWaiting {
                                bundles: waiting.bundles,
                                bytes: waiting.bytes,
                                oldest_expires_at: waiting.oldest_expires_at,
                            },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Ok(false) => not_your_mailbox(),
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxCollect {
                organ_uid,
                roster,
                limit,
            } => match self.engine.may_collect(&organ_uid, peer, &roster).await {
                Ok(true) => {
                    match self
                        .engine
                        .bundles_for_device(&organ_uid, peer, limit.clamp(1, 256))
                        .await
                    {
                        Ok(held) => WireResponse::MailboxBundles {
                            bundles: held
                                .into_iter()
                                .map(|bundle| MailboxBundle {
                                    uid: bundle.uid,
                                    from_organ: bundle.from_organ,
                                    from_cell: bundle.from_cell,
                                    body: bundle.body,
                                    received_at: bundle.received_at,
                                    expires_at: bundle.expires_at,
                                })
                                .collect(),
                        },
                        Err(error) => WireResponse::Error {
                            message: error.to_string(),
                        },
                    }
                }
                Ok(false) => not_your_mailbox(),
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::MailboxCollected {
                organ_uid,
                roster,
                uids,
            } => match self.engine.may_collect(&organ_uid, peer, &roster).await {
                Ok(true) => match self.engine.confirm_collected(&organ_uid, peer, &uids).await {
                    Ok(dropped) => WireResponse::Applied {
                        applied: dropped as usize,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                },
                Ok(false) => not_your_mailbox(),
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::MailboxAskToCarry { roster } => {
                match self.engine.note_carry_request(&roster).await {
                    Ok(()) => WireResponse::MailboxAsked,
                    Err(error) => WireResponse::Refused {
                        code: "mailbox_ask_refused".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxRedeemInvite { token, roster } => {
                match self.engine.redeem_mailbox_invite(&token, &roster).await {
                    Ok(registration) => WireResponse::MailboxCarrying {
                        label: registration.label,
                        quota_bytes: registration.quota_bytes,
                    },
                    Err(error) => WireResponse::Refused {
                        code: "mailbox_invite_refused".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::PublishSealingKey {
                cell_uid,
                sealing_key,
            } => {
                match self
                    .engine
                    .republish_sealing_key(peer, &cell_uid, sealing_key)
                    .await
                {
                    Ok(()) => WireResponse::Applied { applied: 1 },
                    Err(error) => WireResponse::Refused {
                        code: "sealing_key_refused".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxExpiries => match self.engine.expiries_for(peer).await {
                Ok(notices) => WireResponse::MailboxExpired {
                    expired: notices
                        .into_iter()
                        .map(|notice| ExpiredMail {
                            uid: notice.uid,
                            to_organ: notice.to_organ,
                            bytes: notice.bytes,
                            received_at: notice.received_at,
                            expired_at: notice.expires_at,
                        })
                        .collect(),
                },
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::MailboxExpiriesHeard { uids } => {
                match self.engine.forget_expiries(peer, &uids).await {
                    Ok(dropped) => WireResponse::Applied {
                        applied: dropped as usize,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
        }
    }

    async fn is_sibling(&self, authenticated: &str) -> bool {
        !authenticated.is_empty()
            && store::organs::local(&self.engine.store.pool)
                .await
                .ok()
                .flatten()
                .is_some_and(|organ| organ.uid == authenticated)
    }
}

fn not_your_mailbox() -> WireResponse {
    WireResponse::Refused {
        code: "mailbox_not_yours".into(),
        message: "this mailbox holds nothing you may collect".into(),
    }
}

fn reference_gone() -> WireResponse {
    WireResponse::Refused {
        code: "not_shared".into(),
        message: "that is no longer shared with you".into(),
    }
}
