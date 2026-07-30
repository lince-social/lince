//! Entries (blueprint E0): correcting and undoing an authored change.
//!
//! The Ledger has no delete and no edit — a Fact is hash-chained and signed,
//! and a classification is an assertion about one. So every correction here is
//! a *new* pair of Facts, and what these tests defend is that the pair leaves
//! the totals saying what actually happened while the chain keeps both halves.

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://entries.test")
        .await
        .expect("local Organ");
    engine
}

/// A Record and a `@food` concept. The scenario is a household balance because
/// it reads clearly; the primitive under test knows nothing about what it counts.
async fn setup(e: &Engine) -> (String, String) {
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let checking = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("checking"),
            kind: RecordKind::Plain,
            head: "checking",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    (checking, food)
}

async fn capture(e: &Engine, amount: &str, request_id: Option<&str>) -> String {
    e.act(
        Action::CaptureEntry {
            target: "checking".into(),
            amount: amount.into(),
            concept: Some("food".into()),
            note: Some("groceries".into()),
            at: Some("2026-03-05T12:00:00Z".into()),
            request_id: request_id.map(str::to_string),
        },
        None,
    )
    .await
    .expect("capture")
    .created
    .expect("capture returns its entry")
}

async fn level(e: &Engine) -> String {
    let uid = store::records::resolve(&e.store.pool, "checking")
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::facts::level(&e.store.pool, &uid)
        .await
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn revising_an_amount_compensates_and_replaces_rather_than_rewriting() {
    let e = engine().await;
    setup(&e).await;
    let entry = capture(&e, "-15", None).await;
    assert_eq!(level(&e).await, "-15");

    // The typo: it was 150, not 15.
    let outcome = e
        .act(
            Action::ReviseEntry {
                entry: entry.clone(),
                expected_revision: 1,
                request_id: "fix-typo".into(),
                amount: "-150".into(),
                note: Some("groceries".into()),
                at: None,
            },
            None,
        )
        .await
        .unwrap();

    // Two Facts: the reversal and the replacement. Nothing was edited.
    assert_eq!(outcome.facts.len(), 2);
    assert_eq!(level(&e).await, "-150");

    let stored = store::entries::get(&e.store.pool, &entry)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.revision, 2);
    assert_eq!(stored.amount.to_string(), "-150");
    assert_eq!(stored.state, store::entries::STATE_APPLIED);

    // The chain kept all three entries: original, reversal, replacement.
    let record_uid = stored.record_uid.clone();
    let facts = store::facts::for_record(&e.store.pool, &record_uid, 100)
        .await
        .unwrap();
    assert_eq!(facts.iter().filter(|f| !f.delta.is_zero()).count(), 3);

    let history = store::entries::history(&e.store.pool, &entry)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].kind, "revised");
    // The correction is recorded as a pair, so an audit can show what was
    // reversed next to what replaced it rather than two loose changes.
    assert!(history[1].compensated_fact_uid.is_some());
    assert!(history[1].fact_uid.is_some());
}

#[tokio::test]
async fn a_correction_stays_in_the_category_it_was_captured_under() {
    let e = engine().await;
    let (_checking, food) = setup(&e).await;
    let entry = capture(&e, "-15", None).await;

    e.act(
        Action::ReviseEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "fix".into(),
            amount: "-150".into(),
            note: None,
            at: None,
        },
        None,
    )
    .await
    .unwrap();

    // Both new Facts carry @food. Without this, correcting an amount would
    // quietly drop the change out of its category and leave the category
    // showing the ORIGINAL wrong number forever.
    let record_uid = store::records::resolve(&e.store.pool, "checking")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let facts = store::facts::for_record(&e.store.pool, &record_uid, 100)
        .await
        .unwrap();
    for fact in facts.iter().filter(|f| !f.delta.is_zero()) {
        assert_eq!(
            store::ledger::fact_concept(&e.store.pool, &fact.uid)
                .await
                .unwrap()
                .as_deref(),
            Some(food.as_str()),
            "every Fact of a corrected entry stays classified"
        );
    }

    let window = store::ledger::LedgerWindow {
        record_uids: std::slice::from_ref(&record_uid),
        from: "2026-03-01T00:00:00Z".parse().unwrap(),
        to: "2026-04-01T00:00:00Z".parse().unwrap(),
        concept_uid: Some(&food),
    };
    let totals = store::ledger::totals(&e.store.pool, &window)
        .await
        .unwrap();
    // -15 + 15 - 150 = -150. The category shows what was actually spent.
    assert_eq!(totals.net.to_string(), "-150");
}

#[tokio::test]
async fn a_note_only_edit_moves_no_quantity_but_still_earns_a_revision() {
    let e = engine().await;
    setup(&e).await;
    let entry = capture(&e, "-15", None).await;
    let before = store::entries::get(&e.store.pool, &entry)
        .await
        .unwrap()
        .unwrap();

    let outcome = e
        .act(
            Action::ReviseEntry {
                entry: entry.clone(),
                expected_revision: 1,
                request_id: "note".into(),
                amount: "-15".into(),
                note: Some("groceries and a coffee".into()),
                at: None,
            },
            None,
        )
        .await
        .unwrap();

    // Nothing moved, so nothing was appended. A compensating pair here would
    // put two meaningless entries in the chain.
    assert!(outcome.facts.is_empty());
    assert_eq!(level(&e).await, "-15");

    let after = store::entries::get(&e.store.pool, &entry)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.revision, 2);
    assert_eq!(after.note.as_deref(), Some("groceries and a coffee"));
    // The entry still points at the Fact carrying its amount. Blanking it
    // would orphan it from its own Ledger entry.
    assert_eq!(after.fact_uid, before.fact_uid);
    assert_eq!(
        store::entries::history(&e.store.pool, &entry)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn voiding_returns_the_quantity_to_the_category_it_came_from() {
    let e = engine().await;
    let (_checking, food) = setup(&e).await;
    let entry = capture(&e, "-15", None).await;

    let outcome = e
        .act(
            Action::VoidEntry {
                entry: entry.clone(),
                expected_revision: 1,
                request_id: "undo".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(outcome.facts.len(), 1);
    assert_eq!(level(&e).await, "0");

    let stored = store::entries::get(&e.store.pool, &entry)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, store::entries::STATE_VOID);
    // The row survives: an append-only Ledger has no delete, and the change
    // did happen even though it was undone.
    assert_eq!(stored.amount.to_string(), "-15");

    // The compensating Fact carries @food too, so the category nets to zero
    // instead of showing -15 beside an unclassified +15.
    let record_uid = stored.record_uid.clone();
    let window = store::ledger::LedgerWindow {
        record_uids: std::slice::from_ref(&record_uid),
        from: "2026-03-01T00:00:00Z".parse().unwrap(),
        to: "2026-04-01T00:00:00Z".parse().unwrap(),
        concept_uid: Some(&food),
    };
    let totals = store::ledger::totals(&e.store.pool, &window)
        .await
        .unwrap();
    assert_eq!(totals.net.to_string(), "0");
    assert_eq!(totals.count, 2);
}

#[tokio::test]
async fn a_replayed_request_returns_the_first_answer_instead_of_moving_quantity_twice() {
    let e = engine().await;
    setup(&e).await;

    // Capture, retried.
    let first = capture(&e, "-15", Some("capture-1")).await;
    let retried = e
        .act(
            Action::CaptureEntry {
                target: "checking".into(),
                amount: "-15".into(),
                concept: Some("food".into()),
                note: Some("groceries".into()),
                at: Some("2026-03-05T12:00:00Z".into()),
                request_id: Some("capture-1".into()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(retried.facts.is_empty());
    assert_eq!(level(&e).await, "-15");

    // Void, retried. This is the one that matters: a second compensating Fact
    // would hand back a quantity that was only taken once.
    e.act(
        Action::VoidEntry {
            entry: first.clone(),
            expected_revision: 1,
            request_id: "undo-1".into(),
        },
        None,
    )
    .await
    .unwrap();
    let retried = e
        .act(
            Action::VoidEntry {
                entry: first.clone(),
                expected_revision: 1,
                request_id: "undo-1".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(retried.facts.is_empty());
    assert_eq!(level(&e).await, "0");
}

#[tokio::test]
async fn a_stale_revision_is_refused_so_two_editors_cannot_overwrite_each_other() {
    let e = engine().await;
    setup(&e).await;
    let entry = capture(&e, "-15", None).await;

    e.act(
        Action::ReviseEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "first".into(),
            amount: "-150".into(),
            note: None,
            at: None,
        },
        None,
    )
    .await
    .unwrap();

    // Someone still holding revision 1 tries to save.
    let stale = e
        .act(
            Action::ReviseEntry {
                entry: entry.clone(),
                expected_revision: 1,
                request_id: "second".into(),
                amount: "-99".into(),
                note: None,
                at: None,
            },
            None,
        )
        .await;
    assert!(stale.is_err());
    // And it was refused BEFORE any Fact was appended.
    assert_eq!(level(&e).await, "-150");
}

#[tokio::test]
async fn a_voided_entry_cannot_be_revised_back_to_life() {
    let e = engine().await;
    setup(&e).await;
    let entry = capture(&e, "-15", None).await;

    e.act(
        Action::VoidEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "undo".into(),
        },
        None,
    )
    .await
    .unwrap();

    let revived = e
        .act(
            Action::ReviseEntry {
                entry: entry.clone(),
                expected_revision: 2,
                request_id: "revive".into(),
                amount: "-15".into(),
                note: None,
                at: None,
            },
            None,
        )
        .await;
    assert!(revived.is_err());
    assert_eq!(level(&e).await, "0");
}

#[tokio::test]
async fn generic_compensation_cannot_bypass_an_entry() {
    let e = engine().await;
    setup(&e).await;
    let entry = capture(&e, "-15", None).await;
    let fact_uid = store::entries::get(&e.store.pool, &entry)
        .await
        .unwrap()
        .unwrap()
        .fact_uid
        .unwrap();

    // Compensating the Fact directly would return the quantity while the entry
    // still read `applied` — the Ledger and the thing describing it would
    // disagree with no way to tell which is right.
    let bypass = e
        .act(Action::Compensate { fact: fact_uid }, None)
        .await;
    assert!(bypass.is_err());
    assert_eq!(level(&e).await, "-15");

    // The typed route does both halves.
    e.act(
        Action::VoidEntry {
            entry,
            expected_revision: 1,
            request_id: "undo".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(level(&e).await, "0");
}

#[tokio::test]
async fn backdating_a_correction_moves_it_into_the_month_it_belongs_to() {
    let e = engine().await;
    setup(&e).await;
    let entry = capture(&e, "-15", None).await;

    e.act(
        Action::ReviseEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "backdate".into(),
            amount: "-15".into(),
            note: None,
            at: Some("2026-02-20T12:00:00Z".into()),
        },
        None,
    )
    .await
    .unwrap();

    let record_uid = store::records::resolve(&e.store.pool, "checking")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let uids = std::slice::from_ref(&record_uid);
    let march = store::ledger::totals(
        &e.store.pool,
        &store::ledger::LedgerWindow {
            record_uids: uids,
            from: "2026-03-01T00:00:00Z".parse().unwrap(),
            to: "2026-04-01T00:00:00Z".parse().unwrap(),
            concept_uid: None,
        },
    )
    .await
    .unwrap();
    let february = store::ledger::totals(
        &e.store.pool,
        &store::ledger::LedgerWindow {
            record_uids: uids,
            from: "2026-02-01T00:00:00Z".parse().unwrap(),
            to: "2026-03-01T00:00:00Z".parse().unwrap(),
            concept_uid: None,
        },
    )
    .await
    .unwrap();

    // March keeps the original AND its reversal — history is not rewritten —
    // so it nets to zero, and February now carries the change.
    assert_eq!(march.net.to_string(), "0");
    assert_eq!(march.count, 2);
    assert_eq!(february.net.to_string(), "-15");
    assert_eq!(february.count, 1);
}

#[tokio::test]
async fn the_same_correction_works_on_something_that_is_not_a_balance() {
    // A miscounted stock take, corrected. Identical machinery, identical
    // guarantees — the Entry primitive knows nothing about finance, and this
    // test exists so that stays true.
    let e = engine().await;
    let kg = store::concepts::create(&e.store.pool, "kg", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "spoilage", &[])
        .await
        .unwrap();
    let flour = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("flour"),
            kind: RecordKind::Plain,
            head: "flour",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    store::records::set_unit(&e.store.pool, &flour, Some(&kg))
        .await
        .unwrap();

    let entry = e
        .act(
            Action::CaptureEntry {
                target: "flour".into(),
                amount: "-2.5".into(),
                concept: Some("spoilage".into()),
                note: Some("weevils".into()),
                at: Some("2026-03-05T12:00:00Z".into()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(
        store::facts::level(&e.store.pool, &flour)
            .await
            .unwrap()
            .to_string(),
        "-2.5"
    );

    // It was 2.75 kg, not 2.5.
    e.act(
        Action::ReviseEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "recount".into(),
            amount: "-2.75".into(),
            note: None,
            at: None,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::facts::level(&e.store.pool, &flour)
            .await
            .unwrap()
            .to_string(),
        "-2.75"
    );

    // And voiding returns the flour, exactly as it returns a balance.
    e.act(
        Action::VoidEntry {
            entry,
            expected_revision: 2,
            request_id: "never-happened".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::facts::level(&e.store.pool, &flour)
            .await
            .unwrap()
            .to_string(),
        "0.00"
    );
}
