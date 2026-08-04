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
