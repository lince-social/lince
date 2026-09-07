use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;

async fn cell() -> Engine {
    Engine::open_memory().await.expect("engine")
}

async fn person(e: &Engine, head: &str) -> String {
    e.act(
        Action::CreateRecord {
            slug: None,
            kind: RecordKind::Person,
            head: head.into(),
            body: "".into(),
            quantity: 0.0,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

#[tokio::test]
async fn an_agent_is_an_actor_beside_a_person_rather_than_a_kind_of_its_own() {
    let e = cell().await;
    let uid = e
        .act(
            Action::CreateAgent {
                head: "claude".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.kind,
        RecordKind::Person.as_str(),
        "an Agent holds work the same way a Person does, so it is the same type"
    );
    assert!(row.identity_predicate_uid.is_some(), "@agent is what it IS");

    let actor = store::concepts::resolve(&e.store.pool, "actor")
        .await
        .unwrap()
        .expect("the vocabulary was ensured");
    let under = store::concepts::descendants_including(&e.store.pool, &actor)
        .await
        .unwrap();
    for name in ["person", "agent"] {
        let uid = store::concepts::resolve(&e.store.pool, name)
            .await
            .unwrap()
            .expect(name);
        assert!(under.contains(&uid), "{name} is an actor");
    }
}

#[tokio::test]
async fn an_agent_says_which_person_is_answerable_for_it() {
    let e = cell().await;
    let eduardo = person(&e, "Eduardo").await;
    let uid = e
        .act(
            Action::CreateAgent {
                head: "codex".into(),
                operated_by: Some(eduardo.clone()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let assertions = store::assertions::for_subjects(&e.store.pool, &[uid.clone()])
        .await
        .unwrap();
    assert!(
        assertions
            .iter()
            .any(|a| a.predicate == "operated-by"
                && a.object_uid.as_deref() == Some(eduardo.as_str())),
        "operator: {assertions:?}"
    );
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert!(
        row.organ_uid.is_none() || row.organ_uid.is_some(),
        "the column exists"
    );
}

#[tokio::test]
async fn an_operator_that_is_not_a_person_creates_no_agent() {
    let e = cell().await;
    let note = e
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "not a person".into(),
                body: "".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let before = store::records::list_all(&e.store.pool).await.unwrap().len();
    let refused = e
        .act(
            Action::CreateAgent {
                head: "codex".into(),
                operated_by: Some(note),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "refused: {refused:?}");
    let after = store::records::list_all(&e.store.pool).await.unwrap().len();
    assert_eq!(before, after, "and nothing was left behind");
}

#[tokio::test]
async fn an_agent_with_no_name_is_refused() {
    let e = cell().await;
    let refused = e
        .act(
            Action::CreateAgent {
                head: "   ".into(),
                operated_by: None,
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "an assignee list of blanks helps nobody");
}

#[tokio::test]
async fn importing_instinct_puts_the_documentation_in_the_store() {
    let e = cell().await;
    let outcome = e.act(Action::ImportInstinct, None).await.unwrap();
    assert!(outcome.created.is_some(), "something was made");

    let bundle = engine::instinct::records();
    for record in &bundle {
        let uid = record.projection.uid.trim();
        let row = store::records::get(&e.store.pool, uid)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{} is missing", record.head));
        assert_eq!(row.head, record.head);
    }

    let chapter = bundle
        .iter()
        .find(|r| r.head == "Record")
        .expect("the Record chapter");
    let uids: Vec<String> = bundle.iter().map(|r| r.projection.uid.clone()).collect();
    let assertions = store::assertions::for_subjects(&e.store.pool, &uids)
        .await
        .unwrap();
    let parent_of = |uid: &str| {
        assertions
            .iter()
            .find(|a| a.subject_uid == uid && a.predicate == "part-of")
            .and_then(|a| a.object_uid.as_deref())
    };
    for record in &bundle {
        assert_eq!(
            parent_of(&record.projection.uid),
            record.parent_uid(),
            "{} keeps its declared parent",
            record.head
        );
    }
    let (child, parent) = bundle
        .iter()
        .find_map(|child| {
            let parent_uid = child.parent_uid()?;
            let parent = bundle
                .iter()
                .find(|record| record.projection.uid == parent_uid)?;
            Some((child, parent))
        })
        .expect("the bundle contains a declared parent link");
    assert_ne!(child.projection.uid, parent.projection.uid);
    assert_eq!(
        parent_of(&child.projection.uid),
        Some(parent.projection.uid.as_str()),
        "the imported parent link matches its declaration"
    );
    let level = store::records::quantity(&e.store.pool, chapter.projection.uid.trim())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        level.to_string(),
        chapter.quantity().expect("the Record declares a quantity"),
        "the declared quantity is preserved"
    );
}

#[tokio::test]
async fn a_second_import_leaves_your_edits_alone() {
    let e = cell().await;
    e.act(Action::ImportInstinct, None).await.unwrap();
    let uid = engine::instinct::records()[0]
        .projection
        .uid
        .trim()
        .to_string();
    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: None,
            body: Some("I rewrote this in my own words.".into()),
        },
        None,
    )
    .await
    .unwrap();

    e.act(Action::ImportInstinct, None).await.unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "I rewrote this in my own words.");
}
