use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid
}

#[tokio::test]
async fn edit_record_text_sets_fields_and_annotates() {
    let e = engine().await;
    let uid = plain(&e, "note").await;

    let out = e
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some("New Title".into()),
                body: Some("Long body".into()),
            },
            None,
        )
        .await
        .expect("edit");

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.head, "New Title");
    assert_eq!(row.body, "Long body");
    assert_eq!(out.facts.len(), 1);
    assert_eq!(out.facts[0].delta, store::exact::from_f64(0.0));
    assert_eq!(out.facts[0].record_uid, uid);
}

#[tokio::test]
async fn edit_record_text_leaves_absent_field_untouched() {
    let e = engine().await;
    let uid = plain(&e, "note2").await;
    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: Some("H".into()),
            body: Some("B".into()),
        },
        None,
    )
    .await
    .unwrap();

    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: Some("H2".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.head, "H2");
    assert_eq!(row.body, "B");
}

#[tokio::test]
async fn set_slug_renames_and_clears() {
    let e = engine().await;
    let uid = plain(&e, "old-slug").await;

    e.act(
        Action::SetSlug {
            target: uid.clone(),
            slug: Some("new-slug".into()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::records::resolve(&e.store.pool, "new-slug")
            .await
            .unwrap()
            .map(|r| r.uid),
        Some(uid.clone())
    );

    e.act(
        Action::SetSlug {
            target: uid.clone(),
            slug: Some(String::new()),
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .slug
            .is_none()
    );

    assert!(
        e.act(
            Action::SetSlug {
                target: uid.clone(),
                slug: Some("Not Valid".into())
            },
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn set_identity_and_unit_classify_and_clear() {
    let e = engine().await;
    let uid = plain(&e, "apples").await;
    store::concepts::create(&e.store.pool, "fruit", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "kilogram", &[])
        .await
        .unwrap();

    e.act(
        Action::SetIdentity {
            subject: uid.clone(),
            predicate: Some("fruit".into()),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetUnit {
            target: uid.clone(),
            unit: Some("kilogram".into()),
        },
        None,
    )
    .await
    .unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let fruit = store::concepts::resolve(&e.store.pool, "fruit")
        .await
        .unwrap();
    let kg = store::concepts::resolve(&e.store.pool, "kilogram")
        .await
        .unwrap();
    assert_eq!(row.identity_predicate_uid, fruit);
    assert_eq!(row.unit_uid, kg);

    assert!(
        e.act(
            Action::SetIdentity {
                subject: uid.clone(),
                predicate: Some("nope".into()),
            },
            None
        )
        .await
        .is_err()
    );

    e.act(
        Action::SetIdentity {
            subject: uid.clone(),
            predicate: None,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .identity_predicate_uid
            .is_none()
    );
}

#[tokio::test]
async fn compensate_reverses_a_quantity_fact() {
    let e = engine().await;
    let uid = plain(&e, "stock").await;

    let out = e
        .act(
            Action::AddQuantity {
                target: uid.clone(),
                delta: 5.0,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        store::records::quantity(&e.store.pool, &uid)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(5.0)
    );
    let fact_uid = out.facts[0].uid.clone();

    let comp = e
        .act(Action::Compensate { fact: fact_uid }, None)
        .await
        .unwrap();
    assert_eq!(comp.facts.len(), 1);
    assert_eq!(comp.facts[0].delta, store::exact::from_f64(-5.0));
    assert_eq!(
        store::records::quantity(&e.store.pool, &uid)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0)
    );
}

#[tokio::test]
async fn compensate_zero_delta_fact_is_a_noop() {
    let e = engine().await;
    let uid = plain(&e, "note").await;

    let out = e
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some("H".into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.facts[0].delta, store::exact::from_f64(0.0));

    let comp = e
        .act(
            Action::Compensate {
                fact: out.facts[0].uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(comp.facts.is_empty());
    assert!(
        e.act(
            Action::Compensate {
                fact: "f_missing".into()
            },
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn set_extension_writes_readable_sidecar() {
    let e = engine().await;
    let uid = plain(&e, "widget").await;

    e.act(
        Action::SetExtension {
            target: uid.clone(),
            namespace: "board.card".into(),
            fds: serde_json::json!({ "x": 10, "y": 20 }),
        },
        None,
    )
    .await
    .unwrap();

    let got = store::records::get_extension(&e.store.pool, &uid, "board.card")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, serde_json::json!({ "x": 10, "y": 20 }));
}

#[tokio::test]
async fn record_threads_and_messages_are_records_plus_links() {
    let e = engine().await;
    let subject = plain(&e, "abstract-idea").await;
    let receipt = plain(&e, "receipt.july").await;

    let thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion A".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let first = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "First message".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: None,
                references: vec![receipt.clone()],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let _reply = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "Reply message".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: Some(first.clone()),
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let other_thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion B".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        e.act(
            Action::CreateMessage {
                thread: other_thread,
                body: "Cross-thread reply".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: Some(first.clone()),
                references: vec![],
            },
            None,
        )
        .await
        .is_err(),
        "replies must stay inside their thread"
    );

    assert_eq!(
        store::records::get(&e.store.pool, &thread)
            .await
            .unwrap()
            .unwrap()
            .kind,
        "thread"
    );
    assert_eq!(
        store::records::get(&e.store.pool, &first)
            .await
            .unwrap()
            .unwrap()
            .kind,
        "message"
    );

    assert!(
        store::concepts::resolve(&e.store.pool, "thread-of")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::concepts::resolve(&e.store.pool, "message-in")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::concepts::resolve(&e.store.pool, "reply-to")
            .await
            .unwrap()
            .is_some()
    );
    let references = store::concepts::resolve(&e.store.pool, "references")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store::assertions::objects_from_subject(&e.store.pool, &first, &references)
            .await
            .unwrap()
            .into_iter()
            .map(|record| record.uid)
            .collect::<Vec<_>>(),
        vec![receipt.clone()]
    );
    assert!(
        e.act(
            Action::CreateMessage {
                thread,
                body: String::new(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: None,
                references: vec![receipt.clone(), receipt],
            },
            None,
        )
        .await
        .is_err(),
        "all references are validated and deduplicated before message creation"
    );
}

#[tokio::test]
async fn writing_message_lifecycle_persists_and_final_states_are_terminal() {
    let e = engine().await;
    let subject = plain(&e, "stream-subject").await;
    let thread = e
        .act(
            Action::CreateThread {
                target: subject,
                head: "Live response".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let message = e
        .act(
            Action::CreateMessage {
                thread,
                body: String::new(),
                author: None,
                state: nucleus::MessageState::Writing,
                parent: None,
                references: Vec::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let initial = store::records::get_extension(&e.store.pool, &message, "lince.message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(initial["state"], "writing");
    assert_eq!(initial["author"], initial["operator"]);

    e.act(
        Action::ReviseMessage {
            message: message.clone(),
            body: "Growing".into(),
            state: nucleus::MessageState::Writing,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::ReviseMessage {
            message: message.clone(),
            body: "Complete".into(),
            state: nucleus::MessageState::Finished,
        },
        None,
    )
    .await
    .unwrap();
    let finished = store::records::get_extension(&e.store.pool, &message, "lince.message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finished["state"], "finished");
    assert_eq!(
        store::records::get(&e.store.pool, &message)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Complete"
    );
    assert!(
        e.act(
            Action::ReviseMessage {
                message,
                body: "Too late".into(),
                state: nucleus::MessageState::Writing,
            },
            None,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn delegated_message_authorship_requires_the_agents_operator() {
    let e = engine().await;
    let operator = user_with(
        &e,
        "Operator",
        "message-operator",
        &["record:create", "record:update"],
    )
    .await;
    let other = user_with(
        &e,
        "Other",
        "other-operator",
        &["record:create", "record:update"],
    )
    .await;
    let agent = e
        .act(
            Action::CreateAgent {
                head: "Writer".into(),
                operated_by: Some(operator.clone()),
            },
            Some(operator.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let subject = plain(&e, "delegated-subject").await;
    let thread = e
        .act(
            Action::CreateThread {
                target: subject,
                head: "Delegation".into(),
            },
            Some(operator.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let message = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "Authored by an Agent".into(),
                author: Some(agent.clone()),
                state: nucleus::MessageState::Finished,
                parent: None,
                references: Vec::new(),
            },
            Some(operator.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let metadata = store::records::get_extension(&e.store.pool, &message, "lince.message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata["author"], agent);
    assert_eq!(metadata["operator"], operator);
    assert!(
        e.act(
            Action::CreateMessage {
                thread,
                body: "Impersonation".into(),
                author: Some(metadata["author"].as_str().unwrap().into()),
                state: nucleus::MessageState::Finished,
                parent: None,
                references: Vec::new(),
            },
            Some(other),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn message_drafts_are_private_restart_safe_and_consumed_only_after_send() {
    let database = std::env::temp_dir().join(format!(
        "lince-message-drafts-{}.sqlite",
        nucleus::new_uid("test")
    ));
    std::fs::File::create(&database).unwrap();
    let database_url = format!("sqlite://{}", database.display());
    let operator;
    let observer;
    let draft;

    {
        let e = Engine::open(&database_url).await.unwrap();
        operator = user_with(
            &e,
            "Draft owner",
            "draft-owner",
            &[
                "record:read",
                "record:create",
                "record:update",
                "record:delete_own",
            ],
        )
        .await;
        observer = user_with(
            &e,
            "Observer",
            "draft-observer",
            &[
                "record:read",
                "record:create",
                "record:update",
                "record:delete_own",
            ],
        )
        .await;
        let conversation = store::records::create(
            &e.store.pool,
            NewRecord {
                slug: Some("draft-conversation"),
                kind: RecordKind::Conversation,
                head: "Draft conversation",
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap()
        .uid;
        let thread = e
            .act(
                Action::CreateThread {
                    target: conversation.clone(),
                    head: "Draft thread".into(),
                },
                Some(operator.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        draft = e
            .act(
                Action::CreateMessageDraft {
                    conversation,
                    thread,
                    body: "Restart-safe preset".into(),
                    pinned: true,
                    timing: nucleus::MessageDraftTiming::AfterTurn,
                    position: 3,
                },
                Some(operator.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let query = protein::Protein {
            source: protein::Source::Record,
            filter: vec![protein::Predicate::KindEq("message_draft".into())],
            fields: None,
            include: protein::Include {
                extension: Some(protein::ExtensionInclude {
                    namespace: "lince.message-draft".into(),
                }),
                ..protein::Include::default()
            },
            aggregate: None,
            order: vec![],
            limit: None,
        };
        assert_eq!(
            protein::execute_for(&e.store, &query, Some(&operator))
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            protein::execute_for(&e.store, &query, Some(&observer))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            e.act(
                Action::ReviseMessageDraft {
                    draft: draft.clone(),
                    body: "Not yours".into(),
                    pinned: false,
                    timing: nucleus::MessageDraftTiming::Now,
                    position: 0,
                },
                Some(observer.clone()),
            )
            .await
            .is_err()
        );
    }

    {
        let e = Engine::open(&database_url).await.unwrap();
        let metadata = store::records::get_extension(&e.store.pool, &draft, "lince.message-draft")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(metadata["pinned"], true);
        assert_eq!(metadata["timing"], "after_turn");
        assert_eq!(metadata["position"], 3);
        let message = e
            .act(
                Action::SendMessageDraft {
                    draft: draft.clone(),
                },
                Some(operator.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(
            store::records::get(&e.store.pool, &draft)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            store::records::get_extension(&e.store.pool, &message, "lince.message")
                .await
                .unwrap()
                .unwrap()["state"],
            "finished"
        );
        e.act(
            Action::ReviseMessageDraft {
                draft: draft.clone(),
                body: "Consume once".into(),
                pinned: false,
                timing: nucleus::MessageDraftTiming::Now,
                position: 0,
            },
            Some(operator.clone()),
        )
        .await
        .unwrap();
        e.act(
            Action::SendMessageDraft {
                draft: draft.clone(),
            },
            Some(operator),
        )
        .await
        .unwrap();
        assert!(
            store::records::get(&e.store.pool, &draft)
                .await
                .unwrap()
                .is_none()
        );
    }
    std::fs::remove_file(database).unwrap();
}

#[tokio::test]
async fn thread_and_message_deletion_use_normal_record_delete_permission() {
    let e = engine().await;
    let subject = plain(&e, "thread-delete-subject").await;
    let thread = e
        .act(
            Action::CreateThread {
                target: subject,
                head: "Protected thread".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let message = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "Protected message".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let bystander = user_with(&e, "Bystander", "thread-delete-bystander", &[]).await;

    for target in [&thread, &message] {
        let err = e
            .act(
                Action::DeleteRecord {
                    target: target.to_string(),
                },
                Some(bystander.to_string()),
            )
            .await
            .expect_err("thread and message deletion must use record:delete permission");
        assert!(err.to_string().contains("forbidden"));
        assert!(
            store::records::get(&e.store.pool, target)
                .await
                .unwrap()
                .is_some()
        );
    }
}

#[tokio::test]
async fn binary_assertion_actions_annotate_affected_records() {
    let e = engine().await;
    let a = plain(&e, "link-a").await;
    let b = plain(&e, "link-b").await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "contributes".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();

    let added = e
        .act(
            Action::AssertRecord {
                subject: a.clone(),
                predicate: "contributes".into(),
                object: Some(b.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(added.facts.len(), 2);
    assert!(
        added
            .facts
            .iter()
            .all(|fact| fact.delta == store::exact::from_f64(0.0))
    );
    assert!(added.facts.iter().any(|fact| fact.record_uid == a));
    assert!(added.facts.iter().any(|fact| fact.record_uid == b));

    let removed = e
        .act(
            Action::RetractRecord {
                subject: a.clone(),
                predicate: "contributes".into(),
                object: Some(b.clone()),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(removed.facts.len(), 2);
}

#[tokio::test]
async fn refine_turns_unary_assertion_into_binary_under_same_predicate() {
    let e = engine().await;
    let a = plain(&e, "refine-a").await;
    let project = plain(&e, "refine-project").await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "task".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AssertRecord {
            subject: a.clone(),
            predicate: "task".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let out = e
        .act(
            Action::RefineAssertion {
                subject: a.clone(),
                predicate: "task".into(),
                object: project.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.facts.len(), 2);

    let active = store::assertions::list_active(&e.store.pool).await.unwrap();
    let task_predicate = store::concepts::resolve(&e.store.pool, "task")
        .await
        .unwrap()
        .unwrap();
    let matching: Vec<_> = active
        .iter()
        .filter(|row| row.subject_uid == a && row.predicate_uid == task_predicate)
        .collect();
    assert_eq!(matching.len(), 1, "old unary tuple must be retracted");
    assert_eq!(matching[0].object_uid.as_deref(), Some(project.as_str()));
}

#[tokio::test]
async fn refine_is_idempotent_when_called_twice() {
    let e = engine().await;
    let a = plain(&e, "refine-idem-a").await;
    let project = plain(&e, "refine-idem-project").await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "task".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AssertRecord {
            subject: a.clone(),
            predicate: "task".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let first = e
        .act(
            Action::RefineAssertion {
                subject: a.clone(),
                predicate: "task".into(),
                object: project.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let second = e
        .act(
            Action::RefineAssertion {
                subject: a.clone(),
                predicate: "task".into(),
                object: project.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.created, second.created);
}

#[tokio::test]
async fn assertion_order_rewrites_adjacent_relationships() {
    let e = engine().await;
    let a = plain(&e, "order-a").await;
    let b = plain(&e, "order-b").await;
    let c = plain(&e, "order-c").await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "order".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AssertRecord {
            subject: a.clone(),
            predicate: "order".into(),
            object: Some(c.clone()),
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let out = e
        .act(
            Action::SetAssertionOrder {
                predicate: "order".into(),
                ordered: vec![a.clone(), b.clone(), c.clone()],
                reverse: false,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.facts.len(), 3);

    let kind_uid = store::concepts::resolve(&e.store.pool, "order")
        .await
        .unwrap()
        .unwrap();
    let edges = store::assertions::edges_of_predicate(&e.store.pool, &kind_uid)
        .await
        .unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().any(|edge| edge.from == a && edge.to == b));
    assert!(edges.iter().any(|edge| edge.from == b && edge.to == c));
}

#[tokio::test]
async fn delete_record_is_distinct_from_deactivate() {
    let e = engine().await;
    let uid = plain(&e, "doomed").await;
    e.act(
        Action::SetQuantity {
            target: uid.clone(),
            value: 3.0,
        },
        None,
    )
    .await
    .expect("give it a quantity");

    e.act(
        Action::Deactivate {
            target: uid.clone(),
        },
        None,
    )
    .await
    .expect("deactivate");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .expect("deactivated record still exists");
    assert_eq!(row.quantity, store::exact::from_f64(0.0));
    assert_eq!(row.slug.as_deref(), Some("doomed"));

    let out = e
        .act(
            Action::DeleteRecord {
                target: "doomed".into(),
            },
            None,
        )
        .await
        .expect("delete");
    assert_eq!(out.facts.len(), 1);
    assert_eq!(out.facts[0].delta, store::exact::from_f64(0.0));
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_none(),
        "deleted record must not be readable"
    );
    assert!(
        store::records::resolve(&e.store.pool, "doomed")
            .await
            .unwrap()
            .is_none(),
        "deleted record must not resolve by slug"
    );
    assert!(
        !store::records::list_all(&e.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|r| r.uid == uid),
        "deleted record must not appear in the record base set"
    );
    let facts = store::facts::for_record(&e.store.pool, &uid, 50)
        .await
        .unwrap();
    assert!(
        facts.len() >= 3,
        "creation-era + deactivate + deletion facts stay in the Ledger"
    );

    let reused = e
        .act(
            Action::CreateRecord {
                slug: Some("doomed".into()),
                kind: nucleus::RecordKind::Plain,
                head: "reborn".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .expect("slug is free again");
    let new_uid = reused.created.expect("created uid");
    assert_ne!(new_uid, uid);
}

async fn user_with(e: &Engine, name: &str, username: &str, perms: &[&str]) -> String {
    let role_id = store::auth::ensure_role(&e.store.pool, username)
        .await
        .unwrap();
    for perm in perms {
        let (subject, action) = perm.split_once(':').unwrap();
        let perm_id = store::auth::ensure_permission(&e.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&e.store.pool, role_id, perm_id)
            .await
            .unwrap();
    }
    store::auth::create_person_login(
        &e.store.pool,
        name,
        username,
        "hash-not-checked-here",
        role_id,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn delete_record_without_permission_is_forbidden() {
    let e = engine().await;
    let uid = plain(&e, "unauthorized-target").await;
    let bystander = user_with(&e, "Bystander", "bystander", &[]).await;

    let err = e
        .act(
            Action::DeleteRecord {
                target: uid.clone(),
            },
            Some(bystander.to_string()),
        )
        .await
        .expect_err("no record:delete or record:delete_own grant");
    assert!(err.to_string().contains("forbidden"));
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_some(),
        "record must survive a denied delete"
    );
}

#[tokio::test]
async fn delete_own_permission_allows_only_the_creator() {
    let e = engine().await;
    let owner = user_with(
        &e,
        "Owner",
        "owner",
        &["record:delete_own", "record:update"],
    )
    .await;
    let stranger = user_with(&e, "Stranger", "stranger", &["record:delete_own"]).await;

    let mine = plain(&e, "mine").await;
    e.act(
        Action::SetQuantity {
            target: mine.clone(),
            value: 1.0,
        },
        Some(owner.to_string()),
    )
    .await
    .expect("owner's first fact establishes creator_uid");

    let err = e
        .act(
            Action::DeleteRecord {
                target: mine.clone(),
            },
            Some(stranger.to_string()),
        )
        .await
        .expect_err("delete_own does not cover records the actor didn't create");
    assert!(err.to_string().contains("forbidden"));

    e.act(
        Action::DeleteRecord {
            target: mine.clone(),
        },
        Some(owner.to_string()),
    )
    .await
    .expect("delete_own covers the actor's own record");
    assert!(
        store::records::get(&e.store.pool, &mine)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn delete_permission_allows_deleting_any_record() {
    let e = engine().await;
    let owner = user_with(&e, "Owner", "owner2", &["record:update"]).await;
    let admin = user_with(&e, "Admin", "admin2", &["record:delete"]).await;

    let theirs = plain(&e, "theirs").await;
    e.act(
        Action::SetQuantity {
            target: theirs.clone(),
            value: 1.0,
        },
        Some(owner.to_string()),
    )
    .await
    .expect("owner's first fact establishes creator_uid");

    e.act(
        Action::DeleteRecord {
            target: theirs.clone(),
        },
        Some(admin.to_string()),
    )
    .await
    .expect("record:delete covers any record regardless of creator");
    assert!(
        store::records::get(&e.store.pool, &theirs)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn delete_record_with_no_actor_is_unrestricted() {
    let e = engine().await;
    let uid = plain(&e, "local-mode").await;
    e.act(
        Action::DeleteRecord {
            target: uid.clone(),
        },
        None,
    )
    .await
    .expect("local-no-auth mode (actor = None) is unrestricted");
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
}
