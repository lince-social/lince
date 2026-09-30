#[path = "../../engine/tests/support/mod.rs"]
mod support;

use engine::{
    Engine,
    actions::Action,
    karma_preview::{Input, Limits, ProposedRule, Report, Request},
};
use nucleus::{karma::rule_field::RuleFieldInput, transfer::AgreementType};
use support::{DraftOptions, Person, TransferFixture};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-preview", || async move {
        nucleus::execution::Execution::new([44; 32], 1_893_456_000_000)
            .unwrap()
            .scope(test)
            .await;
    });
}

async fn fixture(ready: bool) -> (Engine, Person, TransferFixture) {
    let engine = support::engine().await;
    simulation::karma_preview::install(&engine).unwrap();
    let a = support::person(&engine, "a").await;
    let b = support::person(&engine, "b").await;
    let stock = support::plain(&engine, "stock", 10.0).await;
    let transfer = support::create_transfer(
        &engine,
        &a,
        &[b.clone()],
        vec![
            support::promise("promise-a", &stock, &a, -1.0),
            support::promise("promise-b", &stock, &b, 1.0),
        ],
        DraftOptions {
            slug: "trade",
            agreement: AgreementType::Full,
            reserve_default: engine::actions::TransferReservePoint::Active,
            require_confirmation: true,
        },
    )
    .await;
    if ready {
        for person in [&a, &b] {
            engine.set_signer(person.signer.clone()).await.unwrap();
            engine
                .act(
                    Action::AssignTransferAgreementLevel {
                        transfer: transfer.transfer.clone(),
                        expected_revision: transfer.revision,
                        request_id: nucleus::new_uid("agree"),
                        person: Some(person.uid.clone()),
                        level: 2,
                        expected: None,
                        expected_state: None,
                    },
                    None,
                )
                .await
                .unwrap();
        }
    }
    engine.set_signer(a.signer.clone()).await.unwrap();
    (engine, a, transfer)
}

fn request(condition: &str, consequence: &str, repeated: bool) -> Request {
    Request {
        proposals: vec![ProposedRule {
            identity: None,
            rule: None,
            expected_revision: None,
            fields: [condition, "always", consequence].map(|source| RuleFieldInput::Text {
                source: source.into(),
            }),
        }],
        limits: Limits {
            horizon_ms: 2000,
            rule_evaluations: 50,
            ..Default::default()
        },
        inputs: if repeated {
            vec![
                Input::Occurrence {
                    after_ms: 0,
                    proposal: 0,
                },
                Input::Occurrence {
                    after_ms: 0,
                    proposal: 0,
                },
            ]
        } else {
            vec![Input::Occurrence {
                after_ms: 0,
                proposal: 0,
            }]
        },
        records: vec!["trade".into()],
        quantity_basis: Default::default(),
        checks: Vec::new(),
        saved_checks: None,
        checks_start_ms: None,
        checking: Default::default(),
    }
}

async fn preview(engine: &Engine, request: Request) -> Report {
    let before = engine.store.state_hash().await.unwrap();
    let report = serde_json::from_value(
        engine
            .act(Action::PreviewKarmaProposal { request }, None)
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap();
    assert_eq!(engine.store.state_hash().await.unwrap(), before);
    report
}

#[test]
fn frequency_gated_agreement_advances_once_per_occurrence_and_stops_at_maximum() {
    run(async {
        let (engine, a, _) = fixture(false).await;
        engine
            .act(
                Action::CreateFrequency {
                    slug: "step".into(),
                    head: None,
                    every: nucleus::karma::CadenceStep {
                        seconds: 1,
                        ..Default::default()
                    },
                    anchor_at: Some("2030-01-01T00:00:01Z".into()),
                    request_id: Some("agreement-step".into()),
                },
                None,
            )
            .await
            .unwrap();
        for (horizon_ms, expected) in [(1500, 1), (3500, 2)] {
            let mut input = request(
                "freq(@step) * (1 * (agreement_level(@trade, @a) == 0) + 2 * (agreement_level(@trade, @a) == 1))",
                "@trade: agreement(@a)",
                false,
            );
            input.proposals[0].fields[1] = RuleFieldInput::Text {
                source: "> 0".into(),
            };
            input.inputs.clear();
            input.limits.horizon_ms = horizon_ms;
            let report = preview(&engine, input).await;
            assert!(!report.incomplete, "{report:?}");
            assert_eq!(
                report.final_values[0]
                    .transfer
                    .as_ref()
                    .unwrap()
                    .participants[&a.uid]
                    .guard
                    .level,
                expected,
                "{report:?}"
            );
        }
        assert_eq!(
            engine
                .read_karma_transfer_state("trade", None)
                .await
                .unwrap()
                .participants[&a.uid]
                .guard
                .level,
            0
        );
    });
}

#[test]
fn calculated_agreements_use_the_real_signer_and_report_transfer_changes_in_feedback() {
    run(async {
        let (engine, a, transfer) = fixture(false).await;
        let report = preview(
            &engine,
            request(
                "1 * (agreement_level(@trade, @a) < 1) + 2 * (agreement_level(@trade, @a) >= 1)",
                "@trade: agreement(@a)",
                false,
            ),
        )
        .await;
        assert!(!report.incomplete, "{report:?}");
        let state = report.final_values[0].transfer.as_ref().unwrap();
        assert_eq!(state.transfer, transfer.transfer);
        assert_eq!(state.participants[&a.uid].guard.level, 2);
        assert!(report.final_values[0].quantity.is_none());
        assert!(
            report
                .cycles
                .iter()
                .flat_map(|cycle| &cycle.steps)
                .flat_map(|step| &step.transfer_changes)
                .any(|change| change.after.participants[&a.uid].guard.level == 2),
            "{report:?}"
        );
    });
}

#[test]
fn fixed_zero_is_applied_and_invalid_calculated_levels_are_refused() {
    run(async {
        let (engine, a, _) = fixture(true).await;
        let zero = preview(
            &engine,
            request(
                "agreement_level(@trade, @a)",
                "@trade: agreement(@a, 0)",
                true,
            ),
        )
        .await;
        assert!(!zero.incomplete, "{zero:?}");
        assert_eq!(
            zero.final_values[0].transfer.as_ref().unwrap().participants[&a.uid]
                .guard
                .level,
            0
        );
        let invalid = preview(
            &engine,
            request(
                "0 * agreement_level(@trade, @a) + 1.5",
                "@trade: agreement(@a)",
                false,
            ),
        )
        .await;
        assert!(invalid.incomplete, "{invalid:?}");
        assert_eq!(
            invalid.final_values[0]
                .transfer
                .as_ref()
                .unwrap()
                .participants[&a.uid]
                .guard
                .level,
            2,
            "{invalid:?}"
        );
    });
}

#[test]
fn repeated_publication_and_explicit_fulfillment_keep_the_same_transfer() {
    run(async {
        let (engine, _, transfer) = fixture(true).await;
        let published = preview(
            &engine,
            request("transfer_published(@trade)", "@trade: publish(@a)", true),
        )
        .await;
        assert!(!published.incomplete, "{published:?}");
        let state = published.final_values[0].transfer.as_ref().unwrap();
        assert!(state.published);
        assert_eq!(state.revision, transfer.revision + 1);
        let activated = preview(
            &engine,
            request(
                "transfer_ready(@trade)",
                "@trade: activate(@a, @promise-a, \"purchase\")",
                true,
            ),
        )
        .await;
        assert!(!activated.incomplete, "{activated:?}");
        assert!(activated.final_values[0].transfer.as_ref().unwrap().active);
        assert_eq!(
            activated.final_values[0]
                .transfer
                .as_ref()
                .unwrap()
                .transfer,
            transfer.transfer
        );
    });
}

#[test]
fn delayed_stage_uses_the_prior_actual_change_and_the_copied_scheduler() {
    run(async {
        let (engine, a, transfer) = fixture(false).await;
        engine
            .act(
                Action::AssignTransferAgreementLevel {
                    transfer: transfer.transfer,
                    expected_revision: transfer.revision,
                    request_id: "checked".into(),
                    person: Some(a.uid.clone()),
                    level: 1,
                    expected: None,
                    expected_state: None,
                },
                None,
            )
            .await
            .unwrap();
        let report = preview(
            &engine,
            request(
                "agreement_level(@trade, @a)",
                "@trade: agreement(@a, 2, 1s)",
                false,
            ),
        )
        .await;
        assert!(!report.incomplete, "{report:?}");
        assert_eq!(
            report.final_values[0]
                .transfer
                .as_ref()
                .unwrap()
                .participants[&a.uid]
                .guard
                .level,
            2,
            "{report:?}"
        );
    });
}
