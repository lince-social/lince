use chrono::Utc;
use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;

mod support;

async fn rule(engine: &Engine, condition: &str, consequence: &str) -> String {
    let field = |source: &str| RuleFieldInput::Text {
        source: source.into(),
    };
    engine
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [field(condition), field("!=0"), field(consequence)],
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

#[tokio::test]
async fn startup_activates_fiote_once_per_boot_and_survives_engine_reopen() {
    let engine = support::karma::engine().await;
    let fiote = support::plain(&engine, "coordinator", 9.0).await;
    engine
        .act(
            Action::ConfigureFiote {
                target: fiote.clone(),
                prompt_parent: None,
                run_assigned: false,
            },
            None,
        )
        .await
        .unwrap();
    engine.register_fiote_availability(&fiote, true);
    let rule_uid = rule(
        &engine,
        "startup(@local-cell)",
        "@coordinator: activate-fiote",
    )
    .await;
    let now = Utc::now();
    let preview = engine
        .act(
            Action::PreviewKarmaReading {
                source: "startup(@local-cell)".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(preview.data.unwrap()["value"], "0");
    engine.run_due_effects().await.unwrap();
    assert!(engine.fiote_activations(&fiote).await.unwrap().is_empty());

    let (first, duplicate) = tokio::join!(
        engine.fire_karma_startup("boot-one", now),
        engine.fire_karma_startup("boot-one", now),
    );
    first.unwrap();
    duplicate.unwrap();
    let engine = Engine::new(engine.store.clone()).await.unwrap();
    engine.register_fiote_availability(&fiote, true);
    assert!(
        engine
            .run_due_effects()
            .await
            .unwrap()
            .iter()
            .all(|outcome| outcome.ok)
    );
    let activations = engine.fiote_activations(&fiote).await.unwrap();
    assert_eq!(activations.len(), 1);
    assert_eq!(activations[0].value, "1");
    assert!(
        activations[0].cause["occurrence"]["event_id"]
            .as_str()
            .unwrap()
            .starts_with("startup:")
    );
    let history = engine
        .act(
            Action::InspectKarmaRuleHistory {
                rule: rule_uid,
                limit: 10,
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(history["applications"].as_array().unwrap().len(), 1);
    let reading = &history["applications"][0]["evidence"]["evaluation"]["readings"][0];
    assert_eq!(reading["function"], "startup");
    assert_eq!(reading["value"]["value"], "1");

    let reopened = Engine::new(engine.store.clone()).await.unwrap();
    reopened.register_fiote_availability(&fiote, true);
    reopened
        .fire_karma_startup("boot-one", Utc::now())
        .await
        .unwrap();
    reopened.run_due_effects().await.unwrap();
    assert_eq!(reopened.fiote_activations(&fiote).await.unwrap().len(), 1);
    reopened
        .fire_karma_startup("boot-two", Utc::now())
        .await
        .unwrap();
    assert!(
        reopened
            .run_due_effects()
            .await
            .unwrap()
            .iter()
            .all(|outcome| outcome.ok)
    );
    assert_eq!(reopened.fiote_activations(&fiote).await.unwrap().len(), 2);
    assert_eq!(
        store::records::get(&reopened.store.pool, &fiote)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "9"
    );
}

#[tokio::test]
async fn startup_preserves_pause_runtime_executor_and_reaction_guards() {
    let engine = support::karma::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let rule_uid = rule(&engine, "startup(@local-cell)", "@target += 1").await;
    let now = Utc::now();
    let revision = store::recurrence::get(&engine.store.pool, &rule_uid)
        .await
        .unwrap()
        .unwrap()
        .revision;
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: rule_uid.clone(),
                expected_revision: revision,
                request_id: "pause-startup".into(),
                paused: true,
            },
            None,
        )
        .await
        .unwrap();
    engine.fire_karma_startup("paused-boot", now).await.unwrap();
    let revision = store::recurrence::get(&engine.store.pool, &rule_uid)
        .await
        .unwrap()
        .unwrap()
        .revision;
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: rule_uid.clone(),
                expected_revision: revision,
                request_id: "resume-startup".into(),
                paused: false,
            },
            None,
        )
        .await
        .unwrap();
    store::cells::set_config(
        &engine.store.pool,
        "lince.karma-runtime",
        &serde_json::json!({"running": false}),
    )
    .await
    .unwrap();
    engine
        .fire_karma_startup("disabled-boot", now)
        .await
        .unwrap();
    store::cells::set_config(
        &engine.store.pool,
        "lince.karma-runtime",
        &serde_json::json!({"running": true}),
    )
    .await
    .unwrap();
    store::executor::designate(&engine.store.pool, &target, Some("another-cell"))
        .await
        .unwrap();
    engine
        .fire_karma_startup("wrong-executor", now)
        .await
        .unwrap();
    store::executor::designate(&engine.store.pool, &target, None)
        .await
        .unwrap();
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "0"
    );

    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    engine
        .act(
            Action::SetQuantityExact {
                target: cell.uid,
                amount: "5".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "0"
    );
    engine
        .fire_karma_startup("enabled-boot", now)
        .await
        .unwrap();
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "1"
    );
    assert!(engine.fire_karma_startup("", now).await.is_err());
    assert!(
        engine
            .fire_karma_startup("bad:identity", now)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn startup_requires_one_cell_reference() {
    let engine = support::karma::engine().await;
    support::plain(&engine, "ordinary", 0.0).await;
    for source in [
        "startup()",
        "startup(@local-cell, 1d)",
        "startup(@local-cell, @ordinary)",
    ] {
        assert!(
            nucleus::karma::Condition::parse(source).is_err(),
            "{source}"
        );
    }
    let field = |source: &str| RuleFieldInput::Text {
        source: source.into(),
    };
    assert!(
        engine
            .act(
                Action::SaveKarmaRule {
                    identity: None,
                    rule: None,
                    expected_revision: None,
                    fields: [
                        field("startup(@ordinary)"),
                        field("!=0"),
                        field("@ordinary += 1")
                    ],
                    request_id: "non-cell".into(),
                },
                None
            )
            .await
            .is_err()
    );
}
