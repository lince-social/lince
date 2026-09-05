use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{
    Aggregate, AggregateOp, DateComparison, GroupBy, Include, LinkDirection, LinksInclude,
    Predicate, Protein, Source, ThreadsInclude, WorkDateField,
};

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine")
}

async fn make(e: &Engine, slug: &str, kind: RecordKind, q: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind,
            head: slug.into(),
            body: String::new(),
            quantity: q,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

fn base(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

#[tokio::test]
async fn aggregate_sums_by_group() {
    let e = engine().await;
    make(&e, "checking", RecordKind::Plain, 500.0).await;
    make(&e, "savings", RecordKind::Plain, 1500.0).await;
    make(&e, "rules.x", RecordKind::Rule, 1.0).await;

    let mut p = base(Source::Record, vec![]);
    p.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::Kind,
    });
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let plain = rows.iter().find(|r| r["group"] == "plain").unwrap();
    assert_eq!(plain["value"], "2000");
}

#[tokio::test]
async fn availability_reflects_active_outgoing_promises() {
    let e = engine().await;
    let apples = make(&e, "apples.stock", RecordKind::Plain, 10.0).await;
    make(&e, "someone", RecordKind::Person, 1.0).await;
    store::config::set_transfer_reservation_default(&e.store.pool, "active")
        .await
        .unwrap();
    let promise = e
        .act(
            Action::CreatePromise {
                record: apples.clone(),
                delta: -3.0,
                window_end: None,
                party: Some("someone".into()),
                open: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for to in [nucleus::PromiseState::Agreed, nucleus::PromiseState::Active] {
        e.act(
            Action::PromiseTransition {
                promise: promise.clone(),
                to,
            },
            None,
        )
        .await
        .unwrap();
    }

    let mut p = base(
        Source::Record,
        vec![Predicate::SlugEq("apples.stock".into())],
    );
    p.include.availability = true;
    let rows = protein::execute(&e.store, &p).await.unwrap();
    assert_eq!(rows[0]["quantity"], 10.0);
    assert_eq!(rows[0]["available"], 7.0, "10 - 3 reserved");
    assert_eq!(rows[0]["planned"], 7.0, "10 + (-3) agreed/active");
}

#[tokio::test]
async fn uid_eq_targets_one_record_directly() {
    let e = engine().await;
    let target = make(&e, "target.record", RecordKind::Plain, 3.0).await;
    make(&e, "other.record", RecordKind::Plain, 3.0).await;

    let p = base(Source::Record, vec![Predicate::UidEq(target)]);
    let rows = protein::execute(&e.store, &p).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "target.record");
}

#[tokio::test]
async fn organ_eq_and_organ_in_filter_by_record_origin() {
    let e = engine().await;

    let organ_a = store::organs::ensure_local(&e.store.pool, "http://cell-a")
        .await
        .unwrap()
        .uid;
    let organ_b = store::organs::add_contact(
        &e.store.pool,
        "organ_b_uid",
        Some("organ.b"),
        "Cell B",
        "http://cell-b",
        1,
    )
    .await
    .unwrap();

    let _apple = make(&e, "apple", RecordKind::Plain, 1.0).await;
    let mango = make(&e, "mango", RecordKind::Plain, 1.0).await;
    store::records::set_organ_origin(&e.store.pool, &mango, Some(&organ_b))
        .await
        .unwrap();

    let a_only = protein::execute(
        &e.store,
        &base(
            Source::Record,
            vec![
                Predicate::OrganEq(organ_a.clone()),
                Predicate::KindEq("plain".into()),
            ],
        ),
    )
    .await
    .unwrap();
    assert_eq!(a_only.len(), 1);
    assert_eq!(a_only[0]["slug"], "apple");

    let a_or_b = protein::execute(
        &e.store,
        &base(
            Source::Record,
            vec![
                Predicate::OrganIn(vec![organ_a, organ_b]),
                Predicate::KindEq("plain".into()),
            ],
        ),
    )
    .await
    .unwrap();
    let mut slugs: Vec<String> = a_or_b
        .iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    slugs.sort();
    assert_eq!(slugs, vec!["apple", "mango"]);
}

#[tokio::test]
async fn quantity_lte_and_gte_include_the_boundary() {
    let e = engine().await;
    make(&e, "low", RecordKind::Plain, -1.0).await;
    make(&e, "edge", RecordKind::Plain, 0.0).await;
    make(&e, "high", RecordKind::Plain, 1.0).await;

    let lte = base(
        Source::Record,
        vec![
            Predicate::QuantityLte(0.0),
            Predicate::KindEq("plain".into()),
        ],
    );
    let mut lte_slugs: Vec<String> = protein::execute(&e.store, &lte)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    lte_slugs.sort();
    assert_eq!(lte_slugs, vec!["edge", "low"]);

    let gte_json = serde_json::json!({
        "source": "record",
        "where": [ { "quantity_gte": 0.0 }, { "kind_eq": "plain" } ]
    });
    let gte: Protein = serde_json::from_value(gte_json).unwrap();
    let gte_slugs: Vec<String> = protein::execute(&e.store, &gte)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(gte_slugs, vec!["edge", "high"]);
}

#[tokio::test]
async fn links_include_is_explicit_and_supports_multiple_kinds() {
    let e = engine().await;
    let a = make(&e, "a", RecordKind::Plain, 0.0).await;
    let b = make(&e, "b", RecordKind::Plain, 0.0).await;
    let c = make(&e, "c", RecordKind::Plain, 0.0).await;
    for kind in ["before", "contributes"] {
        e.act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: kind.into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    }
    e.act(
        Action::AssertRecord {
            subject: a.clone(),
            predicate: "before".into(),
            object: Some(b.clone()),
            quantity: Some("1".into()),
            unit: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AssertRecord {
            subject: c,
            predicate: "contributes".into(),
            object: Some(a.clone()),
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let mut none = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    none.include.links = Some(LinksInclude {
        kinds: vec![],
        direction: LinkDirection::Both,
        depth: 0,
    });
    assert!(
        protein::execute(&e.store, &none).await.unwrap()[0]["links"]
            .as_array()
            .unwrap()
            .is_empty(),
        "empty kinds means no links"
    );

    let mut both = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    both.include.links = Some(LinksInclude {
        kinds: vec!["before".into(), "contributes".into()],
        direction: LinkDirection::Both,
        depth: 0,
    });
    let rows = protein::execute(&e.store, &both).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|link| link["kind"] == "before" && link["direction"] == "out")
    );
    assert!(
        links
            .iter()
            .any(|link| link["kind"] == "contributes" && link["direction"] == "in")
    );

    let mut outgoing = both;
    outgoing.include.links.as_mut().unwrap().direction = LinkDirection::Out;
    let rows = protein::execute(&e.store, &outgoing).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "before");

    let mut all = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    all.include.links = Some(LinksInclude {
        kinds: vec!["*".into()],
        direction: LinkDirection::Both,
        depth: 0,
    });
    let rows = protein::execute(&e.store, &all).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().any(|link| link["kind"] == "before"));
    assert!(links.iter().any(|link| link["kind"] == "contributes"));
    all.include.links.as_mut().unwrap().direction = LinkDirection::In;
    let rows = protein::execute(&e.store, &all).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "contributes");
}

#[tokio::test]
async fn relation_filters_by_tag_cluster_with_include_and_exclude() {
    let e = engine().await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "tag".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    make(&e, "tasks", RecordKind::Plain, 0.0).await;
    make(&e, "project-a", RecordKind::Plain, 0.0).await;
    make(&e, "project-b", RecordKind::Plain, 0.0).await;
    let t1 = make(&e, "t1", RecordKind::Plain, 0.0).await;
    let t2 = make(&e, "t2", RecordKind::Plain, 0.0).await;
    let t3 = make(&e, "t3", RecordKind::Plain, 0.0).await;

    let tag = |from: &str, to: &str| {
        let e = &e;
        let from = from.to_string();
        let to = to.to_string();
        async move {
            e.act(
                Action::AssertRecord {
                    subject: from,
                    predicate: "tag".into(),
                    object: Some(to),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        }
    };
    tag(&t1, "tasks").await;
    tag(&t1, "project-a").await;
    tag(&t2, "project-a").await;
    tag(&t2, "project-b").await;
    tag(&t3, "tasks").await;

    let filter = vec![Predicate::All(vec![
        Predicate::Any(vec![
            Predicate::Relation {
                kind: "tag".into(),
                direction: LinkDirection::Out,
                other: Some("tasks".into()),
            },
            Predicate::Relation {
                kind: "tag".into(),
                direction: LinkDirection::Out,
                other: Some("project-a".into()),
            },
        ]),
        Predicate::Not(Box::new(Predicate::Relation {
            kind: "tag".into(),
            direction: LinkDirection::Out,
            other: Some("project-b".into()),
        })),
    ])];
    let rows = protein::execute(&e.store, &base(Source::Record, filter))
        .await
        .unwrap();
    let uids: Vec<&str> = rows.iter().map(|r| r["uid"].as_str().unwrap()).collect();
    assert!(
        uids.contains(&t1.as_str()),
        "t1 (Tasks+ProjectA, no ProjectB) should match"
    );
    assert!(uids.contains(&t3.as_str()), "t3 (Tasks) should match");
    assert!(
        !uids.contains(&t2.as_str()),
        "t2 (has ProjectB) should be excluded"
    );
    assert_eq!(uids.len(), 2, "only t1 and t3 match: {uids:?}");
}

#[tokio::test]
async fn threads_include_keeps_an_empty_thread_without_a_message_predicate() {
    let e = engine().await;
    let subject = make(&e, "empty-conversation", RecordKind::Plain, 1.0).await;
    e.act(
        Action::CreateThread {
            target: subject.clone(),
            head: "Introductions".into(),
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        store::concepts::resolve(&e.store.pool, "message-in")
            .await
            .unwrap()
            .is_none(),
        "the regression requires a Cell on which no message has existed"
    );
    let p = Protein {
        source: Source::Record,
        filter: vec![Predicate::UidEq(subject)],
        fields: None,
        include: Include {
            threads: Some(ThreadsInclude { messages_limit: 20 }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["head"], "Introductions");
    assert_eq!(threads[0]["messages"], serde_json::json!([]));
}

#[tokio::test]
async fn threads_include_returns_nested_record_messages() {
    let e = engine().await;
    let subject = make(&e, "abstract-idea", RecordKind::Plain, 0.0).await;
    let receipt = make(&e, "receipt.july", RecordKind::Plain, 1.0).await;
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
    let reply = e
        .act(
            Action::CreateMessage {
                thread,
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

    let p = Protein {
        source: Source::Record,
        filter: vec![Predicate::UidEq(subject)],
        fields: None,
        include: Include {
            threads: Some(ThreadsInclude { messages_limit: 20 }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["head"], "Discussion A");
    let messages = threads[0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["body"], "First message");
    assert_eq!(messages[0]["references"][0]["uid"], receipt);
    assert_eq!(messages[0]["references"][0]["head"], "receipt.july");
    assert_eq!(messages[1]["body"], "Reply message");
    assert_eq!(
        messages[1]["parent_message_uid"].as_str(),
        Some(first.as_str())
    );
    assert_eq!(messages[1]["uid"].as_str(), Some(reply.as_str()));
    assert!(threads[0]["created_at"].as_str().is_some());
    assert!(messages[0]["created_at"].as_str().is_some());
    assert!(messages[1]["created_at"].as_str().is_some());
    assert!(threads[0]["sender"].is_null());
    assert!(messages[0]["sender"].is_null());
    assert!(threads[0]["created_by"].is_null());
    assert!(messages[0]["created_by"].is_null());
    assert!(messages[0]["author"].as_str().is_some());
    assert!(messages[0]["operator"].as_str().is_some());
    assert_eq!(messages[0]["message_state"], "finished");
}

#[tokio::test]
async fn threads_include_resolves_sender_name_from_the_actor() {
    let e = engine().await;
    let role_id = store::auth::ensure_role(&e.store.pool, "lince")
        .await
        .unwrap();
    let create_permission_id = store::auth::ensure_permission(&e.store.pool, "record", "create")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, role_id, create_permission_id)
        .await
        .unwrap();
    let user_id = store::auth::create_person_login(
        &e.store.pool,
        "Ana Diaz",
        "ana",
        "hash-not-checked-here",
        role_id,
    )
    .await
    .unwrap();

    let subject = make(&e, "abstract-idea", RecordKind::Plain, 0.0).await;
    let thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion B".into(),
            },
            Some(user_id.to_string()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(
        Action::CreateMessage {
            thread,
            body: "hi from ana".into(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
            references: vec![],
        },
        Some(user_id.to_string()),
    )
    .await
    .unwrap();

    let p = Protein {
        source: Source::Record,
        filter: vec![Predicate::UidEq(subject)],
        fields: None,
        include: Include {
            threads: Some(ThreadsInclude { messages_limit: 20 }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads[0]["sender"].as_str(), Some("Ana Diaz"));
    assert_eq!(
        threads[0]["created_by"].as_str(),
        Some(user_id.to_string()).as_deref()
    );
    let messages = threads[0]["messages"].as_array().unwrap();
    assert_eq!(messages[0]["sender"].as_str(), Some("Ana Diaz"));
    assert_eq!(
        messages[0]["created_by"].as_str(),
        Some(user_id.to_string()).as_deref()
    );
}

#[tokio::test]
async fn visibility_gate_is_the_one_read_boundary() {
    let e = engine().await;
    let public_need = make(&e, "public.apples", RecordKind::Plain, -1.0).await;
    make(&e, "private.diary", RecordKind::Plain, -1.0).await;

    e.act(
        Action::GrantVisibility {
            subject_kind: "organ".into(),
            subject: Some("organ.neighbors".into()),
            target: public_need.clone(),
        },
        None,
    )
    .await
    .unwrap();

    let p = base(Source::Record, vec![Predicate::QuantityLt(0.0)]);
    assert_eq!(protein::execute(&e.store, &p).await.unwrap().len(), 2);
    let seen = protein::execute_for(&e.store, &p, Some("organ.neighbors"))
        .await
        .unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["slug"], "public.apples");
    assert!(
        protein::execute_for(&e.store, &p, Some("organ.unknown"))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn saved_protein_is_a_record() {
    let e = engine().await;
    make(&e, "a", RecordKind::Plain, -1.0).await;
    make(&e, "b", RecordKind::Plain, 5.0).await;

    let ast = serde_json::json!({
        "source": "record",
        "where": [{ "all": [{ "quantity_lt": 0.0 }] }]
    });
    e.act(
        Action::SaveProtein {
            slug: "views.my-needs".into(),
            head: "My Needs".into(),
            ast,
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute_saved(&e.store, "views.my-needs", None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "a");
}

#[tokio::test]
async fn near_predicate_uses_the_place_instinct() {
    let e = engine().await;
    let home = make(&e, "home", RecordKind::Plain, 1.0).await;
    let market = make(&e, "market.needs", RecordKind::Plain, -1.0).await;
    let far = make(&e, "far.needs", RecordKind::Plain, -1.0).await;

    e.act(
        Action::SetPlace {
            target: home,
            lat: 0.0,
            lon: 0.0,
            address: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetPlace {
            target: market,
            lat: 0.0009,
            lon: 0.0,
            address: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetPlace {
            target: far,
            lat: 0.09,
            lon: 0.0,
            address: None,
        },
        None,
    )
    .await
    .unwrap();

    let p = base(
        Source::Record,
        vec![
            Predicate::QuantityLt(0.0),
            Predicate::Near {
                of: "home".into(),
                meters: 2000.0,
            },
        ],
    );
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let slugs: Vec<&str> = rows.iter().map(|r| r["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, vec!["market.needs"], "only the nearby need matches");
}

#[tokio::test]
async fn auth_source_lists_roles_users_and_the_permission_catalog() {
    let e = engine().await;
    e.act(
        Action::CreateRole {
            name: "support".into(),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::GrantPermission {
            role: "support".into(),
            permission: "record:delete_own".into(),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::CreateUser {
            username: "amy".into(),
            name: "Amy".into(),
            password: "hunter2".into(),
            role: "support".into(),
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute(&e.store, &base(Source::Auth, vec![]))
        .await
        .unwrap();
    let role = rows
        .iter()
        .find(|r| r["kind"] == "role" && r["name"] == "support")
        .expect("the new role is listed");
    assert!(
        role["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "record:delete_own")
    );
    let user = rows
        .iter()
        .find(|r| r["kind"] == "user" && r["username"] == "amy")
        .expect("the new user is listed");
    assert_eq!(user["role"], "support");
    assert!(user.get("password_hash").is_none(), "never expose the hash");
    assert!(
        rows.iter().any(|r| r["kind"] == "permission_catalog"
            && r["keys"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "record:delete")),
        "the static permission catalog is listed for building a grant UI"
    );
}

#[tokio::test]
async fn auth_source_is_gated_by_read_permission_for_a_remote_subject() {
    let e = engine().await;
    let bystander_role = store::auth::ensure_role(&e.store.pool, "bystander")
        .await
        .unwrap();
    let bystander =
        store::auth::create_person_login(&e.store.pool, "B", "bystander", "hash", bystander_role)
            .await
            .unwrap();
    let reader_role = store::auth::ensure_role(&e.store.pool, "reader")
        .await
        .unwrap();
    let perm_id = store::auth::ensure_permission(&e.store.pool, "role", "read")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, reader_role, perm_id)
        .await
        .unwrap();
    let reader =
        store::auth::create_person_login(&e.store.pool, "R", "reader", "hash", reader_role)
            .await
            .unwrap();

    let p = base(Source::Auth, vec![]);
    let hidden = protein::execute_for(&e.store, &p, Some(&bystander.to_string()))
        .await
        .unwrap();
    assert!(
        hidden.is_empty(),
        "no role/user/permission read grant -> nothing"
    );

    let visible = protein::execute_for(&e.store, &p, Some(&reader.to_string()))
        .await
        .unwrap();
    assert!(!visible.is_empty(), "role:read grants the whole listing");

    let local = protein::execute_for(&e.store, &p, None).await.unwrap();
    assert!(
        !local.is_empty(),
        "the local Cell (subject: None) always sees it"
    );
}

#[tokio::test]
async fn nested_record_filters_cover_text_dates_relations_and_assignee() {
    let e = engine().await;
    for kind in ["part-of", "assigned-to"] {
        e.act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: kind.into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    }
    let root = make(&e, "root", RecordKind::Plain, 0.0).await;
    let middle = make(&e, "middle", RecordKind::Plain, 0.0).await;
    let leaf = make(&e, "leaf", RecordKind::Plain, 0.0).await;
    let design = make(&e, "design-task", RecordKind::Plain, -1.0).await;
    let ana = make(&e, "ana", RecordKind::Person, 1.0).await;
    for (from, kind, to) in [
        (&middle, "part-of", &root),
        (&leaf, "part-of", &middle),
        (&design, "assigned-to", &ana),
    ] {
        e.act(
            Action::AssertRecord {
                subject: from.clone(),
                predicate: kind.into(),
                object: Some(to.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    }
    store::records::set_extension(
        &e.store.pool,
        &middle,
        "work",
        &serde_json::json!({"start":"2026-08-01", "due":"2026-08-10"}),
    )
    .await
    .unwrap();

    let query = base(
        Source::Record,
        vec![Predicate::Any(vec![
            Predicate::All(vec![
                Predicate::QuantityLt(0.0),
                Predicate::TextContains("DESIGN".into()),
                Predicate::Relation {
                    kind: "assigned-to".into(),
                    direction: LinkDirection::Out,
                    other: Some("ana".into()),
                },
            ]),
            Predicate::All(vec![
                Predicate::Relation {
                    kind: "part-of".into(),
                    direction: LinkDirection::In,
                    other: None,
                },
                Predicate::Relation {
                    kind: "part-of".into(),
                    direction: LinkDirection::Out,
                    other: None,
                },
                Predicate::WorkDate {
                    field: WorkDateField::Due,
                    op: DateComparison::Lte,
                    value: Some("2026-08-10".into()),
                },
            ]),
        ])],
    );
    let rows = protein::execute(&e.store, &query).await.unwrap();
    let ids: std::collections::HashSet<&str> =
        rows.iter().filter_map(|row| row["uid"].as_str()).collect();
    assert_eq!(
        ids,
        std::collections::HashSet::from([middle.as_str(), design.as_str()])
    );
}

#[test]
fn filter_groups_allow_ten_indents_and_reject_eleven() {
    fn nested(levels: usize) -> Predicate {
        let mut predicate = Predicate::UidEq("record".into());
        for _ in 0..=levels {
            predicate = Predicate::All(vec![predicate]);
        }
        predicate
    }
    assert!(protein::validate(&base(Source::Record, vec![nested(10)])).is_ok());
    let error = protein::validate(&base(Source::Record, vec![nested(11)])).unwrap_err();
    assert!(error.to_string().contains("protein_filter_depth_exceeded"));
}

#[test]
fn filter_groups_cannot_be_negated() {
    let query = base(
        Source::Record,
        vec![Predicate::Not(Box::new(Predicate::All(vec![
            Predicate::KindEq("plain".into()),
        ])))],
    );
    let error = protein::validate(&query).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("protein_filter_group_negation_unsupported")
    );
}

#[test]
fn legacy_link_include_spelling_is_rejected() {
    let result = serde_json::from_value::<Protein>(serde_json::json!({
        "source": "record",
        "include": { "links": { "kind": "tag" } }
    }));
    assert!(result.is_err());
}

#[test]
fn legacy_filter_field_is_rejected() {
    let result = serde_json::from_value::<Protein>(serde_json::json!({
        "source": "record",
        "filter": [{ "kind_eq": "plain" }]
    }));
    assert!(result.is_err());
}

#[tokio::test]
async fn fields_narrow_a_row_to_what_was_asked_for() {
    let e = engine().await;
    make(&e, "narrowed", RecordKind::Plain, 3.0).await;

    let mut p = base(Source::Record, vec![Predicate::SlugEq("narrowed".into())]);
    p.fields = Some(vec!["head".into()]);
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let row = rows.first().expect("one row");

    assert!(row.get("head").is_some(), "what was asked for is there");
    assert!(
        row.get("body").is_none() && row.get("quantity").is_none(),
        "and nothing else is: {row}"
    );
    assert!(row.get("uid").is_some(), "uid always survives");
    assert!(row.get("kind").is_some(), "and so does kind");
}

#[tokio::test]
async fn a_field_nobody_named_does_not_appear() {
    let e = engine().await;
    make(&e, "unnamed", RecordKind::Plain, 7.0).await;

    let mut p = base(Source::Record, vec![Predicate::SlugEq("unnamed".into())]);
    p.fields = Some(vec!["head".into(), "a_column_invented_later".into()]);
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let row = rows.first().expect("one row");

    assert!(row.get("head").is_some());
    assert!(
        row.get("organ").is_none() && row.get("created_at").is_none(),
        "columns the selector never named stay home: {row}"
    );
}

#[tokio::test]
async fn a_withheld_column_is_absent_rather_than_blank() {
    let e = engine().await;
    make(&e, "blank-body", RecordKind::Plain, 1.0).await;

    let p = base(Source::Record, vec![Predicate::SlugEq("blank-body".into())]);
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let unnarrowed = rows.first().expect("one row");
    assert_eq!(
        unnarrowed.get("body").and_then(|b| b.as_str()),
        Some(""),
        "unnarrowed, an empty body is present and empty"
    );

    let mut p = base(Source::Record, vec![Predicate::SlugEq("blank-body".into())]);
    p.fields = Some(vec!["head".into()]);
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let narrowed = rows.first().expect("one row");
    assert!(
        narrowed.get("body").is_none(),
        "narrowed, it is ABSENT — not null, which a renderer would draw as empty: {narrowed}"
    );
}

#[tokio::test]
async fn no_selector_returns_the_whole_row() {
    let e = engine().await;
    make(&e, "whole", RecordKind::Plain, 1.0).await;

    let p = base(Source::Record, vec![Predicate::SlugEq("whole".into())]);
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let row = rows.first().expect("one row");

    for column in ["uid", "head", "body", "quantity", "organ", "created_at"] {
        assert!(row.get(column).is_some(), "{column} must still be there");
    }
}
