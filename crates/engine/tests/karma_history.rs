use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;
use serde_json::Value;

mod support;

fn text(source: &str) -> RuleFieldInput {
    RuleFieldInput::Text {
        source: source.into(),
    }
}

async fn rule(engine: &Engine, source: &str, gate: &str, target: &str) -> String {
    engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                identity: None,
                fields: [text(source), text(gate), text(&format!("@{target}"))],
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn history(engine: &Engine, rule: &str) -> Value {
    engine
        .act(
            Action::InspectKarmaRuleHistory {
                rule: rule.into(),
                limit: 10,
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn set(engine: &Engine, target: &str, amount: &str) {
    engine
        .act(
            Action::SetQuantityExact {
                target: target.into(),
                amount: amount.into(),
            },
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn an_execution_limit_records_only_the_committed_evaluation() {
    let engine = support::karma::engine().await;
    let source = support::plain(&engine, "source", 0.0).await;
    let uid = rule(&engine, "@source + 1", "always", "source").await;
    let execution = nucleus::execution::Execution::new([3; 32], 1_893_456_000_000)
        .unwrap()
        .controlled(
            nucleus::execution::control::Control::new(1, None, None),
            "history".into(),
        );
    let result = execution
        .scope(engine.act(
            Action::SetQuantityExact {
                target: source,
                amount: "1".into(),
            },
            None,
        ))
        .await;
    assert!(matches!(
        result,
        Err(engine::EngineError::ExecutionLimit(
            nucleus::execution::control::Limit::Evaluations
        ))
    ));
    let orphaned: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM karma_rule_evidence e LEFT JOIN karma_rule_application a ON a.event_id = e.event_id AND a.rule_uid = e.rule_uid AND a.rule_revision = e.rule_revision AND a.attempt = e.attempt WHERE a.event_id IS NULL").fetch_one(&engine.store.pool).await.unwrap();
    assert_eq!(orphaned, 0);
    let result = history(&engine, &uid).await;
    assert_eq!(result["applications"].as_array().unwrap().len(), 1);
    assert_eq!(result["applications"][0]["status"], "applied");
    assert_eq!(
        result["applications"][0]["evidence"]["evaluation"]["computed"]["value"],
        "2"
    );
}

#[tokio::test]
async fn historical_decisions_keep_actual_values_after_later_edits() {
    let engine = support::karma::engine().await;
    let source = support::plain(&engine, "source", 0.0).await;
    support::plain(&engine, "target", 0.0).await;
    let uid = rule(&engine, "@source * 2", ">5", "target").await;
    set(&engine, &source, "2").await;
    let blocked = history(&engine, &uid).await;
    assert_eq!(blocked["applications"][0]["status"], "blocked");
    let evaluation = &blocked["applications"][0]["evidence"]["evaluation"];
    assert_eq!(evaluation["computed"]["value"], "4");
    assert_eq!(evaluation["gate_passed"], false);
    assert!(evaluation["carried"].is_null());
    set(&engine, &source, "4").await;
    let applied = history(&engine, &uid).await;
    assert_eq!(applied["applications"][0]["status"], "applied");
    assert_eq!(
        applied["applications"][0]["evidence"]["evaluation"]["carried"]["value"],
        "8"
    );
    let stored = store::recurrence::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: uid.clone(),
                expected_revision: stored.revision,
                paused: true,
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap();
    set(&engine, &source, "100").await;
    assert_eq!(history(&engine, &uid).await, applied);
}

#[tokio::test]
async fn failed_readings_preserve_partial_evidence_and_the_reason() {
    let engine = support::karma::engine().await;
    let source = support::plain(&engine, "source", 0.0).await;
    support::plain(&engine, "target", 0.0).await;
    let uid = rule(
        &engine,
        "@source + extension(@source, \"inventory\", \"price\")",
        "always",
        "target",
    )
    .await;
    set(&engine, &source, "3").await;
    let result = history(&engine, &uid).await;
    let application = &result["applications"][0];
    assert_eq!(application["status"], "failed");
    assert!(
        application["reason"]
            .as_str()
            .unwrap()
            .contains("extension")
    );
    let readings = application["evidence"]["evaluation"]["readings"]
        .as_array()
        .unwrap();
    assert!(
        readings
            .iter()
            .any(|reading| reading["value"]["value"] == "3")
    );
    assert!(readings.iter().any(|reading| reading["error"].is_string()));
    assert!(application["evidence"]["evaluation"]["computed"].is_null());
}

#[tokio::test]
async fn an_unreadable_threshold_preserves_the_successful_condition_value() {
    let engine = support::karma::engine().await;
    let source = support::plain(&engine, "source", 0.0).await;
    support::plain(&engine, "target", 0.0).await;
    let uid = rule(&engine, "@source", ">0.000000000001", "target").await;
    let value = "1000000000000000000000000000000";
    set(&engine, &source, value).await;
    let result = history(&engine, &uid).await;
    let application = &result["applications"][0];
    assert_eq!(application["status"], "failed");
    assert_eq!(
        application["evidence"]["evaluation"]["computed"]["value"],
        value
    );
    assert!(application["evidence"]["evaluation"]["gate_passed"].is_null());
}

#[tokio::test]
async fn nested_input_history_stays_private_after_the_current_rule_changes() {
    let engine = support::karma::engine().await;
    let secret = support::plain(&engine, "secret", 0.0).await;
    support::plain(&engine, "derived", 0.0).await;
    support::plain(&engine, "target", 0.0).await;
    let inner = rule(&engine, "@secret * 2", "always", "derived").await;
    let outer = rule(&engine, "value(@derived)", "always", "target").await;
    set(&engine, &secret, "9").await;
    let original = history(&engine, &outer).await;
    assert!(
        original["applications"][0]["evidence"]["inputs"]
            .as_array()
            .unwrap()
            .contains(&Value::String(secret.clone()))
    );
    assert!(
        original["applications"][0]["evidence"]["evaluation"]["readings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reading| reading["depth"] == 1 && reading["value"]["value"] == "9")
    );
    let field = store::karma_fields::for_rule(&engine.store.pool, &inner)
        .await
        .unwrap()
        .into_iter()
        .find(|field| field.kind == nucleus::karma::rule_field::RuleFieldKind::Condition)
        .unwrap();
    engine
        .act(
            Action::ReviseKarmaField {
                field: field.uid,
                expected_revision: field.revision,
                source: "0 * @derived".into(),
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap();
    let actor = support::person(&engine, "reader").await;
    let role = store::auth::ensure_role(&engine.store.pool, "history-reader")
        .await
        .unwrap();
    for resource in ["record", "frequency"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, resource, "read")
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(
        &engine.store.pool,
        &actor.uid,
        "history-reader",
        "hash",
        role,
    )
    .await
    .unwrap();
    for uid in [&outer, &secret] {
        store::sqlx::query("DELETE FROM visibility_rule WHERE target_uid = ?")
            .bind(uid)
            .execute(&engine.store.pool)
            .await
            .unwrap();
    }
    store::sqlx::query("INSERT INTO visibility_rule(uid, subject_kind, subject_uid, target_uid, grant_level) VALUES (?, 'actor', ?, ?, 'hidden')")
        .bind(nucleus::new_uid("v")).bind(&actor.uid).bind(&secret).execute(&engine.store.pool).await.unwrap();
    store::visibility::grant(&engine.store.pool, "public", None, &outer)
        .await
        .unwrap();
    let target = store::records::resolve(&engine.store.pool, "target")
        .await
        .unwrap()
        .unwrap();
    store::visibility::grant(&engine.store.pool, "public", None, &target.uid)
        .await
        .unwrap();
    let denied = engine
        .act(
            Action::InspectKarmaRuleHistory {
                rule: outer,
                limit: 10,
            },
            Some(actor.uid),
        )
        .await;
    assert!(matches!(denied, Err(engine::EngineError::Forbidden(_))));
}
