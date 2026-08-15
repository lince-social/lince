use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{Include, LinkDirection, Predicate, Protein, Source};

fn query(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

#[tokio::test]
async fn hierarchy_widens_unary_and_binary_assertions() {
    let engine = Engine::open_memory().await.expect("engine");
    let project = engine
        .act(
            Action::CreateRecord {
                slug: Some("project-k".into()),
                kind: RecordKind::Plain,
                head: "Project K".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let work = engine
        .act(
            Action::CreateRecord {
                slug: Some("api".into()),
                kind: RecordKind::Plain,
                head: "Build API".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    engine
        .act(
            Action::CreateConcept {
                lingua: store::linguas::LOCAL_UID.into(),
                name: "task".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::CreateConcept {
                lingua: store::linguas::LOCAL_UID.into(),
                name: "backend".into(),
                parents: vec!["task".into()],
            },
            None,
        )
        .await
        .unwrap();
    let assertion = engine
        .act(
            Action::AssertRecord {
                subject: work.clone(),
                predicate: "backend".into(),
                object: Some(project.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let records = protein::execute(
        &engine.store,
        &query(Source::Record, vec![Predicate::ConceptIn("task".into())]),
    )
    .await
    .unwrap();
    assert!(records.iter().any(|row| row["uid"] == work));

    let assertions = protein::execute(
        &engine.store,
        &query(Source::Assertion, vec![Predicate::ConceptIn("task".into())]),
    )
    .await
    .unwrap();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0]["uid"], assertion);
    assert_eq!(assertions[0]["object"], project);

    let related = protein::execute(
        &engine.store,
        &query(
            Source::Record,
            vec![Predicate::Relation {
                kind: "task".into(),
                direction: LinkDirection::Out,
                other: Some(project.clone()),
            }],
        ),
    )
    .await
    .unwrap();
    assert!(related.iter().any(|row| row["uid"] == work));

    engine
        .act(Action::RetractAssertion { assertion }, None)
        .await
        .unwrap();
    assert!(
        protein::execute(&engine.store, &query(Source::Assertion, vec![]))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn identity_is_a_constrained_unary_assertion() {
    let engine = Engine::open_memory().await.expect("engine");
    let record = engine
        .act(
            Action::CreateRecord {
                slug: Some("brush".into()),
                kind: RecordKind::Plain,
                head: "Toothbrush".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::CreateConcept {
                lingua: store::linguas::LOCAL_UID.into(),
                name: "toothbrush".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SetIdentity {
                subject: record.clone(),
                predicate: Some("toothbrush".into()),
            },
            None,
        )
        .await
        .unwrap();

    let stored = store::records::get(&engine.store.pool, &record)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.identity_predicate_uid,
        store::concepts::resolve(&engine.store.pool, "toothbrush")
            .await
            .unwrap()
    );
    let assertions = store::assertions::list_active(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0].role, "identity");
    assert!(assertions[0].object_uid.is_none());
}

#[tokio::test]
async fn lingua_membership_is_shared_while_hierarchy_stays_a_dag() {
    let engine = Engine::open_memory().await.expect("engine");
    let peer = engine
        .act(
            Action::CreateLingua {
                name: "Peer terms".into(),
                visibility: "shared".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::CreateConcept {
                lingua: store::linguas::LOCAL_UID.into(),
                name: "task".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::CreateConcept {
                lingua: store::linguas::LOCAL_UID.into(),
                name: "backend".into(),
                parents: vec!["task".into()],
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::AdoptConcept {
                lingua: peer.clone(),
                concept: "task".into(),
            },
            None,
        )
        .await
        .unwrap();

    let task = store::concepts::resolve(&engine.store.pool, "task")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store::linguas::lingua_uids_for_concept(&engine.store.pool, &task)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        engine
            .act(
                Action::AddConceptParent {
                    concept: "task".into(),
                    parent: "backend".into(),
                },
                None,
            )
            .await
            .is_err()
    );

    engine
        .act(
            Action::RemoveConceptFromLingua {
                lingua: peer,
                concept: "task".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        store::linguas::lingua_uids_for_concept(&engine.store.pool, &task)
            .await
            .unwrap(),
        vec![store::linguas::LOCAL_UID.to_string()]
    );
}
