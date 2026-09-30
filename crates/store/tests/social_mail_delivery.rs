use std::sync::Arc;

use serde_json::json;
use store::mailbox::{HeldBundle, delivery};

async fn database() -> (tempfile::TempDir, Arc<store::Store>, String) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("mail.db").display());
    let store = Arc::new(store::Store::open_durable(&url).await.unwrap());
    store::mailbox::register(&store.pool, "recipient", "root", "Recipient", 1000)
        .await
        .unwrap();
    delivery::advance_roster(&store.pool, "recipient", 0,
        &json!({"roster":{"cells":[{"cell_uid":"phone","node_id":"node","capabilities":["write"],"sealing_key":{"key_id":"x25519:cell:phone:1"}}]}}).to_string()).await.unwrap();
    (directory, store, url)
}

fn bundle(uid: &str, bytes: usize) -> HeldBundle {
    let mut body = json!({"recipients":[{"key_id":"x25519:cell:phone:1"}],"ciphertext":""});
    body["ciphertext"] = "x".repeat(bytes - body.to_string().len()).into();
    HeldBundle {
        uid: uid.into(),
        to_organ: "recipient".into(),
        from_organ: "sender".into(),
        from_cell: "sender-cell".into(),
        from_node: "sender-node".into(),
        body: body.to_string(),
        bytes: bytes as i64,
        received_at: nucleus::execution::now().to_rfc3339(),
        expires_at: (nucleus::execution::now() + chrono::Duration::days(30)).to_rfc3339(),
    }
}

#[tokio::test]
async fn concurrent_deposits_cannot_overrun_recipient_or_global_quota() {
    let (_directory, store, _url) = database().await;
    let mut jobs = Vec::new();
    for index in 0..16 {
        let store = store.clone();
        jobs.push(tokio::spawn(async move {
            delivery::deposit(&store.pool, &bundle(&format!("mail-{index}"), 600), 1000)
                .await
                .unwrap()
        }));
    }
    let mut accepted = 0;
    for job in jobs {
        accepted += usize::from(job.await.unwrap() == delivery::Deposit::Stored);
    }
    assert_eq!(accepted, 1);
    assert_eq!(
        store::mailbox::held_bytes(&store.pool, "recipient")
            .await
            .unwrap(),
        600
    );
    store::mailbox::register(&store.pool, "second", "root-2", "Second", 1000)
        .await
        .unwrap();
    let mut other = bundle("other", 600);
    other.to_organ = "second".into();
    assert_eq!(
        delivery::deposit(&store.pool, &other, 1000).await.unwrap(),
        delivery::Deposit::Full
    );
}

#[tokio::test]
async fn response_loss_retries_use_one_slot_even_when_the_quota_is_full() {
    let (_directory, store, _url) = database().await;
    let original = bundle("stable", 1000);
    assert_eq!(
        delivery::deposit(&store.pool, &original, 1000)
            .await
            .unwrap(),
        delivery::Deposit::Stored
    );
    for _ in 0..10 {
        assert_eq!(
            delivery::deposit(&store.pool, &original, 1000)
                .await
                .unwrap(),
            delivery::Deposit::Stored
        );
    }
    let mut forged = original.clone();
    forged.body = "altered".into();
    assert_eq!(
        delivery::deposit(&store.pool, &forged, 1000).await.unwrap(),
        delivery::Deposit::Conflict
    );
    assert_eq!(
        store::mailbox::waiting(&store.pool, "recipient")
            .await
            .unwrap()
            .bundles,
        1
    );
}

#[tokio::test]
async fn roster_floor_survives_restart_and_rejects_rollback_and_equivocation() {
    let (_directory, store, url) = database().await;
    assert!(
        delivery::advance_roster(&store.pool, "recipient", 2, "current")
            .await
            .unwrap()
    );
    store.pool.close().await;
    let reopened = store::Store::open_durable(&url).await.unwrap();
    assert!(
        !delivery::advance_roster(&reopened.pool, "recipient", 1, "old")
            .await
            .unwrap()
    );
    assert!(
        !delivery::advance_roster(&reopened.pool, "recipient", 2, "conflict")
            .await
            .unwrap()
    );
    assert_eq!(
        delivery::roster_floor(&reopened.pool, "recipient")
            .await
            .unwrap(),
        Some((2, "current".into()))
    );
}

#[tokio::test]
async fn one_device_cannot_remove_the_only_copy_for_another_addressed_device() {
    let (_directory, store, _url) = database().await;
    let roster = json!({"roster":{"cells":[
        {"cell_uid":"phone","node_id":"node-phone","capabilities":["write"]},
        {"cell_uid":"laptop","node_id":"node-laptop","capabilities":["write"]},
        {"cell_uid":"new","node_id":"node-new","capabilities":["write"]}
    ]}});
    assert!(
        delivery::advance_roster(&store.pool, "recipient", 1, &roster.to_string())
            .await
            .unwrap()
    );
    let mut mail = bundle("shared", 200);
    mail.body =
        json!({"recipients":[{"key_id":"x25519:cell:phone:1"},{"key_id":"x25519:cell:laptop:1"}]})
            .to_string();
    mail.bytes = mail.body.len() as i64;
    delivery::deposit(&store.pool, &mail, 1000).await.unwrap();
    assert_eq!(
        delivery::acknowledge(&store.pool, "recipient", "node-phone", &["shared".into()])
            .await
            .unwrap(),
        0
    );
    assert!(
        delivery::for_device(&store.pool, "recipient", "node-phone", 50)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        delivery::for_device(&store.pool, "recipient", "node-laptop", 50)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        delivery::acknowledge(&store.pool, "recipient", "revoked", &["shared".into()])
            .await
            .is_err()
    );
    assert_eq!(
        delivery::acknowledge(&store.pool, "recipient", "node-laptop", &["shared".into()])
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        delivery::deposit(&store.pool, &mail, 0).await.unwrap(),
        delivery::Deposit::Stored
    );
    assert_eq!(
        store::mailbox::waiting(&store.pool, "recipient")
            .await
            .unwrap()
            .bundles,
        0
    );
    mail.body.push(' ');
    mail.bytes = mail.body.len() as i64;
    assert_eq!(
        delivery::deposit(&store.pool, &mail, 1000).await.unwrap(),
        delivery::Deposit::Conflict
    );
}

#[tokio::test]
async fn saved_envelope_is_recoverable_after_restart_and_deduplicates_after_processing() {
    let (_directory, store, url) = database().await;
    let hash = "a".repeat(64);
    delivery::receive(
        &store.pool,
        "mail",
        "carrier",
        "sealed bytes",
        &hash,
        "2099-01-01",
    )
    .await
    .unwrap();
    store.pool.close().await;
    let reopened = store::Store::open_durable(&url).await.unwrap();
    assert_eq!(
        delivery::pending(&reopened.pool).await.unwrap(),
        vec![("mail".into(), "sealed bytes".into())]
    );
    delivery::processed(&reopened.pool, "mail", None)
        .await
        .unwrap();
    delivery::receive(
        &reopened.pool,
        "mail",
        "second-carrier",
        "sealed bytes",
        &hash,
        "2099-01-01",
    )
    .await
    .unwrap();
    assert!(delivery::pending(&reopened.pool).await.unwrap().is_empty());
    assert!(
        delivery::receive(
            &reopened.pool,
            "mail",
            "carrier",
            "changed bytes",
            &"b".repeat(64),
            "2099-01-01"
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn unreadable_mail_is_quarantined_without_blocking_other_mail_and_can_be_retried() {
    let (_directory, store, _url) = database().await;
    delivery::receive(
        &store.pool,
        "bad",
        "carrier",
        "bad bytes",
        &"a".repeat(64),
        "2099-01-01",
    )
    .await
    .unwrap();
    for _ in 0..8 {
        delivery::processed(&store.pool, "bad", Some("Missing keys"))
            .await
            .unwrap();
    }
    delivery::receive(
        &store.pool,
        "good",
        "carrier",
        "good bytes",
        &"b".repeat(64),
        "2099-01-01",
    )
    .await
    .unwrap();
    assert_eq!(
        delivery::pending(&store.pool).await.unwrap(),
        vec![("good".into(), "good bytes".into())]
    );
    assert_eq!(
        delivery::inbox_status(&store.pool).await.unwrap()[0]["state"],
        "quarantine"
    );
    assert!(delivery::retry(&store.pool, "bad").await.unwrap());
    assert_eq!(delivery::pending(&store.pool).await.unwrap().len(), 2);
}

#[tokio::test]
async fn failed_registration_does_not_spend_the_invitation() {
    let (_directory, store, _url) = database().await;
    store::mailbox::put_invite(&store.pool, "invite", "Guest", 1000, "2099-01-01T00:00:00Z")
        .await
        .unwrap();
    assert!(
        delivery::redeem_and_register(&store.pool, "invite", "recipient", "wrong-root")
            .await
            .is_err()
    );
    assert!(
        delivery::redeem_and_register(&store.pool, "invite", "recipient", "root")
            .await
            .unwrap()
    );
    assert!(
        !delivery::redeem_and_register(&store.pool, "invite", "recipient", "root")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn collection_is_bounded_in_bytes_and_ignores_expired_mail_without_a_sweep() {
    let (_directory, store, _url) = database().await;
    store::mailbox::register(
        &store.pool,
        "recipient",
        "root",
        "Recipient",
        64 * 1024 * 1024,
    )
    .await
    .unwrap();
    for index in 0..24 {
        delivery::deposit(
            &store.pool,
            &bundle(&format!("large-{index:02}"), 1024 * 1024),
            64 * 1024 * 1024,
        )
        .await
        .unwrap();
    }
    let rows = delivery::for_device(&store.pool, "recipient", "node", 256)
        .await
        .unwrap();
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .map(|row| row.body.len() * 2 + 2048)
            .sum::<usize>()
            <= 16 * 1024 * 1024 - 4096
    );
    store::mailbox::backdate_expiry(&store.pool, "large-00", "2000-01-01T00:00:00Z")
        .await
        .unwrap();
    assert!(
        delivery::for_device(&store.pool, "recipient", "node", 256)
            .await
            .unwrap()
            .iter()
            .all(|row| row.uid != "large-00")
    );
    assert!(
        store::mailbox::for_recipient(&store.pool, "recipient", 256)
            .await
            .unwrap()
            .iter()
            .all(|row| row.uid != "large-00")
    );
}

#[tokio::test]
async fn normal_file_store_initializes_identity_and_uses_full_durability() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("app.db").display());
    let store = store::Store::open(&url).await.unwrap();
    let synchronous: i64 = store::sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(synchronous, 2);
    assert!(store::organs::local(&store.pool).await.unwrap().is_some());
    assert!(store::linguas::ensure_local(&store.pool).await.is_ok());
}

fn outgoing(index: usize) -> store::mailbox::outbox::Envelope {
    store::mailbox::outbox::Envelope {
        intent: "a".repeat(64),
        uid: format!("outgoing-{index}"),
        to_organ: "recipient".into(),
        body: format!("ciphertext-{index}"),
        expires_at: "2099-01-01T00:00:00Z".into(),
        requested_copies: 2,
        next_attempt: 0,
    }
}

#[tokio::test]
async fn concurrent_senders_and_restart_reuse_the_prepared_outgoing_envelope() {
    let (_directory, store, url) = database().await;
    let mut jobs = Vec::new();
    for index in 0..16 {
        let store = store.clone();
        jobs.push(tokio::spawn(async move {
            store::mailbox::outbox::prepare(&store.pool, &outgoing(index))
                .await
                .unwrap()
        }));
    }
    let first = jobs.remove(0).await.unwrap();
    for job in jobs {
        assert_eq!(job.await.unwrap(), first);
    }
    store.pool.close().await;
    let reopened = store::Store::open_durable(&url).await.unwrap();
    assert_eq!(
        store::mailbox::outbox::get(&reopened.pool, &first.intent)
            .await
            .unwrap(),
        Some(first)
    );
}

#[tokio::test]
async fn receipts_keep_each_server_separate_and_expiry_notices_cannot_cross_hosts() {
    let (_directory, store, _url) = database().await;
    let mail = store::mailbox::outbox::prepare(&store.pool, &outgoing(0))
        .await
        .unwrap();
    for _ in 0..2 {
        store::mailbox::outbox::accepted(&store.pool, &mail.uid, "first", "node-first")
            .await
            .unwrap();
    }
    store::mailbox::outbox::accepted(&store.pool, &mail.uid, "second", "node-second")
        .await
        .unwrap();
    assert_eq!(
        store::mailbox::outbox::receipts(&store.pool, &mail.uid)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(store::mail_left::outstanding(&store.pool).await.unwrap(), 2);
    assert!(
        !store::mail_left::mark_expired(&store.pool, "imposter", &mail.uid, "2099-02-01")
            .await
            .unwrap()
    );
    assert!(
        store::mail_left::mark_expired(&store.pool, "node-first", &mail.uid, "2099-02-01")
            .await
            .unwrap()
    );
    assert_eq!(store::mail_left::outstanding(&store.pool).await.unwrap(), 1);
    assert!(
        store::mailbox::outbox::pending(&store.pool, 32)
            .await
            .unwrap()
            .is_empty()
    );
    let status = store::mailbox::outbox::status(&store.pool).await.unwrap();
    assert_eq!(status[0]["copies"], 2);
    assert!(status[0].get("body").is_none());
}

#[tokio::test]
async fn partial_outgoing_delivery_backs_off_and_explicit_retry_resets_it() {
    let (_directory, store, _url) = database().await;
    let mail = store::mailbox::outbox::prepare(&store.pool, &outgoing(0))
        .await
        .unwrap();
    store::mailbox::outbox::accepted(&store.pool, &mail.uid, "first", "node-first")
        .await
        .unwrap();
    assert_eq!(
        store::mailbox::outbox::pending(&store.pool, 32)
            .await
            .unwrap()
            .len(),
        1
    );
    store::mailbox::outbox::attempted(&store.pool, &mail.uid, Some("Second host is offline"))
        .await
        .unwrap();
    assert!(
        store::mailbox::outbox::pending(&store.pool, 32)
            .await
            .unwrap()
            .is_empty()
    );
    store::mailbox::outbox::retry_recipient(&store.pool, "recipient")
        .await
        .unwrap();
    assert_eq!(
        store::mailbox::outbox::pending(&store.pool, 32)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store::mailbox::outbox::status(&store.pool).await.unwrap()[0]["state"],
        "partially accepted"
    );
}

#[tokio::test]
async fn expired_unreadable_ciphertext_releases_space_and_cannot_be_retried() {
    let (_directory, store, _url) = database().await;
    delivery::receive(
        &store.pool,
        "old",
        "carrier",
        "expired ciphertext",
        &"a".repeat(64),
        "2000-01-01T00:00:00Z",
    )
    .await
    .unwrap();
    assert!(delivery::pending(&store.pool).await.unwrap().is_empty());
    assert_eq!(
        delivery::inbox_status(&store.pool).await.unwrap()[0]["state"],
        "expired"
    );
    assert!(!delivery::retry(&store.pool, "old").await.unwrap());
    let body: String = store::sqlx::query_scalar("SELECT body FROM mailbox_inbox WHERE uid='old'")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn completed_delivery_suppresses_redeposit_after_the_host_restarts() {
    let (_directory, store, url) = database().await;
    let roster = json!({"roster":{"cells":[{"cell_uid":"phone","node_id":"node-phone","capabilities":["write"]}]}});
    delivery::advance_roster(&store.pool, "recipient", 1, &roster.to_string())
        .await
        .unwrap();
    let mut mail = bundle("complete", 100);
    mail.body = json!({"recipients":[{"key_id":"x25519:cell:phone:1"}]}).to_string();
    mail.bytes = mail.body.len() as i64;
    assert_eq!(
        delivery::deposit(&store.pool, &mail, 1000).await.unwrap(),
        delivery::Deposit::Stored
    );
    assert_eq!(
        delivery::acknowledge(&store.pool, "recipient", "node-phone", &[mail.uid.clone()])
            .await
            .unwrap(),
        1
    );
    store.pool.close().await;
    let reopened = store::Store::open_durable(&url).await.unwrap();
    assert_eq!(
        delivery::deposit(&reopened.pool, &mail, 0).await.unwrap(),
        delivery::Deposit::Stored
    );
    assert_eq!(
        store::mailbox::waiting(&reopened.pool, "recipient")
            .await
            .unwrap()
            .bundles,
        0
    );
}

#[tokio::test]
async fn unaddressed_old_envelopes_do_not_block_mail_for_a_newly_enrolled_device() {
    let (_directory, store, _url) = database().await;
    store::mailbox::register(&store.pool, "recipient", "root", "Recipient", 100000)
        .await
        .unwrap();
    let roster = json!({"roster":{"cells":[
        {"cell_uid":"phone","node_id":"node-phone","capabilities":["write"]},
        {"cell_uid":"new","node_id":"node-new","capabilities":["write"]},
        {"cell_uid":"carrier","node_id":"node-carrier","capabilities":[]}
    ]}});
    delivery::advance_roster(&store.pool, "recipient", 1, &roster.to_string())
        .await
        .unwrap();
    for index in 0..100 {
        delivery::deposit(
            &store.pool,
            &bundle(&format!("old-{index:03}"), 200),
            100000,
        )
        .await
        .unwrap();
    }
    let mut mail = bundle("new", 200);
    mail.body = json!({"recipients":[{"key_id":"x25519:cell:new:1"}]}).to_string();
    mail.bytes = mail.body.len() as i64;
    delivery::deposit(&store.pool, &mail, 100000).await.unwrap();
    let page = delivery::for_device(&store.pool, "recipient", "node-new", 1)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].uid, "new");
    assert!(
        delivery::for_device(&store.pool, "recipient", "node-carrier", 256)
            .await
            .unwrap()
            .is_empty()
    );
}
