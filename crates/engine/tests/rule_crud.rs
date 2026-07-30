//! Part VII completion: Karma CRUD + Lingua adoption Actions (blueprint VII.2).

use engine::Engine;
use engine::actions::{Action, ConceptSeed, ConsequenceInput};
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

async fn quantity(e: &Engine, slug: &str) -> f64 {
    let uid = store::records::resolve(&e.store.pool, slug)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::records::quantity(&e.store.pool, &uid)
        .await
        .unwrap()
        .map(|q| q.to_f64())
        .unwrap()
}

fn add_quantity(target: &str) -> ConsequenceInput {
    ConsequenceInput {
        kind: "add_quantity".into(),
        target: Some(target.into()),
        params: None,
    }
}

#[tokio::test]
async fn create_and_update_rule_through_actions() {
    let e = engine().await;
    let x = plain(&e, "x").await;
    plain(&e, "alerts").await;

    let rule = e
        .act(
            Action::CreateRule {
                slug: "rules.alert".into(),
                head: "Alert".into(),
                condition: "@x".into(),
                gate: "!=0".into(),
                carry: "one".into(),
                debounce: None,
                consequences: vec![add_quantity("@alerts")],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    e.append_user(&x, 1.0).await.unwrap();
    assert_eq!(quantity(&e, "alerts").await, 1.0, "created rule is live");

    // update: gate now requires x above 10 — the same input stops firing
    e.act(
        Action::UpdateRule {
            rule: rule.clone(),
            condition: None,
            gate: Some(">10".into()),
            carry: None,
            debounce: None,
            consequences: None,
        },
        None,
    )
    .await
    .unwrap();
    e.append_user(&x, 1.0).await.unwrap();
    assert_eq!(quantity(&e, "alerts").await, 1.0, "updated gate holds");

    e.append_user(&x, 20.0).await.unwrap();
    assert_eq!(quantity(&e, "alerts").await, 2.0, "fires past the new gate");
}

#[tokio::test]
async fn creating_a_rule_loop_returns_proof_warnings() {
    let e = engine().await;
    plain(&e, "a").await;
    plain(&e, "b").await;

    let first = e
        .act(
            Action::CreateRule {
                slug: "rules.a-to-b".into(),
                head: "A to B".into(),
                condition: "@a".into(),
                gate: "!=0".into(),
                carry: "one".into(),
                debounce: None,
                consequences: vec![add_quantity("@b")],
            },
            None,
        )
        .await
        .unwrap();
    assert!(first.warnings.is_empty(), "no loop yet");

    let second = e
        .act(
            Action::CreateRule {
                slug: "rules.b-to-a".into(),
                head: "B to A".into(),
                condition: "@b".into(),
                gate: "!=0".into(),
                carry: "one".into(),
                debounce: None,
                consequences: vec![add_quantity("@a")],
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        !second.warnings.is_empty(),
        "the Proof surfaces the two-rule loop on save"
    );
}

#[tokio::test]
async fn create_signal_and_frequency_through_actions() {
    let e = engine().await;
    let signal = e
        .act(
            Action::CreateSignal {
                slug: "signals.fridge".into(),
                head: "Fridge".into(),
                source_kind: "command".into(),
                source: "echo 7".into(),
                schedule: "90s".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        store::misc::list_signals(&e.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|s| s.record_uid == signal)
    );

    let freq = e
        .act(
            Action::CreateFrequency {
                slug: "freq.daily-7am".into(),
                head: "Daily 7am".into(),
                seconds: 0,
                days: 1,
                months: 0,
                day_of_week: None,
                next_at: "2026-07-12T07:00:00Z".into(),
                catch_up: true,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        store::freqs::all_enabled(&e.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|f| f.record_uid == freq)
    );
}

#[tokio::test]
async fn adopt_concepts_preserves_uid_and_lineage_and_equivalence_is_declared() {
    let e = engine().await;
    // the package carries the parent too — adoption inserts both
    e.act(
        Action::AdoptConcepts {
            concepts: vec![
                ConceptSeed {
                    uid: "c_FOOD".into(),
                    name: "food".into(),
                    origin: Some("organ.bakery".into()),
                    parents: vec![],
                },
                ConceptSeed {
                    uid: "c_APPLE".into(),
                    name: "apple".into(),
                    origin: Some("organ.bakery".into()),
                    parents: vec!["c_FOOD".into()],
                },
            ],
        },
        None,
    )
    .await
    .unwrap();

    // uid preserved; DAG walk sees the lineage
    let family = store::concepts::descendants_including(&e.store.pool, "c_FOOD")
        .await
        .unwrap();
    assert!(family.contains(&"c_APPLE".to_string()));

    // re-adoption is a no-op, not an error
    e.act(
        Action::AdoptConcepts {
            concepts: vec![ConceptSeed {
                uid: "c_APPLE".into(),
                name: "apple".into(),
                origin: None,
                parents: vec!["c_FOOD".into()],
            }],
        },
        None,
    )
    .await
    .unwrap();

    let maca = store::concepts::create(&e.store.pool, "maca-fuji", &[])
        .await
        .unwrap();
    e.act(
        Action::DeclareEquivalence {
            a: "apple".into(),
            b: maca.clone(),
        },
        None,
    )
    .await
    .unwrap();
    let row = store::sqlx::query("SELECT COUNT(1) AS n FROM concept_equivalence")
        .fetch_one(&e.store.pool)
        .await
        .unwrap();
    use store::sqlx::Row;
    assert_eq!(row.get::<i64, _>("n"), 1);
}
