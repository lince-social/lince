use super::*;
use crate::wire::{DIAL_TIMEOUT, Wire};
use iroh::endpoint::Connection;
use iroh_blobs::{protocol::Request as BlobRequest, provider::StreamPair};
use nucleus::blob_sync::MAX_OFFER_BYTES;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Offer { manifest: Manifest },
    Accept { id: String },
    Status { id: String },
    Completed { id: String },
    Stop { id: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Response {
    pub state: String,
    pub progress: u64,
    pub error: Option<String>,
}

pub(super) struct Close(pub Connection);

impl Drop for Close {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"blob operation ended");
    }
}

impl Engine {
    pub(super) async fn blob_contact(
        &self,
        peer: &str,
    ) -> Result<Option<store::organs::Contact>, EngineError> {
        if let Some(contact) = store::organs::contact_by_node_id(&self.store.pool, peer).await? {
            if let Some(roster) = self.roster_of(&contact.record_uid).await? {
                if !crate::roster::roster_signature_is_valid(&roster)
                    || !roster
                        .roster
                        .cells
                        .iter()
                        .any(|cell| cell.node_id == peer && cell.may(crate::roster::CAP_REPRESENT))
                {
                    return Err(failure(
                        "This Cell is no longer allowed to represent the contact",
                    ));
                }
            }
            return Ok(Some(contact));
        }
        for contact in store::organs::contacts(&self.store.pool).await? {
            if let Some(roster) = self.roster_of(&contact.record_uid).await? {
                if crate::roster::roster_signature_is_valid(&roster)
                    && roster
                        .roster
                        .cells
                        .iter()
                        .any(|cell| cell.node_id == peer && cell.may(crate::roster::CAP_REPRESENT))
                {
                    return Ok(Some(contact));
                }
            }
        }
        Ok(None)
    }
}

impl Wire {
    pub(super) async fn blob_contact(
        &self,
        peer: &str,
    ) -> Result<Option<store::organs::Contact>, EngineError> {
        self.engine.blob_contact(peer).await
    }

    pub async fn blob_targets(&self) -> Result<Vec<Target>, EngineError> {
        let mut targets = Vec::new();
        let nearby = self.nearby().current();
        for contact in store::organs::contacts(&self.engine.store.pool).await? {
            if contact.trust == "blocked" {
                continue;
            }
            let mut nodes = self
                .engine
                .roster_of(&contact.record_uid)
                .await?
                .filter(crate::roster::roster_signature_is_valid)
                .map(|roster| {
                    roster
                        .roster
                        .cells
                        .into_iter()
                        .filter(|cell| cell.may(crate::roster::CAP_REPRESENT))
                        .map(|cell| cell.node_id)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(node) = contact.node_id {
                if self.blob_contact(&node).await.is_ok() {
                    nodes.insert(0, node);
                }
            }
            nodes.sort_by_key(|node| !nearby.iter().any(|peer| peer.node_id == *node));
            for node in nodes {
                if targets.iter().any(|target: &Target| target.node_id == node) {
                    continue;
                }
                targets.push(Target {
                    nearby: nearby.iter().any(|peer| peer.node_id == node),
                    node_id: node,
                    label: contact.head.clone(),
                });
            }
        }
        for peer in nearby {
            if targets.iter().any(|target| target.node_id == peer.node_id) {
                continue;
            }
            match self.blob_contact(&peer.node_id).await {
                Ok(Some(contact)) if contact.trust == "blocked" => continue,
                Err(_) => continue,
                _ => {}
            }
            targets.push(Target {
                node_id: peer.node_id,
                label: if peer.name.is_empty() {
                    peer.fingerprint
                } else {
                    peer.name
                },
                nearby: true,
            });
        }
        Ok(targets)
    }

    pub async fn send_blob_copy(
        &self,
        node: &str,
        paths: Vec<PathBuf>,
    ) -> Result<String, EngineError> {
        if !self.may_represent().await {
            return Err(failure("This Cell cannot represent this Organ"));
        }
        let target = self
            .blob_targets()
            .await?
            .into_iter()
            .find(|target| target.node_id == node)
            .ok_or_else(|| failure("Choose a current contact or nearby Organ"))?;
        if node == self.node_id().to_string() {
            return Err(failure("Choose another Organ"));
        }
        self.engine
            .prepare_blob_copy(node, &target.label, paths)
            .await
    }

    pub(super) async fn blob_request(
        &self,
        peer: &str,
        request: Request,
    ) -> Result<Response, EngineError> {
        let id = peer.parse::<iroh::EndpointId>().map_err(failure)?;
        let result = tokio::time::timeout(Duration::from_secs(12), async {
            let connection = Close(self.endpoint().connect(id, ALPN).await.map_err(failure)?);
            let (mut send, mut recv) = connection.0.open_bi().await.map_err(failure)?;
            let bytes = serde_json::to_vec(&request).map_err(failure)?;
            if bytes.len() > MAX_OFFER_BYTES {
                return Err(failure("This offer is too large"));
            }
            send.write_all(&bytes).await.map_err(failure)?;
            send.finish().map_err(failure)?;
            let response = recv.read_to_end(4096).await.map_err(failure)?;
            let response: Response = serde_json::from_slice(&response).map_err(failure)?;
            if let Some(error) = response.error {
                return Err(failure(error));
            }
            Ok(response)
        })
        .await
        .map_err(|_| failure("Waiting for the other Organ to reconnect"))?;
        result
    }

    pub(crate) async fn serve_blob_connection(
        &self,
        connection: Connection,
    ) -> Result<(), EngineError> {
        let _slot = self
            .blob_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure("Blob Sync is busy"))?;
        if connection.alpn() == DATA_ALPN {
            return self.serve_blob_data(connection).await;
        }
        let result = tokio::time::timeout(Duration::from_secs(15), async {
            let (mut send, mut recv) = connection.accept_bi().await.map_err(failure)?;
            let bytes = recv.read_to_end(MAX_OFFER_BYTES).await.map_err(failure)?;
            let mut id = String::new();
            let result = match serde_json::from_slice::<Request>(&bytes) {
                Ok(request) => {
                    id = match &request {
                        Request::Offer { manifest } => manifest.id.clone(),
                        Request::Status { id }
                        | Request::Accept { id }
                        | Request::Completed { id }
                        | Request::Stop { id } => id.clone(),
                    };
                    self.handle_blob_request(&connection.remote_id().to_string(), request)
                        .await
                }
                Err(error) => Err(failure(error)),
            };
            let response = match result {
                Ok(state) => {
                    let owner = self.engine.blob_owner().await?;
                    let progress = self.engine.blob_transfer(&owner, &id).await?.progress;
                    Response {
                        state,
                        progress,
                        error: None,
                    }
                }
                Err(error) => Response {
                    state: String::new(),
                    progress: 0,
                    error: Some(error.to_string()),
                },
            };
            send.write_all(&serde_json::to_vec(&response).map_err(failure)?)
                .await
                .map_err(failure)?;
            send.finish().map_err(failure)?;
            let _ = send.stopped().await;
            Ok(())
        })
        .await
        .map_err(failure)?;
        connection.close(0u32.into(), b"offer handled");
        result
    }

    async fn handle_blob_request(
        &self,
        peer: &str,
        request: Request,
    ) -> Result<String, EngineError> {
        let service = self.engine.blob_sync()?;
        let _gate = service.gate.lock().await;
        if !self.may_represent().await {
            return Err(failure("This Cell cannot represent this Organ"));
        }
        let owner = self.engine.blob_owner().await?;
        let contact = self.blob_contact(peer).await?;
        if contact
            .as_ref()
            .is_some_and(|contact| contact.trust == "blocked")
        {
            return Err(failure("Blocked Organ"));
        }
        if let Request::Offer { manifest } = request {
            manifest.validate().map_err(failure)?;
            if let Some(existing) = db::get(&self.engine.store.pool, &owner, &manifest.id).await? {
                if existing.peer != peer
                    || existing.direction != "incoming"
                    || existing.manifest != manifest
                {
                    return Err(failure("The offer identifier is already in use"));
                }
                return Ok(existing.state);
            }
            let nearby = self
                .nearby()
                .current()
                .into_iter()
                .find(|entry| entry.node_id == peer);
            if contact.is_none() && nearby.is_none() {
                return Err(failure("Only contacts and nearby Organs may offer files"));
            }
            let peer_organ = contact.as_ref().map(|contact| contact.record_uid.clone());
            let label = contact
                .map(|contact| contact.head)
                .or_else(|| nearby.map(|peer| peer.name))
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| peer.into());
            let transfer = Transfer {
                id: manifest.id.clone(),
                direction: "incoming".into(),
                peer: peer.into(),
                peer_organ,
                label: label.chars().take(160).collect(),
                manifest,
                state: "offered".into(),
                destination: None,
                progress: 0,
                error: String::new(),
                settled: false,
            };
            self.engine.insert_blob_offer(&owner, &transfer).await?;
            self.engine.notify_notifications_changed();
            utils::diagnostics::Diagnostics::global().report(
                "Blob Sync request",
                &format!(
                    "{} offered a fixed copy ({} bytes). Open Sync Castle to accept or decline.",
                    transfer.label,
                    transfer.manifest.bytes()
                ),
            );
            service.notify();
            return Ok("offered".into());
        }
        let id = match &request {
            Request::Status { id }
            | Request::Accept { id }
            | Request::Completed { id }
            | Request::Stop { id } => id,
            Request::Offer { .. } => unreachable!(),
        };
        let transfer = self.engine.blob_transfer(&owner, id).await?;
        if transfer.peer != peer {
            return Err(failure("This offer belongs to a different Organ"));
        }
        if transfer.peer_organ.as_ref().is_some_and(|organ| {
            contact
                .as_ref()
                .is_none_or(|contact| &contact.record_uid != organ)
        }) {
            return Err(failure("The peer no longer represents the addressed Organ"));
        }
        let next = match request {
            Request::Accept { .. }
                if transfer.direction == "outgoing" && transfer.state == "offered" =>
            {
                "accepted"
            }
            Request::Completed { .. }
                if transfer.direction == "outgoing" && transfer.state == "accepted" =>
            {
                "completed"
            }
            Request::Stop { .. } if matches!(transfer.state.as_str(), "offered" | "accepted") => {
                if transfer.direction == "outgoing" && transfer.state == "offered" {
                    "declined"
                } else {
                    "cancelled"
                }
            }
            _ => return Ok(transfer.state),
        };
        db::transition(
            &self.engine.store.pool,
            &owner,
            id,
            &transfer.state,
            next,
            None,
        )
        .await?;
        if matches!(next, "completed" | "declined" | "cancelled") {
            service.release(id).await?;
            db::settle(&self.engine.store.pool, &owner, id).await?;
        }
        service.notify();
        Ok(next.into())
    }

    async fn authorized_blob(&self, peer: &str, hash: iroh_blobs::Hash) -> Result<(), EngineError> {
        if !self.may_represent().await {
            return Err(failure("This Cell no longer represents the Organ"));
        }
        let contact = self.blob_contact(peer).await?;
        if contact
            .as_ref()
            .is_some_and(|contact| contact.trust == "blocked")
        {
            return Err(failure("Blocked Organ"));
        }
        let allowed = db::authorized(
            &self.engine.store.pool,
            &self.engine.blob_owner().await?,
            peer,
            &hash.to_string(),
        )
        .await?;
        if !allowed.iter().any(|organ| {
            organ.as_ref().is_none_or(|organ| {
                contact
                    .as_ref()
                    .is_some_and(|contact| &contact.record_uid == organ)
            })
        }) {
            return Err(failure("Accept this file offer before downloading"));
        }
        Ok(())
    }

    async fn serve_blob_data(&self, connection: Connection) -> Result<(), EngineError> {
        let connection = Close(connection);
        let service = self.engine.blob_sync()?;
        for _ in 0..nucleus::blob_sync::MAX_ENTRIES {
            let mut pair = tokio::time::timeout(
                Duration::from_secs(30),
                StreamPair::accept(&connection.0, Default::default()),
            )
            .await
            .map_err(failure)?
            .map_err(failure)?;
            let request = tokio::time::timeout(DIAL_TIMEOUT, pair.read_request())
                .await
                .map_err(failure)?
                .map_err(failure)?;
            let BlobRequest::Get(request) = request else {
                return Err(failure("Only file reads are allowed"));
            };
            if !request.ranges.is_blob() {
                return Err(failure("Collections are not exposed by Blob Sync"));
            }
            let hash = request.hash;
            self.authorized_blob(&connection.0.remote_id().to_string(), hash)
                .await?;
            let serving = iroh_blobs::provider::handle_get(pair, service.blobs.clone(), request);
            tokio::pin!(serving);
            let mut check = tokio::time::interval(Duration::from_millis(500));
            loop {
                tokio::select! {
                    result = &mut serving => { result.map_err(failure)?; break; }
                    _ = check.tick() => { self.authorized_blob(&connection.0.remote_id().to_string(), hash).await?; }
                }
            }
        }
        Ok(())
    }
}
