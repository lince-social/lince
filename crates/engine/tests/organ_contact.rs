//! Organ sand contacts manager (blueprint XV, scoped to trust/proximity):
//! `SetContactTrust`/`SetContactProximity` Actions, the Protein `contact`
//! include, and the local organ (no `organ_contact` row) staying `null`.

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{Include, Predicate, Protein, Source};

fn organs_query() -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![Predicate::KindEq("organ".into())],
        include: Include {
            contact: true,
            ..Include::default()
        },
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

async fn cell_with_contact() -> (Engine, String, String) {
    let e = Engine::open_memory().await.expect("engine");
    let local = store::organs::ensure_local(&e.store.pool, "http://cell-a")
        .await
        .unwrap()
        .uid;
    let contact = store::organs::add_contact(
        &e.store.pool,
        "organ_contact_uid",
        Some("friend"),
        "Friend Cell",
        "http://cell-b",
        3,
    )
    .await
    .unwrap();
    (e, local, contact)
}

#[tokio::test]
async fn local_organ_has_no_contact_sidecar() {
    let (e, local, _contact) = cell_with_contact().await;
    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == local).unwrap();
    assert!(row["contact"].is_null());
}

#[tokio::test]
async fn organ_records_support_the_sands_register_edit_and_delete_actions() {
    let e = Engine::open_memory().await.expect("engine");
    let organ = e
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Organ,
                head: "Remote Cell".into(),
                body: "https://remote.example".into(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .expect("register organ")
        .created
        .expect("created organ uid");

    e.act(
        Action::EditRecordText {
            target: organ.clone(),
            head: Some("Renamed Cell".into()),
            body: Some("https://renamed.example".into()),
        },
        None,
    )
    .await
    .expect("edit organ");

    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|row| row["uid"] == organ).unwrap();
    assert_eq!(row["head"], "Renamed Cell");
    assert_eq!(row["body"], "https://renamed.example");

    e.act(
        Action::DeleteRecord {
            target: organ.clone(),
        },
        None,
    )
    .await
    .expect("delete organ");
    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    assert!(rows.iter().all(|row| row["uid"] != organ));
}

#[tokio::test]
async fn contact_starts_unknown_with_its_seeded_proximity() {
    let (e, _local, contact) = cell_with_contact().await;
    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == contact).unwrap();
    // Recording a contact is not trusting one. `known` opens the sync ALPN, so
    // it has to be a decision somebody made rather than what happens by
    // default when an address is written down.
    assert_eq!(row["contact"]["trust"], "unknown");
    assert_eq!(row["contact"]["proximity"], 3);
}

#[tokio::test]
async fn set_contact_trust_updates_and_is_visible_through_protein() {
    let (e, _local, contact) = cell_with_contact().await;
    e.act(
        Action::SetContactTrust {
            target: contact.clone(),
            trust: "blocked".into(),
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == contact).unwrap();
    assert_eq!(row["contact"]["trust"], "blocked");
}

#[tokio::test]
async fn unblock_is_just_set_trust_back_to_known() {
    let (e, _local, contact) = cell_with_contact().await;
    e.act(
        Action::SetContactTrust {
            target: contact.clone(),
            trust: "blocked".into(),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetContactTrust {
            target: contact.clone(),
            trust: "known".into(),
        },
        None,
    )
    .await
    .unwrap();

    let row = store::organs::contact(&e.store.pool, &contact)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.trust, "known");
}

#[tokio::test]
async fn invalid_trust_value_is_rejected() {
    let (e, _local, contact) = cell_with_contact().await;
    let err = e
        .act(
            Action::SetContactTrust {
                target: contact,
                trust: "friendly".into(),
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("invalid trust"));
}

#[tokio::test]
async fn set_contact_proximity_updates_and_is_visible_through_protein() {
    let (e, _local, contact) = cell_with_contact().await;
    e.act(
        Action::SetContactProximity {
            target: contact.clone(),
            proximity: 9,
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == contact).unwrap();
    assert_eq!(row["contact"]["proximity"], 9);
}

#[tokio::test]
async fn set_contact_trust_fires_an_annotation_fact() {
    let (e, _local, contact) = cell_with_contact().await;
    let mut bus = e.subscribe();
    e.act(
        Action::SetContactTrust {
            target: contact.clone(),
            trust: "blocked".into(),
        },
        None,
    )
    .await
    .unwrap();

    let fact = tokio::time::timeout(std::time::Duration::from_secs(1), bus.recv())
        .await
        .expect("a fact arrived on the bus")
        .expect("bus not closed");
    assert_eq!(fact.record_uid, contact);
}

/// A contact's Organ record is filed under THEIR uid, so the ordinary record
/// edit would log a CRDT op and push this Cell's private label for them back
/// to them and to every other contact. Renaming is local, and logs nothing.
#[tokio::test]
async fn renaming_a_contact_is_local_and_logs_no_op() {
    let (e, _local, contact) = cell_with_contact().await;
    let before = store::sync_ops::max_seq(&e.store.pool).await.unwrap();

    e.act(
        Action::RenameOrganContact {
            target: contact.clone(),
            name: "  Marcia  ".into(),
        },
        None,
    )
    .await
    .unwrap();

    let row = store::records::get(&e.store.pool, &contact)
        .await
        .unwrap()
        .expect("the contact record");
    assert_eq!(row.head, "Marcia", "trimmed, and it is the local label");
    let after = store::sync_ops::max_seq(&e.store.pool).await.unwrap();
    assert_eq!(before, after, "renaming a contact must not enter the op log");
}

/// The same action must not become a back door for editing this Cell's own
/// Organ record without logging — that one IS ours and replicates normally.
#[tokio::test]
async fn renaming_refuses_a_record_that_is_not_a_contact() {
    let (e, local, _contact) = cell_with_contact().await;
    let err = e
        .act(
            Action::RenameOrganContact {
                target: local,
                name: "My Cell".into(),
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("not a contact"), "{err}");
}

/// Every Cell calls itself the same thing out of the box, so the surface has
/// nothing to tell a contact from this Cell's own row by unless the NodeId
/// travels with the contact sidecar.
#[tokio::test]
async fn the_contact_include_carries_the_node_id() {
    let (e, _local, contact) = cell_with_contact().await;
    store::organs::set_node_id(&e.store.pool, &contact, Some("beadbeef00"))
        .await
        .unwrap();
    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == contact).unwrap();
    assert_eq!(row["contact"]["node_id"], "beadbeef00");
}

/// Blocking is not removal, so a contact still has to be droppable — but
/// `delete-record` on their uid would log a tombstone against THEIR Organ
/// record and push it to them and every other contact.
#[tokio::test]
async fn forgetting_a_contact_is_local_and_logs_no_op() {
    let (e, _local, contact) = cell_with_contact().await;
    let before = store::sync_ops::max_seq(&e.store.pool).await.unwrap();

    e.act(
        Action::ForgetOrganContact {
            target: contact.clone(),
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        store::organs::contact(&e.store.pool, &contact)
            .await
            .unwrap()
            .is_none(),
        "the sidecar is gone"
    );
    assert!(
        store::records::get(&e.store.pool, &contact)
            .await
            .unwrap()
            .is_none(),
        "and so is the record standing in for them"
    );
    let after = store::sync_ops::max_seq(&e.store.pool).await.unwrap();
    assert_eq!(before, after, "forgetting must not enter the op log");
}

/// Direction is a switch on an enforced boundary: the outbox drops ops for a
/// contact with `sync_out` off, and delivery refuses a feed from one with
/// `sync_in` off. Both start closed, so the surface has to be able to open
/// them one at a time.
#[tokio::test]
async fn sync_policy_sets_each_direction_independently() {
    let (e, _local, contact) = cell_with_contact().await;
    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == contact).unwrap();
    assert_eq!(row["contact"]["sync_out"], false);
    assert_eq!(row["contact"]["sync_in"], false);

    e.act(
        Action::SetSyncPolicy {
            target: contact.clone(),
            sync_out: false,
            sync_in: true,
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute(&e.store, &organs_query()).await.unwrap();
    let row = rows.iter().find(|r| r["uid"] == contact).unwrap();
    assert_eq!(row["contact"]["sync_out"], false, "outbound stays shut");
    assert_eq!(row["contact"]["sync_in"], true, "inbound alone is openable");
}

/// This Cell's own Organ is not a peer, so there is no feed to point in a
/// direction — and the sand never offers it.
#[tokio::test]
async fn sync_policy_refuses_the_local_organ() {
    let (e, local, _contact) = cell_with_contact().await;
    let err = e
        .act(
            Action::SetSyncPolicy {
                target: local,
                sync_out: true,
                sync_in: true,
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("not a contact"), "{err}");
}
