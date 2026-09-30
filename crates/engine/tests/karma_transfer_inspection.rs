mod support;

use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;
use nucleus::transfer::AgreementType;
use support::{DraftOptions, Person, TransferFixture};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-inspection", || async move {
        nucleus::execution::Execution::new([39; 32], 1_893_456_000_000)
            .unwrap()
            .scope(test)
            .await;
    });
}

async fn fixture() -> (Engine, Person, Person, TransferFixture) {
    let engine = support::engine().await;
    let a = support::person(&engine, "a").await;
    let b = support::person(&engine, "b").await;
    let item = support::plain(&engine, "item", 10.0).await;
    let transfer = support::create_transfer(
        &engine,
        &a,
        &[b.clone()],
        vec![
            support::promise("promise-a", &item, &a, -1.0),
            support::promise("promise-b", &item, &b, 1.0),
        ],
        DraftOptions {
            slug: "trade",
            agreement: AgreementType::Full,
            reserve_default: engine::actions::TransferReservePoint::Active,
            require_confirmation: true,
        },
    )
    .await;
    engine.set_signer(a.signer.clone()).await.unwrap();
    (engine, a, b, transfer)
}

async fn save(engine: &Engine, effect: &str) -> String {
    engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                identity: None,
                fields: ["agreement_level(@trade, @a)", "always", effect].map(|source| {
                    RuleFieldInput::Text {
                        source: source.into(),
                    }
                }),
                request_id: nucleus::new_uid("rule"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

#[test]
fn inspection_shows_fixed_calculated_and_pending_own_work_without_mutation() {
    run(async {
        let (engine, a, _, transfer) = fixture().await;
        let fixed = save(&engine, "@trade: agreement(@a, 2)").await;
        let calculated = save(&engine, "@trade: agreement(@a)").await;
        let zero = save(&engine, "@trade: agreement(@a, 0)").await;
        let before = engine.store.state_hash().await.unwrap();
        let data = engine
            .act(
                Action::InspectTransferKarma {
                    transfer: transfer.transfer.clone(),
                    person: Some(a.uid),
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let rules = data["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 3);
        assert!(rules.iter().any(|rule| rule["uid"] == fixed
            && rule["effects"][0]["target"] == 2
            && rule["effects"][0]["can_raise_to_maximum"] == true));
        assert!(rules.iter().any(|rule| rule["uid"] == calculated
            && rule["effects"][0]["target"] == 0
            && rule["effects"][0]["fixed"] == false));
        assert!(
            rules
                .iter()
                .any(|rule| rule["uid"] == zero
                    && rule["effects"][0]["can_raise_to_maximum"] == false)
        );
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: fixed.clone(),
                    expected_revision: 1,
                    request_id: "pause".into(),
                    paused: true,
                },
                None,
            )
            .await
            .unwrap();
        let data = engine
            .act(
                Action::InspectTransferKarma {
                    transfer: transfer.transfer,
                    person: None,
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(
            data["rules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|rule| rule["uid"] == fixed
                    && rule["paused"] == true
                    && rule["revision"] == 2)
        );
    });
}

#[test]
fn inspection_cannot_select_another_persons_agreement_automation() {
    run(async {
        let (engine, _, b, transfer) = fixture().await;
        save(&engine, "@trade: agreement(@a, 2)").await;
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(
                    Action::InspectTransferKarma {
                        transfer: transfer.transfer,
                        person: Some(b.uid)
                    },
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}
