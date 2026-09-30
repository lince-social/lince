use std::future::Future;

use engine::{
    Engine,
    actions::Action,
    karma_habits::{Input as HabitInput, Preview},
};
use nucleus::simulation::{CheckDefinition, Predicate, Quantity, ReplayStatus, Verdict};
use simulation::scenario::{Database, Event, Input, Invocation};

fn run(test: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(test);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn the_instinct_import_uses_the_shared_runtime_and_replays_daily_completion_and_restart() {
    run(async {
        let start = 1_893_456_000_000;
        let directory = tempfile::tempdir().unwrap();
        let source = Engine::open_memory().await.unwrap();
        source
            .install_karma_runtime_config(
                engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("habit-source".into())
                    .unwrap(),
            )
            .unwrap();
        let input = HabitInput {
            time: "00:01".into(),
            ..Default::default()
        };
        let preview: Preview = serde_json::from_value(
            source
                .act_at(
                    Action::PreviewKarmaHabit {
                        input: input.clone(),
                    },
                    None,
                    chrono::DateTime::from_timestamp_millis(start).unwrap(),
                )
                .await
                .unwrap()
                .data
                .unwrap(),
        )
        .unwrap();
        let before = source.store.state_hash().await.unwrap();
        let path = directory.path().join("source.sqlite");
        source.store.snapshot_into(&path).await.unwrap();
        let mut case = simulation::fixtures::current_database(start);
        case.name = "instinct-cleaning-room".into();
        case.end_ms = preview.first_at_ms + 86_400_001;
        case.cells[0].database = Some(Database {
            file: "source.sqlite".into(),
            hash: simulation::artifacts::file_hash(&path).unwrap(),
        });
        case.cells[0].seed = vec![Invocation {
            id: "import".into(),
            actor: None,
            action: Action::ImportKarmaHabit {
                input,
                expected_preview: preview.fingerprint,
                request_id: "import-room".into(),
            },
        }];
        case.inputs = vec![
            Input {
                id: "complete".into(),
                cell: "current".into(),
                at_ms: preview.first_at_ms + 2,
                event: Event::Action {
                    invocation: Invocation {
                        id: "complete".into(),
                        actor: None,
                        action: Action::SetQuantityExact {
                            target: "cleaning-room".into(),
                            amount: "0".into(),
                        },
                    },
                },
            },
            Input {
                id: "restart".into(),
                cell: "current".into(),
                at_ms: preview.first_at_ms + 3,
                event: Event::Restart {},
            },
        ];
        case.checks = [
            (preview.first_at_ms - 1, 0),
            (preview.first_at_ms + 1, -1),
            (preview.first_at_ms + 4, 0),
            (case.end_ms, -1),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (at_ms, amount))| CheckDefinition {
            id: format!("quantity-{index}"),
            options: Default::default(),
            predicate: Predicate::QuantityEquals {
                cell: "current".into(),
                record: "cleaning-room".into(),
                at_ms,
                expected: Quantity {
                    value: store::exact::integer(amount),
                    unit: None,
                },
            },
        })
        .collect();
        let mut session = simulation::artifacts::Session::open(
            case,
            &directory.path().join("run"),
            directory.path(),
        )
        .await
        .unwrap();
        while session.step().await.unwrap() {}
        let result = session.finish().await.unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?} {:?}",
            result.result,
            result.findings
        );
        assert!(matches!(
            simulation::artifacts::replay(&result.directory, &directory.path().join("replay"))
                .await
                .unwrap(),
            ReplayStatus::Verified { .. }
        ));
        assert_eq!(source.store.state_hash().await.unwrap(), before);
        assert!(
            store::recurrence::all(&source.store.pool)
                .await
                .unwrap()
                .is_empty()
        );
    });
}
