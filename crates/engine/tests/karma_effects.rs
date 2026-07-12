//! Part VI completion: debounce, the run_action/run_query effect kinds, and
//! the two remaining worked examples (quiet hours, trust-ahead).

use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::{ConsequenceKind, PromiseState, RecordKind};
use store::misc::NewPromise;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity,
        },
    )
    .await
    .expect("record")
    .uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
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
        .unwrap()
}

#[tokio::test]
async fn debounce_holds_a_rule_between_firings() {
    let e = engine().await;
    let trigger = plain(&e, "x", 0.0).await;
    plain(&e, "alerts", 0.0).await;
    let rule = store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.alert-on-x",
            head: "Alert on x",
            condition: "@x",
            gate: "!=0",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::AddQuantity,
                Some("@alerts".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    store::rules::set_debounce(&e.store.pool, &rule, Some("1h"))
        .await
        .unwrap();
    e.reload_rules().await.unwrap();

    e.append(
        nucleus::NewFact::quantity(trigger.clone(), 1.0, nucleus::Cause::user_edit()),
        at("2026-07-01T08:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(quantity(&e, "alerts").await, 1.0, "first delivery fires");

    e.append(
        nucleus::NewFact::quantity(trigger.clone(), 1.0, nucleus::Cause::user_edit()),
        at("2026-07-01T08:10:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(quantity(&e, "alerts").await, 1.0, "held inside the debounce");

    e.append(
        nucleus::NewFact::quantity(trigger, 1.0, nucleus::Cause::user_edit()),
        at("2026-07-01T09:30:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(quantity(&e, "alerts").await, 2.0, "fires after it elapses");
}

#[tokio::test]
async fn run_action_consequence_executes_through_act() {
    let e = engine().await;
    let trigger = plain(&e, "trigger", 0.0).await;
    plain(&e, "counter", 0.0).await;
    let rule = store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.set-counter",
            head: "Set counter",
            condition: "@trigger",
            gate: "!=0",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::RunAction,
                None,
                Some(serde_json::json!({
                    "action": "set-quantity",
                    "target": "counter",
                    "value": 5.0,
                })),
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&trigger, 1.0).await.unwrap();
    assert_eq!(quantity(&e, "counter").await, 0.0, "queued, not yet run");

    let outcomes = e.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].ok, "action effect ran: {}", outcomes[0].result);
    assert_eq!(quantity(&e, "counter").await, 5.0);

    // provenance: the effect logged a zero-delta fact on the rule record
    let log = store::facts::for_record(&e.store.pool, &rule, 10).await.unwrap();
    assert!(log.iter().any(|f| f
        .payload
        .as_deref()
        .is_some_and(|p| p.contains("\"effect\":\"action\""))));
}

#[tokio::test]
async fn run_query_consequence_executes_a_saved_protein() {
    let e = engine().await;
    let trigger = plain(&e, "trigger", 0.0).await;
    e.act(
        Action::SaveProtein {
            slug: "views.everything".into(),
            head: "Everything".into(),
            ast: serde_json::json!({ "source": "record" }),
        },
        None,
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.run-view",
            head: "Run view",
            condition: "@trigger",
            gate: "!=0",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::RunQuery,
                Some("views.everything".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&trigger, 1.0).await.unwrap();
    let outcomes = e.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].ok, "{}", outcomes[0].result);
    assert!(outcomes[0].result.contains("rows"), "{}", outcomes[0].result);
}

#[tokio::test]
async fn sum_pos_and_sum_neg_tokens_split_the_flows() {
    let e = engine().await;
    let money = plain(&e, "money", 0.0).await;
    plain(&e, "inflow.mirror", 0.0).await;
    plain(&e, "outflow.mirror", 0.0).await;
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.inflow",
            head: "Inflow",
            condition: "sum_pos(@money, 30d)",
            gate: "!=0",
            carry: "value",
            consequences: vec![(
                ConsequenceKind::SetQuantity,
                Some("@inflow.mirror".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.outflow",
            head: "Outflow",
            condition: "sum_neg(@money, 30d)",
            gate: "!=0",
            carry: "value",
            consequences: vec![(
                ConsequenceKind::SetQuantity,
                Some("@outflow.mirror".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&money, 10.0).await.unwrap();
    e.append_user(&money, -4.0).await.unwrap();

    assert_eq!(quantity(&e, "inflow.mirror").await, 10.0);
    assert_eq!(quantity(&e, "outflow.mirror").await, -4.0);
}

#[tokio::test]
async fn quiet_hours_deactivates_a_noisy_rule() {
    let e = engine().await;
    let x = plain(&e, "x", 0.0).await;
    plain(&e, "alerts", 0.0).await;
    let quiet = plain(&e, "quiet-hours", 0.0).await;
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.noisy",
            head: "Noisy",
            condition: "@x",
            gate: "!=0",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::AddQuantity,
                Some("@alerts".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.quiet",
            head: "Quiet hours",
            condition: "@quiet-hours",
            gate: "!=0",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::Deactivate,
                Some("rules.noisy".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&x, 1.0).await.unwrap();
    assert_eq!(quantity(&e, "alerts").await, 1.0, "noisy fires while active");

    e.append_user(&quiet, 1.0).await.unwrap();
    assert_eq!(quantity(&e, "rules.noisy").await, 0.0, "quiet turned it off");

    e.append_user(&x, 1.0).await.unwrap();
    assert_eq!(quantity(&e, "alerts").await, 1.0, "silenced");
}

#[tokio::test]
async fn trust_ahead_advances_a_transfer_on_high_confidence() {
    let e = engine().await;
    plain(&e, "ana.apples", 10.0).await;
    let maria = plain(&e, "maria", 0.0).await;
    plain(&e, "trigger.ping", 0.0).await;

    // Maria's verified history: nine kept promises, zero broken -> Laplace
    // confidence (9+1)/(9+2) ~ 0.909 > 0.9
    for _ in 0..9 {
        store::misc::insert_promise(
            &e.store.pool,
            NewPromise {
                party_uid: Some(maria.clone()),
                record_uid: store::records::resolve(&e.store.pool, "ana.apples")
                    .await
                    .unwrap()
                    .map(|r| r.uid),
                delta: 1.0,
                state: Some(PromiseState::Kept),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    // An agreed bundle with Maria (individual agreement: always satisfied).
    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.apples".into()),
                head: "Apples".into(),
                agreement: "individual".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
                reserve_default: None,
                require_confirmation: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let promise = e
        .act(
            Action::AddPromiseToTransfer {
                transfer: transfer.clone(),
                record: "ana.apples".into(),
                delta: -5.0,
                party: "maria".into(),
                window_end: None,
                condition: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(
        Action::PromiseTransition {
            promise: promise.clone(),
            to: PromiseState::Agreed,
        },
        None,
    )
    .await
    .unwrap();

    // Trust-ahead (blueprint VI.3 worked example): high confidence advances
    // the transfer within policy.
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.trust-ahead",
            head: "Trust ahead",
            condition: &format!("@trigger.ping * confidence(@{promise})"),
            gate: ">0.9",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::AdvanceTransfer,
                Some("xfer.apples".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    let trigger = store::records::resolve(&e.store.pool, "trigger.ping")
        .await
        .unwrap()
        .unwrap()
        .uid;
    e.append_user(&trigger, 1.0).await.unwrap();

    assert_eq!(
        store::misc::promise_state(&e.store.pool, &promise)
            .await
            .unwrap()
            .unwrap(),
        PromiseState::Active,
        "the agreed promise moved to active on Maria's track record"
    );
}
