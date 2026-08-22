use std::sync::Arc;

use engine::Engine;
use engine::roster::CellEntry;
use engine::trust::Signer;
use engine::wire::{Reach, Wire};
use iroh::SecretKey;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn cell(base_url: &str) -> (Arc<Engine>, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (Arc::new(e), organ)
}

fn scratch(who: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lince-reach-{}-{}-{who}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn secret(seed: u8) -> SecretKey {
    SecretKey::from_bytes(&[seed; 32])
}

async fn mailable(
    base_url: &str,
    keyring_dir: &std::path::Path,
    node_id: &str,
) -> (Arc<Engine>, String, Signer) {
    let (engine, organ) = cell(base_url).await;
    engine.set_root_key_path(keyring_dir.join("root.key"));
    let organ_signer = Signer::generate(&organ, "organ");
    engine
        .set_organ_signer(organ_signer.clone())
        .await
        .expect("organ signer");
    engine.set_sealing_keyring_path(keyring_dir.join("keyring.json"));
    let sealing = engine
        .published_sealing_key()
        .await
        .expect("keyring")
        .expect("a fresh keyring generates one");
    let cell_uid = store::cells::local(&engine.store.pool)
        .await
        .expect("cell")
        .expect("a local Cell")
        .uid;
    let root = Signer::load_or_create(
        &keyring_dir.join("root.key"),
        &organ,
        engine::roster::ROOT_KEY_ID,
    )
    .expect("root key on disk");
    engine.publish_root_key(&root).await.expect("root key");
    engine
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid,
                node_id: node_id.into(),
                label: "device".into(),
                operational_key: organ_signer.public_key_b64(),
                sealing_key: Some(sealing),
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .expect("roster");
    (engine, organ, root)
}

async fn introduce(a: &Engine, a_organ: &str, a_root: &Signer, b: &Engine) {
    engine::trust::adopt_key(
        &b.store,
        a_organ,
        engine::roster::ROOT_KEY_ID,
        &a_root.public_key_b64(),
    )
    .await
    .expect("pairing");
    let roster = a.roster_of(a_organ).await.expect("roster").expect("signed");
    assert_eq!(
        b.adopt_roster(&roster).await.expect("adopt"),
        engine::roster::RosterOutcome::Accepted
    );
}

struct Scene {
    sender: Arc<Engine>,
    sender_wire: Wire,
    carrier: Arc<Engine>,
    recipient_organ: String,
}

async fn scene(mode: &str, seed: u8) -> Scene {
    let sender_dir = scratch("sender");
    let recipient_dir = scratch("recipient");

    let (carrier, _carrier_organ) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(seed), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_node = carrier_wire.node_id().to_string();
    tokio::spawn(async move { carrier_wire.serve().await });

    let (sender, sender_organ, sender_root) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_wire = Wire::bind(sender.clone(), secret(seed + 1), Reach::Local)
        .await
        .expect("sender binds");

    let (recipient_engine, recipient_organ, recipient_root) = mailable(
        "http://recipient.test",
        &recipient_dir,
        "node-recipient-that-never-answers",
    )
    .await;

    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    introduce(&sender, &sender_organ, &sender_root, &recipient_engine).await;

    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &recipient_root.public_key_b64(),
        "the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    let published = recipient_engine
        .set_pickup_points(
            &recipient_root,
            vec![engine::roster::PickupPoint {
                organ_uid: "carrier-organ".into(),
                node_id: carrier_node,
                label: "the institute VPS".into(),
            }],
        )
        .await
        .expect("publish pickup");
    assert_eq!(
        sender.adopt_roster(&published).await.expect("adopt"),
        engine::roster::RosterOutcome::Accepted
    );

    store::organs::add_contact(
        &sender.store.pool,
        &recipient_organ,
        None,
        "them",
        "http://recipient.test",
        1,
    )
    .await
    .expect("contact");
    store::organs::set_trust(&sender.store.pool, &recipient_organ, "known")
        .await
        .expect("trust");
    store::organs::set_sync_policy(&sender.store.pool, &recipient_organ, true, false)
        .await
        .expect("policy");
    store::organs::set_mode(&sender.store.pool, &recipient_organ, mode)
        .await
        .expect("mode");

    store::records::create(
        &sender.store.pool,
        NewRecord {
            slug: Some("a-thing"),
            kind: RecordKind::Plain,
            head: "a thing",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    assert!(
        !store::sync_ops::outbox_due(&sender.store.pool)
            .await
            .expect("outbox")
            .is_empty(),
        "the write queued something to deliver"
    );

    Scene {
        sender,
        sender_wire,
        carrier,
        recipient_organ,
    }
}

async fn dial_was_attempted(engine: &Engine, organ: &str) -> bool {
    store::organs::contact(&engine.store.pool, organ)
        .await
        .expect("contact")
        .expect("a row")
        .unreachable_since
        .is_some()
}

async fn held_by_carrier(carrier: &Engine, organ: &str) -> usize {
    store::mailbox::for_recipient(&carrier.store.pool, organ, 10)
        .await
        .expect("held")
        .len()
}

#[tokio::test]
async fn a_mailbox_contact_is_mailed_without_a_dial_attempt() {
    let scene = scene("mailbox", 71).await;

    scene.sender_wire.push_outbox().await.expect("drain");

    assert!(
        !dial_was_attempted(&scene.sender, &scene.recipient_organ).await,
        "a mailbox contact must not be dialled: reach is consulted before the \
         dial, not only after it fails"
    );
    assert_eq!(
        held_by_carrier(&scene.carrier, &scene.recipient_organ).await,
        1,
        "the batch went straight to the pickup point"
    );
    assert!(
        store::sync_ops::outbox_due(&scene.sender.store.pool)
            .await
            .expect("outbox")
            .is_empty(),
        "a mailed batch leaves nothing queued"
    );
}

#[tokio::test]
async fn an_auto_contact_still_dials_first_and_waits_out_the_window() {
    let scene = scene("auto", 81).await;

    scene.sender_wire.push_outbox().await.expect("drain");

    assert!(
        dial_was_attempted(&scene.sender, &scene.recipient_organ).await,
        "auto is today's behaviour: dial, and only fall back once the retry \
         window has passed"
    );
    assert_eq!(
        held_by_carrier(&scene.carrier, &scene.recipient_organ).await,
        0,
        "the retry window has not passed, so nothing was left with anyone"
    );
}

async fn stopped_answering_long_ago(engine: &Engine, organ: &str) {
    let long_ago = chrono::Utc::now() - Wire::MAIL_AFTER - chrono::Duration::minutes(1);
    store::sqlx::query("UPDATE organ_contact SET unreachable_since = ? WHERE record_uid = ?")
        .bind(long_ago.to_rfc3339())
        .bind(organ)
        .execute(&engine.store.pool)
        .await
        .expect("back-date");
}

#[tokio::test]
async fn a_direct_contact_never_falls_back_to_mail() {
    let scene = scene("direct", 91).await;
    stopped_answering_long_ago(&scene.sender, &scene.recipient_organ).await;

    scene.sender_wire.push_outbox().await.expect("drain");

    assert_eq!(
        held_by_carrier(&scene.carrier, &scene.recipient_organ).await,
        0,
        "direct means direct: a contact who asked never to be mailed is never \
         mailed, however long they stay unreachable"
    );
}

#[tokio::test]
async fn an_auto_contact_is_mailed_once_the_window_has_passed() {
    let scene = scene("auto", 101).await;
    stopped_answering_long_ago(&scene.sender, &scene.recipient_organ).await;

    scene.sender_wire.push_outbox().await.expect("drain");

    assert_eq!(
        held_by_carrier(&scene.carrier, &scene.recipient_organ).await,
        1,
        "the same back-dated state that leaves `direct` unmailed does mail \
         `auto` — which is what makes the direct case a real refusal rather \
         than a window that had not elapsed"
    );
}
