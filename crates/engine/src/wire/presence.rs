use super::{Wire, WireRequest, WireResponse};
use crate::EngineError;
use std::time::Duration;

impl Wire {
    pub async fn sync_presence(&self) -> Result<(), EngineError> {
        let records = self.engine.presence.records();
        let contacts = store::organs::contacts(&self.engine.store.pool).await?;
        self.presence_connections
            .lock()
            .await
            .retain(|uid, connection| {
                let keep = contacts
                    .iter()
                    .any(|contact| contact.record_uid == *uid && contact.trust != "blocked");
                if !keep {
                    connection.close(0u32.into(), b"Presence access removed");
                }
                keep
            });
        let mut tasks = tokio::task::JoinSet::new();
        for contact in contacts
            .into_iter()
            .filter(|contact| contact.trust != "blocked")
        {
            let wire = self.clone();
            let records = records.clone();
            tasks.spawn(async move {
                if let Err(error) = wire.sync_contact_presence(&contact, &records).await {
                    tracing::debug!(%error, "Cursor presence exchange failed");
                }
            });
            if tasks.len() >= 8 {
                let _ = tasks.join_next().await;
            }
        }
        while tasks.join_next().await.is_some() {}
        Ok(())
    }

    async fn sync_contact_presence(
        &self,
        contact: &store::organs::Contact,
        records: &[String],
    ) -> Result<(), EngineError> {
        let peer = &contact.record_uid;
        let mut shared = Vec::new();
        for record in records {
            if self.engine.presence_shared(peer, record, "head").await?
                || self.engine.presence_shared(peer, record, "body").await?
            {
                shared.push(record.clone());
            }
        }
        let existing = self
            .presence_connections
            .lock()
            .await
            .get(peer)
            .filter(|connection| connection.close_reason().is_none())
            .cloned();
        if shared.is_empty() && existing.is_none() {
            return Ok(());
        }
        let entries = self.engine.presence_for(peer, &shared).await?;
        let connection = match existing {
            Some(connection) => connection,
            None => {
                let Ok(Some(connection)) =
                    tokio::time::timeout(Duration::from_secs(2), self.dial(contact)).await
                else {
                    return Ok(());
                };
                self.presence_connections
                    .lock()
                    .await
                    .insert(peer.clone(), connection.clone());
                connection
            }
        };
        let closing = shared.is_empty();
        let request = WireRequest::Presence {
            records: shared,
            entries,
        };
        let response =
            tokio::time::timeout(Duration::from_secs(2), self.exchange(&connection, &request))
                .await;
        match response {
            Ok(Ok(WireResponse::Presence { entries })) => {
                self.engine.receive_presence(peer, entries).await?
            }
            _ => {
                self.presence_connections.lock().await.remove(peer);
                connection.close(0u32.into(), b"Presence exchange failed");
            }
        }
        if closing {
            self.presence_connections.lock().await.remove(peer);
            connection.close(0u32.into(), b"Editors left");
        }
        Ok(())
    }
}
