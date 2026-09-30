use engine::actions::Action;
use nucleus::karma::{
    Capability, CapabilitySet, DelegationGrantSchema, DelegationGrantSpec,
    GrantProgramRevisionScope, GrantStatus, GrantTargetScope, GrantTemplateScope, ProgramAst,
    ProgramSchema, ReferenceKind, Slug, TimestampMs, TypedUid,
};
use nucleus::simulation::{Predicate, Refusal, ReplayStatus, Verdict};
use simulation::scenario::{Check, Database, Event, Input, Invocation};

#[test]
fn revoked_grant_survives_restart_and_refuses_a_stale_activation() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(exercise())
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn exercise() {
    let directory = tempfile::tempdir().unwrap();
    let mut case = simulation::fixtures::daily();
    case.name = "grant-revocation".into();
    case.cells[0].seed = vec![Invocation {
        id: "owner".into(),
        actor: None,
        action: Action::CreateRecord {
            slug: Some("owner".into()),
            kind: nucleus::RecordKind::Person,
            head: "Owner".into(),
            body: String::new(),
            quantity: 1.0,
        },
    }];
    case.end_ms = case.start_ms + 1000;
    case.checks
        .retain(|check| matches!(check.predicate, Predicate::FactChain {}));
    let world = simulation::world::World::open(case.clone(), &directory.path().join("source"))
        .await
        .unwrap();
    let owner = world.captured["owner"].clone();
    let node = &world.nodes["a"];
    let engine = node.engine();
    let grant = node
        .execution
        .scope(async {
            engine
                .set_signer(engine::trust::Signer::from_bytes(
                    &owner,
                    "seed-person",
                    [9; 32],
                ))
                .await
                .unwrap();
            let program = engine
                .act(
                    Action::CreateKarmaProgram {
                        request_id: "program".into(),
                        program: ProgramAst {
                            schema: ProgramSchema::V1,
                            slug: Slug::new("grant-host").unwrap(),
                            purpose: "Grant test".into(),
                            tags: Default::default(),
                            parameters: Default::default(),
                            nodes: Default::default(),
                            outputs: Default::default(),
                            required_capabilities: Default::default(),
                        },
                        owner_person_uid: Some(owner.clone()),
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap();
            let source_revision = store::projection::revision(&engine.store.pool)
                .await
                .unwrap();
            store::records::set_extension_raw(
                &engine.store.pool,
                &owner,
                "lince.pairing",
                &serde_json::json!({"invite": "changed-network-metadata"}),
            )
            .await
            .unwrap();
            assert!(
                store::projection::revision(&engine.store.pool)
                    .await
                    .unwrap()
                    > source_revision
            );
            let grant = engine
                .act(
                    Action::CreateKarmaGrant {
                        request_id: "grant".into(),
                        slug: Slug::new("stock-authority").unwrap(),
                        grant: DelegationGrantSpec {
                            schema: DelegationGrantSchema::V1,
                            purpose: "Change stock".into(),
                            program_uid: TypedUid::new(ReferenceKind::Program, program).unwrap(),
                            program_revision: GrantProgramRevisionScope::AnyActive,
                            candidate_templates: GrantTemplateScope::Any,
                            capabilities: CapabilitySet::new([Capability::RecordAddQuantity]),
                            targets: GrantTargetScope::Any,
                            budget: Default::default(),
                            valid_from: TimestampMs::from_millis(case.start_ms).unwrap(),
                            expires_at: TimestampMs::from_millis(case.end_ms + 1000).unwrap(),
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap();
            let handle = engine.get_karma_grant(&grant).await.unwrap().unwrap();
            engine
                .act(
                    Action::ActivateKarmaGrant {
                        request_id: "activate".into(),
                        grant_uid: grant.clone(),
                        expected_handle_revision: 1,
                        revision_hash: handle.head_revision_hash,
                    },
                    None,
                )
                .await
                .unwrap();
            grant
        })
        .await;
    let revision = engine
        .get_karma_grant(&grant)
        .await
        .unwrap()
        .unwrap()
        .head_revision_hash;
    let snapshot = directory.path().join("input.sqlite");
    engine.store.snapshot_into(&snapshot).await.unwrap();
    case.cells[0].seed.clear();
    case.cells[0].database = Some(Database {
        file: "input.sqlite".into(),
        hash: simulation::artifacts::file_hash(&snapshot).unwrap(),
    });
    for (id, at, event) in [
        ("person", 0, Event::PersonKey { person: owner }),
        (
            "revoke",
            10,
            Event::Action {
                invocation: Invocation {
                    id: "revoke".into(),
                    actor: None,
                    action: Action::RevokeKarmaGrant {
                        request_id: "revoke".into(),
                        grant_uid: grant.clone(),
                        expected_handle_revision: 2,
                    },
                },
            },
        ),
        ("restart", 20, Event::Restart {}),
        (
            "stale",
            30,
            Event::Action {
                invocation: Invocation {
                    id: "stale".into(),
                    actor: None,
                    action: Action::ActivateKarmaGrant {
                        request_id: "stale".into(),
                        grant_uid: grant.clone(),
                        expected_handle_revision: 2,
                        revision_hash: revision,
                    },
                },
            },
        ),
    ] {
        case.inputs.push(Input {
            id: id.into(),
            cell: "a".into(),
            at_ms: case.start_ms + at,
            event,
        });
    }
    case.checks.push(Check {
        options: Default::default(),
        id: "stale-authority-refused".into(),
        predicate: Predicate::ExpectedRefusal {
            input: "stale".into(),
            refusal: Refusal::Conflict {
                code: "karma_stale_handle_revision".into(),
            },
        },
    });
    let original = directory.path().join("original");
    let result = simulation::artifacts::execute_with_sources(case, &original, directory.path())
        .await
        .unwrap();
    assert_eq!(
        result.result.verdict,
        Verdict::Passed,
        "{:?} {:?}",
        result.result,
        result.findings
    );
    let final_store = store::Store::open_existing_durable(&format!(
        "sqlite://{}",
        original.join("working/a.sqlite").display()
    ))
    .await
    .unwrap();
    let final_engine = engine::Engine::new(final_store).await.unwrap();
    assert_eq!(
        final_engine
            .get_karma_grant(&grant)
            .await
            .unwrap()
            .unwrap()
            .status,
        GrantStatus::Revoked
    );
    assert!(matches!(
        simulation::artifacts::replay(&original, &directory.path().join("repeated"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
}
