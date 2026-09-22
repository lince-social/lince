use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::{RuleFieldInput, RuleFieldKind};

mod support;

fn text(source: &str) -> RuleFieldInput {
    RuleFieldInput::Text {
        source: source.into(),
    }
}

async fn create(engine: &Engine, condition: RuleFieldInput, target: &str) -> String {
    engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: [condition, text(">0"), text(&format!("@{target}"))],
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn fields(engine: &Engine, rule: &str) -> Vec<store::karma_fields::Field> {
    store::karma_fields::for_rule(&engine.store.pool, rule)
        .await
        .unwrap()
}

async fn change(
    engine: &Engine,
    field: &store::karma_fields::Field,
    source: &str,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    engine
        .act(
            Action::ReviseKarmaField {
                field: field.uid.clone(),
                expected_revision: field.revision,
                source: source.into(),
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
}

#[tokio::test]
async fn linked_conditions_update_all_rules_and_detached_text_stays_independent() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 2.0).await;
    for target in ["first", "second", "copy"] {
        support::plain(&engine, target, 0.0).await;
    }
    let first = create(&engine, text("@source"), "first").await;
    let condition = fields(&engine, &first)
        .await
        .into_iter()
        .find(|field| field.kind == RuleFieldKind::Condition)
        .unwrap();
    let second = create(
        &engine,
        RuleFieldInput::Reference {
            uid: condition.uid.clone(),
            revision: condition.revision,
        },
        "second",
    )
    .await;
    let copy = create(&engine, text("@source"), "copy").await;
    change(&engine, &condition, "@source * 3").await.unwrap();
    for rule in [&first, &second] {
        assert_eq!(
            store::recurrence::get(&engine.store.pool, rule)
                .await
                .unwrap()
                .unwrap()
                .condition
                .unwrap()
                .source,
            "@source * 3"
        );
    }
    assert_eq!(
        store::recurrence::get(&engine.store.pool, &copy)
            .await
            .unwrap()
            .unwrap()
            .condition
            .unwrap()
            .source,
        "@source"
    );
    engine.append_user(&source, 1.0).await.unwrap();
    for (slug, expected) in [("first", "9"), ("second", "9"), ("copy", "3")] {
        let uid = store::records::resolve(&engine.store.pool, slug)
            .await
            .unwrap()
            .unwrap()
            .uid;
        assert_eq!(
            store::facts::level(&engine.store.pool, &uid)
                .await
                .unwrap()
                .to_string(),
            expected
        );
    }
    assert!(change(&engine, &condition, "@source * 4").await.is_err());
}

#[tokio::test]
async fn threshold_and_consequence_links_are_real_references_and_invalid_edits_are_atomic() {
    let engine = support::engine().await;
    support::plain(&engine, "source", 2.0).await;
    support::plain(&engine, "target", 0.0).await;
    let first = create(&engine, text("@source"), "target").await;
    let original = fields(&engine, &first).await;
    let linked = RuleFieldKind::ALL.map(|kind| {
        let field = original.iter().find(|field| field.kind == kind).unwrap();
        RuleFieldInput::Reference {
            uid: field.uid.clone(),
            revision: field.revision,
        }
    });
    let second = engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: linked,
                request_id: "linked".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let threshold = original
        .iter()
        .find(|field| field.kind == RuleFieldKind::Threshold)
        .unwrap();
    assert!(change(&engine, threshold, "not a threshold").await.is_err());
    assert_eq!(
        store::karma_fields::get(&engine.store.pool, &threshold.uid)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    change(&engine, threshold, ">=10").await.unwrap();
    let consequence = original
        .iter()
        .find(|field| field.kind == RuleFieldKind::Consequence)
        .unwrap();
    change(&engine, consequence, "@target += 0.000001")
        .await
        .unwrap();
    for uid in [first, second] {
        let rule = store::recurrence::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rule.condition.unwrap().gate.as_text(), ">=10");
        assert_eq!(
            rule.consequences.declared_delta().unwrap().to_string(),
            "0.000001"
        );
        assert_eq!(rule.revision, 3);
    }
}

#[tokio::test]
async fn replacing_a_link_preserves_other_readers_and_stale_saves_do_not_write() {
    let engine = support::engine().await;
    support::plain(&engine, "source", 2.0).await;
    support::plain(&engine, "target", 0.0).await;
    let first = create(&engine, text("@source"), "target").await;
    let fields = fields(&engine, &first).await;
    let condition = fields
        .iter()
        .find(|field| field.kind == RuleFieldKind::Condition)
        .unwrap();
    let second = create(
        &engine,
        RuleFieldInput::Reference {
            uid: condition.uid.clone(),
            revision: 1,
        },
        "target",
    )
    .await;
    let action = Action::SaveKarmaRule {
        rule: Some(second.clone()),
        expected_revision: Some(1),
        fields: [text("@source * 5"), text(">0"), text("@target = result")],
        request_id: "detach".into(),
    };
    engine.act(action.clone(), None).await.unwrap();
    engine.act(action, None).await.unwrap();
    assert_eq!(
        store::karma_fields::readers(&engine.store.pool, &condition.uid)
            .await
            .unwrap(),
        vec![first.clone()]
    );
    assert!(
        engine
            .act(
                Action::SaveKarmaRule {
                    rule: Some(second.clone()),
                    expected_revision: Some(1),
                    fields: [text("@source"), text("always"), text("@target = 0")],
                    request_id: "stale".into(),
                },
                None
            )
            .await
            .is_err()
    );
    change(&engine, condition, "@source * 2").await.unwrap();
    assert_eq!(
        store::recurrence::get(&engine.store.pool, &second)
            .await
            .unwrap()
            .unwrap()
            .condition
            .unwrap()
            .source,
        "@source * 5"
    );
}

#[tokio::test]
async fn protein_projects_three_shared_fields_without_expanding_occurrences() {
    let engine = support::engine().await;
    support::plain(&engine, "source", 2.0).await;
    support::plain(&engine, "target", 0.0).await;
    let rule = create(&engine, text("@source"), "target").await;
    let query: protein::Protein =
        serde_json::from_value(serde_json::json!({"source":"karma_rule"})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], rule);
    assert_eq!(rows[0]["fields"].as_array().unwrap().len(), 3);
    assert!(
        rows[0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field["source"] == "@target")
    );
    let outsider = support::plain(&engine, "outsider", 0.0).await;
    assert!(
        protein::execute_for(&engine.store, &query, Some(&outsider))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn unknown_references_and_field_kind_confusion_are_rejected_without_partial_rules() {
    let engine = support::engine().await;
    support::plain(&engine, "target", 0.0).await;
    for fields in [
        [text("@missing"), text("always"), text("@target")],
        [text("@target"), text("always"), text("@missing")],
        [text("@target"), text("always"), text("@missing = 3")],
    ] {
        assert!(
            engine
                .act(
                    Action::SaveKarmaRule {
                        rule: None,
                        expected_revision: None,
                        fields,
                        request_id: nucleus::new_uid("request")
                    },
                    None
                )
                .await
                .is_err()
        );
    }
    assert!(
        store::recurrence::all(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    let first = create(&engine, text("@target"), "target").await;
    let threshold = fields(&engine, &first)
        .await
        .into_iter()
        .find(|field| field.kind == RuleFieldKind::Threshold)
        .unwrap();
    assert!(
        engine
            .act(
                Action::SaveKarmaRule {
                    rule: None,
                    expected_revision: None,
                    fields: [
                        RuleFieldInput::Reference {
                            uid: threshold.uid,
                            revision: 1
                        },
                        text("always"),
                        text("@target = 3")
                    ],
                    request_id: "wrong-kind".into()
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::recurrence::all(&engine.store.pool)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn hover_uses_the_runtime_evaluator_without_enacting_consequences() {
    let engine = support::engine().await;
    support::plain(&engine, "source", 7.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    create(&engine, text("@source * 3"), "target").await;
    let result = engine
        .act(
            Action::PreviewKarmaReading {
                source: "value(@target)".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.data.unwrap()["value"], "21");
    assert!(result.facts.is_empty());
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "0"
    );
}

#[tokio::test]
async fn castle_commands_still_require_organ_authority() {
    let engine = support::engine().await;
    let person = support::person(&engine, "author").await;
    let role = store::auth::ensure_role(&engine.store.pool, "castle-author")
        .await
        .unwrap();
    for (subject, action) in [
        ("frequency", "create"),
        ("record", "update"),
        ("record", "read"),
    ] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(
        &engine.store.pool,
        &person.uid,
        "castle-author",
        "hash",
        role,
    )
    .await
    .unwrap();
    let target = support::plain(&engine, "target", 0.0).await;
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &target)
        .await
        .unwrap();
    let result = engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: [
                    text("@target"),
                    text("always"),
                    text("@target: command(\"printf denied\")"),
                ],
                request_id: "no-command-permission".into(),
            },
            Some(person.uid),
        )
        .await
        .unwrap_err();
    assert!(result.to_string().contains("organ:update"), "{result}");
    assert!(
        store::recurrence::all(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store::misc::due_effects(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn frequency_projection_reports_the_actual_next_scheduler_boundary() {
    let engine = support::engine().await;
    support::plain(&engine, "target", 0.0).await;
    let now = chrono::DateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis() + 100)
        .unwrap();
    let first = now + chrono::TimeDelta::seconds(1);
    engine
        .act(
            Action::CreateFrequency {
                slug: "pulse".into(),
                head: None,
                every: nucleus::karma::CadenceStep {
                    seconds: 1,
                    ..Default::default()
                },
                anchor_at: Some(first.to_rfc3339()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap();
    create(&engine, text("freq(@pulse)"), "target").await;
    engine.advance_karma_time(now).await.unwrap();
    let query = serde_json::from_value(serde_json::json!({"source":"frequency"})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows[0]["next_at_ms"], first.timestamp_millis());
    engine.advance_karma_time(first).await.unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(
        rows[0]["next_at_ms"],
        (first + chrono::TimeDelta::seconds(1)).timestamp_millis()
    );
}

async fn brushing_rule(
    engine: &Engine,
    quantity: f64,
    threshold: &str,
) -> (String, chrono::DateTime<chrono::Utc>) {
    let target = support::plain(engine, "escovar-dentes", quantity).await;
    let start =
        chrono::DateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis() + 100)
            .unwrap();
    let first = start + chrono::TimeDelta::seconds(1);
    engine
        .act(
            Action::CreateFrequency {
                slug: "daily".into(),
                head: None,
                every: nucleus::karma::CadenceStep {
                    days: 1,
                    ..Default::default()
                },
                anchor_at: Some(first.to_rfc3339()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap();
    let rule = engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: [
                    text("-1 * freq(@daily) + @escovar-dentes"),
                    text(threshold),
                    text("@escovar-dentes"),
                ],
                request_id: nucleus::new_uid("bare-consequence"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let stored = store::recurrence::get(&engine.store.pool, &rule)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        stored.consequences.iter().next(),
        Some(nucleus::karma::Consequence::SetQuantity { value: None })
    ));
    engine.advance_karma_time(start).await.unwrap();
    (target, first)
}

#[tokio::test]
async fn bare_record_consequence_enacts_the_conditions_final_quantity_once_per_beat() {
    let engine = support::engine().await;
    let (target, first) = brushing_rule(&engine, 0.0, "!=0").await;
    for (at, expected) in [
        (first, "-1"),
        (first, "-1"),
        (first + chrono::TimeDelta::days(1), "-2"),
    ] {
        engine.advance_karma_time(at).await.unwrap();
        assert_eq!(
            store::facts::level(&engine.store.pool, &target)
                .await
                .unwrap()
                .to_string(),
            expected
        );
    }
    engine.append_user(&target, 1.0).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
    engine
        .advance_karma_time(first + chrono::TimeDelta::days(2))
        .await
        .unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-2"
    );
    let failed: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'failed'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(failed, 0);
}

#[tokio::test]
async fn bare_record_consequence_still_obeys_the_threshold_when_the_result_is_zero() {
    for (threshold, expected) in [("!=0", "1"), ("always", "0")] {
        let engine = support::engine().await;
        let (target, first) = brushing_rule(&engine, 1.0, threshold).await;
        engine.advance_karma_time(first).await.unwrap();
        assert_eq!(
            store::facts::level(&engine.store.pool, &target)
                .await
                .unwrap()
                .to_string(),
            expected
        );
    }
}

#[tokio::test]
async fn bare_record_destination_does_not_bypass_record_write_permissions() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "protected", 5.0).await;
    let person = support::person(&engine, "rule-reader").await;
    let role = store::auth::ensure_role(&engine.store.pool, "rule-reader")
        .await
        .unwrap();
    for (subject, action) in [("frequency", "create"), ("record", "read")] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &person.uid, "rule-reader", "hash", role)
        .await
        .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &target)
        .await
        .unwrap();
    let error = engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: [text("@protected - 1"), text("always"), text("@protected")],
                request_id: "bare-record-no-write".into(),
            },
            Some(person.uid),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("record:update"), "{error}");
    assert!(
        store::recurrence::all(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "5"
    );
}

#[tokio::test]
async fn private_dependencies_cannot_be_revealed_through_a_derived_hover() {
    let engine = support::engine().await;
    let person = support::person(&engine, "reader").await;
    let role = store::auth::ensure_role(&engine.store.pool, "hover-reader")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_credential(
        &engine.store.pool,
        &person.uid,
        "hover-reader",
        "hash",
        role,
    )
    .await
    .unwrap();
    let secret = support::plain(&engine, "secret", 17.0).await;
    let public = support::plain(&engine, "public", 0.0).await;
    create(&engine, text("@secret * 2"), "public").await;
    store::sqlx::query("DELETE FROM visibility_rule WHERE target_uid = ?")
        .bind(&secret)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &public)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::PreviewKarmaReading {
                    source: "value(@public)".into()
                },
                Some(person.uid)
            )
            .await
            .is_err()
    );
}
