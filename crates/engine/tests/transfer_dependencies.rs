mod support;

use engine::{
    Engine,
    actions::{
        Action, TransferDependencyInput, TransferDependencyScopeInput,
        TransferDependencyUpstreamKindInput, TransferDraftRevisionInput,
        TransferOccurrenceClaimRole, TransferPromiseInput, TransferReservePoint, TransferSatiation,
        TransferVisibility,
    },
};
use nucleus::{
    PromiseState,
    transfer::{AgreementType, disclosure::TransferItem},
};
use support::{DraftOptions, Person, TransferFixture};

fn dependency(upstream: &str, state: &str) -> TransferDependencyInput {
    TransferDependencyInput {
        uid: None,
        scope: TransferDependencyScopeInput::Transfer,
        promise: None,
        upstream_kind: TransferDependencyUpstreamKindInput::Transfer,
        upstream: upstream.into(),
        required_state: state.into(),
    }
}

fn revision(
    owner: &Person,
    fixture: &TransferFixture,
    promises: Vec<TransferPromiseInput>,
    dependencies: Vec<TransferDependencyInput>,
    request: &str,
    parent: Option<String>,
) -> Action {
    Action::ReviseTransferDraft {
        transfer: fixture.transfer.clone(),
        expected_revision: fixture.revision,
        request_id: request.into(),
        draft: TransferDraftRevisionInput {
            creator: owner.uid.clone(),
            slug: None,
            head: request.into(),
            agreement: AgreementType::Dependency,
            agreement_pct: None,
            satiation: TransferSatiation::None,
            parent,
            source: None,
            visibility: TransferVisibility::Hidden,
            max_proximity: None,
            reserve_default: TransferReservePoint::Active,
            require_confirmation: true,
            default_place: None,
            invitees: vec![],
            promises,
            dependencies,
        },
    }
}

async fn conditional(
    engine: &Engine,
    owner: &Person,
    invitees: &[Person],
    name: &str,
    upstream: &str,
    state: &str,
    record: Option<&str>,
) -> TransferFixture {
    let mut promise = support::promise(
        &format!("{name}-promise"),
        record.unwrap_or_default(),
        owner,
        if record.is_some() { -1.0 } else { 1.0 },
    );
    if record.is_none() {
        promise.item = Some(TransferItem {
            title: name.into(),
            ..Default::default()
        });
    }
    let mut fixture = support::create_transfer(
        engine,
        owner,
        invitees,
        vec![promise.clone()],
        DraftOptions {
            slug: name,
            agreement: AgreementType::Full,
            reserve_default: TransferReservePoint::Active,
            require_confirmation: true,
        },
    )
    .await;
    engine.set_signer(owner.signer.clone()).await.unwrap();
    engine
        .act(
            revision(
                owner,
                &fixture,
                vec![promise],
                vec![dependency(upstream, state)],
                &format!("requirements-{name}"),
                None,
            ),
            None,
        )
        .await
        .unwrap();
    fixture.revision = support::current_revision(engine, &fixture.transfer).await;
    fixture
}

async fn state(
    engine: &Engine,
    fixture: &TransferFixture,
) -> store::transfer_agreement::AgreementReadinessProjection {
    store::transfer_agreement::read(&engine.store.pool, &fixture.transfer)
        .await
        .unwrap()
        .0
}

#[test]
fn upstream_agreement_and_settlement_are_distinct_and_observation_has_no_resource_effect() {
    support::run_async_test("dependency-outcomes", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "outcome.ana").await;
        let beto = support::person(&engine, "outcome.beto").await;
        let dora = support::person(&engine, "outcome.dora").await;
        let stock = support::plain(&engine, "food", 10.0).await;
        let dora_stock = support::plain(&engine, "dora.resource", 7.0).await;
        let upstream = support::create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            vec![support::promise("food-promise", &stock, &ana, -1.0)],
            DraftOptions {
                slug: "food-transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        let agreement = conditional(
            &engine,
            &dora,
            &[],
            "agreement-observer",
            &upstream.transfer,
            "agreed",
            None,
        )
        .await;
        let settlement = conditional(
            &engine,
            &dora,
            &[],
            "settlement-observer",
            &upstream.transfer,
            "settled",
            None,
        )
        .await;
        assert!(!state(&engine, &agreement).await.ready);
        assert!(!state(&engine, &settlement).await.ready);
        for person in [&ana, &beto] {
            support::agree(&engine, &upstream, person, &format!("food:{}", person.uid)).await;
        }
        assert!(state(&engine, &agreement).await.ready);
        assert!(state(&engine, &agreement).await.observed);
        assert!(!state(&engine, &settlement).await.ready);
        let occurrence =
            support::activate(&engine, &upstream, &ana, "food-promise", "start-food").await;
        support::claim(
            &engine,
            &occurrence,
            &ana,
            TransferOccurrenceClaimRole::Delivery,
            "food-delivered",
        )
        .await;
        support::claim(
            &engine,
            &occurrence,
            &beto,
            TransferOccurrenceClaimRole::Receipt,
            "food-received",
        )
        .await;
        let preview = support::settlement_preview(&engine, &occurrence, &ana, 1.0).await;
        support::settle_from_preview(&engine, &occurrence, &ana, "food-settled", &preview).await;
        let observed = state(&engine, &settlement).await;
        assert!(observed.ready && observed.settled && observed.observed);
        let (_, evidence) =
            store::transfer_agreement::read(&engine.store.pool, &settlement.transfer)
                .await
                .unwrap();
        assert!(
            !evidence[0]["evidence"]["settlement_facts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            store::transfers::party_levels(&engine.store.pool, &settlement.transfer)
                .await
                .unwrap()
                .iter()
                .all(|(_, _, level)| *level == 0)
        );
        assert!(
            store::transfers::party_for_actor(&engine.store.pool, &upstream.transfer, &dora.uid)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store::records::get(&engine.store.pool, &dora_stock)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            7.0
        );
        assert!(
            store::transfers::occurrences_of(&engine.store.pool, &settlement.transfer)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store::misc::get_promise(&engine.store.pool, "settlement-observer-promise")
                .await
                .unwrap()
                .unwrap()
                .state,
            PromiseState::Proposed
        );
    });
}

#[test]
fn derived_agreement_does_not_grant_another_persons_action_and_later_dispute_preserves_started_work()
 {
    support::run_async_test("dependency-action-authority", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "authority.ana").await;
        let beto = support::person(&engine, "authority.beto").await;
        let stock = support::plain(&engine, "authority.food", 10.0).await;
        let deliveries = support::plain(&engine, "authority.deliveries", 5.0).await;
        let upstream = support::create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            vec![support::promise("upstream-promise", &stock, &ana, -1.0)],
            DraftOptions {
                slug: "upstream-authority",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        let downstream = conditional(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            "delivery",
            &upstream.transfer,
            "agreed",
            Some(&deliveries),
        )
        .await;
        for person in [&ana, &beto] {
            support::agree(
                &engine,
                &upstream,
                person,
                &format!("upstream:{}", person.uid),
            )
            .await;
        }
        assert!(state(&engine, &downstream).await.ready);
        assert!(!state(&engine, &downstream).await.observed);
        let rows = protein::execute_for_with_signer(
            &engine.store,
            &protein::Protein {
                source: protein::Source::Transfer,
                filter: vec![protein::Predicate::UidEq(downstream.transfer.clone())],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            },
            None,
            Some(&ana.uid),
        )
        .await
        .unwrap();
        let row = rows
            .iter()
            .find(|row| row["uid"] == downstream.transfer)
            .unwrap();
        assert_eq!(row["status"], "agreed", "{row}");
        assert_eq!(row["capabilities"]["activate"], true, "{row}");
        engine.set_signer(beto.signer.clone()).await.unwrap();
        let denied = engine
            .act(
                Action::ActivateTransferOccurrence {
                    transfer: downstream.transfer.clone(),
                    promise: "delivery-promise".into(),
                    expected_revision: downstream.revision,
                    request_id: "wrong-owner".into(),
                    person: Some(beto.uid.clone()),
                },
                None,
            )
            .await;
        assert!(denied.is_err());
        let active = support::activate(
            &engine,
            &downstream,
            &ana,
            "delivery-promise",
            "begin-delivery",
        )
        .await;
        assert!(
            store::transfers::party_levels(&engine.store.pool, &downstream.transfer)
                .await
                .unwrap()
                .iter()
                .all(|(_, _, level)| *level == 0)
        );
        let source_occurrence =
            support::activate(&engine, &upstream, &ana, "upstream-promise", "begin-source").await;
        support::claim(
            &engine,
            &source_occurrence,
            &ana,
            TransferOccurrenceClaimRole::Delivery,
            "source-delivered",
        )
        .await;
        support::claim(
            &engine,
            &source_occurrence,
            &beto,
            TransferOccurrenceClaimRole::Receipt,
            "source-received",
        )
        .await;
        let preview = support::settlement_preview(&engine, &source_occurrence, &ana, 1.0).await;
        let settlement = support::settle_from_preview(
            &engine,
            &source_occurrence,
            &ana,
            "source-settled",
            &preview,
        )
        .await;
        engine
            .act(
                Action::CompensateTransferOccurrenceSettlement {
                    settlement,
                    request_id: "correct-source-accounting".into(),
                    person: Some(ana.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        assert!(state(&engine, &downstream).await.ready);
        assert_eq!(
            store::records::get(&engine.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            10.0
        );
        engine
            .act(
                Action::SetTransferOccurrenceDispute {
                    occurrence: source_occurrence,
                    person: Some(ana.uid.clone()),
                    request_id: "source-dispute".into(),
                    disputed: true,
                },
                None,
            )
            .await
            .unwrap();
        assert!(!state(&engine, &downstream).await.ready);
        assert!(
            store::transfers::occurrence(&engine.store.pool, &active)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            store::misc::get_promise(&engine.store.pool, "delivery-promise")
                .await
                .unwrap()
                .unwrap()
                .state,
            PromiseState::Active
        );
        assert_eq!(
            store::records::get(&engine.store.pool, &deliveries)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            5.0
        );
    });
}

#[test]
fn seven_transfer_chains_and_parent_edges_share_cycle_guards() {
    support::run_async_test("dependency-chain", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "chain.ana").await;
        let stock = support::plain(&engine, "chain.stock", 1.0).await;
        let root_promise = support::promise("root-promise", &stock, &ana, -1.0);
        let root = support::create_transfer(
            &engine,
            &ana,
            &[],
            vec![root_promise.clone()],
            DraftOptions {
                slug: "chain-root",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        let mut previous = root.transfer.clone();
        let mut chain = Vec::new();
        for index in 1..7 {
            let next = conditional(
                &engine,
                &ana,
                &[],
                &format!("chain-{index}"),
                &previous,
                "agreed",
                None,
            )
            .await;
            assert!(!state(&engine, &next).await.ready);
            previous = next.transfer.clone();
            chain.push(next);
        }
        let branch = conditional(
            &engine,
            &ana,
            &[],
            "chain-branch",
            &chain[2].transfer,
            "agreed",
            None,
        )
        .await;
        support::agree(&engine, &root, &ana, "root-agreed").await;
        for transfer in chain.iter().chain(std::iter::once(&branch)) {
            assert!(state(&engine, transfer).await.ready);
        }
        let cycle = engine
            .act(
                revision(
                    &ana,
                    &root,
                    vec![root_promise],
                    vec![dependency(&previous, "agreed")],
                    "cycle-back",
                    None,
                ),
                None,
            )
            .await;
        assert!(cycle.unwrap_err().to_string().contains("cycle"));
        let child: Action=serde_json::from_value(serde_json::json!({"action":"create-transfer-draft","creator":ana.uid,"head":"Cycle child","request_id":"cycle-child","agreement":"dependency","parent":root.transfer,"promises":[{"uid":"cycle-promise","party":ana.uid,"delta":1,"item":{"title":"Outcome"}}],"dependencies":[{"scope":"transfer","upstream_kind":"transfer","upstream":root.transfer,"required_state":"agreed"}]})).unwrap();
        assert!(
            engine
                .act(child, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("cycle")
        );
        assert_eq!(
            support::current_revision(&engine, &root.transfer).await,
            root.revision
        );
        assert!(state(&engine, &root).await.ready);

        let visitor = support::person(&engine, "chain.visitor").await;
        let role = store::auth::ensure_role(&engine.store.pool, "Outcome reader")
            .await
            .unwrap();
        for resource in ["record", "transfer", "fact"] {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, "read")
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(
            &engine.store.pool,
            &visitor.uid,
            "chain.visitor",
            "test-hash",
            role,
        )
        .await
        .unwrap();
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                serde_json::from_value(
                    serde_json::json!({"action":"grant-visibility", "target":branch.transfer,
            "subject_kind":"actor", "subject":visitor.uid}),
                )
                .unwrap(),
                None,
            )
            .await
            .unwrap();
        let query = protein::Protein {
            source: protein::Source::Transfer,
            filter: vec![protein::Predicate::UidEq(branch.transfer.clone())],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let rows = protein::execute_for(&engine.store, &query, Some(&visitor.uid))
            .await
            .unwrap();
        let row = rows
            .iter()
            .find(|row| row["uid"] == branch.transfer)
            .unwrap();
        assert_eq!(row["readiness"]["ready"], true);
        for secret in [&chain[2].transfer, "chain-3"] {
            assert!(!row.to_string().contains(secret), "{row}");
        }
        let facts = protein::execute_for(
            &engine.store,
            &protein::Protein {
                source: protein::Source::Fact,
                filter: vec![protein::Predicate::RecordEq(branch.transfer)],
                ..query
            },
            Some(&visitor.uid),
        )
        .await
        .unwrap();
        assert!(
            !serde_json::to_string(&facts)
                .unwrap()
                .contains(&chain[2].transfer)
        );
    });
}
