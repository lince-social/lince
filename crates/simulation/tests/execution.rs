use nucleus::execution::Execution;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scoped_environment_survives_yields_without_leaking_into_other_tasks() {
    let mut tasks = Vec::new();
    for index in 1..=8 {
        tasks.push(tokio::spawn(async move {
            let execution = Execution::new([index; 32], i64::from(index) * 1000).unwrap();
            let expected = Execution::new([index; 32], i64::from(index) * 1000).unwrap();
            for _ in 0..100 {
                let observed = execution
                    .scope(async {
                        tokio::task::yield_now().await;
                        let first = nucleus::execution::now();
                        tokio::task::yield_now().await;
                        let id = nucleus::new_uid("r");
                        tokio::task::yield_now().await;
                        assert_eq!(first, nucleus::execution::now());
                        (id, nucleus::hlc::next(), first)
                    })
                    .await;
                assert!(nucleus::execution::current().is_none());
                assert_eq!(
                    observed,
                    expected.with(|| (
                        nucleus::new_uid("r"),
                        nucleus::hlc::next(),
                        nucleus::execution::now()
                    ))
                );
            }
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
}

#[test]
fn calendar_windows_follow_timezone_gaps_and_folds() {
    let march = nucleus::projection::Window::month(2030, 3, "America/New_York".into()).unwrap();
    let november = nucleus::projection::Window::month(2030, 11, "America/New_York".into()).unwrap();
    assert_eq!(march.until_ms - march.from_ms, 31 * 86_400_000 - 3_600_000);
    assert_eq!(
        november.until_ms - november.from_ms,
        30 * 86_400_000 + 3_600_000
    );
    assert_eq!(march.date(march.from_ms).as_deref(), Some("2030-03-01"));
    assert_eq!(
        march.date(march.until_ms - 1).as_deref(),
        Some("2030-03-31")
    );
}
