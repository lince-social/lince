use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    vocabulary::{Command, TOPOS_UID},
};

async fn command(
    engine: &Engine,
    request: Command,
    actor: Option<&str>,
) -> engine::actions::ActionOutcome {
    engine
        .act(Action::Vocabulary { request }, actor.map(str::to_owned))
        .await
        .unwrap()
}

#[tokio::test]
async fn contexts_preserve_wording_across_source_updates_and_restore_original() {
    let engine = Engine::open_memory().await.unwrap();
    let term = command(
        &engine,
        Command::AddTerm {
            concept: TOPOS_UID.into(),
            name: "Where".into(),
            language: "en".into(),
            preferred: false,
            personal: false,
        },
        None,
    )
    .await
    .created
    .unwrap();
    command(
        &engine,
        Command::SetTerminologyContext {
            context: "Travel".into(),
        },
        None,
    )
    .await;
    command(
        &engine,
        Command::PreferTerm {
            concept: TOPOS_UID.into(),
            term: term.clone(),
        },
        None,
    )
    .await;
    command(
        &engine,
        Command::EditTerm {
            term: term.clone(),
            expected_revision: 1,
            name: "Destination".into(),
            language: "en".into(),
            preferred: false,
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "Where"
    );
    assert_eq!(
        store::vocabulary::resolve(&engine.store.pool, "Where", None)
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
    command(
        &engine,
        Command::SetTerminologyContext {
            context: "Work".into(),
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "location"
    );
    command(
        &engine,
        Command::SetTerminologyContext {
            context: "Travel".into(),
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "Where"
    );
    command(
        &engine,
        Command::RestoreOriginal {
            concept: TOPOS_UID.into(),
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "tópos"
    );
    command(
        &engine,
        Command::ResetTerminology {
            concept: TOPOS_UID.into(),
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "location"
    );
}

#[tokio::test]
async fn hiding_is_contextual_personal_and_keeps_explicit_identity_resolvable() {
    let engine = Engine::open_memory().await.unwrap();
    let location = store::vocabulary::terms(&engine.store.pool, None)
        .await
        .unwrap()
        .into_iter()
        .find(|term| term.name == "location")
        .unwrap();
    let before = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    command(
        &engine,
        Command::HideTerm {
            term: location.uid.clone(),
            hidden: true,
        },
        None,
    )
    .await;
    command(
        &engine,
        Command::UseWording {
            concept: TOPOS_UID.into(),
            wording: "مكان".into(),
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[TOPOS_UID],
        "مكان"
    );
    assert!(
        store::vocabulary::resolve(&engine.store.pool, "location", None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store::vocabulary::resolve(&engine.store.pool, TOPOS_UID, None)
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        before
    );
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: "Reader",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    assert_eq!(
        store::vocabulary::resolve(&engine.store.pool, "location", Some(&person))
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
    command(
        &engine,
        Command::HideTerm {
            term: location.uid,
            hidden: false,
        },
        None,
    )
    .await;
    assert_eq!(
        store::vocabulary::resolve(&engine.store.pool, "location", None)
            .await
            .unwrap()
            .as_deref(),
        Some(TOPOS_UID)
    );
}

#[tokio::test]
async fn personal_choice_cannot_reveal_another_persons_private_wording() {
    let engine = Engine::open_memory().await.unwrap();
    let mut people = Vec::new();
    for head in ["One", "Two"] {
        people.push(
            store::records::create(
                &engine.store.pool,
                store::records::NewRecord {
                    slug: None,
                    kind: RecordKind::Person,
                    head,
                    body: "",
                    quantity: store::exact::zero(),
                },
            )
            .await
            .unwrap()
            .uid,
        );
    }
    let role = store::auth::ensure_role(&engine.store.pool, "Terminology users")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    for person in &people {
        engine
            .act(
                Action::AssignRoles {
                    person: person.clone(),
                    roles: vec!["Terminology users".into()],
                    expected_revision: 0,
                },
                None,
            )
            .await
            .unwrap();
    }
    let term = command(
        &engine,
        Command::AddTerm {
            concept: TOPOS_UID.into(),
            name: "Secret place".into(),
            language: "".into(),
            preferred: true,
            personal: true,
        },
        Some(&people[0]),
    )
    .await
    .created
    .unwrap();
    for actor in [None, Some(people[1].clone())] {
        assert!(
            engine
                .act(
                    Action::Vocabulary {
                        request: Command::PreferTerm {
                            concept: TOPOS_UID.into(),
                            term: term.clone()
                        }
                    },
                    actor
                )
                .await
                .is_err()
        );
    }
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    let dictionary = store::vocabulary::dictionary_on(&mut tx, TOPOS_UID)
        .await
        .unwrap();
    assert!(
        dictionary
            .terms
            .iter()
            .all(|term| term.name != "Secret place" && term.owner_uid.is_none())
    );
}
