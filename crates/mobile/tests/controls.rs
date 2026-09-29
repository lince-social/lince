use engine::{Engine, actions::Action};
use lince_mobile::{app::Mobile, attachments, views};
use nucleus::{MessageState, RecordKind, message::MessagePart};
use store::records::NewRecord;

#[tokio::test]
async fn saved_views_page_beyond_five_thousand_with_bounded_results() {
    let engine = Engine::open_memory().await.unwrap();
    for index in 0..5053 {
        store::records::create(
            &engine.store.pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: &format!("Task {index:04}"),
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let mut state = Mobile::new(directory.path().into());
    state.search = "Task".into();
    let mut query = views::query(&state, false).unwrap();
    query
        .filter
        .push(protein::Predicate::KindEq("plain".into()));
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.len(), 51);
    assert_eq!(rows[0]["head"], "Task 0000");
    let saved = engine
        .act(
            Action::SaveProtein {
                head: "My tasks".into(),
                slug: "mobile-tasks".into(),
                ast: lince_interface::queries::ProteinDraft::from_protein(
                    "My tasks".into(),
                    "mobile-tasks".into(),
                    query,
                )
                .query,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let ast = store::records::get_extension(&engine.store.pool, &saved, "lince.protein")
        .await
        .unwrap()
        .unwrap();
    state.view = Some(lince_interface::queries::ProteinDraft::from_protein(
        "My tasks".into(),
        "mobile-tasks".into(),
        serde_json::from_value(ast).unwrap(),
    ));
    state.search.clear();
    let mut seen = std::collections::HashSet::new();
    loop {
        let rows = protein::execute(&engine.store, &views::query(&state, false).unwrap())
            .await
            .unwrap();
        assert!(rows.len() <= views::PAGE_SIZE + 1);
        for row in rows.iter().take(views::PAGE_SIZE) {
            assert_eq!(row["head"], format!("Task {:04}", seen.len()));
            assert!(seen.insert(row["uid"].as_str().unwrap().to_owned()));
        }
        if rows.len() <= views::PAGE_SIZE {
            break;
        }
        state
            .record_pages
            .push(rows[views::PAGE_SIZE - 1]["uid"].as_str().unwrap().into());
    }
    assert_eq!(seen.len(), 5053);
}

#[tokio::test]
async fn attachment_download_checks_person_visibility_and_validates_bytes() {
    let engine = Engine::open_memory().await.unwrap();
    let record = engine
        .act(
            Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    head: "Files".into(),
                    ..Default::default()
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let thread = engine
        .act(
            Action::CreateThread {
                target: record,
                head: "Files".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let part =
        attachments::prepare("ação.txt".into(), "text/plain".into(), "Olá ☕".as_bytes()).unwrap();
    let message = engine
        .act(
            Action::CreateMessage {
                thread,
                body: String::new(),
                content: vec![part.clone()],
                author: None,
                state: MessageState::Finished,
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let action = Action::ReadMessageAttachment {
        message: message.clone(),
        index: 0,
    };
    let restored: MessagePart = serde_json::from_value(
        engine
            .act(action.clone(), None)
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap();
    assert_eq!(restored, part);
    let role = store::auth::ensure_role(&engine.store.pool, "admin")
        .await
        .unwrap();
    let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, read)
        .await
        .unwrap();
    let person = engine
        .act(
            Action::CreateUser {
                username: "limited".into(),
                name: "Limited".into(),
                password: "disposable-test-password".into(),
                role: "admin".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    store::visibility::grant(&engine.store.pool, "public", None, &message)
        .await
        .unwrap();
    engine
        .act(action.clone(), Some(person.clone()))
        .await
        .unwrap();
    engine
        .act(
            Action::SetPersonReadFilter {
                person: person.clone(),
                filter: Some(protein::Predicate::KindEq("plain".into())),
            },
            None,
        )
        .await
        .unwrap();
    assert!(matches!(
        engine.act(action, Some(person)).await,
        Err(engine::EngineError::Forbidden(_))
    ));
    assert!(
        engine
            .act(Action::ReadMessageAttachment { message, index: 99 }, None)
            .await
            .is_err()
    );
    assert!(
        attachments::prepare(
            "big.bin".into(),
            "application/octet-stream".into(),
            &vec![0; nucleus::message::MAX_CONTENT_BYTES + 1]
        )
        .is_err()
    );
}

#[tokio::test]
async fn concept_picker_search_is_case_insensitive() {
    let engine = Engine::open_memory().await.unwrap();
    store::concepts::create(&engine.store.pool, "mobile-search-target", &[])
        .await
        .unwrap();
    let query: protein::Protein = serde_json::from_value(serde_json::json!({"source":"concept","where":[{"text_contains":"MOBILE-SEARCH-TARGET"}],"limit":31})).unwrap();
    protein::validate(&query).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.len(), 1);
}
