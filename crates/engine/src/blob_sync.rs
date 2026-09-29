mod files;
mod network;
#[cfg(test)]
mod tests;
mod worker;

use crate::{Engine, EngineError};
use iroh_blobs::{api::Store, store::fs::FsStore};
use nucleus::blob_sync::Manifest;
use std::path::{Path, PathBuf};
use store::blob_sync as db;
pub use store::blob_sync::Transfer;
use tokio::sync::{Mutex, watch};

pub use nucleus::blob_sync::{Entry, Target};
pub const ALPN: &[u8] = b"lince/blob-offer/1";
pub const DATA_ALPN: &[u8] = iroh_blobs::ALPN;

pub struct BlobSync {
    blobs: Store,
    gate: Mutex<()>,
    changed: watch::Sender<u64>,
    pub(crate) preparing: tokio::sync::Semaphore,
}

pub(super) fn failure(error: impl std::fmt::Display) -> EngineError {
    EngineError::Consequence(error.to_string())
}

impl BlobSync {
    pub async fn open(path: &Path) -> Result<Self, EngineError> {
        tokio::fs::create_dir_all(path).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
        }
        let mut options = iroh_blobs::store::fs::options::Options::new(path);
        options.gc = Some(iroh_blobs::store::GcConfig {
            interval: std::time::Duration::from_secs(60),
            add_protected: None,
        });
        let blobs = FsStore::load_with_opts(path.join("blobs.db"), options)
            .await
            .map_err(failure)?;
        let (changed, _) = watch::channel(0);
        Ok(Self {
            blobs: (*blobs).clone(),
            gate: Mutex::new(()),
            changed,
            preparing: tokio::sync::Semaphore::new(1),
        })
    }

    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    pub fn notify(&self) {
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub async fn shutdown(&self) -> Result<(), EngineError> {
        self.blobs.shutdown().await.map_err(failure)
    }

    async fn release(&self, id: &str) -> Result<(), EngineError> {
        self.blobs
            .tags()
            .delete_prefix(format!("lince-blob/{id}/"))
            .await
            .map_err(failure)?;
        Ok(())
    }
}

impl Engine {
    pub async fn initialize_blob_sync(&self, path: &Path) -> Result<(), EngineError> {
        use n0_future::StreamExt;
        let service = BlobSync::open(path).await?;
        let retained: std::collections::HashSet<String> = store::sqlx::query_scalar(
            "SELECT id FROM blob_sync WHERE state IN ('offered', 'accepted')",
        )
        .fetch_all(&self.store.pool)
        .await?
        .into_iter()
        .collect();
        let mut tags = service
            .blobs
            .tags()
            .list_prefix("lince-blob/")
            .await
            .map_err(failure)?;
        while let Some(tag) = tags.next().await {
            let tag = tag.map_err(failure)?;
            let name = std::str::from_utf8(tag.name.as_ref()).map_err(failure)?;
            if name
                .split('/')
                .nth(1)
                .is_none_or(|id| !retained.contains(id))
            {
                service
                    .blobs
                    .tags()
                    .delete(tag.name)
                    .await
                    .map_err(failure)?;
            }
        }
        self.blobs
            .set(service)
            .map_err(|_| failure("Blob Sync was already initialized"))?;
        Ok(())
    }

    pub fn blob_sync(&self) -> Result<&BlobSync, EngineError> {
        self.blobs
            .get()
            .ok_or_else(|| failure("Blob Sync is unavailable on this Cell"))
    }

    pub(super) async fn blob_owner(&self) -> Result<String, EngineError> {
        if self.identity_in_flux() {
            return Err(failure("Wait for the Organ identity change to finish"));
        }
        store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .ok_or_else(|| failure("The local Organ is missing"))
    }

    pub async fn blob_transfers(&self) -> Result<Vec<Transfer>, EngineError> {
        Ok(db::list(&self.store.pool, &self.blob_owner().await?).await?)
    }

    pub async fn prepare_blob_copy(
        &self,
        peer: &str,
        label: &str,
        paths: Vec<PathBuf>,
    ) -> Result<String, EngineError> {
        let service = self.blob_sync()?;
        let _preparing = service
            .preparing
            .try_acquire()
            .map_err(|_| failure("Another copy is being prepared"))?;
        peer.parse::<iroh::EndpointId>().map_err(failure)?;
        let owner = self.blob_owner().await?;
        let peer_organ = self
            .blob_contact(peer)
            .await?
            .map(|contact| contact.record_uid);
        self.blob_capacity(&owner, peer).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let result = service.snapshot(&id, paths).await;
        let manifest = match result {
            Ok(manifest) => manifest,
            Err(error) => {
                service.release(&id).await?;
                return Err(error);
            }
        };
        let _gate = service.gate.lock().await;
        if self.blob_owner().await? != owner
            || self
                .blob_contact(peer)
                .await?
                .map(|contact| contact.record_uid)
                != peer_organ
        {
            service.release(&id).await?;
            return Err(failure("The Organ changed while preparing this copy"));
        }
        let transfer = Transfer {
            id: id.clone(),
            direction: "outgoing".into(),
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
        if let Err(error) = self.insert_blob_offer(&owner, &transfer).await {
            service.release(&id).await?;
            return Err(error);
        }
        service.notify();
        Ok(id)
    }

    async fn insert_blob_offer(&self, owner: &str, transfer: &Transfer) -> Result<(), EngineError> {
        self.blob_capacity(owner, &transfer.peer).await?;
        db::insert(&self.store.pool, owner, transfer).await?;
        Ok(())
    }

    async fn blob_capacity(&self, owner: &str, peer: &str) -> Result<(), EngineError> {
        let pending: (i64, i64) = store::sqlx::query_as("SELECT COUNT(*), COALESCE(SUM(peer = ?), 0) FROM blob_sync WHERE owner = ? AND state IN ('offered', 'accepted')")
            .bind(peer).bind(owner).fetch_one(&self.store.pool).await?;
        if pending.0 >= 128 || pending.1 >= 8 {
            return Err(failure("Too many pending Blob Sync offers"));
        }
        Ok(())
    }

    pub async fn accept_blob_copy(&self, id: &str, directory: &Path) -> Result<(), EngineError> {
        let service = self.blob_sync()?;
        let _gate = service.gate.lock().await;
        let owner = self.blob_owner().await?;
        let transfer = self.blob_transfer(&owner, id).await?;
        if transfer.direction != "incoming"
            || !matches!(transfer.state.as_str(), "offered" | "accepted")
        {
            return Err(failure("This copy is no longer waiting for acceptance"));
        }
        let directory = tokio::fs::canonicalize(directory).await?;
        if !tokio::fs::metadata(&directory).await?.is_dir() {
            return Err(failure("Choose a destination directory"));
        }
        let destination = directory.join(format!("Blob Sync {id}"));
        if destination.try_exists()? {
            return Err(failure("The destination already exists"));
        }
        let destination = destination
            .to_str()
            .ok_or_else(|| failure("Use a UTF-8 destination path"))?;
        db::transition(
            &self.store.pool,
            &owner,
            id,
            &transfer.state,
            "accepted",
            Some(destination),
        )
        .await?;
        service.notify();
        self.notify_notifications_changed();
        Ok(())
    }

    pub async fn stop_blob_copy(&self, id: &str) -> Result<(), EngineError> {
        let service = self.blob_sync()?;
        let _gate = service.gate.lock().await;
        let owner = self.blob_owner().await?;
        let transfer = self.blob_transfer(&owner, id).await?;
        if !matches!(transfer.state.as_str(), "offered" | "accepted") {
            return Err(failure("This copy has already finished"));
        }
        let state = if transfer.direction == "incoming" && transfer.state == "offered" {
            "declined"
        } else {
            "cancelled"
        };
        db::transition(&self.store.pool, &owner, id, &transfer.state, state, None).await?;
        service.notify();
        self.notify_notifications_changed();
        Ok(())
    }

    async fn blob_transfer(&self, owner: &str, id: &str) -> Result<Transfer, EngineError> {
        db::get(&self.store.pool, owner, id)
            .await?
            .ok_or_else(|| failure("Unknown Blob Sync offer"))
    }

    async fn blob_active(&self, owner: &str, id: &str) -> Result<(), EngineError> {
        let transfer = self.blob_transfer(owner, id).await?;
        if self.blob_owner().await? != owner || transfer.state != "accepted" {
            return Err(failure("Blob Sync stopped"));
        }
        let contact = self.blob_contact(&transfer.peer).await?;
        if contact
            .as_ref()
            .is_some_and(|contact| contact.trust == "blocked")
            || transfer.peer_organ.as_ref().is_some_and(|organ| {
                contact
                    .as_ref()
                    .is_none_or(|contact| &contact.record_uid != organ)
            })
        {
            return Err(failure(
                "This peer is no longer allowed to transfer this copy",
            ));
        }
        Ok(())
    }
}
