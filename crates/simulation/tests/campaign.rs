fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
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

#[test]
fn completed_cases_resume_without_repeating_their_indices() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            simulation::campaign::run(directory.path(), Some(1))
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            simulation::campaign::run(directory.path(), Some(1))
                .await
                .unwrap(),
            0
        );
        let campaign = directory
            .path()
            .join("campaigns")
            .join(simulation::BUILD_HASH.trim_start_matches("sha256:"));
        let cursor: simulation::campaign::Cursor =
            serde_json::from_slice(&std::fs::read(campaign.join("cursor.json")).unwrap()).unwrap();
        assert_eq!(cursor.next_case, 2);
        for index in 0..2 {
            let summary: simulation::campaign::Summary = serde_json::from_slice(
                &std::fs::read(campaign.join("summaries").join(format!("{index:08}.json")))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(summary.index, index);
            assert!(summary.retained.is_none());
        }
    });
}

#[test]
fn reduction_removes_irrelevant_inputs_and_reexecutes_the_same_failure() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = simulation::fixtures::daily();
        case.checks.push(simulation::scenario::Check {
            id: "nonnegative".into(),
            predicate: nucleus::simulation::Predicate::Nonnegative {
                cell: "a".into(),
                record: "stock".into(),
            },
        });
        case.inputs.push(simulation::scenario::Input {
            id: "unrelated-restart".into(),
            cell: "a".into(),
            at_ms: case.start_ms + 1000,
            event: simulation::scenario::Event::Restart {},
        });
        let source = directory.path().join("source");
        simulation::artifacts::execute(case, &source).await.unwrap();
        let reduction =
            simulation::campaign::minimize(&source, &directory.path().join("reduction"), 4)
                .await
                .unwrap();
        assert!(reduction.verified);
        assert_eq!(reduction.remaining_inputs, 0);
        assert!(source.join("result.json").exists());
    });
}

#[test]
fn reduction_keeps_a_prerequisite_when_removing_it_introduces_another_failure() {
    run(async {
        use engine::actions::Action;
        use simulation::scenario::{Event, Input, Invocation};
        let directory = tempfile::tempdir().unwrap();
        let mut case = simulation::fixtures::daily();
        case.checks.push(simulation::scenario::Check {
            id: "nonnegative".into(),
            predicate: nucleus::simulation::Predicate::Nonnegative {
                cell: "a".into(),
                record: "stock".into(),
            },
        });
        for (at, id, action) in [
            (
                1,
                "prerequisite",
                Action::CreateRecord {
                    slug: Some("scratch".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Scratch".into(),
                    body: String::new(),
                    quantity: 1.0,
                },
            ),
            (
                2,
                "dependent",
                Action::SetQuantity {
                    target: "scratch".into(),
                    value: 2.0,
                },
            ),
        ] {
            case.inputs.push(Input {
                id: id.into(),
                cell: "a".into(),
                at_ms: case.start_ms + at,
                event: Event::Action {
                    invocation: Invocation {
                        id: id.into(),
                        actor: None,
                        action,
                    },
                },
            });
        }
        let source = directory.path().join("source");
        simulation::artifacts::execute(case, &source).await.unwrap();
        let reduced = simulation::campaign::minimize(&source, &directory.path().join("reduced"), 1)
            .await
            .unwrap();
        assert!(reduced.verified);
        assert_eq!(reduced.remaining_inputs, 2);
    });
}
