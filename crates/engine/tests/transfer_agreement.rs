use engine::Engine;
use engine::actions::{
    Action, TransferDependencyInput, TransferDependencyScopeInput,
    TransferDependencyUpstreamKindInput, TransferDraftRevisionInput, TransferPromiseInput,
    TransferReservePoint, TransferSatiation, TransferVisibility,
};
use engine::trust::{self, Signer};
use nucleus::transfer::{AgreementType, OpenPromiseReusePolicy};
use nucleus::{PromiseState, RecordKind};
use protein::{Include, Predicate, Protein, Source};
use std::future::Future;

#[derive(Clone)]
struct Person {
    uid: String,
    signer: Signer,
}

struct Fixture {
    transfer: String,
    revision: u64,
}

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine");
    store::organs::ensure_local(&engine.store.pool, "http://phase3.test")
        .await
        .expect("local Organ");
    engine
}

fn run_async_test<F, Fut>(test: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    std::thread::Builder::new()
        .name("transfer-agreement-test".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime")
                .block_on(test());
        })
        .expect("test thread")
        .join()
        .expect("Phase 3 test thread");
}

async fn person(engine: &Engine, slug: &str) -> Person {
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: RecordKind::Person,
                head: slug.into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .expect("person record")
        .created
        .expect("person uid");
    Person {
        signer: Signer::generate(&uid, &format!("test:{slug}:agreement")),
        uid,
    }
}

async fn record(engine: &Engine, slug: &str) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: RecordKind::Plain,
                head: slug.into(),
                body: String::new(),
                quantity: 10.0,
            },
            None,
        )
        .await
        .expect("record")
        .created
        .expect("record uid")
}

fn promise(uid: &str, record: &str, party: &Person, delta: f64) -> TransferPromiseInput {
    TransferPromiseInput {
        uid: Some(uid.into()),
        record: record.into(),
        party: Some(party.uid.clone()),
        open: false,
        delta,
        unit: None,
        window_start: None,
        window_end: None,
        place: None,
        condition: None,
        reserve_from: Some(TransferReservePoint::Active),
        reuse_policy: OpenPromiseReusePolicy::Duplicate,
        withdrawn: false,
    }
}

fn draft(
    creator: &Person,
    slug: &str,
    agreement: AgreementType,
    agreement_pct: Option<u8>,
    invitees: &[Person],
    promises: Vec<TransferPromiseInput>,
    dependencies: Vec<TransferDependencyInput>,
) -> TransferDraftRevisionInput {
    TransferDraftRevisionInput {
        creator: creator.uid.clone(),
        slug: Some(slug.into()),
        head: slug.into(),
        agreement,
        agreement_pct,
        satiation: TransferSatiation::None,
        parent: None,
        source: None,
        visibility: TransferVisibility::Hidden,
        max_proximity: None,
        reserve_default: TransferReservePoint::Active,
        require_confirmation: true,
        default_place: None,
        invitees: invitees.iter().map(|person| person.uid.clone()).collect(),
        promises,
        dependencies,
    }
}

async fn create_fixture(
    engine: &Engine,
    creator: &Person,
    slug: &str,
    agreement: AgreementType,
    agreement_pct: Option<u8>,
    invitees: &[Person],
    promises: Vec<TransferPromiseInput>,
    dependencies: Vec<TransferDependencyInput>,
) -> Fixture {
    engine
        .set_signer(creator.signer.clone())
        .await
        .expect("creator signer");
    let input = draft(
        creator,
        slug,
        agreement,
        agreement_pct,
        invitees,
        promises,
        dependencies,
    );
    let create = Action::CreateTransferDraft {
        request_id: format!("create:{slug}"),
        creator: Some(creator.uid.clone()),
        slug: input.slug,
        head: input.head,
        agreement: input.agreement,
        agreement_pct: input.agreement_pct,
        satiation: input.satiation,
        parent: input.parent,
        source: input.source,
        visibility: input.visibility,
        max_proximity: input.max_proximity,
        reserve_default: input.reserve_default,
        require_confirmation: input.require_confirmation,
        default_place: input.default_place,
        invitees: input.invitees,
        promises: input.promises,
        dependencies: input.dependencies,
    };
    let transfer = engine
        .act(create.clone(), None)
        .await
        .expect("create signed transfer draft")
        .created
        .expect("transfer uid");
    assert_eq!(
        engine
            .act(create, None)
            .await
            .expect("replay signed transfer draft")
            .created
            .as_deref(),
        Some(transfer.as_str()),
        "draft creation replay returns the original Transfer"
    );

    for invitee in invitees {
        let invitation = store::transfers::invitations_for_transfer(&engine.store.pool, &transfer)
            .await
            .expect("invitations")
            .into_iter()
            .find(|invitation| invitation.addressed_person_uid == invitee.uid)
            .expect("invitee invitation");
        let revision = current_revision(engine, &transfer).await;
        engine
            .set_signer(invitee.signer.clone())
            .await
            .expect("invitee signer");
        engine
            .act(
                Action::AcceptTransferInvitation {
                    invitation: invitation.uid,
                    expected_revision: revision,
                    request_id: format!("accept:{slug}:{}", invitee.uid),
                    transfer: Some(transfer.clone()),
                    person: Some(invitee.uid.clone()),
                },
                None,
            )
            .await
            .expect("accept invitation");
    }

    Fixture {
        revision: current_revision(engine, &transfer).await,
        transfer,
    }
}

async fn current_revision(engine: &Engine, transfer: &str) -> u64 {
    store::transfers::get(&engine.store.pool, transfer)
        .await
        .expect("transfer lookup")
        .expect("transfer")
        .revision as u64
}

async fn set_level(
    engine: &Engine,
    fixture: &Fixture,
    person: &Person,
    level: u8,
    request_id: &str,
) -> String {
    engine
        .set_signer(person.signer.clone())
        .await
        .expect("person signer");
    engine
        .act(
            Action::SetTransferAgreementLevel {
                transfer: fixture.transfer.clone(),
                expected_revision: fixture.revision,
                request_id: request_id.into(),
                person: Some(person.uid.clone()),
                level,
            },
            None,
        )
        .await
        .expect("agreement transition")
        .created
        .expect("agreement event uid")
}

async fn agree(engine: &Engine, fixture: &Fixture, person: &Person, prefix: &str) {
    set_level(engine, fixture, person, 1, &format!("{prefix}:review")).await;
    set_level(engine, fixture, person, 2, &format!("{prefix}:commit")).await;
}

async fn projected_transfer(engine: &Engine, transfer: &str, signer: &Person) -> serde_json::Value {
    let query = Protein {
        source: Source::Transfer,
        filter: vec![Predicate::UidEq(transfer.into())],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    protein::execute_for_with_signer(&engine.store, &query, None, Some(&signer.uid))
        .await
        .expect("transfer Protein")
        .into_iter()
        .find(|row| row["uid"] == transfer)
        .expect("projected transfer")
}

async fn promise_state(engine: &Engine, uid: &str) -> PromiseState {
    store::misc::get_promise(&engine.store.pool, uid)
        .await
        .expect("promise lookup")
        .expect("promise")
        .state
}

#[test]
fn signed_adjacent_transitions_are_idempotent_person_scoped_and_projected() {
    run_async_test(|| async {
        let engine = engine().await;
        let ana = person(&engine, "phase3.ana").await;
        let bia = person(&engine, "phase3.bia").await;
        let goods = record(&engine, "phase3.goods").await;
        let fixture = create_fixture(
            &engine,
            &ana,
            "phase3.signed-levels",
            AgreementType::Full,
            None,
            std::slice::from_ref(&bia),
            vec![
                promise("phase3-p-ana", &goods, &ana, -2.0),
                promise("phase3-p-bia", &goods, &bia, 2.0),
            ],
            vec![],
        )
        .await;

        engine.set_signer(ana.signer.clone()).await.unwrap();
        let skipped = engine
            .act(
                Action::SetTransferAgreementLevel {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: "phase3:skip-level".into(),
                    person: Some(ana.uid.clone()),
                    level: 2,
                },
                None,
            )
            .await
            .expect_err("level 0 cannot skip directly to level 2");
        assert!(skipped.to_string().contains("adjacent"));

        let before = projected_transfer(&engine, &fixture.transfer, &ana).await;
        assert_eq!(before["capabilities"]["review"], true);
        assert_eq!(before["capabilities"]["commit"], false);

        let reviewed = set_level(&engine, &fixture, &ana, 1, "phase3:ana:review").await;
        let replayed = set_level(&engine, &fixture, &ana, 1, "phase3:ana:review").await;
        assert_eq!(reviewed, replayed, "a retry returns the original event");
        assert_eq!(
            store::transfers::agreement_events(&engine.store.pool, &fixture.transfer, None)
                .await
                .unwrap()
                .len(),
            1,
            "a retry appends no second event"
        );

        let checked = projected_transfer(&engine, &fixture.transfer, &ana).await;
        assert_eq!(checked["capabilities"]["review"], false);
        assert_eq!(checked["capabilities"]["commit"], true);
        assert_eq!(checked["capabilities"]["agreement_back"], true);

        set_level(&engine, &fixture, &ana, 2, "phase3:ana:commit").await;
        assert_eq!(
            promise_state(&engine, "phase3-p-ana").await,
            PromiseState::Agreed
        );
        assert_eq!(
            promise_state(&engine, "phase3-p-bia").await,
            PromiseState::Proposed,
            "one Person's signature cannot advance another Person's promise"
        );
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &fixture.transfer)
                .await
                .unwrap()
        );

        engine.set_signer(ana.signer.clone()).await.unwrap();
        let forged = engine
            .act(
                Action::SetTransferAgreementLevel {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: "phase3:forged-bia".into(),
                    person: Some(bia.uid.clone()),
                    level: 1,
                },
                None,
            )
            .await
            .expect_err("Ana's installed key cannot sign Bia's agreement");
        assert!(!forged.to_string().is_empty());
        assert!(
            store::transfers::agreement_event_for_request(&engine.store.pool, "phase3:forged-bia")
                .await
                .unwrap()
                .is_none(),
            "a rejected identity attempt must append no agreement evidence"
        );

        agree(&engine, &fixture, &bia, "phase3:bia").await;
        assert!(
            protein::transfer_agreement_ready(&engine.store, &fixture.transfer)
                .await
                .unwrap()
        );
        assert_eq!(
            promise_state(&engine, "phase3-p-bia").await,
            PromiseState::Agreed
        );

        set_level(&engine, &fixture, &ana, 1, "phase3:ana:retract").await;
        assert_eq!(
            promise_state(&engine, "phase3-p-ana").await,
            PromiseState::Proposed
        );
        assert_eq!(
            promise_state(&engine, "phase3-p-bia").await,
            PromiseState::Agreed
        );

        let events =
            store::transfers::agreement_events(&engine.store.pool, &fixture.transfer, None)
                .await
                .unwrap();
        assert_eq!(events.len(), 5);
        for event in events {
            let fact = store::facts::get(&engine.store.pool, &event.fact_uid)
                .await
                .unwrap()
                .expect("agreement fact");
            assert_eq!(fact.actor_uid.as_deref(), Some(event.person_uid.as_str()));
            assert!(trust::verify_fact(&engine.store, &fact).await.unwrap());
        }
    });
}

#[test]
fn individual_and_full_policies_gate_the_relevant_people() {
    run_async_test(|| async {
        let engine = engine().await;
        let ana = person(&engine, "policy.ana").await;
        let bia = person(&engine, "policy.bia").await;
        let caio = person(&engine, "policy.caio").await;
        let goods = record(&engine, "policy.goods").await;
        let labor = record(&engine, "policy.labor").await;

        for (slug, agreement, prefix) in [
            ("policy.individual", AgreementType::Individual, "individual"),
            ("policy.full", AgreementType::Full, "full"),
        ] {
            let ana_promise = format!("{prefix}-p-ana");
            let bia_promise = format!("{prefix}-p-bia");
            let caio_promise = format!("{prefix}-p-caio");
            let fixture = create_fixture(
                &engine,
                &ana,
                slug,
                agreement,
                None,
                &[bia.clone(), caio.clone()],
                vec![
                    promise(&ana_promise, &goods, &ana, -2.0),
                    promise(&bia_promise, &goods, &bia, 2.0),
                    promise(&caio_promise, &labor, &caio, -1.0),
                ],
                vec![],
            )
            .await;
            agree(&engine, &fixture, &ana, &format!("{prefix}:ana")).await;
            agree(&engine, &fixture, &bia, &format!("{prefix}:bia")).await;

            let ana_ready = protein::transfer_ready_promises_for_person(
                &engine.store,
                &fixture.transfer,
                &ana.uid,
            )
            .await
            .unwrap();
            if agreement == AgreementType::Individual {
                assert_eq!(ana_ready, vec![ana_promise]);
            } else {
                assert!(ana_ready.is_empty(), "full policy still needs Caio");
            }
            assert!(
                !protein::transfer_agreement_ready(&engine.store, &fixture.transfer)
                    .await
                    .unwrap(),
                "Caio's separate path remains unsigned"
            );

            agree(&engine, &fixture, &caio, &format!("{prefix}:caio")).await;
            assert!(
                protein::transfer_agreement_ready(&engine.store, &fixture.transfer)
                    .await
                    .unwrap()
            );
        }
    });
}

#[test]
fn percentage_policy_freezes_the_first_quorum_and_excludes_late_signers() {
    run_async_test(|| async {
        let engine = engine().await;
        let ana = person(&engine, "quorum.ana").await;
        let bia = person(&engine, "quorum.bia").await;
        let caio = person(&engine, "quorum.caio").await;
        let goods = record(&engine, "quorum.goods").await;
        let fixture = create_fixture(
            &engine,
            &ana,
            "quorum.transfer",
            AgreementType::Percentage,
            Some(66),
            &[bia.clone(), caio.clone()],
            vec![
                promise("quorum-p-ana", &goods, &ana, -1.0),
                promise("quorum-p-bia", &goods, &bia, 1.0),
                promise("quorum-p-caio", &goods, &caio, -1.0),
            ],
            vec![],
        )
        .await;

        agree(&engine, &fixture, &ana, "quorum:ana").await;
        agree(&engine, &fixture, &bia, "quorum:bia").await;
        let coalition = store::transfers::agreement_coalition(
            &engine.store.pool,
            &fixture.transfer,
            fixture.revision,
        )
        .await
        .unwrap()
        .expect("quorum frozen");
        assert_eq!(coalition.threshold_pct, 66);
        assert_eq!(coalition.eligible_count, 3);
        let party_levels = store::transfers::party_levels(&engine.store.pool, &fixture.transfer)
            .await
            .unwrap();
        let ana_party = &party_levels
            .iter()
            .find(|(_, actor, _)| actor == &ana.uid)
            .unwrap()
            .0;
        let bia_party = &party_levels
            .iter()
            .find(|(_, actor, _)| actor == &bia.uid)
            .unwrap()
            .0;
        let caio_party = &party_levels
            .iter()
            .find(|(_, actor, _)| actor == &caio.uid)
            .unwrap()
            .0;
        assert!(coalition.party_uids.contains(ana_party));
        assert!(coalition.party_uids.contains(bia_party));
        assert!(!coalition.party_uids.contains(caio_party));
        assert!(
            protein::transfer_agreement_ready(&engine.store, &fixture.transfer)
                .await
                .unwrap()
        );

        engine.set_signer(caio.signer.clone()).await.unwrap();
        let late = engine
            .act(
                Action::SetTransferAgreementLevel {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: "quorum:caio:late".into(),
                    person: Some(caio.uid.clone()),
                    level: 1,
                },
                None,
            )
            .await
            .expect_err("a frozen quorum cannot silently gain a late signer");
        assert!(late.to_string().contains("coalition is frozen"));

        let projected = projected_transfer(&engine, &fixture.transfer, &caio).await;
        let caio_projection = projected["parties"]
            .as_array()
            .unwrap()
            .iter()
            .find(|party| party["actor"] == caio.uid)
            .unwrap();
        assert_eq!(caio_projection["coalition_member"], false);
        assert_eq!(caio_projection["capabilities"]["review"], false);
    });
}

#[test]
fn structured_dependencies_and_public_private_changes_have_distinct_effects() {
    run_async_test(|| async {
        let engine = engine().await;
        let ana = person(&engine, "sensitivity.ana").await;
        let bia = person(&engine, "sensitivity.bia").await;
        let goods = record(&engine, "sensitivity.goods").await;

        let upstream = create_fixture(
            &engine,
            &ana,
            "sensitivity.upstream",
            AgreementType::Individual,
            None,
            &[],
            vec![promise("sensitivity-upstream-p", &goods, &ana, -1.0)],
            vec![],
        )
        .await;
        let dependent = create_fixture(
            &engine,
            &ana,
            "sensitivity.dependent",
            AgreementType::Dependency,
            None,
            &[],
            vec![promise("sensitivity-dependent-p", &goods, &ana, -1.0)],
            vec![TransferDependencyInput {
                uid: Some("sensitivity-dependency".into()),
                scope: TransferDependencyScopeInput::Transfer,
                promise: None,
                upstream_kind: TransferDependencyUpstreamKindInput::Promise,
                upstream: "sensitivity-upstream-p".into(),
                required_state: "agreed".into(),
            }],
        )
        .await;
        agree(&engine, &dependent, &ana, "dependency:dependent").await;
        assert!(
            !protein::transfer_agreement_ready(&engine.store, &dependent.transfer)
                .await
                .unwrap()
        );
        let blocked = projected_transfer(&engine, &dependent.transfer, &ana).await;
        assert!(blocked["dependencies"][0]["satisfied"] == false);
        assert_eq!(
            blocked["readiness"]["blockers"][0]["code"],
            "dependency_not_satisfied"
        );

        agree(&engine, &upstream, &ana, "dependency:upstream").await;
        assert!(
            protein::transfer_agreement_ready(&engine.store, &dependent.transfer)
                .await
                .unwrap()
        );

        let editable = create_fixture(
            &engine,
            &ana,
            "sensitivity.public-edit",
            AgreementType::Full,
            None,
            std::slice::from_ref(&bia),
            vec![
                promise("sensitivity-edit-ana", &goods, &ana, -2.0),
                promise("sensitivity-edit-bia", &goods, &bia, 2.0),
            ],
            vec![],
        )
        .await;
        agree(&engine, &editable, &ana, "sensitivity:edit:ana").await;
        agree(&engine, &editable, &bia, "sensitivity:edit:bia").await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let mut ana_terms = promise("sensitivity-edit-ana", &goods, &ana, -2.0);
        ana_terms.window_end = Some("2099-01-01T00:00:00Z".into());
        engine
            .act(
                Action::ReviseTransferDraft {
                    transfer: editable.transfer.clone(),
                    expected_revision: editable.revision,
                    request_id: "sensitivity:public-time-edit".into(),
                    draft: draft(
                        &ana,
                        "sensitivity.public-edit",
                        AgreementType::Full,
                        None,
                        &[],
                        vec![
                            ana_terms,
                            promise("sensitivity-edit-bia", &goods, &bia, 2.0),
                        ],
                        vec![],
                    ),
                },
                None,
            )
            .await
            .expect("time changes create a new signed revision");
        let levels = store::transfers::party_levels(&engine.store.pool, &editable.transfer)
            .await
            .unwrap();
        assert!(levels.iter().all(|(_, _, level)| *level == 0));
        assert_eq!(
            current_revision(&engine, &editable.transfer).await,
            editable.revision + 1
        );

        let private = create_fixture(
            &engine,
            &ana,
            "sensitivity.private-formula",
            AgreementType::Full,
            None,
            std::slice::from_ref(&bia),
            vec![
                promise("sensitivity-private-ana", &goods, &ana, -2.0),
                promise("sensitivity-private-bia", &goods, &bia, 2.0),
            ],
            vec![],
        )
        .await;
        agree(&engine, &private, &ana, "sensitivity:private:ana").await;
        agree(&engine, &private, &bia, "sensitivity:private:bia").await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let occurrence = engine
            .act(
                Action::ActivateTransferOccurrence {
                    transfer: private.transfer.clone(),
                    promise: "sensitivity-private-ana".into(),
                    expected_revision: private.revision,
                    request_id: "sensitivity:activate".into(),
                    person: Some(ana.uid.clone()),
                },
                None,
            )
            .await
            .expect("activate matched path")
            .created
            .expect("occurrence uid");
        engine.set_signer(bia.signer.clone()).await.unwrap();
        engine
            .act(
                Action::SetTransferOccurrenceApplicationFormula {
                    occurrence,
                    request_id: "sensitivity:private-formula".into(),
                    person: Some(bia.uid.clone()),
                    formula: "incoming() * 2 + 1".into(),
                },
                None,
            )
            .await
            .expect("receiver updates private formula");
        assert_eq!(
            current_revision(&engine, &private.transfer).await,
            private.revision
        );
        assert!(
            store::transfers::party_levels(&engine.store.pool, &private.transfer)
                .await
                .unwrap()
                .iter()
                .all(|(_, _, level)| *level == 2),
            "private application policy does not invalidate public agreement"
        );
    });
}
