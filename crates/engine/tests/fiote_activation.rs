use engine::actions::Action;
mod support;

#[tokio::test]
async fn karma_activation_carries_each_occurrence_value_without_changing_the_fiote() {
    let engine = support::karma::engine().await;
    support::plain(&engine, "source", 0.0).await;
    let fiote = support::plain(&engine, "fiote", 9.0).await;
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
    let field = |source: &str| nucleus::karma::rule_field::RuleFieldInput::Text {
        source: source.into(),
    };
    engine
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [
                    field("@source"),
                    field("!=0"),
                    field("@fiote: activate-fiote"),
                ],
                request_id: "wake-rule".into(),
            },
            None,
        )
        .await
        .unwrap();
    for value in ["2.5", "-4"] {
        engine
            .act(
                Action::SetQuantityExact {
                    target: "source".into(),
                    amount: value.into(),
                },
                None,
            )
            .await
            .unwrap();
        let outcomes = engine.run_due_effects().await.unwrap();
        assert!(outcomes.iter().all(|outcome| outcome.ok), "{outcomes:?}");
    }
    let requests = engine.fiote_activations(&fiote).await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        nucleus::DecimalValue::parse_inferred(&requests[0].value)
            .unwrap()
            .exact_numeric_cmp(store::exact::integer(-4)),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        nucleus::DecimalValue::parse_inferred(&requests[1].value)
            .unwrap()
            .exact_numeric_cmp(nucleus::DecimalValue::parse_inferred("2.5").unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(requests[0].cause["kind"], "karma");
    assert_ne!(requests[0].request_id, requests[1].request_id);
    assert_eq!(
        store::records::get(&engine.store.pool, &fiote)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "9"
    );
}

#[tokio::test]
async fn activation_requests_are_durable_idempotent_and_leave_quantities_unchanged() {
    let engine = support::engine().await;
    let fiote = support::plain(&engine, "fiote", 9.0).await;
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
    let request = Action::ActivateFiote {
        target: fiote.clone(),
        value: "3.25".into(),
        request_id: "occurrence-one".into(),
    };
    assert_eq!(
        engine
            .act(request.clone(), None)
            .await
            .unwrap()
            .data
            .unwrap()["state"],
        "queued"
    );
    assert_eq!(
        engine.act(request, None).await.unwrap().data.unwrap()["duplicate"],
        true
    );
    assert!(
        engine
            .act(
                Action::ActivateFiote {
                    target: fiote.clone(),
                    value: "2".into(),
                    request_id: "occurrence-one".into()
                },
                None
            )
            .await
            .is_err()
    );
    for value in ["0", "0.0", "-0.00"] {
        assert_eq!(
            engine
                .act(
                    Action::ActivateFiote {
                        target: fiote.clone(),
                        value: value.into(),
                        request_id: format!("zero:{value}")
                    },
                    None
                )
                .await
                .unwrap()
                .data
                .unwrap()["activated"],
            false
        );
    }
    let activations = engine.fiote_activations(&fiote).await.unwrap();
    assert_eq!(activations.len(), 1);
    assert_eq!(activations[0].value, "3.25");
    assert_eq!(
        store::records::get(&engine.store.pool, &fiote)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "9"
    );
    engine.register_fiote_availability(&fiote, false);
    assert!(
        engine
            .act(
                Action::ActivateFiote {
                    target: fiote,
                    value: "1".into(),
                    request_id: "disabled".into()
                },
                None
            )
            .await
            .is_err()
    );
}
