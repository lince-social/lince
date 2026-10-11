use engine::{
    Engine,
    actions::Action,
    record_change::{Mutation, Request},
};
use nucleus::vocabulary::{Command, SavedLocation, SavedLocationCommand, TOPOS_UID};
use serde_json::json;

async fn record(engine: &Engine, slug: &str) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: nucleus::RecordKind::Plain,
                head: slug.into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn coordinates(engine: &Engine, record: &str, lat: f64, lon: f64) {
    engine
        .act(
            Action::SetPlace {
                target: record.into(),
                lat,
                lon,
                address: None,
            },
            None,
        )
        .await
        .unwrap();
}

async fn link(engine: &Engine, record: &str, predicate: &str, object: &str) -> String {
    engine
        .change_record(
            Request {
                id: nucleus::new_uid("op"),
                record_uid: record.into(),
                mutation: Mutation::Assertion {
                    predicate: predicate.into(),
                    object: Some(object.into()),
                    quantity: None,
                    unit: None,
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn location(engine: &Engine, record: &str) -> SavedLocation {
    let data = engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Inspect {
                    record: record.into(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    serde_json::from_value(data).unwrap()
}

#[tokio::test]
async fn address_and_topos_use_one_meaning_and_language_switch_preserves_assertions() {
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &street, -23.5, -46.6).await;
    let before = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    assert!(
        engine
            .change_record(
                Request {
                    id: nucleus::new_uid("op"),
                    record_uid: party.clone(),
                    mutation: Mutation::Assertion {
                        predicate: "addres".into(),
                        object: Some(street.clone()),
                        quantity: None,
                        unit: None,
                    },
                },
                None,
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        before
    );
    assert!(location(&engine, &party).await.candidates.is_empty());
    let assertion = link(&engine, &party, "address", &street).await;
    assert_eq!(
        store::assertions::get(&engine.store.pool, &assertion)
            .await
            .unwrap()
            .unwrap()
            .predicate_uid,
        TOPOS_UID
    );
    let selected = location(&engine, &party).await.selected.unwrap();
    assert_eq!(selected.record_uid, street);
    assert_eq!(selected.latitude, -23.5);
    let before = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    let changes = engine.watch_query_changes();
    engine
        .act(
            Action::Vocabulary {
                request: Command::SetLanguage {
                    language: "pt-BR".into(),
                },
            },
            None,
        )
        .await
        .unwrap();
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":party}],"fields":["uid","assertions"]}),
    )
    .unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows[0]["assertions"][0]["predicate"], "localização");
    assert_eq!(rows[0]["assertions"][0]["predicate_uid"], TOPOS_UID);
    assert!(changes.has_changed().unwrap());
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        before
    );
    assert_eq!(link(&engine, &party, "tópos", &street).await, assertion);
}

#[tokio::test]
async fn location_selection_survives_rename_and_never_falls_back_after_retraction() {
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &party, 1.0, 2.0).await;
    coordinates(&engine, &street, 3.0, 4.0).await;
    let assertion = link(&engine, &party, "location", &street).await;
    let initial = location(&engine, &party).await;
    assert_eq!(initial.candidates.len(), 2);
    assert!(initial.selected.is_none());
    engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Select {
                    record: party.clone(),
                    assertion: Some(assertion.clone()),
                    expected_revision: 0,
                },
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::Vocabulary {
                request: Command::AddTerm {
                    concept: TOPOS_UID.into(),
                    name: "whereabouts".into(),
                    language: "en".into(),
                    preferred: true,
                    personal: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        location(&engine, &party).await.selected.unwrap().record_uid,
        street
    );
    coordinates(&engine, &street, 5.0, 6.0).await;
    assert_eq!(
        location(&engine, &party).await.selected.unwrap().latitude,
        5.0
    );
    assert_eq!(
        store::places::of_record(&engine.store.pool, &party)
            .await
            .unwrap()
            .unwrap()
            .lat,
        1.0
    );
    engine
        .act(Action::RetractAssertion { assertion }, None)
        .await
        .unwrap();
    let unavailable = location(&engine, &party).await;
    assert!(unavailable.unavailable);
    assert!(unavailable.selected.is_none());
    assert_eq!(unavailable.candidates.len(), 1);
    assert!(
        engine
            .act(
                Action::SavedLocation {
                    request: SavedLocationCommand::Select {
                        record: party.clone(),
                        assertion: None,
                        expected_revision: 0
                    }
                },
                None
            )
            .await
            .is_err()
    );
    engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Select {
                    record: party.clone(),
                    assertion: None,
                    expected_revision: unavailable.revision,
                },
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        location(&engine, &party).await.selected.unwrap().latitude,
        1.0
    );
}

#[tokio::test]
async fn derived_meaning_requires_explicit_binding_and_shapes_are_checked() {
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &street, 3.0, 4.0).await;
    let concept = engine
        .act(
            Action::Vocabulary {
                request: Command::CreateConcept {
                    vocabulary: store::linguas::LOCAL_UID.into(),
                    name: "meeting-place".into(),
                    language: "en".into(),
                    definition: "A saved geographic place used for meeting".into(),
                    derived_from: Some(TOPOS_UID.into()),
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    link(&engine, &party, &concept, &street).await;
    assert!(location(&engine, &party).await.selected.is_none());
    engine
        .act(
            Action::Vocabulary {
                request: Command::BindLocationConcept {
                    concept: concept.clone(),
                    enabled: true,
                    write: true,
                    expected_revision: 1,
                },
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        location(&engine, &party).await.selected.unwrap().record_uid,
        street
    );
    let second = record(&engine, "second-party").await;
    engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Bind {
                    record: second.clone(),
                    address: "@street4".into(),
                },
            },
            None,
        )
        .await
        .unwrap();
    let assertions = store::assertions::for_subjects(&engine.store.pool, &[second])
        .await
        .unwrap();
    assert_eq!(assertions[0].predicate_uid, concept);
    assert!(
        engine
            .change_record(
                Request {
                    id: nucleus::new_uid("op"),
                    record_uid: party.clone(),
                    mutation: Mutation::Assertion {
                        predicate: TOPOS_UID.into(),
                        object: None,
                        quantity: None,
                        unit: None
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .change_record(
                Request {
                    id: nucleus::new_uid("op"),
                    record_uid: party.clone(),
                    mutation: Mutation::Assertion {
                        predicate: TOPOS_UID.into(),
                        object: Some(party),
                        quantity: None,
                        unit: None
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn near_queries_use_linked_coordinates_and_require_location_selection() {
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    let anchor = record(&engine, "anchor").await;
    coordinates(&engine, &street, 1.0, 2.0).await;
    coordinates(&engine, &anchor, 1.0, 2.0).await;
    link(&engine, &party, "address", &street).await;
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"near":{"of":anchor,"meters":100}}],"fields":["uid"]}),
    )
    .unwrap();
    assert!(
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .iter()
            .any(|row| row["uid"] == party)
    );
    let distance = engine
        .act(
            Action::PreviewKarmaReading {
                source: "distance(@party, @anchor)".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(distance.data.unwrap()["value"], "0");
    coordinates(&engine, &party, 4.0, 5.0).await;
    assert!(
        !protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .iter()
            .any(|row| row["uid"] == party)
    );
}

#[tokio::test]
async fn hidden_address_coordinates_are_filtered_and_selected_sources_remain_unavailable() {
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &street, 1.0, 2.0).await;
    let assertion = link(&engine, &party, "address", &street).await;
    engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Select {
                    record: party.clone(),
                    assertion: Some(assertion),
                    expected_revision: 0,
                },
            },
            None,
        )
        .await
        .unwrap();
    let visible = [party.clone()].into_iter().collect();
    let locations = store::places::saved_locations(&engine.store.pool, Some(&visible))
        .await
        .unwrap();
    assert!(locations[&party].candidates.is_empty());
    assert!(locations[&party].selected.is_none());
    assert!(locations[&party].unavailable);
}

async fn reader(engine: &Engine, slug: &str) -> String {
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind: nucleus::RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&engine.store.pool, "Vocabulary readers")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["Vocabulary readers".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    person
}

#[tokio::test]
async fn personal_language_and_terms_do_not_change_another_person_or_grant_shared_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let portuguese = reader(&engine, "portuguese-reader").await;
    let english = reader(&engine, "english-reader").await;
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &street, 1.0, 2.0).await;
    let assertion = link(&engine, &party, "address", &street).await;
    let before = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    engine
        .act(
            Action::Vocabulary {
                request: Command::SetLanguage {
                    language: "pt-BR".into(),
                },
            },
            Some(portuguese.clone()),
        )
        .await
        .unwrap();
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":party}],"fields":["uid","assertions"]}),
    )
    .unwrap();
    let pt = protein::execute_for(&engine.store, &query, Some(&portuguese))
        .await
        .unwrap();
    let en = protein::execute_for(&engine.store, &query, Some(&english))
        .await
        .unwrap();
    assert_eq!(pt[0]["assertions"][0]["predicate"], "localização");
    assert_eq!(en[0]["assertions"][0]["predicate"], "location");
    assert_eq!(pt[0]["assertions"][0]["uid"], assertion);
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        before
    );
    let term = engine
        .act(
            Action::Vocabulary {
                request: Command::AddTerm {
                    concept: TOPOS_UID.into(),
                    name: "donkey".into(),
                    language: "".into(),
                    preferred: true,
                    personal: true,
                },
            },
            Some(portuguese.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let private_query = serde_json::from_value(json!({"source":"record","where":[{"relation":{"kind":"donkey","direction":"out","other":street}}],"fields":["uid"]})).unwrap();
    assert_eq!(
        protein::execute_for(&engine.store, &private_query, Some(&portuguese))
            .await
            .unwrap()[0]["uid"],
        party
    );
    assert!(
        protein::execute_for(&engine.store, &private_query, Some(&english))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store::vocabulary::resolve(&engine.store.pool, "donkey", Some(&english))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::EditTerm {
                        term,
                        expected_revision: 1,
                        name: "mine".into(),
                        language: "".into(),
                        preferred: true
                    }
                },
                Some(english.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::AddTerm {
                        concept: TOPOS_UID.into(),
                        name: "shared".into(),
                        language: "en".into(),
                        preferred: false,
                        personal: false
                    }
                },
                Some(portuguese.clone())
            )
            .await
            .is_err()
    );
    let role = store::auth::ensure_role(&engine.store.pool, "Vocabulary readers")
        .await
        .unwrap();
    let update = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, update)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::RenameConcept {
                    concept: TOPOS_UID.into(),
                    name: "changed-by-editor".into()
                },
                Some(portuguese.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::AddTerm {
                        concept: TOPOS_UID.into(),
                        name: "still-shared".into(),
                        language: "en".into(),
                        preferred: false,
                        personal: false
                    }
                },
                Some(portuguese.clone())
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn actor_read_policies_filter_linked_addresses_from_inspect_search_and_near() {
    let engine = Engine::open_memory().await.unwrap();
    let actor = reader(&engine, "limited-reader").await;
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &street, 1.0, 2.0).await;
    link(&engine, &party, "address", &street).await;
    engine
        .act(
            Action::SetRolePolicy {
                role: "Vocabulary readers".into(),
                expected_revision: 0,
                policy: json!({"read":{"uid_eq":party},"grants":[]}),
            },
            None,
        )
        .await
        .unwrap();
    let result = engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Inspect {
                    record: party.clone(),
                },
            },
            Some(actor.clone()),
        )
        .await
        .unwrap();
    let location: SavedLocation = serde_json::from_value(result.data.unwrap()).unwrap();
    assert!(location.candidates.is_empty());
    assert!(location.selected.is_none());
    let search = engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Search {
                    text: "street4".into(),
                },
            },
            Some(actor.clone()),
        )
        .await
        .unwrap();
    assert_eq!(search.data.unwrap()["addresses"], json!([]));
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"near":{"of":party,"meters":100}}],"fields":["uid"]}),
    )
    .unwrap();
    assert!(
        protein::execute_for(&engine.store, &query, Some(&actor))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn replicated_selection_can_arrive_before_its_assertion_without_choosing_another_location() {
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &party, 1.0, 2.0).await;
    coordinates(&engine, &street, 3.0, 4.0).await;
    let assertion = nucleus::new_uid("a");
    let register = engine::record_change::Register {
        clock: nucleus::hlc::next(),
        peer: nucleus::new_uid("r"),
        change_uid: nucleus::new_uid("op"),
        value: json!({"assertion":assertion}),
    };
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let op = engine::sync::WireOp {
        tbl: "record".into(),
        uid: party.clone(),
        field: "property:location.source".into(),
        kind: "set".into(),
        value: Some(serde_json::to_string(&register).unwrap()),
        hlc: nucleus::hlc::next(),
        actor_cell: register.peer.clone(),
        organ_uid: organ,
        fact: None,
    };
    engine
        .materialise(engine::sync::Materialise {
            op: &op,
            kind: store::sync_ops::OpKind::Set,
            replica_root: None,
            undelete: false,
        })
        .await
        .unwrap();
    assert!(location(&engine, &party).await.unavailable);
    assert!(location(&engine, &party).await.selected.is_none());
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::assertions::insert_tx(
        &mut tx,
        &assertion,
        store::assertions::NewAssertion {
            subject_uid: &party,
            predicate_uid: TOPOS_UID,
            object_uid: Some(&street),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        location(&engine, &party).await.selected.unwrap().record_uid,
        street
    );
    assert_eq!(location(&engine, &party).await.revision, register.clock);
}

#[tokio::test]
async fn only_our_organ_can_replicate_feature_bindings() {
    let engine = Engine::open_memory().await.unwrap();
    let concept = store::concepts::create(&engine.store.pool, "meeting-place", &[])
        .await
        .unwrap();
    let binding = nucleus::vocabulary::LocationBinding {
        write_concept_uid: concept.clone(),
        concept_uids: vec![TOPOS_UID.into(), concept.clone()],
        revision: 2,
    };
    let mut op = engine::sync::WireOp {
        tbl: "concept".into(),
        uid: TOPOS_UID.into(),
        field: "location_binding".into(),
        kind: "set".into(),
        value: Some(serde_json::to_string(&binding).unwrap()),
        hlc: nucleus::hlc::next(),
        actor_cell: nucleus::new_uid("r"),
        organ_uid: nucleus::new_uid("r"),
        fact: None,
    };
    engine
        .materialise(engine::sync::Materialise {
            op: &op,
            kind: store::sync_ops::OpKind::Set,
            replica_root: None,
            undelete: false,
        })
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::location_bindings(&engine.store.pool)
            .await
            .unwrap()
            .0,
        TOPOS_UID
    );
    op.organ_uid = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    engine
        .materialise(engine::sync::Materialise {
            op: &op,
            kind: store::sync_ops::OpKind::Set,
            replica_root: None,
            undelete: false,
        })
        .await
        .unwrap();
    assert_eq!(
        store::vocabulary::location_bindings(&engine.store.pool)
            .await
            .unwrap()
            .0,
        concept
    );
}

#[tokio::test]
async fn organ_place_reads_require_both_address_record_and_coordinate_visibility() {
    use nucleus::visibility::{Command as VisibilityCommand, Condition, Data, Policy};
    let engine = Engine::open_memory().await.unwrap();
    let party = record(&engine, "party").await;
    let street = record(&engine, "street4").await;
    coordinates(&engine, &street, 3.0, 4.0).await;
    link(&engine, &party, "address", &street).await;
    let peer = nucleus::new_uid("r");
    store::organs::add_contact(&engine.store.pool, &peer, None, "Visitor", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &peer, "known")
        .await
        .unwrap();
    let policy = Policy {
        include: vec![Condition {
            organs: vec![peer.clone()],
            ..Default::default()
        }],
        exclude: Vec::new(),
    };
    for (record, data, expected_revision) in [(&party, Data::Place, 1), (&street, Data::Record, 0)]
    {
        engine
            .act(
                Action::Visibility {
                    request: VisibilityCommand::Save {
                        record_uid: record.clone(),
                        data,
                        policy: policy.clone(),
                        expected_revision,
                    },
                },
                None,
            )
            .await
            .unwrap();
    }
    assert!(
        engine
            .visibility_read(&peer, &party, Data::Place)
            .await
            .unwrap()["place"]
            .is_null()
    );
    engine
        .act(
            Action::Visibility {
                request: VisibilityCommand::Save {
                    record_uid: street.clone(),
                    data: Data::Place,
                    policy,
                    expected_revision: 1,
                },
            },
            None,
        )
        .await
        .unwrap();
    let visible = engine
        .visibility_read(&peer, &party, Data::Place)
        .await
        .unwrap();
    assert_eq!(visible["place"]["latitude"], 3.0);
    assert_eq!(visible["place"]["address"], "street4");
    engine
        .act(
            Action::Visibility {
                request: VisibilityCommand::Save {
                    record_uid: street.clone(),
                    data: Data::Record,
                    policy: Policy::default(),
                    expected_revision: 1,
                },
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .visibility_read(&peer, &party, Data::Place)
            .await
            .unwrap()["place"]
            .is_null()
    );
}
