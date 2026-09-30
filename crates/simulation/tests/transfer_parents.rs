use nucleus::simulation::{ReplayStatus, Verdict};

#[test]
fn nested_parent_result_reaches_other_organs_without_disclosing_the_required_branch() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false, true] {
                let case = simulation::fixtures::transfer::nested_parents(replicated);
                let milestones = ["waiting", "agreed", "settled"].map(|stage| (
                    case.inputs.iter().find(|input| input.id == format!("parent-send-{stage}")).unwrap().at_ms + 10, stage));
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("run");
                let mut session = simulation::artifacts::Session::open(case, &path, std::path::Path::new(".")).await.unwrap();
                for (at, stage) in milestones {
                    while session.world.next_ms().is_some_and(|next| next <= at) { assert!(session.step().await.unwrap()); }
                    let parent = session.world.resolve_reference("$parent");
                    let branch = session.world.resolve_reference("$branch");
                    let pool = &session.world.nodes["b"].engine().store.pool;
                    let projection: String = store::sqlx::query_scalar("SELECT projection FROM transfer_remote_reference WHERE transfer_uid = ?")
                        .bind(&parent).fetch_one(pool).await.unwrap();
                    let row: serde_json::Value = serde_json::from_str(&projection).unwrap();
                    assert_eq!(row["readiness"]["ready"], stage != "waiting", "{stage}: {row}");
                    assert_eq!(row["phase6"]["ready"], stage != "waiting");
                    assert_eq!(row["children_details_hidden"], true);
                    for secret in [&branch, "Private required branch"] { assert!(!projection.contains(secret), "{row}"); }
                    let own = store::transfer_agreement::read(&session.world.nodes["a"].engine().store.pool, &parent).await.unwrap().0;
                    assert_eq!(own.ready, stage != "waiting");
                    assert_eq!(own.settled, stage == "settled");
                }
                let branch = session.world.resolve_reference("$branch");
                let projections: Vec<String> = store::sqlx::query_scalar("SELECT projection FROM transfer_remote_reference WHERE projection IS NOT NULL")
                    .fetch_all(&session.world.nodes["b"].engine().store.pool).await.unwrap();
                for projection in projections {
                    assert!(!projection.contains(&branch), "{projection}");
                    assert!(!projection.contains("Private required branch"), "{projection}");
                }
                while session.step().await.unwrap() {}
                let run = session.finish().await.unwrap();
                if run.result.verdict != Verdict::Passed { panic!("{:?}; {}", run.findings, directory.keep().display()); }
                assert!(matches!(simulation::artifacts::replay(&path, &directory.path().join("replay")).await.unwrap(), ReplayStatus::Verified { .. }));
            }
        });
    }).unwrap().join().unwrap();
}
