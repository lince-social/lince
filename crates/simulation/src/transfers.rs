use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::actions::Action;
use nucleus::simulation::Cause;

use crate::scenario::Invocation;
use crate::world::{Payload, World};

impl World {
    pub(crate) async fn refresh_transfer(
        &mut self,
        cell: &str,
        id: &str,
        transfer: &str,
        person: &str,
        cause: Cause,
    ) -> crate::Result<()> {
        let transfer = self.resolve_reference(transfer);
        let person = self.resolve_reference(person);
        let reference: String = store::sqlx::query_scalar(
            "SELECT uid FROM transfer_remote_reference WHERE transfer_uid = ? AND recipient_person_uid = ? AND state = 'active'",
        ).bind(&transfer).bind(&person).fetch_one(&self.nodes[cell].engine().store.pool).await?;
        self.invoke(
            cell,
            &Invocation {
                id: id.into(),
                actor: None,
                action: Action::RefreshTransferDelivery {
                    transfer,
                    delivery: reference,
                    person: Some(person),
                    request_id: id.into(),
                },
            },
            cause,
        )
        .await
    }

    pub(crate) async fn apply_received_transfer(
        &mut self,
        cell: &str,
        id: &str,
        transfer: &str,
        occurrence: &str,
        person: &str,
        local_record: &str,
        cause: Cause,
    ) -> crate::Result<()> {
        let transfer = self.resolve_reference(transfer);
        let occurrence = self.resolve_reference(occurrence);
        let person = self.resolve_reference(person);
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        if node
            .person_key
            .as_ref()
            .is_none_or(|signer| signer.actor_uid != person)
        {
            return Err(
                "received Transfer applications require the Cell's own selected Person key".into(),
            );
        }
        let handoff: String = store::sqlx::query_scalar(
            "SELECT h.uid FROM transfer_application_effect_handoff h
             LEFT JOIN transfer_remote_reference r ON r.uid = h.reference_uid
             WHERE h.transfer_uid = ? AND h.occurrence_uid = ? AND h.participant_person_uid = ?
               AND h.state = 'pending' AND (h.reference_uid = '' OR r.state = 'active')
             ORDER BY h.canonical_cumulative_before, h.uid LIMIT 1",
        )
        .bind(&transfer)
        .bind(&occurrence)
        .bind(&person)
        .fetch_optional(&node.engine().store.pool)
        .await?
        .ok_or("no received application is ready for this occurrence and Person")?;
        let query = protein::Protein {
            source: protein::Source::Transfer,
            filter: vec![protein::Predicate::UidEq(transfer.clone())],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        };
        let rows = node
            .execution
            .scope(protein::execute_for_with_signer(
                &node.engine().store,
                &query,
                None,
                Some(&person),
            ))
            .await?;
        let preview = rows
            .iter()
            .find(|row| row["uid"] == transfer)
            .and_then(|row| {
                row["application_handoffs"]
                    .as_array()
                    .or_else(|| row["social_delivery"]["application_handoffs"].as_array())
            })
            .and_then(|handoffs| handoffs.iter().find(|entry| entry["uid"] == handoff))
            .ok_or("received application preview unavailable")?;
        if preview["capabilities"]["apply"] != true {
            return Err(format!(
                "received application is blocked: {}",
                preview["blocking_reasons"]["apply"]
            )
            .into());
        }
        let mut action = preview["action_payloads"]["apply"].clone();
        action["local_record"] = self.resolve_reference(local_record).into();
        action["request_id"] = id.into();
        self.invoke(
            cell,
            &Invocation {
                id: id.into(),
                actor: None,
                action: serde_json::from_value(action)?,
            },
            cause,
        )
        .await
    }

    pub(crate) async fn decide_invitation(
        &mut self,
        cell: &str,
        id: &str,
        transfer: &str,
        person: &str,
        peer: Option<&str>,
        accept: bool,
        cause: Cause,
    ) -> crate::Result<()> {
        let transfer = self.resolve_reference(transfer);
        let person = self.resolve_reference(person);
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let (invitation, revision) = if peer.is_some() {
            let organ = store::organs::local(&node.engine().store.pool)
                .await?
                .ok_or("missing Organ")?;
            let projection: String = store::sqlx::query_scalar(
                "SELECT projection FROM transfer_remote_reference WHERE transfer_uid = ?
                 AND recipient_person_uid = ? AND recipient_organ_uid = ? AND state = 'active'",
            )
            .bind(&transfer)
            .bind(&person)
            .bind(&organ.uid)
            .fetch_one(&node.engine().store.pool)
            .await?;
            let projection: serde_json::Value = serde_json::from_str(&projection)?;
            let invitation = projection["invitations"]
                .as_array()
                .ok_or("missing invitations")?
                .iter()
                .find(|entry| {
                    entry["addressed_person"] == person || entry["addressed_person_uid"] == person
                })
                .and_then(|entry| entry["uid"].as_str())
                .ok_or("no visible invitation addressed to Person")?;
            (
                invitation.to_owned(),
                projection["revision"].as_u64().ok_or("missing revision")?,
            )
        } else {
            let invitation =
                store::transfers::invitations_for_transfer(&node.engine().store.pool, &transfer)
                    .await?
                    .into_iter()
                    .find(|invitation| invitation.addressed_person_uid == person)
                    .ok_or("no invitation addressed to Person")?;
            let revision = store::transfers::get(&node.engine().store.pool, &transfer)
                .await?
                .ok_or("Transfer missing")?
                .revision
                .try_into()?;
            (invitation.uid, revision)
        };
        let invocation = Invocation {
            id: id.into(),
            actor: None,
            action: if accept {
                Action::AcceptTransferInvitation {
                    invitation,
                    expected_revision: revision,
                    request_id: id.into(),
                    transfer: Some(transfer.clone()),
                    person: Some(person),
                }
            } else {
                Action::RejectTransferInvitation {
                    invitation,
                    request_id: id.into(),
                    transfer: Some(transfer.clone()),
                    person: Some(person),
                }
            },
        };
        if let Some(peer) = peer {
            self.transfer_command(cell, peer, &transfer, &invocation, (0, 1, 0, false), cause)
                .await
        } else {
            self.invoke(cell, &invocation, cause).await
        }
    }

    pub(crate) async fn transfer_command(
        &mut self,
        cell: &str,
        peer: &str,
        transfer: &str,
        invocation: &Invocation,
        delivery: (u64, u8, u64, bool),
        cause: Cause,
    ) -> crate::Result<()> {
        let action = crate::world::resolve(&invocation.action, &self.captured)?;
        let transfer = self.resolve_reference(transfer);
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let signer = node
            .person_key
            .as_ref()
            .ok_or("remote command requires this Cell's Person key")?;
        let terms = serde_json::to_value(&action)?;
        if terms["person"].as_str() != Some(signer.actor_uid.as_str()) {
            return Err("remote command cannot impersonate another Person".into());
        }
        let sender = store::organs::local(&node.engine().store.pool)
            .await?
            .ok_or("missing sender Organ")?;
        let origin = store::organs::local(&self.nodes[peer].engine().store.pool)
            .await?
            .ok_or("missing origin Organ")?;
        let reference = store::transfer_delivery::remote_reference_by_identity(
            &node.engine().store.pool,
            &origin.uid,
            &transfer,
            &signer.actor_uid,
            &sender.uid,
        )
        .await?
        .ok_or("remote command requires received Transfer terms")?;
        if reference.state != "active" {
            return Err("remote Transfer reference is inactive".into());
        }
        let mut command = nucleus::transfer_delivery::TransferRemoteCommandV1 {
            version: nucleus::transfer_delivery::TRANSFER_ENVELOPE_VERSION,
            command_uid: format!("simulation-command:{}:{}", cell, invocation.id),
            request_id: terms["request_id"]
                .as_str()
                .ok_or("remote command requires request id")?
                .into(),
            origin_organ_uid: origin.uid,
            sender_organ_uid: sender.uid,
            transfer_uid: transfer,
            expected_revision: terms["expected_revision"].as_u64(),
            actor_person_uid: signer.actor_uid.clone(),
            key_id: signer.key_id.clone(),
            public_key_base64: signer.public_key_b64(),
            session_id: format!("simulation:{}:{}", cell, invocation.id),
            session_challenge: nucleus::fact::sha256_hex(
                format!("{}:{}:{}", self.scenario.seed, cell, invocation.id).as_bytes(),
            ),
            sequence: 1,
            message_id: invocation.id.clone(),
            action_base64: B64.encode(serde_json::to_vec(&action)?),
            created_at: node.execution.now().to_rfc3339(),
            signature: String::new(),
        };
        command.signature = signer.sign_bytes(&command.signing_bytes());
        let row = node
            .execution
            .scope(store::transfer_delivery::persist_remote_command(
                &node.engine().store.pool,
                "outgoing",
                &command,
                node.execution.now(),
            ))
            .await?;
        let row = match row {
            store::transfer_delivery::RemoteCommandCommit::Applied(row)
            | store::transfer_delivery::RemoteCommandCommit::Replayed(row) => row,
        };
        let wire = node
            .execution
            .scope(cell::transfer::prepare_command(node.cell.runtime(), &row))
            .await?;
        self.queue(
            cell,
            peer,
            Payload::Command {
                input: invocation.id.clone(),
                wire,
            },
            delivery,
            cause.clone(),
        )?;
        self.observe(cell, cause).await
    }

    pub(crate) async fn settle_reviewed(
        &mut self,
        cell: &str,
        id: &str,
        occurrence: &str,
        person: &str,
        quantity: nucleus::DecimalValue,
        cause: Cause,
    ) -> crate::Result<()> {
        let occurrence = self.resolve_reference(occurrence);
        let person = self.resolve_reference(person);
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let query = protein::Protein {
            source: protein::Source::TransferSettlementPreview,
            filter: vec![
                protein::Predicate::UidEq(occurrence.clone()),
                protein::Predicate::QuantityEq(quantity),
            ],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        };
        let signer = node
            .person_key
            .as_ref()
            .map(|signer| signer.actor_uid.as_str());
        let rows = node
            .execution
            .scope(protein::execute_for_with_signer(
                &node.engine().store,
                &query,
                None,
                signer,
            ))
            .await?;
        let preview = rows.first().ok_or("settlement preview unavailable")?;
        let number = |field: &str| {
            preview[field]
                .as_f64()
                .ok_or_else(|| format!("settlement preview missing {field}"))
        };
        let action = Action::SettleTransferOccurrence {
            occurrence,
            request_id: id.into(),
            person: Some(person),
            canonical_quantity: number("canonical_quantity")?,
            expected_remaining_quantity: number("expected_remaining_quantity")?,
            expected_local_delta: number("expected_local_delta")?,
            expected_effects_hash: preview["expected_effects_hash"].as_str().map(str::to_owned),
            expected_application_formula_hash: preview["expected_application_formula_hash"]
                .as_str()
                .ok_or("settlement formula hash missing")?
                .into(),
            expected_application_formula_version: preview["expected_application_formula_version"]
                .as_u64()
                .ok_or("settlement formula version missing")?,
            expected_remainder_policy: match preview["expected_remainder_policy"].as_str() {
                Some("local_draft") => nucleus::transfer::TransferRemainderPolicy::LocalDraft,
                Some("visible") => nucleus::transfer::TransferRemainderPolicy::Visible,
                _ => return Err("settlement remainder policy missing".into()),
            },
        };
        self.invoke(
            cell,
            &Invocation {
                id: id.into(),
                actor: None,
                action,
            },
            cause,
        )
        .await
    }
}
