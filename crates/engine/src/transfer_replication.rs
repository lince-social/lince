use store::transfer_replication::{self as replication, Message};

use crate::{Engine, EngineError, sync::WireOp};

const TABLE: &str = "transfer_replication";

impl Engine {
    pub(crate) async fn export_transfer_transactions(
        &self,
        organ: &str,
        vector: &[store::sync_ops::VectorEntry],
        limit: usize,
    ) -> Result<Vec<WireOp>, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| {
                EngineError::Consequence("Transfer sync requires a local Cell".into())
            })?;
        if cell.organ_uid != organ {
            return Err(EngineError::Forbidden(
                "private Transfer state is only shared with this Organ's Cells".into(),
            ));
        }
        let signer = self.organ_signer.lock().await.clone().ok_or_else(|| {
            EngineError::Consequence("Transfer sync requires the Cell signing key".into())
        })?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let (mut sequence, mut previous) = replication::head(&mut tx, organ, &cell.uid).await?;
        for (end, mut transaction) in
            replication::pending(&mut tx, organ, &cell.uid, limit.min(128)).await?
        {
            sequence += 1;
            transaction.sequence = sequence;
            transaction.previous = previous;
            let signature = signer.sign_bytes(&Message::signing_bytes(&transaction)?);
            let message = Message {
                transaction,
                signature,
            };
            previous = message.hash()?;
            replication::save(&mut tx, &message, Some(end)).await?;
        }
        tx.commit().await?;
        let messages =
            replication::missing(&self.store.pool, organ, vector, limit.min(128)).await?;
        let mut result = Vec::new();
        let mut bytes = 0;
        for message in messages {
            let raw = serde_json::to_string(&message).map_err(replication::invalid)?;
            bytes += raw.len();
            if bytes > 8 * 1024 * 1024 {
                break;
            }
            let data = &message.transaction;
            result.push(WireOp {
                tbl: TABLE.into(),
                uid: format!("{}:{}", data.cell, data.sequence),
                field: String::new(),
                kind: "transaction".into(),
                value: Some(raw),
                hlc: 0,
                actor_cell: data.cell.clone(),
                organ_uid: organ.into(),
                fact: None,
            });
        }
        Ok(result)
    }

    async fn verified_transfer_message(
        &self,
        op: &WireOp,
        organ: &str,
    ) -> Result<Message, EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        if local.uid != organ || op.organ_uid != organ {
            return Err(EngineError::Forbidden(
                "private Transfer state is only accepted from this Organ's Cells".into(),
            ));
        }
        let raw = op.value.as_deref().ok_or_else(|| {
            EngineError::Consequence("Transfer transaction has no payload".into())
        })?;
        if raw.len() > replication::MAX_MESSAGE_BYTES {
            return Err(EngineError::Consequence(
                "Transfer transaction exceeds its byte budget".into(),
            ));
        }
        let message: Message = serde_json::from_str(raw).map_err(replication::invalid)?;
        let data = &message.transaction;
        if data.organ != organ
            || data.cell != op.actor_cell
            || op.uid != format!("{}:{}", data.cell, data.sequence)
            || data.sequence <= 0
            || op.kind != "transaction"
            || op.hlc != 0
            || !op.field.is_empty()
            || op.fact.is_some()
        {
            return Err(EngineError::Consequence(
                "Transfer transaction identity does not match its envelope".into(),
            ));
        }
        let roster = self.roster_of(organ).await?.ok_or_else(|| {
            EngineError::Consequence("Transfer sync requires a current Cell roster".into())
        })?;
        if !crate::roster::roster_signature_is_valid(&roster) {
            return Err(EngineError::Forbidden(
                "Transfer sync requires unexpired Cell authority".into(),
            ));
        }
        let author = roster
            .roster
            .cells
            .iter()
            .find(|cell| cell.cell_uid == data.cell && cell.may(crate::roster::CAP_WRITE))
            .ok_or_else(|| {
                EngineError::Forbidden(
                    "Transfer transaction author has no writing authority".into(),
                )
            })?;
        if !crate::roster::verify_with(
            &author.operational_key,
            &Message::signing_bytes(data)?,
            &message.signature,
        ) {
            return Err(EngineError::Forbidden(
                "Transfer transaction signature does not verify".into(),
            ));
        }
        if data.changes.len() > replication::MAX_CHANGES
            || data.facts.len() > replication::MAX_CHANGES
            || data.prerequisites.len() > replication::MAX_CHANGES
        {
            return Err(EngineError::Consequence(
                "Transfer transaction exceeds its row budget".into(),
            ));
        }
        Ok(message)
    }

    pub(crate) async fn transfer_signature_keys(
        &self,
        ops: &[WireOp],
        organ: &str,
    ) -> Result<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>, EngineError>
    {
        let mut messages = Vec::new();
        for op in ops.iter().filter(|op| op.tbl == TABLE) {
            messages.push(self.verified_transfer_message(op, organ).await?);
        }
        let mut keys =
            std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
        if messages.is_empty() {
            return Ok(keys);
        }
        let mut connection = self.store.pool.acquire().await?;
        let table = replication::schema::Table::read(&mut connection, "identity_key").await?;
        let mut identities = std::collections::BTreeMap::new();
        for message in messages {
            for prerequisite in message
                .transaction
                .prerequisites
                .iter()
                .filter(|row| row.table == "identity_key")
            {
                table.validate(&prerequisite.row)?;
                let actor = prerequisite.row["actor_uid"]
                    .as_str()
                    .ok_or_else(|| replication::invalid("identity key requires an actor"))?;
                let public_key = prerequisite.row["public_key"]
                    .as_str()
                    .ok_or_else(|| replication::invalid("identity key requires a public key"))?;
                if identities
                    .insert(table.key(&prerequisite.row)?, public_key.to_owned())
                    .is_some_and(|old| old != public_key)
                {
                    return Err(EngineError::Forbidden(
                        "Transfer prerequisites contain conflicting identity keys".into(),
                    ));
                }
                if table
                    .current(&mut connection, &prerequisite.row)
                    .await?
                    .as_ref()
                    .is_some_and(|old| old["public_key"] != prerequisite.row["public_key"])
                {
                    return Err(EngineError::Forbidden(
                        "Transfer prerequisite conflicts with an accepted identity key".into(),
                    ));
                }
                keys.entry(actor.into())
                    .or_default()
                    .insert(public_key.into());
            }
        }
        Ok(keys)
    }

    pub(crate) async fn import_transfer_transaction(
        &self,
        op: &WireOp,
        organ: &str,
    ) -> Result<bool, EngineError> {
        let message = self.verified_transfer_message(op, organ).await?;
        let raw = op
            .value
            .as_deref()
            .expect("verified Transfer transaction has a payload");
        let data = &message.transaction;
        for fact in &data.facts {
            if let Some(signature) = &fact.signature {
                let actor = fact.actor_uid.as_deref().ok_or_else(|| {
                    EngineError::Consequence("signed Transfer Fact has no author".into())
                })?;
                let existing = crate::trust::verify_fact(&self.store, fact).await?;
                let supplied = data
                    .prerequisites
                    .iter()
                    .filter(|row| row.table == "identity_key" && row.row["actor_uid"] == actor)
                    .filter_map(|row| row.row["public_key"].as_str())
                    .any(|key| crate::roster::verify_with(key, fact.hash.as_bytes(), signature));
                if !existing && !supplied {
                    return Err(EngineError::Forbidden(
                        "original Transfer Fact signature does not verify".into(),
                    ));
                }
            }
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        let (sequence, previous) = replication::head(&mut tx, organ, &data.cell).await?;
        if data.sequence <= sequence {
            let saved: String = store::sqlx::query_scalar("SELECT payload FROM transfer_sync_message WHERE organ_uid = ? AND cell_uid = ? AND sequence = ?")
                .bind(organ).bind(&data.cell).bind(data.sequence).fetch_one(&mut *tx).await?;
            if serde_json::from_str::<serde_json::Value>(&saved).map_err(replication::invalid)?
                != serde_json::from_str::<serde_json::Value>(raw).map_err(replication::invalid)?
            {
                return Err(EngineError::Consequence(
                    "Transfer transaction identity was reused with conflicting evidence".into(),
                ));
            }
            tx.rollback().await?;
            return Ok(false);
        }
        if data.sequence != sequence + 1 {
            tx.rollback().await?;
            return Ok(false);
        }
        if data.previous != previous {
            return Err(EngineError::Consequence(
                "Transfer transaction does not extend the accepted history".into(),
            ));
        }
        let facts = replication::apply(&mut tx, &message).await?;
        if !store::sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut *tx)
            .await?
            .is_empty()
        {
            tx.rollback().await?;
            return Ok(false);
        }
        tx.commit().await?;
        for fact in facts {
            let _ = self.bus.send(fact);
        }
        self.notify_karma_deadline_change();
        Ok(true)
    }

    pub(crate) async fn transfer_transaction_saved(
        &self,
        op: &WireOp,
    ) -> Result<bool, EngineError> {
        let Some(raw) = &op.value else {
            return Ok(false);
        };
        let message: Message = serde_json::from_str(raw).map_err(replication::invalid)?;
        let data = &message.transaction;
        let saved: Option<String> = store::sqlx::query_scalar("SELECT payload FROM transfer_sync_message WHERE organ_uid = ? AND cell_uid = ? AND sequence = ?")
            .bind(&data.organ).bind(&data.cell).bind(data.sequence).fetch_optional(&self.store.pool).await?;
        Ok(saved.is_some_and(|saved| saved == *raw))
    }

    pub(crate) async fn require_transfer_state_writer(
        &self,
        table: &str,
        key: &[&str],
    ) -> Result<(), EngineError> {
        let Some(owner) = replication::owner(&self.store.pool, table, key).await? else {
            return Ok(());
        };
        if store::cells::local(&self.store.pool)
            .await?
            .is_some_and(|cell| cell.uid != owner)
        {
            return Err(EngineError::Conflict {
                code: "transfer_origin_cell_required",
                message: format!("Submit this Transfer change to its writing Cell {owner}"),
            });
        }
        Ok(())
    }
}
