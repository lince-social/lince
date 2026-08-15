//! The Organ/Cell split (Ontology §11 "Profile vs device surfaces").
//!
//! An Organ is the published identity; a Cell is one device of it. Before the
//! split one row did both jobs, and nothing checked the invariant `hlc.rs`
//! states in its own module comment — which is how the two drifted apart. This
//! file is that check.

use engine::Engine;
use engine::sync::{OpBatch, WireOp};
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;
use store::sync_ops::{self, OpKind};

async fn cell(base_url: &str) -> (Engine, String, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    let this_cell = store::cells::local(&e.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ, this_cell)
}

/// The bug the split exists to fix, stated as a test.
///
/// `nucleus::hlc` is a per-PROCESS counter, so two Cells of one Organ mint
/// identical HLC values as a matter of course. With the Organ in the identity
/// column the second Cell's op collides on `UNIQUE(actor_cell, hlc)`, import
/// treats it as an already-seen duplicate, and it never applies — no error, no
/// log. The op identity has to be the DEVICE for that not to happen.
#[tokio::test]
async fn two_cells_of_one_organ_mint_distinct_op_identities() {
    let (e, organ, cell_a) = cell("http://split.test").await;
    let cell_b = "r-second-cell";
    let hlc = nucleus::hlc::next();

    let one = sync_ops::append(
        &e.store.pool,
        "record",
        "r-shared",
        "head",
        OpKind::Set,
        Some("\"from the laptop\""),
        hlc,
        &cell_a,
        &organ,
        None,
        None,
    )
    .await
    .expect("append");
    // Same Organ, same stamp, DIFFERENT device: this is the ordinary case, and
    // it must land.
    let two = sync_ops::append(
        &e.store.pool,
        "record",
        "r-shared",
        "head",
        OpKind::Set,
        Some("\"from the phone\""),
        hlc,
        cell_b,
        &organ,
        None,
        None,
    )
    .await
    .expect("append");
    assert!(one.is_some(), "the first Cell's op lands");
    assert!(
        two.is_some(),
        "a second Cell of the same Organ must not be swallowed as a duplicate"
    );

    // Same DEVICE, same stamp, arriving from a CONTACT: that IS the same op,
    // and re-importing it is a quiet no-op rather than an error.
    //
    // The `from_contact` argument is what makes this the import path, and it
    // matters: a LOCAL write with a taken identity means another PROCESS is
    // writing as this Cell, so it re-mints its stamp rather than skipping —
    // see `insert_local`. Idempotency-by-identity is an import property, and
    // this is where it is asserted.
    let again = sync_ops::append(
        &e.store.pool,
        "record",
        "r-shared",
        "head",
        OpKind::Set,
        Some("\"replayed\""),
        hlc,
        &cell_a,
        &organ,
        Some("r-some-contact"),
        None,
    )
    .await
    .expect("append");
    assert!(again.is_none(), "op identity is (actor_cell, hlc)");
}

/// Local writes are attributed to the Cell and to the Organ, and the two are
/// not the same value. If they ever are again, the collision above is back.
#[tokio::test]
async fn a_local_write_carries_both_identities() {
    let (e, organ, this_cell) = cell("http://split.test").await;
    let uid = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("apples"),
            kind: RecordKind::Plain,
            head: "Apples",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    let ops = sync_ops::for_field(&e.store.pool, "record", &uid, "head")
        .await
        .expect("ops");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].actor_cell, this_cell, "the writer is the device");
    assert_eq!(ops[0].organ_uid, organ, "the attribution is the identity");
    assert_ne!(
        ops[0].actor_cell, ops[0].organ_uid,
        "a Cell is not its Organ"
    );
}

/// Every Record has an origin Organ, enforced by the schema rather than by
/// every caller remembering.
#[tokio::test]
async fn a_record_cannot_be_stored_without_an_origin() {
    let (e, _organ, _cell) = cell("http://split.test").await;
    let refused = store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             created_at, updated_at)
         VALUES ('r-orphan', NULL, 'plain', 'Orphan', '', '0', 0, 'now', 'now')",
    )
    .execute(&e.store.pool)
    .await;
    assert!(refused.is_err(), "an unattributable Record is refused");

    let uid = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("attributed"),
            kind: RecordKind::Plain,
            head: "Attributed",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let cleared = store::records::set_organ_origin(&e.store.pool, &uid, None).await;
    assert!(
        cleared.is_err(),
        "the origin cannot be cleared back to unknown afterwards"
    );
}

/// An op may not claim an Organ other than the one on the other end of the
/// connection. `op.organ_uid` is stamped straight onto `record.organ_uid`, so
/// an unchecked one lets any sync contact author records in anyone's name.
#[tokio::test]
async fn an_op_claiming_another_organ_is_quarantined() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    let sender = "r-sender-organ";
    store::organs::add_contact(&e.store.pool, sender, None, "Sender", "", 1)
        .await
        .expect("contact");

    let applied = e
        .import_op_batch(&OpBatch {
            from_organ: sender.to_string(),
            ops: vec![WireOp {
                tbl: "record".into(),
                uid: "r-forged".into(),
                field: "head".into(),
                kind: "set".into(),
                value: Some("\"forged\"".into()),
                hlc: nucleus::hlc::next(),
                actor_cell: "r-sender-cell".into(),
                organ_uid: "r-someone-else".into(),
                fact: None,
            }],
        })
        .await
        .expect("import");
    assert_eq!(applied, 0, "nothing applied");
    assert!(
        store::records::get(&e.store.pool, "r-forged")
            .await
            .expect("get")
            .is_none(),
        "the forged record was never created"
    );
}

/// A stamp far in the future is refused rather than believed. Without the
/// bound it would drag this Cell's clock there permanently, and every later
/// local write would be stamped in that same future.
#[tokio::test]
async fn an_op_stamped_far_in_the_future_is_quarantined() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    let sender = "r-sender-organ";
    store::organs::add_contact(&e.store.pool, sender, None, "Sender", "", 1)
        .await
        .expect("contact");

    let far = nucleus::hlc::pack(
        chrono::Utc::now().timestamp_millis() + nucleus::hlc::MAX_CLOCK_DRIFT_MS * 10,
        0,
    );
    let applied = e
        .import_op_batch(&OpBatch {
            from_organ: sender.to_string(),
            ops: vec![WireOp {
                tbl: "record".into(),
                uid: "r-future".into(),
                field: "head".into(),
                kind: "set".into(),
                value: Some("\"from the year 3000\"".into()),
                hlc: far,
                actor_cell: "r-sender-cell".into(),
                organ_uid: sender.to_string(),
                fact: None,
            }],
        })
        .await
        .expect("import");
    assert_eq!(applied, 0, "nothing applied");
    assert!(
        nucleus::hlc::next() < far,
        "the local clock was not dragged into the future"
    );
}

/// Quarantine is bounded per contact (Ontology §11 "Quarantine needs a
/// lifecycle").
///
/// Before this the reject path was CHEAPER for a hostile contact than the
/// valid one — every malformed op was stored verbatim and nothing aged it out,
/// so filling a disk cost them nothing. C1 and C2 each added new ways in
/// without adding a way out, which is what made it urgent.
#[tokio::test]
async fn quarantine_is_a_bounded_ring_per_contact() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    let noisy = "r-noisy-organ";
    let quiet = "r-quiet-organ";
    for organ in [noisy, quiet] {
        store::organs::add_contact(&e.store.pool, organ, None, "Peer", "", 1)
            .await
            .expect("contact");
    }
    store::organs::quarantine(&e.store.pool, quiet, "one bad op", "{}")
        .await
        .expect("quarantine");
    let flood = store::organs::QUARANTINE_PER_CONTACT + 50;
    for n in 0..flood {
        store::organs::quarantine(&e.store.pool, noisy, "garbage", &format!("{{\"n\":{n}}}"))
            .await
            .expect("quarantine");
    }

    let counts = store::organs::quarantine_by_contact(&e.store.pool)
        .await
        .expect("counts");
    let noisy_count = counts.iter().find(|(o, _)| o == noisy).expect("noisy").1;
    let quiet_count = counts.iter().find(|(o, _)| o == quiet).expect("quiet").1;
    assert_eq!(
        noisy_count,
        store::organs::QUARANTINE_PER_CONTACT,
        "the flood is capped"
    );
    assert_eq!(
        quiet_count, 1,
        "and it did not evict a different contact's evidence"
    );

    // The ring keeps the NEWEST, which is what someone diagnosing a live
    // problem needs.
    let kept = store::organs::quarantined_for(&e.store.pool, noisy, 1)
        .await
        .expect("read");
    assert!(kept[0].1.contains(&format!("{}", flood - 1)));

    let cleared = store::organs::clear_quarantine(&e.store.pool, noisy)
        .await
        .expect("clear");
    assert_eq!(cleared, store::organs::QUARANTINE_PER_CONTACT as u64);
}

/// The inbound half of the per-contact scope: `sync_in` said whether to accept
/// their feed at all, and this says WHICH columns of it are applied. Dropped
/// silently rather than quarantined — an out-of-scope op is our own policy
/// working, not the peer misbehaving, and quarantining it would fill the ring
/// on the first sync with any contact wider than our acceptance.
#[tokio::test]
async fn an_accept_scope_drops_the_columns_it_does_not_name() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    let sender = "r-sender-organ";
    store::organs::add_contact(&e.store.pool, sender, None, "Sender", "", 1)
        .await
        .expect("contact");
    store::organs::set_contact_accept_scope(
        &e.store.pool,
        sender,
        Some(&["quantity".to_string()]),
    )
    .await
    .expect("accept scope");

    let op = |field: &str, kind: &str, value: &str| WireOp {
        tbl: "record".into(),
        uid: "r-inbound".into(),
        field: field.into(),
        kind: kind.into(),
        value: Some(value.into()),
        hlc: nucleus::hlc::next(),
        actor_cell: "r-sender-cell".into(),
        organ_uid: sender.into(),
        fact: None,
    };
    e.import_op_batch(&OpBatch {
        from_organ: sender.to_string(),
        ops: vec![
            op("head", "set", "\"their head\""),
            op("quantity", "set", "\"3\""),
        ],
    })
    .await
    .expect("import");

    let row = store::records::get(&e.store.pool, "r-inbound")
        .await
        .expect("get");
    if let Some(row) = row {
        assert_ne!(
            row.head, "their head",
            "a column outside the acceptance must not reach our copy"
        );
    }
    // The quarantine ring stays empty: this is policy, not misbehaviour, and
    // burying real reports under it is the failure being avoided.
    let held = store::organs::quarantined_for(&e.store.pool, sender, 10)
        .await
        .expect("quarantine");
    assert!(held.is_empty(), "dropping by policy is not a report");
}

/// A DELETE is accepted under every acceptance scope there is, including the
/// empty one. Refusing one would leave us holding a Record they removed —
/// exactly the mirror of the outbound rule, for the same reason.
#[tokio::test]
async fn a_delete_arrives_under_the_narrowest_acceptance() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    let sender = "r-sender-organ";
    store::organs::add_contact(&e.store.pool, sender, None, "Sender", "", 1)
        .await
        .expect("contact");
    store::organs::set_contact_accept_scope(&e.store.pool, sender, Some(&[]))
        .await
        .expect("accept nothing");

    let base = WireOp {
        tbl: "record".into(),
        uid: "r-doomed".into(),
        field: String::new(),
        kind: "tombstone".into(),
        value: None,
        hlc: nucleus::hlc::next(),
        actor_cell: "r-sender-cell".into(),
        organ_uid: sender.into(),
        fact: None,
    };
    let applied = e
        .import_op_batch(&OpBatch {
            from_organ: sender.to_string(),
            ops: vec![base],
        })
        .await
        .expect("import");
    assert_eq!(applied, 1, "the delete is applied even accepting nothing");
}
