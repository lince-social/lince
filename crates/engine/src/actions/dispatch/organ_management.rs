// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DispatchFuture, Engine, EngineError, invalid_dispatch,
};

impl Engine {
    pub(super) fn dispatch_organ_management(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::ProposeGroup {
                    thread,
                    title,
                    organs,
                } => {
                    let group = self
                        .propose_group(&thread, &title, &organs, actor.as_deref())
                        .await?;
                    outcome.created = Some(group.membership.root);
                }
                Action::SetGroupPerson {
                    root,
                    person,
                    allowed,
                } => {
                    self.set_group_person(&root, &person, allowed, actor.as_deref())
                        .await?;
                }
                Action::RemoveGroupOrgan { root, organ } => {
                    self.remove_group_organ(&root, &organ, actor.as_deref())
                        .await?;
                }
                Action::GrantOrganLogin { organ, person_name } => {
                    let organ_uid = self.resolve(&organ).await?;
                    let contact = store::organs::contact(&self.store.pool, &organ_uid)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
                    if contact.trust != "known" {
                        return Err(EngineError::Consequence(
                            "only a known contact may be given a login".into(),
                        ));
                    }
                    if let Some(person) =
                        store::logins::person_for_organ(&self.store.pool, &organ_uid).await?
                    {
                        outcome.created = Some(person);
                        return Ok(ControlFlow::Continue(outcome));
                    }
                    let person_name = person_name.trim();
                    if person_name.is_empty() {
                        return Err(EngineError::Consequence(
                            "give the Person a name you will recognise".into(),
                        ));
                    }
                    let person = store::records::create(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Person,
                            head: person_name,
                            body: "",
                            quantity: store::exact::zero(),
                        },
                    )
                    .await?;
                    store::logins::grant(&self.store.pool, &organ_uid, &person.uid).await?;
                    outcome.warnings.push(
                    "Assign this Person a role and grant visibility to the records they may use."
                        .into(),
                );
                    outcome.created = Some(person.uid);
                }
                Action::RevokeOrganLogin { organ } => {
                    let organ_uid = self.resolve(&organ).await?;
                    store::logins::revoke(&self.store.pool, &organ_uid).await?;
                }
                Action::RosterCreateOrgan => {
                    let roster = self.create_organ_identity().await?;
                    outcome.data = Some(serde_json::json!({"organ_uid": roster.roster.organ_uid}));
                    outcome
                        .warnings
                        .push("Organ created. You can now enrol your other devices.".into());
                }
                Action::RosterSetKarmaExecution {
                    cell_uid,
                    enabled,
                    additional,
                    expected_roster_version,
                } => {
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Forbidden(
                            "The Organ root key is needed to change device execution permission"
                                .into(),
                        )
                    })?;
                    let roster = self
                        .choose_karma_executor(
                            &root,
                            &cell_uid,
                            enabled,
                            additional,
                            expected_roster_version,
                        )
                        .await?;
                    outcome.data = Some(
                        serde_json::json!({"version":roster.roster.version,"karma":self.karma_device_execution().await?}),
                    );
                }
                Action::RosterRenameCell { cell_uid, label } => {
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "Manage device names on the Cell holding the Organ root key".into(),
                        )
                    })?;
                    self.rename_roster_cell(&root, &cell_uid, &label).await?;
                }
                Action::RosterEnrolToken => {
                    let root = self.root_signer().await?.ok_or_else(|| {
                        EngineError::Consequence(
                            "the root key is not on this Cell — bring it back to enrol a device"
                                .into(),
                        )
                    })?;
                    if !self
                        .key_chains(&root.actor_uid, &root.public_key_b64())
                        .await?
                    {
                        return Err(EngineError::Forbidden(
                            "The configured owner key is not currently recognized".into(),
                        ));
                    }
                    let token = self.issue_enrolment_token().await?;
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                    let root_key = root.public_key_b64();
                    let pairing =
                        self.current_pairing_invite()
                            .await?
                            .or(store::records::get_extension(
                                &self.store.pool,
                                &organ.uid,
                                "lince.pairing",
                            )
                            .await?
                            .and_then(|fields| {
                                fields
                                    .get("invite")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                            })
                            .and_then(|encoded| {
                                crate::pairing::PairingInvite::decode(&encoded).ok()
                            }));
                    match pairing {
                        Some(pairing) => {
                            let invite = crate::pairing::EnrolmentInvite {
                                node_id: pairing.node_id,
                                organ_uid: organ.uid.clone(),
                                root_key,
                                token: token.clone(),
                                addrs: pairing.addrs,
                            };
                            outcome.created = Some(invite.encode());
                            outcome.data = Some(serde_json::json!({
                                "code": invite.encode(),
                                "qr_svg": invite.qr_svg().unwrap_or_default(),
                                "expires_in_minutes": crate::roster::ENROLMENT_TOKEN_TTL_MINUTES,
                            }));
                        }
                        None => {
                            return Err(EngineError::Consequence(
                                "this Cell has no network identity yet, so a device cannot be \
                             told where to reach it. Wait for the endpoint to bind and try \
                             again."
                                    .into(),
                            ));
                        }
                    }
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
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
