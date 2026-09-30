use engine::actions::Action;
use nucleus::{
    karma::rule_field::RuleFieldInput,
    simulation::{Predicate, Quantity, ReplayStatus, Verdict},
};
use simulation::scenario::{Event, Input, Invocation};

#[tokio::test]
async fn extension_changes_use_the_shared_runtime_and_replay_exactly() {
    let mut scenario = simulation::fixtures::daily();
    scenario.name = "extension-price".into();
    scenario.cells[0].seed.truncate(1);
    scenario.end_ms = scenario.start_ms + 1000;
    scenario.inputs.clear();
    let invocation = |id: &str, action| Invocation {
        id: id.into(),
        actor: None,
        action,
    };
    scenario.cells[0].seed.extend([
        invocation(
            "total",
            Action::CreateRecord {
                slug: Some("total".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Total".into(),
                body: String::new(),
                quantity: 0.0,
            },
        ),
        invocation(
            "price",
            Action::SetExtension {
                target: "stock".into(),
                namespace: "shop.inventory".into(),
                fds: serde_json::json!({"price":1.25}),
            },
        ),
        invocation(
            "rule",
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [
                    "@stock * extension(@stock, \"shop.inventory\", \"price\")",
                    "always",
                    "@total",
                ]
                .map(|source| RuleFieldInput::Text {
                    source: source.into(),
                }),
                request_id: "rule".into(),
            },
        ),
    ]);
    scenario.inputs.push(Input {
        id: "new-price".into(),
        at_ms: scenario.start_ms + 500,
        cell: "a".into(),
        event: Event::Action {
            invocation: invocation(
                "new-price",
                Action::SetExtension {
                    target: "stock".into(),
                    namespace: "shop.inventory".into(),
                    fds: serde_json::json!({"price":2.75}),
                },
            ),
        },
    });
    scenario.checks.truncate(1);
    scenario.checks[0].predicate = Predicate::QuantityEquals {
        cell: "a".into(),
        record: "total".into(),
        at_ms: scenario.end_ms,
        expected: Quantity {
            value: nucleus::DecimalValue::parse_inferred("27.5").unwrap(),
            unit: None,
        },
    };
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(scenario, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.findings);
    assert!(matches!(
        simulation::artifacts::replay(&run.directory, &directory.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
}
