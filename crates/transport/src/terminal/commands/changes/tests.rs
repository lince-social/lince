use super::*;
use std::time::Duration;

#[tokio::test]
async fn run_changes_coalesce_without_waking_history_or_unrelated_sources_and_release_registration()
{
    let host = CommandHost::new(PathBuf::new());
    let command = nucleus::new_uid("r");
    let run = nucleus::new_uid("run");
    let mut history = host.observe(&command, None).unwrap();
    let mut output = host.observe(&command, Some(&run)).unwrap();
    let output_peer = host.observe(&command, Some(&run)).unwrap();
    let mut other = host.observe(&nucleus::new_uid("r"), None).unwrap();
    for _ in 0..10_000 {
        host.0.changes.notify(&command, &run, false);
    }
    assert!(output.changed().await);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), output.changed())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), history.changed())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), other.changed())
            .await
            .is_err()
    );
    host.0.changes.notify(&command, &run, true);
    assert!(history.changed().await);
    assert!(output.changed().await);
    assert!(output.source().matches(&host));
    assert!(!output.source().matches(&CommandHost::new(PathBuf::new())));
    drop(output);
    assert_eq!(
        host.0
            .changes
            .topics
            .lock()
            .unwrap()
            .get(&command)
            .unwrap()
            .runs
            .len(),
        1
    );
    drop(output_peer);
    assert!(
        host.0
            .changes
            .topics
            .lock()
            .unwrap()
            .get(&command)
            .unwrap()
            .runs
            .is_empty()
    );
    drop(history);
    drop(other);
    assert!(host.0.changes.topics.lock().unwrap().is_empty());
    assert!(host.observe("../command", None).is_err());
    assert!(host.observe(&command, Some("../run")).is_err());
}

#[tokio::test]
async fn observers_do_not_keep_the_host_alive_and_notice_its_shutdown() {
    let host = CommandHost::new(PathBuf::new());
    let mut changes = host.observe(&nucleus::new_uid("r"), None).unwrap();
    let weak = Arc::downgrade(&host.0);
    drop(host);
    assert!(weak.upgrade().is_none());
    assert!(!changes.changed().await);
}
