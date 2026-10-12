use nucleus::simulation::{ReplayStatus, Stop, Verdict};
use simulation::{artifacts, fixtures};

#[tokio::test]
async fn stepping_and_resuming_preserve_the_headless_result_and_cancellation_replays() {
    let directory = tempfile::tempdir().unwrap();
    let stepped = directory.path().join("stepped");
    let mut session =
        artifacts::Session::open(fixtures::daily(), &stepped, std::path::Path::new("."))
            .await
            .unwrap();
    assert!(session.step().await.unwrap());
    assert_eq!(session.world.steps, 1);
    let time = session.world.now_ms;
    let steps = session.world.steps;
    tokio::task::yield_now().await;
    assert_eq!(session.world.now_ms, time);
    assert_eq!(session.world.steps, steps);
    assert!(!stepped.join("result.json").exists());
    while session.step().await.unwrap() {}
    let result = session.finish().await.unwrap();
    assert_eq!(result.result.verdict, Verdict::Passed);
    let replay = artifacts::replay(&stepped, &directory.path().join("replayed"))
        .await
        .unwrap();
    if !matches!(replay, ReplayStatus::Verified { .. }) {
        panic!("{replay:?}; evidence retained in {}", directory.keep().display());
    }
    let stopped = directory.path().join("stopped");
    let mut session =
        artifacts::Session::open(fixtures::daily(), &stopped, std::path::Path::new("."))
            .await
            .unwrap();
    assert!(session.step().await.unwrap());
    session.cancel();
    let result = session.finish().await.unwrap();
    assert_eq!(result.result.stop, Stop::Cancelled {});
    assert_eq!(result.result.verdict, Verdict::Inconclusive);
    assert_eq!(result.result.steps, 1);
    let replay = artifacts::replay(&stopped, &directory.path().join("stopped-replayed"))
        .await
        .unwrap();
    if !matches!(replay, ReplayStatus::Verified { .. }) {
        panic!("{replay:?}; evidence retained in {}", directory.keep().display());
    }
}
