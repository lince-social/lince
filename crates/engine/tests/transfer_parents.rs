mod support;

use engine::{
    Engine,
    actions::{
        Action, TransferDraftRevisionInput, TransferPromiseInput, TransferReservePoint,
        TransferSatiation, TransferVisibility,
    },
};
use nucleus::transfer::AgreementType;
use protein::{Include, Predicate, Protein, Source};
use serde_json::Value;
use support::{Person, TransferFixture};

async fn create(
    engine: &Engine,
    owner: &Person,
    name: &str,
    parent: Option<&str>,
    promises: Vec<TransferPromiseInput>,
) -> TransferFixture {
    engine.set_signer(owner.signer.clone()).await.unwrap();
    let uid = engine
        .act(
            Action::CreateTransferDraft {
                creator: Some(owner.uid.clone()),
                request_id: format!("create:{name}"),
                slug: Some(name.into()),
                head: name.into(),
                agreement: AgreementType::Full,
                agreement_pct: None,
                satiation: TransferSatiation::None,
                parent: parent.map(str::to_string),
                source: None,
                visibility: TransferVisibility::Hidden,
                max_proximity: None,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
                default_place: None,
                invitees: vec![],
                promises,
                dependencies: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    TransferFixture {
        revision: support::current_revision(engine, &uid).await,
        transfer: uid,
    }
}

async fn view(engine: &Engine, transfer: &str, filter: bool) -> Value {
    let rows = protein::execute(
        &engine.store,
        &Protein {
            source: Source::Transfer,
            filter: if filter {
                vec![Predicate::UidEq(transfer.into())]
            } else {
                vec![]
            },
            fields: None,
            include: Include::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        },
    )
    .await
    .unwrap();
    rows.into_iter().find(|row| row["uid"] == transfer).unwrap()
}

fn reparent(
    engine_person: &Person,
    child: &TransferFixture,
    parent: Option<&str>,
    request: &str,
    promises: Vec<TransferPromiseInput>,
) -> Action {
    Action::ReviseTransferDraft {
        transfer: child.transfer.clone(),
        expected_revision: child.revision,
        request_id: request.into(),
        draft: TransferDraftRevisionInput {
            creator: engine_person.uid.clone(),
            slug: None,
            head: "Moved child".into(),
            agreement: AgreementType::Full,
            agreement_pct: None,
            satiation: TransferSatiation::None,
            parent: parent.map(str::to_string),
            source: None,
            visibility: TransferVisibility::Hidden,
            max_proximity: None,
            reserve_default: TransferReservePoint::Active,
            require_confirmation: true,
            default_place: None,
            invitees: vec![],
            promises,
            dependencies: vec![],
        },
    }
}

#[test]
fn parent_readiness_is_independent_of_query_and_required_membership_is_signed() {
    support::run_async_test("parent-readiness", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "parent.ana").await;
        let record = support::plain(&engine, "parent.resource", 10.0).await;
        let mut parent = create(&engine, &ana, "parent", None, vec![]).await;
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        let branch = create(&engine, &ana, "branch", Some(&parent.transfer), vec![]).await;
        assert_eq!(
            support::current_revision(&engine, &parent.transfer).await,
            parent.revision + 1
        );
        let leaf = create(
            &engine,
            &ana,
            "leaf",
            Some(&branch.transfer),
            vec![support::promise("leaf-promise", &record, &ana, -1.0)],
        )
        .await;
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        support::agree(&engine, &leaf, &ana, "leaf").await;
        for target in [&parent.transfer, &branch.transfer, &leaf.transfer] {
            assert!(
                protein::transfer_agreement_ready(&engine.store, target)
                    .await
                    .unwrap()
            );
            for filter in [false, true] {
                let row = view(&engine, target, filter).await;
                assert_eq!(row["readiness"]["ready"], true, "{row}");
                assert_eq!(row["phase6"]["ready"], true);
            }
        }
        let waiting = create(&engine, &ana, "waiting", Some(&parent.transfer), vec![]).await;
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        parent.revision = support::current_revision(&engine, &parent.transfer).await;
        let change = Action::SetTransferChildRequirement {
            transfer: parent.transfer.clone(),
            child: waiting.transfer.clone(),
            required: false,
            expected_revision: parent.revision,
            request_id: "optional-waiting".into(),
            person: Some(ana.uid.clone()),
        };
        let result = engine.act(change.clone(), None).await.unwrap();
        assert_eq!(result.facts.len(), 1);
        assert!(engine.act(change, None).await.unwrap().facts.is_empty());
        assert!(
            protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        assert_eq!(
            support::current_revision(&engine, &parent.transfer).await,
            parent.revision + 1
        );
        let evidence: nucleus::transfer::TransferRevisionEvidence =
            serde_json::from_str(result.facts[0].payload.as_deref().unwrap()).unwrap();
        assert_eq!(
            evidence
                .terms
                .children
                .iter()
                .find(|child| child.uid == waiting.transfer)
                .unwrap()
                .required,
            false
        );
        let before = support::current_revision(&engine, &parent.transfer).await;
        let failed = engine
            .act(
                Action::SetTransferChildRequirement {
                    transfer: parent.transfer.clone(),
                    child: waiting.transfer.clone(),
                    required: true,
                    expected_revision: parent.revision,
                    request_id: "stale-required".into(),
                    person: Some(ana.uid.clone()),
                },
                None,
            )
            .await;
        assert!(failed.is_err());
        assert_eq!(
            support::current_revision(&engine, &parent.transfer).await,
            before
        );
        let bad = engine
            .act(
                reparent(
                    &ana,
                    &TransferFixture {
                        transfer: parent.transfer.clone(),
                        revision: before,
                    },
                    Some(&leaf.transfer),
                    "parent-cycle",
                    vec![],
                ),
                None,
            )
            .await;
        assert!(bad.unwrap_err().to_string().contains("cycle"));
        engine
            .act(
                Action::SetTransferAgreementLevel {
                    transfer: leaf.transfer.clone(),
                    expected_revision: leaf.revision,
                    person: Some(ana.uid.clone()),
                    level: 1,
                    request_id: "leaf-back".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        let row = view(&engine, &parent.transfer, true).await;
        assert_eq!(row["readiness"]["ready"], false);
        assert_eq!(row["children"].as_array().unwrap().len(), 2);
    });
}

#[test]
fn required_child_blocks_activation_and_membership_resets_parent_agreement() {
    support::run_async_test("parent-activation", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "activation.ana").await;
        let beto = support::person(&engine, "activation.beto").await;
        let outsider = support::person(&engine, "activation.outsider").await;
        let record = support::plain(&engine, "activation.resource", 10.0).await;
        let mut parent = support::create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            vec![support::promise("parent-promise", &record, &ana, -1.0)],
            support::DraftOptions {
                slug: "parent-with-promise",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        for person in [&ana, &beto] {
            support::agree(&engine, &parent, person, &format!("initial:{}", person.uid)).await;
        }
        let child = create(
            &engine,
            &ana,
            "blocking-child",
            Some(&parent.transfer),
            vec![],
        )
        .await;
        parent.revision = support::current_revision(&engine, &parent.transfer).await;
        let levels = store::transfers::party_levels(&engine.store.pool, &parent.transfer)
            .await
            .unwrap();
        assert!(levels.iter().all(|(_, _, level)| *level == 0));
        for person in [&ana, &beto] {
            support::agree(&engine, &parent, person, &format!("renewed:{}", person.uid)).await;
        }
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let action = Action::ActivateTransferOccurrence {
            transfer: parent.transfer.clone(),
            promise: "parent-promise".into(),
            expected_revision: parent.revision,
            request_id: "start-parent".into(),
            person: Some(ana.uid.clone()),
        };
        assert!(engine.act(action, None).await.is_err());
        let attempted = store::transfers::activate_occurrences(
            &engine.store.pool,
            store::transfers::ActivateOccurrencesInput {
                transfer_uid: parent.transfer.clone(),
                expected_revision: parent.revision,
                idempotency_key: "direct-start".into(),
                actor_person_uid: ana.uid.clone(),
                occurrences: vec![store::transfers::OccurrenceActivationInput {
                    promise_uid: "parent-promise".into(),
                    opposite_promise_uid: None,
                    giver_person_uid: ana.uid.clone(),
                    receiver_person_uid: beto.uid.clone(),
                }],
                authorization_intent_uid: None,
            },
            chrono::Utc::now(),
            |hash| Some(ana.signer.sign_hash(hash)),
        )
        .await;
        assert!(attempted.unwrap_err().to_string().contains("not ready"));
        engine.set_signer(outsider.signer.clone()).await.unwrap();
        let refused = engine
            .act(
                Action::SetTransferChildRequirement {
                    transfer: parent.transfer.clone(),
                    child: child.transfer.clone(),
                    required: false,
                    expected_revision: parent.revision,
                    request_id: "outsider-child".into(),
                    person: Some(outsider.uid.clone()),
                },
                None,
            )
            .await;
        assert!(refused.is_err());
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(reparent(&ana, &child, None, "unlink-child", vec![]), None)
            .await
            .unwrap();
        parent.revision = support::current_revision(&engine, &parent.transfer).await;
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        for person in [&ana, &beto] {
            support::agree(
                &engine,
                &parent,
                person,
                &format!("unlinked:{}", person.uid),
            )
            .await;
        }
        let occurrence =
            support::activate(&engine, &parent, &ana, "parent-promise", "actual-start").await;
        engine
            .act(
                Action::SetTransferOccurrenceDispute {
                    occurrence: occurrence.clone(),
                    disputed: true,
                    person: Some(ana.uid.clone()),
                    request_id: "dispute-parent".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &parent.transfer)
                .await
                .unwrap()
        );
        assert_eq!(
            store::transfers::occurrences_of(&engine.store.pool, &parent.transfer)
                .await
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn hidden_required_children_expose_only_the_parent_result() {
    support::run_async_test("hidden-parent-child", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "hidden.ana").await;
        let beto = support::person(&engine, "hidden.beto").await;
        let parent = support::create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            vec![],
            support::DraftOptions {
                slug: "visible-parent",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        let stock = support::plain(&engine, "secret-child-stock", 10.0).await;
        let child = create(
            &engine,
            &ana,
            "secret-child-name",
            Some(&parent.transfer),
            vec![support::promise(
                "private-child-promise",
                &stock,
                &ana,
                -1.0,
            )],
        )
        .await;
        let role = store::auth::ensure_role(&engine.store.pool, "Parent reader")
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
            &beto.uid,
            "hidden.beto",
            "test-hash",
            role,
        )
        .await
        .unwrap();
        for agreed in [false, true] {
            if agreed {
                support::agree(&engine, &child, &ana, "private-child").await;
            }
            let query = Protein {
                source: Source::Transfer,
                filter: vec![Predicate::UidEq(parent.transfer.clone())],
                fields: None,
                include: Include::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            };
            let rows = protein::execute_for(&engine.store, &query, Some(&beto.uid))
                .await
                .unwrap();
            let row = rows
                .iter()
                .find(|row| row["uid"] == parent.transfer)
                .unwrap();
            assert_eq!(row["readiness"]["ready"], agreed, "{row}");
            assert_eq!(row["phase6"]["ready"], agreed);
            assert_eq!(row["children_details_hidden"], true);
            for secret in [
                &child.transfer,
                &stock,
                "secret-child-name",
                "private-child-promise",
            ] {
                assert!(!row.to_string().contains(secret), "{row}");
            }
            if !agreed {
                assert!(row.to_string().contains("Waiting on a required part"));
            }
            let facts = protein::execute_for(
                &engine.store,
                &Protein {
                    source: Source::Fact,
                    filter: vec![Predicate::RecordEq(parent.transfer.clone())],
                    ..query.clone()
                },
                Some(&beto.uid),
            )
            .await
            .unwrap();
            assert!(
                !serde_json::to_string(&facts)
                    .unwrap()
                    .contains(&child.transfer)
            );
        }
    });
}
