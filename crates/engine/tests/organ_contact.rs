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
        fields: None,
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

/// The scope's three states have to survive the whole round trip — action in,
/// column out — as three, not as two. `null` is unnarrowed and `[]` is
/// narrowed to nothing, and anything that maps them together maps the strict
/// one onto the wide one.
#[tokio::test]
async fn a_scope_keeps_absent_and_empty_apart_end_to_end() {
    let (e, _local, contact) = cell_with_contact().await;
    let read = |e: &Engine, uid: String| {
        let store = e.store.clone();
        async move {
            let rows = protein::execute(&store, &organs_query()).await.unwrap();
            rows.iter().find(|r| r["uid"] == uid).unwrap()["contact"]["scope_fields"].clone()
        }
    };

    assert!(
        read(&e, contact.clone()).await.is_null(),
        "a contact starts unnarrowed, which is what the boolean alone meant"
    );

    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: Some(vec!["quantity".into(), "when".into()]),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        read(&e, contact.clone()).await,
        serde_json::json!(["quantity", "when"]),
        "a named scope comes back naming the same columns"
    );

    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: Some(Vec::new()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        read(&e, contact.clone()).await,
        serde_json::json!([]),
        "the empty scope must stay empty, not decay into unnarrowed"
    );

    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: None,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        read(&e, contact.clone()).await.is_null(),
        "and a narrowed contact can be returned to unnarrowed — not a one-way door"
    );
}

/// Every change to the scope has to move `scope_version`, because that is the
/// only thing a widening leaves behind. The columns alone cannot be compared
/// after the fact — the old value is gone by then.
#[tokio::test]
async fn every_scope_change_moves_the_version() {
    let (e, _local, contact) = cell_with_contact().await;
    let version = |e: &Engine, uid: String| {
        let pool = e.store.pool.clone();
        async move {
            store::organs::contact(&pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .scope_version
        }
    };

    let before = version(&e, contact.clone()).await;
    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: Some(vec!["quantity".into()]),
        },
        None,
    )
    .await
    .unwrap();
    let after = version(&e, contact.clone()).await;
    assert!(after > before, "narrowing moves the version");

    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: None,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        version(&e, contact.clone()).await > after,
        "and so does widening, which is the case the version exists for"
    );
}

/// A column name that is blank matches nothing, so it narrows to nothing
/// while looking configured. The usual way one appears is a trailing comma in
/// a text field, which is exactly the input a surface hands over.
#[tokio::test]
async fn a_scope_refuses_a_blank_column_name() {
    let (e, _local, contact) = cell_with_contact().await;
    assert!(
        e.act(
            Action::SetContactScope {
                target: contact.clone(),
                fields: Some(vec!["quantity".into(), "  ".into()]),
            },
            None,
        )
        .await
        .is_err()
    );
    assert!(
        store::organs::contact(&e.store.pool, &contact)
            .await
            .unwrap()
            .unwrap()
            .scope_fields
            .is_none(),
        "a refused scope must not be half-stored"
    );
}

/// The two directions are two settings, and setting one must not move the
/// other. They are a privacy control and an integrity control over the same
/// vocabulary, with no reason to agree — a contact we tell everything is
/// routinely one we accept little from.
#[tokio::test]
async fn the_two_directions_of_a_scope_are_independent() {
    let (e, _local, contact) = cell_with_contact().await;
    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetContactAcceptScope {
            target: contact.clone(),
            fields: Some(vec!["quantity".into()]),
        },
        None,
    )
    .await
    .unwrap();

    let row = store::organs::contact(&e.store.pool, &contact)
        .await
        .unwrap()
        .unwrap();
    assert!(
        row.scope_fields.is_none(),
        "we still tell them everything…"
    );
    assert_eq!(
        row.accept_fields.as_deref(),
        Some(&["quantity".to_string()][..]),
        "…while taking one column back"
    );
}

/// The inbound scope refuses the same expressions the outbound one does,
/// through the same validator. Two copies of these rules is how one direction
/// quietly starts accepting something the other refuses.
#[tokio::test]
async fn accepting_is_validated_like_sending() {
    let (e, _local, contact) = cell_with_contact().await;
    for bad in [
        vec!["head".to_string()],
        vec!["quantity".to_string(), " ".to_string()],
    ] {
        assert!(
            e.act(
                Action::SetContactAcceptScope {
                    target: contact.clone(),
                    fields: Some(bad.clone()),
                },
                None,
            )
            .await
            .is_err(),
            "{bad:?} is unsayable in both directions"
        );
    }
}

/// `head` and `body` are one Loro document whose ops carry no field, so a
/// scope naming one of them would silently deliver the other. Serve time
/// cannot enforce the difference; configuration time can at least report it,
/// which is the only place there is anybody to tell.
#[tokio::test]
async fn a_scope_refuses_to_split_the_collaborative_document() {
    let (e, _local, contact) = cell_with_contact().await;
    for one in ["head", "body"] {
        assert!(
            e.act(
                Action::SetContactScope {
                    target: contact.clone(),
                    fields: Some(vec![one.into(), "quantity".into()]),
                },
                None,
            )
            .await
            .is_err(),
            "naming {one} alone must be refused, not quietly widened"
        );
    }
    // Both together is the expressible request, and it is allowed.
    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: Some(vec!["head".into(), "body".into()]),
        },
        None,
    )
    .await
    .unwrap();
    // So is neither — that is the scope that withholds the document entirely.
    e.act(
        Action::SetContactScope {
            target: contact.clone(),
            fields: Some(vec!["quantity".into()]),
        },
        None,
    )
    .await
    .unwrap();
}

/// This Cell's own Organ has no feed and so no scope on it — the same guard
/// the direction switch uses, for the same reason.
#[tokio::test]
async fn a_scope_refuses_the_local_organ() {
    let (e, local, _contact) = cell_with_contact().await;
    assert!(
        e.act(
            Action::SetContactScope {
                // A scope that would be VALID on a contact, so the refusal
                // can only be about the local organ.
                target: local,
                fields: Some(vec!["quantity".into()]),
            },
            None,
        )
        .await
        .is_err()
    );
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

/// A contact that has actually been paired carries adopted keys, a NodeId and
/// a record the local user renamed — forgetting has to survive all of it.
#[tokio::test]
async fn forgetting_a_paired_contact_with_keys_succeeds() {
    let (e, _local, contact) = cell_with_contact().await;
    engine::trust::adopt_key(&e.store, &contact, "ed25519:root:v1", "AAAA").await.unwrap();
    engine::trust::adopt_key(&e.store, &contact, "ed25519:organ:v1", "BBBB").await.unwrap();
    store::organs::set_node_id(&e.store.pool, &contact, Some("beadbeef")).await.unwrap();
    store::organs::set_trust(&e.store.pool, &contact, "known").await.unwrap();
    store::records::set_text(&e.store.pool, &contact, Some("Known B"), None).await.unwrap();

    e.act(Action::ForgetOrganContact { target: contact.clone() }, None)
        .await
        .expect("a paired contact must be forgettable");
    assert!(store::records::get(&e.store.pool, &contact).await.unwrap().is_none());
}

/// Touching a contact's trust, proximity or feed direction commits a Fact
/// against its record, and `fact.record_uid` is a foreign key — so a contact
/// anyone has actually configured could not be forgotten at all: the delete
/// came back as `FOREIGN KEY constraint failed` and the button looked broken.
#[tokio::test]
async fn a_configured_contact_can_still_be_forgotten() {
    let (e, _local, contact) = cell_with_contact().await;
    for action in [
        Action::SetContactTrust {
            target: contact.clone(),
            trust: "known".into(),
        },
        Action::SetContactProximity {
            target: contact.clone(),
            proximity: 2,
        },
        Action::SetSyncPolicy {
            target: contact.clone(),
            sync_out: true,
            sync_in: true,
        },
    ] {
        e.act(action, None).await.unwrap();
    }

    e.act(
        Action::ForgetOrganContact {
            target: contact.clone(),
        },
        None,
    )
    .await
    .expect("a contact with annotations must still be forgettable");
    assert!(
        store::records::get(&e.store.pool, &contact)
            .await
            .unwrap()
            .is_none()
    );
}

/// A stored scope that will not parse is read as UNNARROWED — the choice is
/// legibility over strictness, because failing closed would stop a contact's
/// sync with no error at all. What it must NOT do is look like an ordinary
/// unnarrowed scope: it is a WIDER setting than anyone asked for, so the raw
/// text survives to the surface and the panel says so.
#[tokio::test]
async fn an_unreadable_scope_reads_as_unnarrowed_and_says_it_is_unreadable() {
    let (e, _local, contact) = cell_with_contact().await;

    store::sqlx::query("UPDATE organ_contact SET scope_fields = ? WHERE record_uid = ?")
        .bind("{not a list at all")
        .bind(&contact)
        .execute(&e.store.pool)
        .await
        .expect("write a value the parser cannot read");

    let stored = store::organs::contact(&e.store.pool, &contact)
        .await
        .unwrap()
        .expect("contact");
    assert_eq!(
        stored.scope_fields, None,
        "the engine goes on serving rather than silently stopping this feed"
    );
    assert_eq!(
        stored.scope_unreadable.as_deref(),
        Some("{not a list at all"),
        "and the text that could not be read is carried out, so a repair is not a guess"
    );
    assert_eq!(
        stored.accept_unreadable, None,
        "the other direction is a separate setting and is not reported as broken"
    );

    // A GENUINELY absent scope is not an unreadable one, and the two must not
    // be confused: one is the ordinary case and the other is a fault.
    store::organs::set_contact_scope(&e.store.pool, &contact, None)
        .await
        .unwrap();
    let repaired = store::organs::contact(&e.store.pool, &contact)
        .await
        .unwrap()
        .expect("contact");
    assert_eq!(repaired.scope_fields, None);
    assert_eq!(
        repaired.scope_unreadable, None,
        "saving over it is the repair path"
    );
}
