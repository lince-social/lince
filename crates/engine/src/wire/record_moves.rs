use super::*;

impl Wire {
    pub(super) async fn sync_record_moves(&self) -> Result<usize, EngineError> {
        if !self.may_represent().await {
            return Ok(0);
        }
        let pool = &self.engine.store.pool;
        let mut completed = 0;
        for offer in store::record_move::offers::list(pool)
            .await?
            .into_iter()
            .filter(|o| {
                o.direction == "outgoing"
                    && matches!(o.state.as_str(), "offered" | "transferring" | "cancelled")
                    && !o.cancel_sent
            })
        {
            let result = self.send_record_move(&offer).await;
            match result {
                Ok(true) => completed += 1,
                Ok(false) => {}
                Err(error) => {
                    store::record_move::offers::error(pool, &offer.uid, &error.to_string()).await?;
                }
            }
        }
        Ok(completed)
    }

    async fn send_record_move(
        &self,
        offer: &store::record_move::offers::Offer,
    ) -> Result<bool, EngineError> {
        self.move_sender_authorized().await?;
        let contact = store::organs::contact(&self.engine.store.pool, &offer.peer)
            .await?
            .ok_or_else(|| {
                EngineError::Consequence("Move contact was forgotten; source retained".into())
            })?;
        if contact.trust != "known" || contact.delivery() == store::organs::Delivery::Mailbox {
            return Err(EngineError::Forbidden(
                "Move waits for a known contact using Direct or Automatic delivery".into(),
            ));
        }
        let connection = self.dial(&contact).await.ok_or_else(|| {
            EngineError::Consequence("Move waits for the recipient to reconnect".into())
        })?;
        self.refresh_roster(&connection, &offer.peer).await;
        self.move_recipient_authorized(&connection, &offer.peer)
            .await?;
        if offer.state == "cancelled" {
            let response = self
                .exchange(
                    &connection,
                    &WireRequest::MoveCancel {
                        uid: offer.uid.clone(),
                    },
                )
                .await?;
            if !matches!(response, WireResponse::Applied { .. }) {
                return Err(EngineError::Consequence(format!(
                    "Move cancellation: {response:?}"
                )));
            }
            store::record_move::offers::cancellation_sent(&self.engine.store.pool, &offer.uid)
                .await?;
            return Ok(false);
        }
        let (current, _) = self
            .engine
            .preview_record_move(&offer.root, &offer.peer)
            .await?;
        if current.hash != offer.preview.hash {
            store::record_move::offers::transition(
                &self.engine.store.pool,
                &offer.uid,
                "offered",
                "changed",
            )
            .await?;
            return Err(EngineError::Consequence(
                "Move source changed; retained data needs a new preview".into(),
            ));
        }
        let response = self
            .exchange(
                &connection,
                &WireRequest::MoveOffer {
                    uid: offer.uid.clone(),
                    preview: offer.preview.clone(),
                },
            )
            .await?;
        let WireResponse::MoveStatus { state, receipt } = response else {
            return Err(EngineError::Consequence(format!(
                "Move offer: {response:?}"
            )));
        };
        if offer.state == "offered" {
            if state != "accepted" {
                return Ok(false);
            }
            if !self.engine.claim_record_move(&offer.uid).await? {
                return Ok(false);
            }
        } else if let Some(hash) = receipt {
            self.engine.finish_record_move(&offer.uid, &hash).await?;
            return Ok(true);
        }
        let held = store::record_move::offers::get(&self.engine.store.pool, &offer.uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("Move no longer retained".into()))?;
        if held.state != "transferring" {
            return Ok(false);
        }
        let payload =
            store::record_move::offers::payload(&self.engine.store.pool, &offer.uid).await?;
        self.refresh_roster(&connection, &offer.peer).await;
        self.move_recipient_authorized(&connection, &offer.peer)
            .await?;
        self.move_sender_authorized().await?;
        let (current, _) = self
            .engine
            .preview_record_move(&held.root, &held.peer)
            .await?;
        if current.hash != held.preview.hash {
            return Err(EngineError::Consequence(
                "Move data changed before delivery; source retained".into(),
            ));
        }
        let response = self
            .exchange(
                &connection,
                &WireRequest::MoveBundle {
                    uid: offer.uid.clone(),
                    bundle: serde_json::from_str(&payload)?,
                },
            )
            .await?;
        match response {
            WireResponse::MoveStatus {
                state,
                receipt: Some(hash),
            } if state == "received" => {
                self.engine.finish_record_move(&offer.uid, &hash).await?;
                Ok(true)
            }
            other => Err(EngineError::Consequence(format!(
                "Move waits for durable receipt: {other:?}"
            ))),
        }
    }

    async fn move_sender_authorized(&self) -> Result<(), EngineError> {
        let pool = &self.engine.store.pool;
        let organ = store::organs::local(pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("No local Organ".into()))?;
        let cell = store::cells::local(pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("No local Cell".into()))?;
        let roster = self.engine.roster_of(&organ.uid).await?;
        let node = self.node_id().to_string();
        if roster.is_none_or(|r| {
            !crate::roster::roster_signature_is_valid(&r)
                || !r.roster.cells.iter().any(|entry| {
                    entry.cell_uid == cell.uid
                        && entry.node_id == node
                        && entry.may(crate::roster::CAP_WRITE)
                        && entry.may(crate::roster::CAP_REPRESENT)
                })
        }) {
            return Err(EngineError::Forbidden("Move work waits for this sender device's current write and representation permission; source retained".into()));
        }
        Ok(())
    }

    async fn move_recipient_authorized(
        &self,
        connection: &Connection,
        peer: &str,
    ) -> Result<(), EngineError> {
        let roster = self.engine.roster_of(peer).await?;
        let node = connection.remote_id().to_string();
        if roster.is_none_or(|r| {
            !crate::roster::roster_signature_is_valid(&r)
                || !r.roster.cells.iter().any(|cell| {
                    cell.node_id == node
                        && cell.may(crate::roster::CAP_WRITE)
                        && cell.may(crate::roster::CAP_REPRESENT)
                })
        }) {
            return Err(EngineError::Forbidden("The connected recipient device is no longer authorized to receive moves. Reconnect to a current owner device; source retained.".into()));
        }
        Ok(())
    }
}
