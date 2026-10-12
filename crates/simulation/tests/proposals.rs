use base64::Engine as _;
use engine::{actions::Action, record_change::Mutation};
use nucleus::simulation::{ReplayStatus, Verdict};
use simulation::{
    artifacts, comparison, fixtures,
    proposal::{Change, Edit, Proposal},
    scenario::{Database, Event, Input, Invocation, Scenario},
    world::World,
};

fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}

struct Fixture {
    directory: tempfile::TempDir,
    world: World,
    scenario: Scenario,
    record: String,
}

async fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let mut scenario = fixtures::daily();
    let world = World::open(scenario.clone(), &directory.path().join("source"))
        .await
        .unwrap();
    let node = &world.nodes["a"];
    let record = node
        .execution
        .scope(node.engine().act(
            Action::CreateRecord {
                slug: Some("appointment".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Appointment".into(),
                body: "Original description".into(),
                quantity: -1.0,
            },
            None,
        ))
        .await
        .unwrap()
        .created
        .unwrap();
    node.execution.scope(node.engine().act(Action::SetExtension {
        target: record.clone(), namespace: "work".into(),
        fds: serde_json::json!({"start":"2030-01-03T09:00:00Z","due":"2030-01-03T10:00:00Z","estimate_min":60}),
    }, None)).await.unwrap();
    let path = directory.path().join("input.sqlite");
    node.engine().store.snapshot_into(&path).await.unwrap();
    scenario.cells[0].database = Some(Database {
        file: "input.sqlite".into(),
        hash: artifacts::file_hash(&path).unwrap(),
    });
    scenario.cells[0].seed.clear();
    scenario.proposal = Some(
        Proposal::capture(
            node.engine(),
            &node.engine().store,
            None,
            "a",
            scenario.start_ms,
            &[record.clone()],
        )
        .await
        .unwrap(),
    );
    Fixture {
        directory,
        world,
        scenario,
        record,
    }
}

#[test]
fn record_drafts_compare_one_base_preserve_background_events_and_replay() {
    run(async {
        let mut fixture = fixture().await;
        let live = fixture.world.nodes["a"].engine();
        let before = live.store.state_hash().await.unwrap();
        let stock = store::records::resolve(&live.store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        let id = "background-add".to_string();
        fixture.scenario.inputs.push(Input {
            id: id.clone(),
            cell: "a".into(),
            at_ms: fixture.scenario.start_ms + 86_400_000 + 10,
            event: Event::Action {
                invocation: Invocation {
                    id,
                    actor: None,
                    action: Action::AddQuantityExact {
                        target: stock,
                        delta: nucleus::DecimalValue::parse_inferred("2").unwrap(),
                    },
                },
            },
        });
        fixture
            .scenario
            .checks
            .retain(|check| check.id != "final-quantity");
        fixture.scenario.checks.retain(|check| {
            !matches!(
                check.predicate,
                nucleus::simulation::Predicate::QuantityEquals { .. }
            )
        });
        let proposal = fixture.scenario.proposal.as_mut().unwrap();
        let mut work = proposal.records[0].work.clone();
        work["start"] = "2030-01-04T09:00:00Z".into();
        work["due"] = "2030-01-04T10:00:00Z".into();
        proposal.changes = vec![
            Change {
                id: "date".into(),
                record: fixture.record.clone(),
                after_ms: 0,
                edit: Edit::Work { value: work },
            },
            Change {
                id: "text".into(),
                record: fixture.record.clone(),
                after_ms: 0,
                edit: Edit::Text {
                    head: Some("$100 appointment".into()),
                    body: Some("$a is literal text 👩‍💻".into()),
                },
            },
            Change {
                id: "quantity".into(),
                record: fixture.record.clone(),
                after_ms: 100,
                edit: Edit::Property {
                    mutation: Mutation::Quantity {
                        value: "-2.75".into(),
                    },
                },
            },
        ];
        let result = comparison::run(
            fixture.scenario,
            &fixture.directory.path().join("comparison"),
            fixture.directory.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            result.proposed.verdict,
            Verdict::Passed,
            "{:?}",
            result.proposed_failure
        );
        assert_eq!(
            result.baseline.verdict,
            Verdict::Passed,
            "{:?}",
            result.baseline_failure
        );
        let changed = result
            .changes
            .iter()
            .find(|change| change.record == fixture.record)
            .unwrap();
        let proposed = changed.proposed.as_ref().unwrap().state.as_ref().unwrap();
        let baseline = changed.baseline.as_ref().unwrap().state.as_ref().unwrap();
        assert_eq!(proposed.work["start"], "2030-01-04T09:00:00Z");
        assert_eq!(baseline.work["start"], "2030-01-03T09:00:00Z");
        assert_eq!(proposed.head, "$100 appointment");
        assert_eq!(proposed.body, "$a is literal text 👩‍💻");
        assert_eq!(proposed.quantity.to_string(), "-2.75");
        let left = artifacts::load(std::path::Path::new(&result.proposed_run)).unwrap();
        let right = artifacts::load(std::path::Path::new(&result.baseline_run)).unwrap();
        assert_eq!(
            left.manifest.sources["a"].database,
            right.manifest.sources["a"].database
        );
        assert_eq!(
            left.manifest.sources["a"].state,
            right.manifest.sources["a"].state
        );
        assert!(right.events.iter().any(|event| matches!(&event.observation, nucleus::simulation::Observation::ActionAccepted { input, .. } if input == "background-add")));
        let replay = artifacts::replay(
            std::path::Path::new(&result.proposed_run),
            &fixture.directory.path().join("replay"),
        )
        .await
        .unwrap();
        assert!(
            matches!(replay, ReplayStatus::Verified { .. }),
            "{replay:?}"
        );
        assert_eq!(before, live.store.state_hash().await.unwrap());
    });
}

#[test]
fn alternative_proposals_share_the_same_base_and_reject_different_backgrounds() {
    run(async {
        let mut fixture = fixture().await;
        fixture
            .scenario
            .proposal
            .as_mut()
            .unwrap()
            .changes
            .push(Change {
                id: "title".into(),
                record: fixture.record.clone(),
                after_ms: 0,
                edit: Edit::Text {
                    head: Some("First option".into()),
                    body: None,
                },
            });
        let mut other = fixture.scenario.clone();
        other.proposal.as_mut().unwrap().changes[0].edit = Edit::Text {
            head: Some("Second option".into()),
            body: None,
        };
        let (mut first, mut second) = comparison::open_pair(
            fixture.scenario.clone(),
            other.clone(),
            &fixture.directory.path().join("options"),
            fixture.directory.path(),
        )
        .await
        .unwrap();
        while first.step().await.unwrap() {}
        while second.step().await.unwrap() {}
        let result = comparison::save(
            &fixture.directory.path().join("options"),
            first.finish().await.unwrap(),
            second.finish().await.unwrap(),
        )
        .unwrap();
        let changed = result
            .changes
            .iter()
            .find(|change| change.record == fixture.record)
            .unwrap();
        assert_eq!(
            changed
                .proposed
                .as_ref()
                .unwrap()
                .state
                .as_ref()
                .unwrap()
                .head,
            "First option"
        );
        assert_eq!(
            changed
                .baseline
                .as_ref()
                .unwrap()
                .state
                .as_ref()
                .unwrap()
                .head,
            "Second option"
        );
        other.inputs.push(Input {
            id: "different".into(),
            cell: "a".into(),
            at_ms: other.start_ms,
            event: Event::Clock { offset_ms: 100 },
        });
        assert!(
            comparison::open_pair(
                fixture.scenario,
                other,
                &fixture.directory.path().join("invalid"),
                fixture.directory.path()
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn stale_properties_sources_and_unsupported_edits_cannot_start_verified_runs() {
    run(async {
        let mut fixture = fixture().await;
        fixture
            .scenario
            .proposal
            .as_mut()
            .unwrap()
            .changes
            .push(Change {
                id: "title".into(),
                record: fixture.record.clone(),
                after_ms: 0,
                edit: Edit::Text {
                    head: Some("Proposed".into()),
                    body: None,
                },
            });
        let mut stale = fixture.scenario.clone();
        stale.proposal.as_mut().unwrap().records[0].revision += 1;
        let result = artifacts::Session::open(
            stale,
            &fixture.directory.path().join("stale"),
            fixture.directory.path(),
        )
        .await;
        assert!(result.is_err());
        assert!(!fixture.directory.path().join("stale/result.json").exists());
        let mut changed = fixture.scenario.clone();
        changed.proposal.as_mut().unwrap().source =
            nucleus::karma::CanonicalHash::parse(format!("sha256:{}", "0".repeat(64))).unwrap();
        assert!(
            artifacts::Session::open(
                changed,
                &fixture.directory.path().join("changed-source"),
                fixture.directory.path()
            )
            .await
            .is_err()
        );
        let mut unsupported = fixture.scenario;
        unsupported.proposal.as_mut().unwrap().changes[0].edit = Edit::Property {
            mutation: Mutation::Timer { running: true },
        };
        assert!(unsupported.validate().is_err());
    });
}

#[test]
fn readable_records_do_not_grant_permission_to_apply_proposed_edits() {
    run(async {
        let mut fixture = fixture().await;
        let node = &fixture.world.nodes["a"];
        let actor = node
            .execution
            .scope(node.engine().act(
                Action::CreateRecord {
                    slug: Some("reader".into()),
                    kind: nucleus::RecordKind::Person,
                    head: "Reader".into(),
                    body: String::new(),
                    quantity: 1.0,
                },
                None,
            ))
            .await
            .unwrap()
            .created
            .unwrap();
        let role = store::auth::ensure_role(&node.engine().store.pool, "proposal-reader")
            .await
            .unwrap();
        let read = store::auth::ensure_permission(&node.engine().store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::grant(&node.engine().store.pool, role, read)
            .await
            .unwrap();
        store::auth::create_credential(&node.engine().store.pool, &actor, "reader", "hash", role)
            .await
            .unwrap();
        store::visibility::grant(
            &node.engine().store.pool,
            "actor",
            Some(&actor),
            &fixture.record,
        )
        .await
        .unwrap();
        let path = fixture.directory.path().join("reader.sqlite");
        node.engine().store.snapshot_into(&path).await.unwrap();
        fixture.scenario.cells[0].database = Some(Database {
            file: "reader.sqlite".into(),
            hash: artifacts::file_hash(&path).unwrap(),
        });
        let mut proposal = Proposal::capture(
            node.engine(),
            &node.engine().store,
            Some(&actor),
            "a",
            fixture.scenario.start_ms,
            &[fixture.record.clone()],
        )
        .await
        .unwrap();
        proposal.changes.push(Change {
            id: "unauthorized".into(),
            record: fixture.record.clone(),
            after_ms: 0,
            edit: Edit::Text {
                head: Some("Unauthorized".into()),
                body: None,
            },
        });
        fixture.scenario.proposal = Some(proposal);
        let before = node.engine().store.state_hash().await.unwrap();
        assert!(
            artifacts::Session::open(
                fixture.scenario,
                &fixture.directory.path().join("denied"),
                fixture.directory.path()
            )
            .await
            .is_err()
        );
        assert_eq!(before, node.engine().store.state_hash().await.unwrap());
        assert!(!fixture.directory.path().join("denied/result.json").exists());
    });
}

#[test]
fn relation_changes_use_normal_assertion_semantics_and_protected_result_files() {
    run(async {
        let mut fixture = fixture().await;
        let node = &fixture.world.nodes["a"];
        let lingua = store::linguas::ensure_local(&node.engine().store.pool)
            .await
            .unwrap();
        let predicate = node
            .execution
            .scope(node.engine().act(
                Action::CreateConcept {
                    lingua,
                    name: "related".into(),
                    parents: Vec::new(),
                },
                None,
            ))
            .await
            .unwrap()
            .created
            .unwrap();
        let object = store::records::resolve(&node.engine().store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        let path = fixture.directory.path().join("relations.sqlite");
        node.engine().store.snapshot_into(&path).await.unwrap();
        fixture.scenario.cells[0].database = Some(Database {
            file: "relations.sqlite".into(),
            hash: artifacts::file_hash(&path).unwrap(),
        });
        let mut proposal = Proposal::capture(
            node.engine(),
            &node.engine().store,
            None,
            "a",
            fixture.scenario.start_ms,
            &[fixture.record.clone()],
        )
        .await
        .unwrap();
        proposal.changes.push(Change {
            id: "relation".into(),
            record: fixture.record.clone(),
            after_ms: 0,
            edit: Edit::Property {
                mutation: Mutation::Assertion {
                    predicate: predicate.clone(),
                    object: Some(object.clone()),
                    quantity: Some("1.25".into()),
                    unit: None,
                },
            },
        });
        fixture.scenario.proposal = Some(proposal);
        let result = comparison::run(
            fixture.scenario,
            &fixture.directory.path().join("relations"),
            fixture.directory.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            result.proposed.verdict,
            Verdict::Passed,
            "{:?}",
            result.proposed_failure
        );
        let outcome = result
            .changes
            .iter()
            .find(|change| change.record == fixture.record)
            .unwrap()
            .proposed
            .as_ref()
            .unwrap()
            .state
            .as_ref()
            .unwrap();
        assert!(
            outcome
                .assertions
                .iter()
                .any(|assertion| assertion.predicate == predicate
                    && assertion.object.as_ref() == Some(&object)
                    && assertion.quantity.unwrap().to_string() == "1.25")
        );
        let path = std::path::Path::new(&result.proposed_run);
        assert!(
            artifacts::load(path)
                .unwrap()
                .manifest
                .files
                .contains_key("records.json")
        );
        std::fs::write(path.join("records.json"), "[]\n").unwrap();
        assert!(matches!(
            artifacts::load(path),
            Err(nucleus::simulation::ArtifactError::Corrupt { .. })
        ));
    });
}

#[test]
fn oversized_record_evidence_stays_incomplete_bounded_and_replayable() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut scenario = fixtures::daily();
        scenario.cells[0].seed.clear();
        scenario.end_ms = scenario.start_ms;
        scenario.checks.retain(|check| {
            matches!(
                check.predicate,
                nucleus::simulation::Predicate::FactChain {}
            )
        });
        scenario.limits.evidence_bytes = 4096;
        scenario.inputs.push(Input {
            id: "large-record".into(),
            cell: "a".into(),
            at_ms: scenario.start_ms,
            event: Event::Action {
                invocation: Invocation {
                    id: "large-record".into(),
                    actor: None,
                    action: Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: "Large Record".into(),
                        body: "x".repeat(8192),
                        quantity: 0.0,
                    },
                },
            },
        });
        let destination = directory.path().join("bounded");
        let result = artifacts::execute(scenario, &destination).await.unwrap();
        assert_eq!(result.result.verdict, Verdict::Inconclusive);
        assert!(matches!(
            result.result.stop,
            nucleus::simulation::Stop::EvidenceBudget {}
        ));
        assert!(result.records.is_empty());
        assert!(
            std::fs::metadata(destination.join("records.json"))
                .unwrap()
                .len()
                < 4096
        );
        artifacts::load(&destination).unwrap();
        let replay = artifacts::replay(&destination, &directory.path().join("bounded-replay"))
            .await
            .unwrap();
        assert!(
            matches!(replay, ReplayStatus::Verified { .. }),
            "{replay:?}"
        );
    });
}

#[test]
fn executable_bodies_are_guarded_through_both_text_action_paths() {
    run(async {
        let fixture = fixture().await;
        let node = &fixture.world.nodes["a"];
        let body = nucleus::component::Document::encode(nucleus::component::Composition {
            name: "Protected composition".into(),
            origin: None,
            parts: vec![nucleus::component::Part {
                settings: Default::default(),
                id: "text".into(),
                position: [0, 0],
                size: [100, 100],
                events: Vec::new(),
                component: nucleus::component::ComponentState::Text {
                    text: "Content".into(),
                },
            }],
        })
        .unwrap();
        let record = node
            .execution
            .scope(node.engine().act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Composition".into(),
                    body,
                    quantity: 0.0,
                },
                None,
            ))
            .await
            .unwrap()
            .created
            .unwrap();
        let mut proposal = Proposal::capture(
            node.engine(),
            &node.engine().store,
            None,
            "a",
            fixture.scenario.start_ms,
            &[record.clone()],
        )
        .await
        .unwrap();
        let before = node.engine().store.state_hash().await.unwrap();
        let captured = &proposal.records[0];
        let document = engine::collab_guard::AcceptedDoc::from_trusted_text(
            &captured.head,
            &captured.body,
            &engine::collab::limits(),
        )
        .unwrap();
        let prepared = document
            .prepare_text(42, None, Some("Replacement"), &engine::collab::limits())
            .unwrap();
        let update_base64 =
            base64::engine::general_purpose::STANDARD.encode(prepared.normalized_delta());
        for edit in [
            Edit::Text {
                head: None,
                body: Some("Replacement".into()),
            },
            Edit::Property {
                mutation: Mutation::Text { update_base64 },
            },
        ] {
            proposal.changes = vec![Change {
                id: "body".into(),
                record: record.clone(),
                after_ms: 0,
                edit,
            }];
            let error = proposal.bind(node.engine()).await.unwrap_err();
            assert!(error.to_string().contains("composition editor"), "{error}");
        }
        proposal.changes[0].edit = Edit::Text {
            head: Some("New title".into()),
            body: None,
        };
        proposal.bind(node.engine()).await.unwrap();
        assert_eq!(before, node.engine().store.state_hash().await.unwrap());
    });
}
