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
    store::organs::set_contact_accept_scope(&e.store.pool, sender, Some(&["quantity".to_string()]))
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

/// Store a roster for `organ` naming `cells`, without signing it.
///
/// The lookup this exercises reads the payload and nothing else — signature
/// checking lives in `engine::roster` and has its own tests — so putting one
/// here keeps the test about the admissibility gate rather than about key
/// handling, and lets it stand up a third Organ we have never talked to.
async fn hold_roster(e: &Engine, organ: &str, cells: &[&str]) {
    let payload = serde_json::json!({
        "organ_uid": organ,
        "root_key": "k-root",
        "version": 1,
        "not_after": "2099-01-01T00:00:00Z",
        // Every field, not only the uid the ownership check reads: this
        // payload is also parsed as a whole roster on the import path, and a
        // partial one fails there for a reason that has nothing to do with
        // what the test is about.
        "cells": cells
            .iter()
            .map(|uid| serde_json::json!({
                "cell_uid": uid,
                "node_id": format!("node-{uid}"),
                "label": uid,
                "operational_key": format!("opkey-{uid}"),
                "front_door": false,
                "capabilities": engine::roster::full_capabilities(),
            }))
            .collect::<Vec<_>>(),
    })
    .to_string();
    store::roster::put(
        &e.store.pool,
        &store::roster::StoredRoster {
            organ_uid: organ.into(),
            root_key: "k-root".into(),
            version: 1,
            not_after: "2099-01-01T00:00:00Z".into(),
            payload,
            signature: "unchecked-here".into(),
        },
    )
    .await
    .expect("hold a roster");
}

/// Dedup poisoning, and the case the roster check alone cannot reach.
///
/// The op log is idempotent by `(actor_cell, hlc)`. A contact that may name
/// any Cell it likes can therefore pre-occupy a Cell uid belonging to somebody
/// ELSE with a stamp slightly in the future; when that Organ's real op arrives
/// later it is dropped as an already-seen duplicate, silently and permanently.
///
/// The positive check — is this Cell in the SENDER's roster — needs a roster
/// we may not hold, and refusing every rosterless sender is not available: a
/// refusal is quarantined, the ring is bounded and nothing replays it, so it
/// would permanently drop an Organ that legitimately has not published one.
/// So the question is asked the other way round, against what we already hold.
#[tokio::test]
async fn an_op_claiming_a_cell_we_know_belongs_to_someone_else_is_refused() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    let sender = "r-sender-organ";
    let victim = "r-victim-organ";
    let victim_cell = "r-victim-cell";
    store::organs::add_contact(&e.store.pool, sender, None, "Sender", "", 1)
        .await
        .expect("contact");
    // We hold the victim's roster — we know that Cell is theirs — but none for
    // the sender, which is the state this branch exists for.
    hold_roster(&e, victim, &[victim_cell]).await;

    let applied = e
        .import_op_batch(&OpBatch {
            from_organ: sender.to_string(),
            ops: vec![WireOp {
                tbl: "record".into(),
                uid: "r-poisoned".into(),
                field: "head".into(),
                kind: "set".into(),
                value: Some("\"pre-occupying the dedup key\"".into()),
                // Everything else about this op is admissible: the Organ it
                // claims IS the sending one, and the stamp is inside the drift
                // window. Only the authorship is a lie — which is what makes
                // this test fail when the branch is deleted rather than being
                // caught by one of the other refusals.
                hlc: nucleus::hlc::next(),
                actor_cell: victim_cell.into(),
                organ_uid: sender.into(),
                fact: None,
            }],
        })
        .await
        .expect("import");

    assert_eq!(applied, 0, "an op authored by a Cell that is not theirs");
    assert!(
        store::records::get(&e.store.pool, "r-poisoned")
            .await
            .expect("get")
            .is_none(),
        "and nothing of it reached the read model"
    );
    let held = store::organs::quarantined_for(&e.store.pool, sender, 10)
        .await
        .expect("quarantine");
    assert!(
        held.iter()
            .any(|(reason, _, _)| reason.contains("belongs to another Organ")),
        "the refusal is a REPORT, not a silent drop — it is the only trace a \
         person diagnosing this would have: {held:?}"
    );
}

/// The same attack aimed at us, which is the form the plan describes: a
/// contact pre-occupying OUR Cell's dedup key so our next real op is dropped
/// as a duplicate everywhere it lands.
///
/// Checked by name rather than only through the lookup above, because a Cell
/// on a first boot that has not published a roster yet would not be found by
/// it — and being unable to protect our own identity until we have published
/// is the wrong order of dependency.
#[tokio::test]
async fn an_op_claiming_this_cell_as_its_author_is_refused() {
    let (e, _organ, this_cell) = cell("http://receiver.test").await;
    let sender = "r-sender-organ";
    store::organs::add_contact(&e.store.pool, sender, None, "Sender", "", 1)
        .await
        .expect("contact");

    let applied = e
        .import_op_batch(&OpBatch {
            from_organ: sender.to_string(),
            ops: vec![WireOp {
                tbl: "record".into(),
                uid: "r-ours".into(),
                field: "head".into(),
                kind: "set".into(),
                value: Some("\"claiming to be your own laptop\"".into()),
                hlc: nucleus::hlc::next(),
                actor_cell: this_cell.clone(),
                organ_uid: sender.into(),
                fact: None,
            }],
        })
        .await
        .expect("import");

    assert_eq!(applied, 0, "nobody else authors ops as this Cell");
    let held = store::organs::quarantined_for(&e.store.pool, sender, 10)
        .await
        .expect("quarantine");
    assert!(
        held.iter()
            .any(|(reason, _, _)| reason.contains("claims this Cell")),
        "refused by name: {held:?}"
    );
}

/// The control, and the reason the two above are not simply "refuse anything
/// from a sender with no roster": an ordinary contact who has published
/// nothing still syncs. Rosterless is a LEGITIMATE state — a Cell that has
/// never had an endpoint, or whose root key is deliberately offline — and the
/// migration that enforces write capability says so in its own comment.
#[tokio::test]
async fn a_rosterless_sender_naming_a_cell_nobody_claims_still_syncs() {
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
                uid: "r-ordinary".into(),
                field: "head".into(),
                kind: "set".into(),
                value: Some("\"an ordinary update\"".into()),
                hlc: nucleus::hlc::next(),
                actor_cell: "r-their-own-cell".into(),
                organ_uid: sender.into(),
                fact: None,
            }],
        })
        .await
        .expect("import");
    assert_eq!(applied, 1, "a contact with no published roster still syncs");
}

/// The other half of "the anchor moves rather than disappearing" (C4).
///
/// Over a connection, `from_organ` is tied to a real peer by the authenticated
/// stream. Out of a mailbox there is no stream, so the tie has to be made
/// again from the bundle signature — which names a CELL — against the signed
/// roster that says who owns that Cell. Without this, a mailed batch would be
/// checked by comparing two fields the same sender wrote.
#[tokio::test]
async fn a_mailed_batch_must_be_signed_by_a_cell_the_claimed_organ_owns() {
    let (e, _organ, _cell) = cell("http://receiver.test").await;
    hold_roster(&e, "organ-friend", &["cell-friend"]).await;
    hold_roster(&e, "organ-stranger", &["cell-stranger"]).await;
    store::organs::add_contact(&e.store.pool, "organ-friend", None, "Friend", "", 1)
        .await
        .expect("contact");

    let opened = |from_cell: &str, from_organ: &str| engine::seal::OpenedBundle {
        from_cell: from_cell.into(),
        from_organ: from_organ.into(),
        root: None,
        batch: engine::sync::OpBatch {
            from_organ: from_organ.into(),
            ops: vec![WireOp {
                tbl: "record".into(),
                uid: "r-mailed".into(),
                field: "head".into(),
                kind: "set".into(),
                value: Some("\"mailed\"".into()),
                hlc: nucleus::hlc::next(),
                actor_cell: "cell-friend".into(),
                organ_uid: from_organ.to_string(),
                fact: None,
            }],
        },
    };

    // The honest case: signed by a Cell the claimed Organ actually owns.
    let honest = e
        .import_mailed_batch(&opened("cell-friend", "organ-friend"))
        .await;
    assert!(
        honest.is_ok(),
        "mail from a contact whose roster we hold must apply: {honest:?}"
    );

    // Signed by someone else's Cell while claiming to be your friend. This is
    // the whole reason the bundle is signed at all.
    let wrong = e
        .import_mailed_batch(&opened("cell-stranger", "organ-friend"))
        .await;
    assert!(
        wrong.is_err(),
        "a batch claiming an Organ that does not own the signing Cell must refuse"
    );

    // Signed by a Cell no roster we hold names. REFUSED here, unlike on the
    // connection path where an unplaceable Cell is admitted — mail is only
    // ever sealed to a roster we already hold, so an unknown signer is not a
    // relationship starting, it is a stranger using a carrier.
    let unknown = e
        .import_mailed_batch(&opened("cell-nobody", "organ-friend"))
        .await;
    assert!(unknown.is_err(), "an unplaceable signer must refuse");
}
