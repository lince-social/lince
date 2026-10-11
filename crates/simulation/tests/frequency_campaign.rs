use nucleus::simulation::{Predicate, ReplayStatus, Verdict};
use simulation::{artifacts, fixtures::frequencies, scenario::Scenario};

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

fn small() -> Scenario {
    let mut case = frequencies::shared(0);
    case.name = "frequency-rewind-regression".into();
    let keep = |name: &str| {
        name == "daily"
            || ["daily-task-0", "daily-task-1", "daily-task-2"]
                .iter()
                .any(|target| {
                    name == *target
                        || name == format!("rule-{target}")
                        || name == format!("complete-{target}")
                })
    };
    case.cells[0].seed.retain(|invocation| keep(&invocation.id));
    case.inputs.retain(|input| {
        keep(&input.id)
            || matches!(input.id.as_str(), "rewind-daily" | "retry-rewind")
            || input.id.starts_with("restart-")
            || input.id.starts_with("complete-")
    });
    for input in &mut case.inputs {
        if let simulation::scenario::Event::Action { invocation } = &mut input.event
            && let engine::actions::Action::AddQuantityGroupExact { changes } =
                &mut invocation.action
        {
            changes.retain(|record, _| keep(record));
        }
    }
    case.inputs.retain(|input| !matches!(&input.event, simulation::scenario::Event::Action {invocation}
        if matches!(&invocation.action,engine::actions::Action::AddQuantityGroupExact {changes} if changes.is_empty())));
    for check in &mut case.checks {
        if let Predicate::QuantitiesEqual { records, .. } = &mut check.predicate {
            records.retain(|record, _| keep(record));
        }
    }
    case
}

#[test]
fn random_frequency_cases_are_serializable_reproducible_and_in_the_campaign() {
    for seed in 0..64 {
        let case = frequencies::shared(seed);
        case.validate().unwrap();
        let wire = serde_json::to_vec(&case).unwrap();
        let restored: Scenario = serde_json::from_slice(&wire).unwrap();
        assert_eq!(wire, serde_json::to_vec(&restored).unwrap());
        assert_eq!(
            wire,
            serde_json::to_vec(&frequencies::shared(seed)).unwrap()
        );
        assert_eq!(
            case.checks
                .iter()
                .filter(|check| matches!(check.predicate, Predicate::QuantitiesEqual { .. }))
                .count(),
            100
        );
        for check in &case.checks {
            if let Predicate::QuantitiesEqual { records, .. } = &check.predicate {
                assert_eq!(records.len(), 132);
            }
        }
    }
    assert_eq!(simulation::campaign::generate(23).cells[0].seed.len(), 268);
    for seed in 0..4 {
        let generated = simulation::campaign::generate(seed * 24 + 23);
        generated.validate().unwrap();
        let expected = frequencies::shared(seed);
        assert_eq!(generated.start_ms, expected.start_ms);
        assert_eq!(
            serde_json::to_vec(&generated.cells[0].seed).unwrap(),
            serde_json::to_vec(&expected.cells[0].seed).unwrap()
        );
    }
}

#[test]
fn rewinding_an_applied_daily_boundary_resets_completed_tasks_and_replays_exactly() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let result = artifacts::execute(small(), &directory.path().join("rewind"))
            .await
            .unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?}",
            result.findings
        );
        assert!(matches!(
            artifacts::replay(&result.directory, &directory.path().join("replay"))
                .await
                .unwrap(),
            ReplayStatus::Verified { .. }
        ));
    });
}

#[test]
fn shared_frequencies_set_every_task_correctly_for_every_day_of_randomized_hundred_day_runs() {
    let directory = std::env::var_os("LINCE_FREQUENCY_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("lince-frequency-campaign-{}", std::process::id()))
        });
    std::fs::create_dir_all(&directory).unwrap();
    let workers = [0, 1, 77, 902].map(|seed| {
        let directory = directory.clone();
        std::thread::spawn(move || {
            run(async move {
                let case = simulation::campaign::generate(seed * 24 + 23);
                let result = artifacts::execute(case, &directory.join(format!("seed-{seed}")))
                    .await
                    .unwrap();
                assert_eq!(
                    result.result.verdict,
                    Verdict::Passed,
                    "seed {seed}: {:?}",
                    result.findings
                );
                assert_eq!(
                    result
                        .cost
                        .checks
                        .iter()
                        .filter(|cost| cost.check.starts_with("day-"))
                        .count(),
                    100
                );
                for cost in result
                    .cost
                    .checks
                    .iter()
                    .filter(|cost| cost.check.starts_with("day-"))
                {
                    assert_eq!(cost.evaluations, 132);
                }
                println!(
                    "100 days passed for seed {seed}: {}",
                    result.directory.display()
                );
            })
        })
    });
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
#[ignore = "requires an isolated workspace snapshot and its owner authorization"]
fn copied_workspace_resets_every_frequency_task_on_every_simulated_day() {
    run(async {
        use engine::actions::Action;
        use nucleus::karma::{Cadence, Slug, TimestampMs};
        use nucleus::simulation::Quantity;
        use simulation::scenario::{Check, Database, Event, Input, Invocation};
        use store::sqlx::Row;
        let input = std::path::PathBuf::from(
            std::env::var_os("LINCE_FREQUENCY_COPY").expect("provide a private database copy"),
        );
        let root_path = std::path::PathBuf::from(
            std::env::var_os("LINCE_FREQUENCY_ROOT_KEY")
                .expect("provide the copy owner's root authorization"),
        );
        let output = std::path::PathBuf::from(
            std::env::var_os("LINCE_FREQUENCY_EVIDENCE").expect("provide an evidence directory"),
        );
        let staging = tempfile::tempdir().unwrap();
        let prepared = staging.path().join("prepared.sqlite");
        std::fs::copy(&input, &prepared).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let until = now + 100 * frequencies::DAY;
        let execution = nucleus::execution::Execution::new([93; 32], now).unwrap();
        let engine = execution
            .scope(async {
                engine::Engine::new(
                    store::Store::open_existing_durable(&format!(
                        "sqlite://{}",
                        prepared.display()
                    ))
                    .await
                    .unwrap(),
                )
                .await
                .unwrap()
            })
            .await;
        execution
            .scope(async {
                let organ = store::organs::local(&engine.store.pool)
                    .await
                    .unwrap()
                    .unwrap();
                let bytes = std::fs::read(root_path).unwrap();
                let secret: [u8; 32] = bytes
                    .try_into()
                    .unwrap_or_else(|_| panic!("invalid owner root key length"));
                let root = engine::trust::Signer::from_bytes(
                    &organ.uid,
                    engine::roster::ROOT_KEY_ID,
                    secret,
                );
                let mut roster = engine.roster_of(&organ.uid).await.unwrap().unwrap();
                assert_eq!(root.public_key_b64(), roster.roster.root_key);
                roster.roster.version += 1;
                roster.roster.not_after =
                    chrono::DateTime::from_timestamp_millis(until + frequencies::DAY)
                        .unwrap()
                        .to_rfc3339();
                roster.signature = root
                    .sign_bytes(&engine::roster::roster_signing_payload(&roster.roster).unwrap());
                assert_eq!(
                    engine.adopt_roster(&roster).await.unwrap(),
                    engine::roster::RosterOutcome::Accepted
                );
            })
            .await;
        let mut case = simulation::fixtures::daily();
        case.name = "copied-workspace-frequency-100-days".into();
        case.start_ms = now;
        case.end_ms = until;
        case.cells[0].seed.clear();
        case.checks.clear();
        case.inputs.clear();
        case.limits.steps = 50_000;
        case.limits.rule_evaluations = 500_000;
        case.limits.evidence_bytes = 512 * 1024 * 1024;
        let rows=store::sqlx::query("SELECT c.record_uid,c.condition_src,c.consequences_json,c.bindings_json FROM recurrence c JOIN record r ON r.uid=c.record_uid WHERE c.state='active' AND r.deleted_at IS NULL")
            .fetch_all(&engine.store.pool).await.unwrap();
        let mut tasks = Vec::new();
        for row in rows {
            let source: String = row.get("condition_src");
            let amount =
                nucleus::DecimalValue::parse_inferred(source.split(" * freq(@").next().unwrap())
                    .unwrap();
            let consequences: serde_json::Value =
                serde_json::from_str(row.get("consequences_json")).unwrap();
            assert_eq!(consequences, serde_json::json!([{"kind":"set-quantity"}]));
            let bindings: serde_json::Value =
                serde_json::from_str(row.get("bindings_json")).unwrap();
            assert_eq!(bindings.as_array().unwrap().len(), 1);
            let uid = bindings[0]["target"]["uid"].as_str().unwrap();
            let frequency = store::frequency::get(&engine.store.pool, uid)
                .await
                .unwrap()
                .unwrap();
            let cadence = frequency.cadence();
            assert_eq!(cadence.every.calendar_months(), Some(0));
            let interval = cadence.every.fixed_milliseconds().unwrap();
            let record: String = row.get("record_uid");
            let unit = store::records::get(&engine.store.pool, &record)
                .await
                .unwrap()
                .unwrap()
                .unit_uid
                .map(|uid| {
                    nucleus::karma::TypedUid::new(nucleus::karma::ReferenceKind::Unit, uid).unwrap()
                });
            tasks.push((
                record,
                amount,
                unit,
                frequency.uid.clone(),
                frequency.anchor().unwrap().timestamp_millis(),
                interval,
            ));
        }
        assert_eq!(
            tasks.len(),
            8,
            "all current Frequency-linked tasks must be checked"
        );
        let daily = store::records::resolve(&engine.store.pool, "diario")
            .await
            .unwrap()
            .unwrap();
        let handle = store::karma::frequencies::get_handle(&engine.store.pool, &daily.uid)
            .await
            .unwrap()
            .unwrap();
        let frequency = store::frequency::get(&engine.store.pool, &daily.uid)
            .await
            .unwrap()
            .unwrap();
        let anchor = frequency.anchor().unwrap().timestamp_millis();
        let first = anchor + ((now - anchor).div_euclid(frequencies::DAY) + 1) * frequencies::DAY;
        let invoke = |id: String, action| Invocation {
            id,
            actor: None,
            action,
        };
        for (index, (record, ..)) in tasks.iter().enumerate() {
            case.cells[0].seed.push(invoke(
                format!("initial-completion-{index}"),
                Action::SetQuantity {
                    target: record.clone(),
                    value: 0.0,
                },
            ));
        }
        let rewind = nucleus::karma::simple_frequency::frequency_from_cadence(
            Slug::new("diario").unwrap(),
            daily.head.clone(),
            &Cadence::every_days(1),
            TimestampMs::from_millis(first - 2 * frequencies::DAY).unwrap(),
        )
        .unwrap();
        case.cells[0].seed.push(invoke(
            "rewind-banho".into(),
            Action::SaveKarmaFrequency {
                request_id: "copied-rewind".into(),
                frequency_uid: Some(daily.uid.clone()),
                expected_handle_revision: Some(handle.handle_revision),
                frequency: rewind,
                restart: true,
            },
        ));
        case.checks.push(Check {
            id: "rewound-current-needs".into(),
            options: Default::default(),
            predicate: Predicate::QuantitiesEqual {
                cell: "a".into(),
                at_ms: now + 1,
                records: tasks
                    .iter()
                    .map(|(record, amount, unit, uid, ..)| {
                        (
                            record.clone(),
                            Quantity {
                                value: if uid == &daily.uid {
                                    *amount
                                } else {
                                    nucleus::fact::zero_delta()
                                },
                                unit: unit.clone(),
                            },
                        )
                    })
                    .collect(),
            },
        });
        for day in 0..100 {
            let boundary = first + day * frequencies::DAY;
            let records = tasks
                .iter()
                .map(|(record, amount, unit, _, anchor, interval)| {
                    let due = boundary >= *anchor && (boundary - anchor) % interval == 0;
                    (
                        record.clone(),
                        Quantity {
                            value: if due {
                                *amount
                            } else {
                                nucleus::fact::zero_delta()
                            },
                            unit: unit.clone(),
                        },
                    )
                })
                .collect();
            case.checks.push(Check {
                id: format!("day-{}", day + 1),
                options: Default::default(),
                predicate: Predicate::QuantitiesEqual {
                    cell: "a".into(),
                    records,
                    at_ms: boundary + 60_000,
                },
            });
            for (index, (record, ..)) in tasks.iter().enumerate() {
                let id = format!("complete-{day}-{index}");
                case.inputs.push(Input {
                    id: id.clone(),
                    cell: "a".into(),
                    at_ms: boundary + 61_000,
                    event: Event::Action {
                        invocation: invoke(
                            id,
                            Action::SetQuantity {
                                target: record.clone(),
                                value: 0.0,
                            },
                        ),
                    },
                });
            }
        }
        for (id, predicate) in [
            ("fact-chain", Predicate::FactChain {}),
            ("once-per-occurrence", Predicate::OncePerOccurrence {}),
            ("refusals", Predicate::NoUnexpectedRefusals {}),
        ] {
            case.checks.push(Check {
                id: id.into(),
                predicate,
                options: nucleus::simulation::checks::CheckOptions {
                    evaluation: nucleus::simulation::checks::Evaluation::End,
                    ..Default::default()
                },
            });
        }
        let snapshot = staging.path().join("input.sqlite");
        engine.store.snapshot_into(&snapshot).await.unwrap();
        engine.store.pool.close().await;
        case.cells[0].database = Some(Database {
            file: "input.sqlite".into(),
            hash: artifacts::file_hash(&snapshot).unwrap(),
        });
        let result = artifacts::execute_with_sources(
            case,
            &output.join("workspace-100-days"),
            staging.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?}",
            result.findings
        );
        println!(
            "100 copied-workspace days passed for all {} tasks: {}",
            tasks.len(),
            result.directory.display()
        );
    });
}

#[test]
fn batch_quantity_checks_detect_wrong_tasks_and_exhausted_budgets() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = small();
        case.name = "batch-check-regression".into();
        case.end_ms = case.start_ms + frequencies::DAY - 1;
        case.inputs.clear();
        case.checks.retain(|check| check.id == "day-1");
        let mut wrong = case.clone();
        if let Predicate::QuantitiesEqual { records, .. } = &mut wrong.checks[0].predicate {
            records.get_mut("daily-task-2").unwrap().value =
                nucleus::DecimalValue::parse_inferred("-2").unwrap();
        }
        let result = artifacts::execute(wrong, &directory.path().join("wrong"))
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Failed);
        assert_eq!(result.cost.checks[0].evaluations, 3);
        assert!(
            matches!(&result.findings[0].witness,nucleus::simulation::Witness::Quantity {expected,observed,..}
            if expected.value.to_string()=="-2" && observed.value.to_string()=="-1")
        );
        case.checking.evaluations = 2;
        let result = artifacts::execute(case, &directory.path().join("budget"))
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Inconclusive);
        assert_eq!(result.cost.checks[0].evaluations, 2);
    });
}
