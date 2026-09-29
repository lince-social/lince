use super::network::{Close, Request};
use super::*;
use crate::wire::Wire;
use bao_tree::{ChunkNum, ChunkRanges};
use iroh_blobs::{Hash, api::remote::GetProgressItem, protocol::GetRequest};
use n0_future::StreamExt;
use std::time::Duration;

impl Wire {
    pub async fn blob_step(&self, id: &str) -> Result<(), EngineError> {
        let owner = self.engine.blob_owner().await?;
        let transfer = self.engine.blob_transfer(&owner, id).await?;
        let result = self.blob_step_inner(&owner, &transfer).await;
        if let Err(error) = &result {
            db::error(&self.engine.store.pool, &owner, id, &error.to_string()).await?;
            self.engine.blob_sync()?.notify();
        }
        result
    }

    async fn blob_step_inner(&self, owner: &str, transfer: &Transfer) -> Result<(), EngineError> {
        let service = self.engine.blob_sync()?;
        let contact = self.blob_contact(&transfer.peer).await?;
        if contact
            .as_ref()
            .is_some_and(|contact| contact.trust == "blocked")
        {
            return Err(failure("This Organ is blocked"));
        }
        if transfer.peer_organ.as_ref().is_some_and(|organ| {
            contact
                .as_ref()
                .is_none_or(|contact| &contact.record_uid != organ)
        }) {
            return Err(failure("The peer no longer represents the addressed Organ"));
        }
        if matches!(
            transfer.state.as_str(),
            "cancelled" | "declined" | "completed"
        ) {
            service.release(&transfer.id).await?;
            if !transfer.settled {
                let request = if transfer.state == "completed" {
                    Request::Completed {
                        id: transfer.id.clone(),
                    }
                } else {
                    Request::Stop {
                        id: transfer.id.clone(),
                    }
                };
                self.blob_request(&transfer.peer, request).await?;
                db::settle(&self.engine.store.pool, owner, &transfer.id).await?;
                service.notify();
            }
            return Ok(());
        }
        if transfer.direction == "outgoing" {
            let request = if transfer.state == "offered" {
                Request::Offer {
                    manifest: transfer.manifest.clone(),
                }
            } else {
                Request::Status {
                    id: transfer.id.clone(),
                }
            };
            let response = self.blob_request(&transfer.peer, request).await?;
            if response.state == "offered" {
                db::offered(&self.engine.store.pool, owner, &transfer.id).await?;
                service.notify();
            }
            if matches!(
                response.state.as_str(),
                "accepted" | "declined" | "cancelled" | "completed"
            ) {
                if response.state == "accepted" {
                    db::progress(
                        &self.engine.store.pool,
                        owner,
                        &transfer.id,
                        response.progress.min(transfer.manifest.bytes()),
                    )
                    .await?;
                }
                db::transition(
                    &self.engine.store.pool,
                    owner,
                    &transfer.id,
                    &transfer.state,
                    &response.state,
                    None,
                )
                .await?;
                service.notify();
            }
        } else if transfer.state == "accepted" {
            self.receive_blob_copy(owner, transfer).await?;
        }
        Ok(())
    }

    async fn receive_blob_copy(&self, owner: &str, transfer: &Transfer) -> Result<(), EngineError> {
        let service = self.engine.blob_sync()?;
        if let Some(destination) = &transfer.destination {
            if Path::new(destination).try_exists()? {
                let destination = PathBuf::from(destination);
                let manifest = transfer.manifest.clone();
                tokio::task::spawn_blocking(move || {
                    super::files::verify_copy(&destination, &manifest)
                })
                .await
                .map_err(failure)??;
                let _gate = service.gate.lock().await;
                self.engine.blob_active(owner, &transfer.id).await?;
                if self
                    .engine
                    .blob_transfer(owner, &transfer.id)
                    .await?
                    .destination
                    != transfer.destination
                {
                    return Err(failure(
                        "The save folder changed; continuing in the new folder",
                    ));
                }
                db::transition(
                    &self.engine.store.pool,
                    owner,
                    &transfer.id,
                    "accepted",
                    "completed",
                    None,
                )
                .await?;
                service.notify();
                return Ok(());
            }
        }
        let response = self
            .blob_request(
                &transfer.peer,
                Request::Accept {
                    id: transfer.id.clone(),
                },
            )
            .await?;
        if response.state != "accepted" {
            if matches!(response.state.as_str(), "cancelled" | "declined") {
                db::transition(
                    &self.engine.store.pool,
                    owner,
                    &transfer.id,
                    "accepted",
                    "cancelled",
                    None,
                )
                .await?;
                db::settle(&self.engine.store.pool, owner, &transfer.id).await?;
                service.notify();
            }
            return Err(failure("The sender is no longer offering this copy"));
        }
        let peer = transfer.peer.parse::<iroh::EndpointId>().map_err(failure)?;
        let connection = Close(
            tokio::time::timeout(
                Duration::from_secs(12),
                self.endpoint().connect(peer, DATA_ALPN),
            )
            .await
            .map_err(failure)?
            .map_err(failure)?,
        );
        let mut completed = 0;
        for (index, entry) in transfer.manifest.entries.iter().enumerate() {
            let Some(hash) = &entry.hash else { continue };
            self.engine.blob_active(owner, &transfer.id).await?;
            let hash = hash.parse::<Hash>().map_err(failure)?;
            service
                .blobs
                .tags()
                .set(format!("lince-blob/{}/{index}", transfer.id), hash)
                .await
                .map_err(failure)?;
            let request = GetRequest::builder()
                .root(ChunkRanges::from(
                    ChunkNum(0)..ChunkNum(entry.size.div_ceil(1024).max(1)),
                ))
                .build(hash);
            let local = service
                .blobs
                .remote()
                .local_for_request(request)
                .await
                .map_err(failure)?;
            let available = local.local_bytes().min(entry.size);
            let complete = service.blobs.observe(hash).await.map_err(failure)?;
            if !complete.is_complete() {
                let stream = service
                    .blobs
                    .remote()
                    .execute_get(connection.0.clone(), local.missing())
                    .stream();
                tokio::pin!(stream);
                let mut check = tokio::time::interval(Duration::from_millis(250));
                let mut last_progress = std::time::Instant::now();
                let mut last_saved = std::time::Instant::now();
                let mut received = 0;
                let mut done = false;
                loop {
                    tokio::select! {
                        item = stream.next() => match item {
                            Some(GetProgressItem::Progress(bytes)) => {
                                if bytes > entry.size.saturating_add(16384) { return Err(failure("The file exceeds the accepted size")); }
                                received = bytes;
                                last_progress = std::time::Instant::now();
                            }
                            Some(GetProgressItem::Done(_)) => { done = true; break; }
                            Some(GetProgressItem::Error(error)) => return Err(failure(error)),
                            None => break,
                        },
                        _ = check.tick() => {
                            self.engine.blob_active(owner, &transfer.id).await?;
                            if last_progress.elapsed() > Duration::from_secs(30) { return Err(failure("Transfer interrupted; it will resume when the sender reconnects")); }
                            if last_saved.elapsed() >= Duration::from_millis(500) {
                                db::progress(&self.engine.store.pool, owner, &transfer.id, completed + available.saturating_add(received).min(entry.size)).await?;
                                service.notify();
                                last_saved = std::time::Instant::now();
                            }
                        }
                    }
                }
                if !done {
                    return Err(failure("The file download was interrupted"));
                }
            }
            let info = service.blobs.observe(hash).await.map_err(failure)?;
            if !info.is_complete() || info.size() != entry.size {
                return Err(failure(
                    "The downloaded file does not match the accepted size",
                ));
            }
            completed += entry.size;
            db::progress(&self.engine.store.pool, owner, &transfer.id, completed).await?;
            service.notify();
        }
        self.engine.blob_active(owner, &transfer.id).await?;
        service.blobs.sync_db().await.map_err(failure)?;
        let staging = service.export(transfer).await?;
        let _gate = service.gate.lock().await;
        self.engine.blob_active(owner, &transfer.id).await?;
        if self
            .engine
            .blob_transfer(owner, &transfer.id)
            .await?
            .destination
            != transfer.destination
        {
            return Err(failure(
                "The save folder changed; continuing in the new folder",
            ));
        }
        let destination = PathBuf::from(
            transfer
                .destination
                .as_deref()
                .ok_or_else(|| failure("Missing destination"))?,
        );
        publish(staging.path(), &destination)?;
        db::transition(
            &self.engine.store.pool,
            owner,
            &transfer.id,
            "accepted",
            "completed",
            None,
        )
        .await?;
        service.notify();
        Ok(())
    }
}

fn publish(source: &Path, destination: &Path) -> Result<(), EngineError> {
    #[cfg(target_os = "linux")]
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)?;
    #[cfg(not(target_os = "linux"))]
    {
        if destination.try_exists()? {
            return Err(failure(
                "The destination already exists; it was not overwritten",
            ));
        }
        std::fs::rename(source, destination)?;
    }
    Ok(())
}
