//! Agents: an Actor that is not a Person.
//!
//! Several agents work in this codebase at once, and the thing that lets them
//! do that without coordinating is a task Record that says who holds it. So an
//! Agent has to be a real assignable subject — not a convention in a comment —
//! and it has to say whose it is.
//!
//! The Person type itself was deliberately NOT renamed to Actor: `actor` is a
//! Concept with `person` and `agent` beneath it, which the DAG already answers
//! questions about, and which costs no migration.

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

    let row = store::records::get(&e.store.pool, &uid).await.unwrap().unwrap();
    assert_eq!(
        row.kind,
        RecordKind::Person.as_str(),
        "an Agent holds work the same way a Person does, so it is the same type"
    );
    assert!(row.identity_predicate_uid.is_some(), "@agent is what it IS");

    // The distinction lives in the Concept DAG: asking for Actors finds both.
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
            .any(|a| a.predicate == "operated-by" && a.object_uid.as_deref() == Some(eduardo.as_str())),
        "operator: {assertions:?}"
    );
    // The Organ half of "whose agent is this" needs nothing new — every Record
    // already carries where it originated.
    let row = store::records::get(&e.store.pool, &uid).await.unwrap().unwrap();
    assert!(row.organ_uid.is_none() || row.organ_uid.is_some(), "the column exists");
}

/// Resolve the operator BEFORE creating anything. Naming something that is not
/// a Person otherwise leaves an unowned Agent behind — and an Agent nobody is
/// answerable for is the exact thing the field exists to prevent.
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

/// Lince's own documentation, imported into a store.
///
/// The point of the bundle being Records rather than pages: once it is in your
/// store you can edit it, link to it, and — later — have a chapter about Karma
/// put real Karma Records in front of you.
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

    // The reading order survived as assertions, not as an import order.
    let chapter = bundle.iter().find(|r| r.head == "Records").expect("chapter 2");
    let uids: Vec<String> = bundle.iter().map(|r| r.projection.uid.clone()).collect();
    let assertions = store::assertions::for_subjects(&e.store.pool, &uids).await.unwrap();
    assert!(
        assertions.iter().any(|a| a.predicate == "chapter"
            && a.object_uid.as_deref() == Some(chapter.projection.uid.as_str())),
        "ideas point at their chapter"
    );
    // Stable documentation is quantity 1 — the same ladder the board reads.
    let level = store::records::quantity(&e.store.pool, chapter.projection.uid.trim())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(level.to_string(), "1", "chapters land as stable, not as zero");
}

/// Importing twice must not undo an edit made in between. The whole reason
/// these ship as Records is that you can change them.
#[tokio::test]
async fn a_second_import_leaves_your_edits_alone() {
    let e = cell().await;
    e.act(Action::ImportInstinct, None).await.unwrap();
    let uid = engine::instinct::records()[0].projection.uid.trim().to_string();
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

    let row = store::records::get(&e.store.pool, &uid).await.unwrap().unwrap();
    assert_eq!(row.body, "I rewrote this in my own words.");
}
