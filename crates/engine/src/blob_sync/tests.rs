use super::network::Request;
use super::*;
use crate::wire::{Reach, Wire};
use bao_tree::{ChunkNum, ChunkRanges};
use iroh::{EndpointAddr, SecretKey};
use iroh_blobs::{Hash, protocol::GetRequest};
use std::{sync::Arc, time::Duration};

struct Peer {
    engine: Arc<Engine>,
    wire: Arc<Wire>,
    serving: tokio::task::JoinHandle<()>,
    directory: tempfile::TempDir,
}

impl Peer {
    async fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        engine
            .blobs
            .set(BlobSync::open(directory.path()).await.unwrap())
            .ok()
            .unwrap();
        let wire = Arc::new(
            Wire::bind_with_discovery(
                engine.clone(),
                SecretKey::from_bytes(&[seed; 32]),
                Reach::Local,
                None,
                false,
            )
            .await
            .unwrap(),
        );
        let serving = {
            let wire = wire.clone();
            tokio::spawn(async move { wire.serve().await })
        };
        Self {
            engine,
            wire,
            serving,
            directory,
        }
    }

    fn address(&self) -> EndpointAddr {
        let port = self
            .wire
            .endpoint()
            .bound_sockets()
            .iter()
            .find(|address| address.is_ipv4())
            .unwrap()
            .port();
        EndpointAddr::new(self.wire.node_id())
            .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)))
    }

    fn discover(&self, other: &Self) {
        self.wire.remember_addr(other.address());
        self.wire.nearby().observe(
            other.wire.node_id().to_string(),
            "test".into(),
            "Nearby Organ".into(),
        );
    }

    async fn close(self) {
        self.wire.shutdown().await;
        self.serving.await.unwrap();
        self.engine.blob_sync().unwrap().shutdown().await.unwrap();
    }
}

async fn transfer(engine: &Engine, id: &str) -> Transfer {
    engine
        .blob_transfers()
        .await
        .unwrap()
        .into_iter()
        .find(|transfer| transfer.id == id)
        .unwrap()
}

async fn offer(sender: &Peer, receiver: &Peer, path: PathBuf) -> String {
    sender.discover(receiver);
    receiver.discover(sender);
    let id = sender
        .wire
        .send_blob_copy(&receiver.wire.node_id().to_string(), vec![path])
        .await
        .unwrap();
    sender.wire.blob_step(&id).await.unwrap();
    id
}

async fn fetch(peer: &Peer, sender: &Peer, hash: Hash) -> bool {
    peer.wire.remember_addr(sender.address());
    let connection = peer
        .wire
        .endpoint()
        .connect(sender.address(), DATA_ALPN)
        .await
        .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        peer.engine
            .blob_sync()
            .unwrap()
            .blobs
            .remote()
            .execute_get(connection.clone(), GetRequest::blob(hash))
            .complete(),
    )
    .await;
    connection.close(0u32.into(), b"test finished");
    matches!(result, Ok(Ok(_)))
}

#[tokio::test]
async fn fixed_folder_copy_requires_acceptance_and_is_bound_to_recipient() {
    let sender = Peer::new(11).await;
    let receiver = Peer::new(12).await;
    let stranger = Peer::new(13).await;
    let source = tempfile::tempdir().unwrap();
    let folder = source.path().join("Photos");
    std::fs::create_dir_all(folder.join("Empty")).unwrap();
    std::fs::write(folder.join("photo.bin"), b"the original bytes").unwrap();
    std::fs::write(folder.join("empty.bin"), b"").unwrap();
    let id = offer(&sender, &receiver, folder.clone()).await;
    std::fs::write(folder.join("photo.bin"), b"changed later").unwrap();
    let incoming = transfer(&receiver.engine, &id).await;
    assert_eq!(incoming.state, "offered");
    assert!(incoming.destination.is_none());
    stranger.wire.remember_addr(sender.address());
    assert!(
        stranger
            .wire
            .blob_request(
                &sender.wire.node_id().to_string(),
                Request::Accept { id: id.clone() }
            )
            .await
            .is_err()
    );
    let hash = incoming
        .manifest
        .entries
        .iter()
        .find(|entry| entry.path == "Photos/photo.bin")
        .and_then(|entry| entry.hash.as_ref())
        .unwrap()
        .parse()
        .unwrap();
    assert!(!fetch(&receiver, &sender, hash).await);
    let destination = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, destination.path())
        .await
        .unwrap();
    receiver
        .wire
        .blob_request(
            &sender.wire.node_id().to_string(),
            Request::Accept { id: id.clone() },
        )
        .await
        .unwrap();
    assert!(!fetch(&stranger, &sender, hash).await);
    receiver.wire.blob_step(&id).await.unwrap();
    let received = transfer(&receiver.engine, &id).await;
    assert_eq!(received.state, "completed");
    let output = Path::new(received.destination.as_deref().unwrap());
    assert_eq!(
        std::fs::read(output.join("Photos/photo.bin")).unwrap(),
        b"the original bytes"
    );
    assert!(output.join("Photos/Empty").is_dir());
    assert!(
        std::fs::read(output.join("Photos/empty.bin"))
            .unwrap()
            .is_empty()
    );
    receiver.wire.blob_step(&id).await.unwrap();
    assert_eq!(transfer(&sender.engine, &id).await.state, "completed");
    assert!(!fetch(&receiver, &sender, hash).await);
    store::sqlx::query("UPDATE blob_sync SET state = 'accepted', settled = 0 WHERE id = ?")
        .bind(&id)
        .execute(&receiver.engine.store.pool)
        .await
        .unwrap();
    receiver.wire.blob_step(&id).await.unwrap();
    assert_eq!(transfer(&receiver.engine, &id).await.state, "completed");
    assert!(
        store::organs::contact_by_node_id(
            &receiver.engine.store.pool,
            &sender.wire.node_id().to_string()
        )
        .await
        .unwrap()
        .is_none()
    );
    stranger.close().await;
    receiver.close().await;
    sender.close().await;
}

#[tokio::test]
async fn declined_offers_cannot_be_replayed_or_downloaded() {
    let sender = Peer::new(21).await;
    let receiver = Peer::new(22).await;
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("file.txt");
    std::fs::write(&path, b"private").unwrap();
    let id = offer(&sender, &receiver, path).await;
    receiver.engine.stop_blob_copy(&id).await.unwrap();
    receiver.wire.blob_step(&id).await.unwrap();
    let sent = transfer(&sender.engine, &id).await;
    assert_eq!(sent.state, "declined");
    let response = sender
        .wire
        .blob_request(
            &receiver.wire.node_id().to_string(),
            Request::Offer {
                manifest: sent.manifest.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(response.state, "declined");
    let destination = tempfile::tempdir().unwrap();
    assert!(
        receiver
            .engine
            .accept_blob_copy(&id, destination.path())
            .await
            .is_err()
    );
    assert!(
        !fetch(
            &receiver,
            &sender,
            sent.manifest.entries[0]
                .hash
                .as_ref()
                .unwrap()
                .parse()
                .unwrap()
        )
        .await
    );
    receiver.close().await;
    sender.close().await;
}

#[tokio::test]
async fn accepted_partial_download_resumes_after_store_restart() {
    let sender = Peer::new(31).await;
    let receiver = Peer::new(32).await;
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("large.bin");
    let bytes = (0..2 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    std::fs::write(&path, &bytes).unwrap();
    let id = offer(&sender, &receiver, path).await;
    let destination = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, destination.path())
        .await
        .unwrap();
    receiver
        .wire
        .blob_request(
            &sender.wire.node_id().to_string(),
            Request::Accept { id: id.clone() },
        )
        .await
        .unwrap();
    let incoming = transfer(&receiver.engine, &id).await;
    let hash: Hash = incoming.manifest.entries[0]
        .hash
        .as_ref()
        .unwrap()
        .parse()
        .unwrap();
    let service = receiver.engine.blob_sync().unwrap();
    service
        .blobs
        .tags()
        .set(format!("lince-blob/{id}/0"), hash)
        .await
        .unwrap();
    let connection = receiver
        .wire
        .endpoint()
        .connect(sender.address(), DATA_ALPN)
        .await
        .unwrap();
    service
        .blobs
        .remote()
        .execute_get(
            connection.clone(),
            GetRequest::builder()
                .root(ChunkRanges::from(ChunkNum(0)..ChunkNum(64)))
                .build(hash),
        )
        .await
        .unwrap();
    connection.close(0u32.into(), b"interrupted");
    let partial = service
        .blobs
        .remote()
        .local(hash)
        .await
        .unwrap()
        .local_bytes();
    assert!(partial >= 65536 && partial < bytes.len() as u64);
    let store = receiver.engine.store.clone();
    let directory = receiver.directory.path().to_owned();
    receiver.wire.shutdown().await;
    receiver.serving.await.unwrap();
    service.shutdown().await.unwrap();
    let restarted = Arc::new(Engine::new(store).await.unwrap());
    restarted
        .blobs
        .set(BlobSync::open(&directory).await.unwrap())
        .ok()
        .unwrap();
    assert!(
        restarted
            .blob_sync()
            .unwrap()
            .blobs
            .remote()
            .local(hash)
            .await
            .unwrap()
            .local_bytes()
            >= partial
    );
    let wire = Wire::bind_with_discovery(
        restarted.clone(),
        SecretKey::from_bytes(&[32; 32]),
        Reach::Local,
        None,
        false,
    )
    .await
    .unwrap();
    wire.remember_addr(sender.address());
    wire.blob_step(&id).await.unwrap();
    let completed = transfer(&restarted, &id).await;
    assert_eq!(
        std::fs::read(Path::new(completed.destination.as_deref().unwrap()).join("large.bin"))
            .unwrap(),
        bytes
    );
    wire.blob_step(&id).await.unwrap();
    wire.shutdown().await;
    restarted.blob_sync().unwrap().shutdown().await.unwrap();
    sender.close().await;
}

#[tokio::test]
async fn cancellation_revokes_existing_permission_and_preserves_destination() {
    let sender = Peer::new(41).await;
    let receiver = Peer::new(42).await;
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("file.txt");
    std::fs::write(&path, b"copy").unwrap();
    let id = offer(&sender, &receiver, path).await;
    let destination = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, destination.path())
        .await
        .unwrap();
    receiver
        .wire
        .blob_request(
            &sender.wire.node_id().to_string(),
            Request::Accept { id: id.clone() },
        )
        .await
        .unwrap();
    sender.engine.stop_blob_copy(&id).await.unwrap();
    let sent = transfer(&sender.engine, &id).await;
    assert!(
        !fetch(
            &receiver,
            &sender,
            sent.manifest.entries[0]
                .hash
                .as_ref()
                .unwrap()
                .parse()
                .unwrap()
        )
        .await
    );
    sender.wire.blob_step(&id).await.unwrap();
    assert_eq!(transfer(&receiver.engine, &id).await.state, "cancelled");
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
    receiver.close().await;
    sender.close().await;
}

#[test]
fn manifests_refuse_traversal_conflicts_and_unbounded_sizes() {
    let id = uuid::Uuid::new_v4().to_string();
    for path in [
        "../private",
        "/absolute",
        "folder/../private",
        "C:\\private",
        "file:stream",
        "CON",
        "folder//file",
        "file.",
    ] {
        let manifest = Manifest {
            id: id.clone(),
            entries: vec![Entry {
                path: path.into(),
                hash: Some("0".repeat(64)),
                size: 1,
            }],
        };
        assert!(manifest.validate().is_err(), "{path}");
    }
    let entry = Entry {
        path: "file".into(),
        hash: Some("0".repeat(64)),
        size: 1,
    };
    assert!(
        Manifest {
            id: id.clone(),
            entries: vec![entry.clone(), entry.clone()]
        }
        .validate()
        .is_err()
    );
    assert!(
        Manifest {
            id,
            entries: vec![Entry {
                size: u64::MAX,
                ..entry
            }]
        }
        .validate()
        .is_err()
    );
}

#[tokio::test]
async fn contacts_can_offer_without_lan_discovery_and_blocking_revokes_access() {
    let sender = Peer::new(61).await;
    let receiver = Peer::new(62).await;
    for (local, remote) in [(&sender, &receiver), (&receiver, &sender)] {
        let organ = remote.engine.blob_owner().await.unwrap();
        store::organs::add_contact(&local.engine.store.pool, &organ, None, "Friend", "", 1)
            .await
            .unwrap();
        store::organs::set_node_id(
            &local.engine.store.pool,
            &organ,
            Some(&remote.wire.node_id().to_string()),
        )
        .await
        .unwrap();
        store::organs::set_trust(&local.engine.store.pool, &organ, "known")
            .await
            .unwrap();
        local.wire.remember_addr(remote.address());
    }
    assert!(sender.wire.nearby().current().is_empty());
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("contact.txt");
    std::fs::write(&path, b"contact copy").unwrap();
    let id = sender
        .wire
        .send_blob_copy(&receiver.wire.node_id().to_string(), vec![path])
        .await
        .unwrap();
    sender.wire.blob_step(&id).await.unwrap();
    assert!(
        receiver
            .engine
            .notifications()
            .await
            .unwrap()
            .iter()
            .any(|notice| notice["kind"] == "blob_sync")
    );
    assert!(
        store::offers::pending(&receiver.engine.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|offer| offer.kind == store::offers::OfferKind::BlobSync)
    );
    let destination = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, destination.path())
        .await
        .unwrap();
    receiver
        .wire
        .blob_request(
            &sender.wire.node_id().to_string(),
            Request::Accept { id: id.clone() },
        )
        .await
        .unwrap();
    let offered = transfer(&sender.engine, &id).await;
    let hash = offered.manifest.entries[0]
        .hash
        .as_ref()
        .unwrap()
        .parse()
        .unwrap();
    let contact = receiver.engine.blob_owner().await.unwrap();
    store::organs::set_node_id(
        &sender.engine.store.pool,
        &contact,
        Some(&SecretKey::from_bytes(&[63; 32]).public().to_string()),
    )
    .await
    .unwrap();
    assert!(!fetch(&receiver, &sender, hash).await);
    store::organs::set_node_id(
        &sender.engine.store.pool,
        &contact,
        Some(&receiver.wire.node_id().to_string()),
    )
    .await
    .unwrap();
    store::organs::set_trust(
        &sender.engine.store.pool,
        &receiver.engine.blob_owner().await.unwrap(),
        "blocked",
    )
    .await
    .unwrap();
    assert!(
        !fetch(
            &receiver,
            &sender,
            offered.manifest.entries[0]
                .hash
                .as_ref()
                .unwrap()
                .parse()
                .unwrap()
        )
        .await
    );
    assert!(sender.wire.blob_targets().await.unwrap().is_empty());
    receiver.close().await;
    sender.close().await;
}

#[tokio::test]
async fn offer_replay_cannot_change_manifest_and_pending_requests_are_bounded() {
    let sender = Peer::new(71).await;
    let receiver = Peer::new(72).await;
    sender.discover(&receiver);
    receiver.discover(&sender);
    let peer = receiver.wire.node_id().to_string();
    let mut manifest = Manifest {
        id: uuid::Uuid::new_v4().to_string(),
        entries: vec![Entry {
            path: "file.txt".into(),
            hash: Some("0".repeat(64)),
            size: 8,
        }],
    };
    sender
        .wire
        .blob_request(
            &peer,
            Request::Offer {
                manifest: manifest.clone(),
            },
        )
        .await
        .unwrap();
    manifest.entries[0].path = "changed.txt".into();
    assert!(
        sender
            .wire
            .blob_request(
                &peer,
                Request::Offer {
                    manifest: manifest.clone()
                }
            )
            .await
            .is_err()
    );
    for _ in 1..8 {
        manifest.id = uuid::Uuid::new_v4().to_string();
        sender
            .wire
            .blob_request(
                &peer,
                Request::Offer {
                    manifest: manifest.clone(),
                },
            )
            .await
            .unwrap();
    }
    manifest.id = uuid::Uuid::new_v4().to_string();
    assert!(
        sender
            .wire
            .blob_request(&peer, Request::Offer { manifest })
            .await
            .is_err()
    );
    assert_eq!(receiver.engine.blob_transfers().await.unwrap().len(), 8);
    receiver.close().await;
    sender.close().await;
}

#[tokio::test]
async fn destination_conflicts_preserve_existing_files() {
    let sender = Peer::new(81).await;
    let receiver = Peer::new(82).await;
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("file.txt");
    std::fs::write(&path, b"new copy").unwrap();
    let id = offer(&sender, &receiver, path).await;
    let directory = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, directory.path())
        .await
        .unwrap();
    let incoming = transfer(&receiver.engine, &id).await;
    let output = Path::new(incoming.destination.as_deref().unwrap());
    std::fs::create_dir(output).unwrap();
    std::fs::write(output.join("file.txt"), b"existing copy").unwrap();
    assert!(receiver.wire.blob_step(&id).await.is_err());
    assert_eq!(
        std::fs::read(output.join("file.txt")).unwrap(),
        b"existing copy"
    );
    assert_eq!(transfer(&receiver.engine, &id).await.state, "accepted");
    let alternate = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, alternate.path())
        .await
        .unwrap();
    receiver.wire.blob_step(&id).await.unwrap();
    let received = transfer(&receiver.engine, &id).await;
    assert_eq!(received.state, "completed");
    assert_eq!(
        std::fs::read(Path::new(received.destination.as_deref().unwrap()).join("file.txt"))
            .unwrap(),
        b"new copy"
    );
    receiver.close().await;
    sender.close().await;
}

#[tokio::test]
async fn dishonest_size_is_bounded_and_never_published() {
    let sender = Peer::new(91).await;
    let receiver = Peer::new(92).await;
    sender.discover(&receiver);
    receiver.discover(&sender);
    let source = tempfile::tempdir().unwrap();
    let file = source.path().join("large.bin");
    std::fs::write(&file, vec![7u8; 1024 * 1024]).unwrap();
    let id = sender
        .wire
        .send_blob_copy(&receiver.wire.node_id().to_string(), vec![file])
        .await
        .unwrap();
    let mut original = transfer(&sender.engine, &id).await;
    original.manifest.entries[0].size = 1;
    store::sqlx::query("UPDATE blob_sync SET manifest = ? WHERE id = ?")
        .bind(serde_json::to_string(&original.manifest).unwrap())
        .bind(&id)
        .execute(&sender.engine.store.pool)
        .await
        .unwrap();
    sender.wire.blob_step(&id).await.unwrap();
    let destination = tempfile::tempdir().unwrap();
    receiver
        .engine
        .accept_blob_copy(&id, destination.path())
        .await
        .unwrap();
    assert!(receiver.wire.blob_step(&id).await.is_err());
    let hash: Hash = original.manifest.entries[0]
        .hash
        .as_ref()
        .unwrap()
        .parse()
        .unwrap();
    let received = receiver
        .engine
        .blob_sync()
        .unwrap()
        .blobs
        .remote()
        .local(hash)
        .await
        .unwrap()
        .local_bytes();
    assert!(received <= 16 * 1024);
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
    receiver.close().await;
    sender.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn snapshots_refuse_symlinks_without_exposing_their_targets() {
    let peer = Peer::new(51).await;
    let source = tempfile::tempdir().unwrap();
    let private = source.path().join("private");
    std::fs::write(&private, b"secret").unwrap();
    let folder = source.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    std::os::unix::fs::symlink(private, folder.join("link")).unwrap();
    assert!(
        peer.engine
            .prepare_blob_copy(
                &SecretKey::from_bytes(&[52; 32]).public().to_string(),
                "test",
                vec![folder]
            )
            .await
            .is_err()
    );
    assert!(peer.engine.blob_transfers().await.unwrap().is_empty());
    peer.close().await;
}
