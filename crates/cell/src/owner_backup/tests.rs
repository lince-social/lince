use super::*;
use nucleus::social::{Command, PostDraft};
use utils::owner_backup::{Passphrase, open_archive_into_staging};

struct Fixture {
    directory: tempfile::TempDir,
    data: PathBuf,
    context: String,
    message: String,
    wallet: String,
    posting_wallet: String,
}

impl Fixture {
    fn new(fail_delete: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("owner");
        fs::create_dir(&data).unwrap();
        fs::create_dir(data.join("keys")).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (context, message, wallet, posting_wallet) = runtime.block_on(async {
            let engine = engine::Engine::open(&format!("sqlite://{}?mode=rwc", data.join("lince.db").display()))
                .await.unwrap();
            let organ = store::organs::local(&engine.store.pool).await.unwrap().unwrap().uid;
            let cell = store::cells::local(&engine.store.pool).await.unwrap().unwrap().uid;
            let root_path = data.join("keys").join(filename(FileKind::OwnerRoot));
            private_write(&root_path, &[181; 32]).unwrap();
            engine.set_root_key_path(root_path);
            engine.set_sealing_keyring_path(data.join("keys").join(filename(FileKind::RecordKeyring)));
            let root = Signer::from_bytes(&organ, engine::roster::ROOT_KEY_ID, [181; 32]);
            engine.publish_root_key(&root).await.unwrap();
            let device = Signer::from_bytes(&organ, &engine::roster::cell_key_id(&cell), [182; 32]);
            engine.set_signer(device.clone()).await.unwrap();
            engine.set_organ_signer(device).await.unwrap();
            engine.sealing_keyring().await.unwrap();
            let draft = engine.social_command(Command::SaveDraft {
                record: None, source: None, draft: PostDraft {
                    title: "Stopped owner capture".into(), ..Default::default()
                },
            }, None, nucleus::execution::now()).await.unwrap().data.unwrap();
            let context = draft["record"].as_str().unwrap().to_owned();
            let host = engine::wire::node_secret(&directory.path().join("host.key")).unwrap().public().to_string();
            engine.social_command(Command::PrepareReplyKeys { record: context.clone(), services: vec![host] },
                None, nucleus::execution::now()).await.unwrap();
            let preview = engine.social_command(Command::Preview { record: context.clone(), state: nucleus::social::PostState::Active },
                None, nucleus::execution::now()).await.unwrap().data.unwrap();
            engine.social_command(Command::Publish { record: context.clone(),
                preview_hash: preview["preview_hash"].as_str().unwrap().into(),
                document: serde_json::from_value(preview["document"].clone()).unwrap() },
                None, nucleus::execution::now()).await.unwrap();
            let message = store::records::create(&engine.store.pool, store::records::NewRecord {
                slug: None, kind: nucleus::RecordKind::Message, head: "Retained message",
                body: "Private retained message from the owner backup fixture",
                quantity: nucleus::DecimalValue::from_mantissa(0, 1).unwrap(),
            }).await.unwrap().uid;
            store::sqlx::query("INSERT INTO social_private_outbox(id,context,body,hash,expires_at) VALUES('backup-envelope',?,'{}','fixture-hash',?)")
                .bind(&context).bind(nucleus::execution::now().timestamp()+86400).execute(&engine.store.pool).await.unwrap();
            store::sqlx::query("INSERT INTO social_private_destination(envelope,service) VALUES('backup-envelope','fixture-host')")
                .execute(&engine.store.pool).await.unwrap();
            if fail_delete {
                store::sqlx::query("CREATE TRIGGER fail_backup_sanitize BEFORE DELETE ON social_device_state WHEN OLD.kind='account' BEGIN SELECT RAISE(ABORT,'injected snapshot failure'); END")
                    .execute(&engine.store.pool).await.unwrap();
            }
            let wallet = store::social::device_state(&engine.store.pool, &format!("authority:{context}"))
                .await.unwrap().unwrap().0;
            let posting_wallet = store::social::device_state(&engine.store.pool, &format!("posting:{context}"))
                .await.unwrap().unwrap().0;
            engine.store.pool.close().await;
            (context, message, wallet, posting_wallet)
        });
        drop(runtime);
        Self {
            directory,
            data,
            context,
            message,
            wallet,
            posting_wallet,
        }
    }

    fn request(&self, name: &str) -> BackupRequest {
        BackupRequest {
            destination: self.directory.path().join(name),
            passphrase: Passphrase::new("several words kept outside serialized actions".into())
                .unwrap(),
        }
    }

    fn no_staging(&self) {
        let names: Vec<_> = fs::read_dir(self.directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".lince-owner-"))
            .collect();
        assert!(
            names.is_empty(),
            "Disposable staging must be removed: {names:?}"
        );
    }
}

#[test]
fn stopped_capture_preserves_source_history_wallet_and_excludes_snapshot_ratchets() {
    let fixture = Fixture::new(false);
    let database_before = fs::read(fixture.data.join("lince.db")).unwrap();
    let root_before = fs::read(
        fixture
            .data
            .join("keys")
            .join(filename(FileKind::OwnerRoot)),
    )
    .unwrap();
    let request = fixture.request("owner.lince-backup");
    let path = request.destination.clone();
    let manifest = capture_stopped(&fixture.data, request).unwrap();
    assert_eq!(manifest.files.len(), 4);
    assert_eq!(
        fs::read(fixture.data.join("lince.db")).unwrap(),
        database_before
    );
    assert_eq!(
        fs::read(
            fixture
                .data
                .join("keys")
                .join(filename(FileKind::OwnerRoot))
        )
        .unwrap(),
        root_before
    );
    fixture.no_staging();
    let encrypted = fs::read(&path).unwrap();
    assert!(
        !encrypted
            .windows(manifest.organ.len())
            .any(|part| part == manifest.organ.as_bytes())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let inspection = tempfile::tempdir().unwrap();
    let opened = open_archive_into_staging(
        &mut File::open(path).unwrap(),
        fixture.request("unused").passphrase.expose(),
        |entry| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(inspection.path().join(filename(entry.kind)))
        },
    )
    .unwrap();
    assert_eq!(manifest, opened);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for (path, sanitized) in [(fixture.data.join("lince.db"), false), (inspection.path().join("lince.db"), true)] {
            let store = open_database(&path, true).await.unwrap();
            let accounts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_device_state WHERE kind IN ('account','session')")
                .fetch_one(&store.pool).await.unwrap();
            assert_eq!(accounts, if sanitized { 0 } else { 1 });
            let held = store::social::device_state(&store.pool, &format!("authority:{}", fixture.context))
                .await.unwrap().unwrap().0;
            assert_eq!(held, fixture.wallet);
            let posting_wallet = store::social::device_state(&store.pool, &format!("posting:{}", fixture.context))
                .await.unwrap().unwrap().0;
            assert_eq!(posting_wallet, fixture.posting_wallet);
            let message = store::records::get(&store.pool, &fixture.message).await.unwrap().unwrap();
            assert_eq!(message.body, "Private retained message from the owner backup fixture");
            assert_eq!(message.quantity, nucleus::DecimalValue::from_mantissa(0, 1).unwrap());
            let (state, destination): (String, String) = store::sqlx::query_as("SELECT o.state,d.state FROM social_private_outbox o JOIN social_private_destination d ON d.envelope=o.id WHERE o.id='backup-envelope'")
                .fetch_one(&store.pool).await.unwrap();
            assert_eq!(state, if sanitized { "held" } else { "pending" });
            assert_eq!(destination, if sanitized { "cancelled" } else { "pending" });
            store.pool.close().await;
        }
    });
}

#[test]
fn unsafe_or_existing_destinations_are_refused_without_replacing_data() {
    let fixture = Fixture::new(false);
    let before = fs::read(fixture.data.join("lince.db")).unwrap();
    let existing = fixture.request("existing.lince-backup");
    private_write(&existing.destination, b"Keep the earlier archive").unwrap();
    assert_eq!(
        capture_stopped(&fixture.data, existing).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        fs::read(fixture.directory.path().join("existing.lince-backup")).unwrap(),
        b"Keep the earlier archive"
    );
    let mut inside = fixture.request("unused");
    inside.destination = fixture.data.join("backup");
    assert!(capture_stopped(&fixture.data, inside).is_err());
    let mut traversal = fixture.request("unused");
    traversal.destination = fixture.directory.path().join("owner/../traversal");
    assert!(capture_stopped(&fixture.data, traversal).is_err());
    #[cfg(unix)]
    {
        let link = fixture.directory.path().join("linked-directory");
        std::os::unix::fs::symlink(fixture.directory.path(), &link).unwrap();
        let mut linked = fixture.request("unused");
        linked.destination = link.join("backup");
        assert!(capture_stopped(&fixture.data, linked).is_err());
        let dangling = fixture.request("dangling-backup");
        std::os::unix::fs::symlink(
            fixture.directory.path().join("absent"),
            &dangling.destination,
        )
        .unwrap();
        assert!(capture_stopped(&fixture.data, dangling).is_err());
    }
    assert_eq!(fs::read(fixture.data.join("lince.db")).unwrap(), before);
    fixture.no_staging();
}

#[test]
fn missing_or_mismatched_owner_material_never_publishes_an_archive() {
    let fixture = Fixture::new(false);
    let before = fs::read(fixture.data.join("lince.db")).unwrap();
    for kind in [
        FileKind::OwnerRoot,
        FileKind::AuthorityWallet,
        FileKind::RecordKeyring,
    ] {
        let path = fixture.data.join("keys").join(filename(kind));
        let original = fs::read(&path).unwrap();
        fs::write(
            &path,
            if kind == FileKind::RecordKeyring {
                vec![b'{', b'}']
            } else {
                vec![199; 32]
            },
        )
        .unwrap();
        let request = fixture.request("rejected.lince-backup");
        let destination = request.destination.clone();
        assert!(capture_stopped(&fixture.data, request).is_err());
        assert!(!destination.exists());
        fs::write(path, original).unwrap();
        fixture.no_staging();
    }
    let wallet = fixture
        .data
        .join("keys")
        .join(filename(FileKind::AuthorityWallet));
    fs::remove_file(wallet).unwrap();
    assert!(capture_stopped(&fixture.data, fixture.request("missing-wallet")).is_err());
    let root = fixture
        .data
        .join("keys")
        .join(filename(FileKind::OwnerRoot));
    fs::remove_file(root).unwrap();
    assert!(capture_stopped(&fixture.data, fixture.request("missing-root")).is_err());
    assert_eq!(fs::read(fixture.data.join("lince.db")).unwrap(), before);
    fixture.no_staging();
}

#[test]
fn snapshot_failure_preserves_source_and_cleans_plaintext_staging() {
    let fixture = Fixture::new(true);
    let before = fs::read(fixture.data.join("lince.db")).unwrap();
    let request = fixture.request("failed.lince-backup");
    let destination = request.destination.clone();
    assert!(capture_stopped(&fixture.data, request).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read(fixture.data.join("lince.db")).unwrap(), before);
    fixture.no_staging();
}

#[test]
fn forged_reply_or_anonymous_owner_bindings_are_refused_with_matching_wallet_keys() {
    for (namespace, field) in [
        (
            nucleus::social::requests::SESSION_AUTHORITY_NAMESPACE,
            "$.binding.signature",
        ),
        (
            nucleus::social::PUBLICATION_NAMESPACE,
            "$.anonymous_binding.signature",
        ),
    ] {
        let fixture = Fixture::new(false);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let store = open_database(&fixture.data.join("lince.db"), false).await.unwrap();
            store::sqlx::query("UPDATE record_extension SET fds=json_set(fds,?,'forged-owner-binding') WHERE record_uid=? AND namespace=?")
                .bind(field).bind(&fixture.context).bind(namespace).execute(&store.pool).await.unwrap();
            store.pool.close().await;
        });
        drop(runtime);
        let before = fs::read(fixture.data.join("lince.db")).unwrap();
        let request = fixture.request("forged.lince-backup");
        let destination = request.destination.clone();
        assert!(capture_stopped(&fixture.data, request).is_err());
        assert!(!destination.exists());
        assert_eq!(fs::read(fixture.data.join("lince.db")).unwrap(), before);
        fixture.no_staging();
    }
}
