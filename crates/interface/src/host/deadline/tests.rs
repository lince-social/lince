use super::*;

#[tokio::test]
async fn repeated_requests_keep_the_first_deadline_and_independent_owners_both_fire() {
    let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
    let signal = WakeSignal::new(move || {
        sent.send(()).unwrap();
    });
    let mut first = WakeDeadline::default();
    let mut second = WakeDeadline::default();
    assert!(first.after(&signal, Duration::from_millis(30)));
    let due = first.due().unwrap();
    for _ in 0..128 {
        assert!(!first.after(&signal, Duration::from_millis(40)));
        assert_eq!(first.due(), Some(due));
    }
    assert!(second.after(&signal, Duration::from_millis(60)));
    assert!(first.after(&signal, Duration::from_millis(5)));
    for _ in 0..2 {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), received.recv())
                .await
                .unwrap(),
            Some(())
        );
    }
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(received.try_recv().is_err());
    assert!(first.after(&signal, Duration::from_millis(5)));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap(),
        Some(())
    );
}

#[tokio::test]
async fn cancellation_and_drop_release_the_target_without_a_late_wake() {
    use std::sync::atomic::AtomicUsize;
    let owner = Arc::new(AtomicUsize::new(0));
    let weak = Arc::downgrade(&owner);
    let observed = owner.clone();
    let signal = WakeSignal::new(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    });
    let mut cancelled = WakeDeadline::default();
    let mut removed = WakeDeadline::default();
    assert!(cancelled.after(&signal, Duration::from_millis(80)));
    assert!(removed.after(&signal, Duration::from_millis(80)));
    cancelled.cancel();
    assert!(cancelled.pending.is_none());
    drop(removed);
    drop(signal);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(owner.load(Ordering::Relaxed), 0);
    drop(owner);
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn replacing_the_callback_cannot_retain_the_previous_target() {
    let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
    let previous = sent.clone();
    let old = WakeSignal::new(move || {
        previous.send(1).unwrap();
    });
    let current = WakeSignal::new(move || {
        sent.send(2).unwrap();
    });
    let mut deadline = WakeDeadline::default();
    assert!(deadline.after(&old, Duration::from_millis(20)));
    assert!(deadline.after(&current, Duration::from_millis(60)));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap(),
        Some(2)
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(received.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fired_callback_can_rearm_before_its_task_finishes() {
    use std::sync::{Barrier, atomic::AtomicUsize};
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let release = Arc::new(Barrier::new(2));
    let hold = release.clone();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let signal = WakeSignal::new(move || {
        if observed.fetch_add(1, Ordering::Relaxed) == 0 {
            entered.send(()).unwrap();
            hold.wait();
        }
    });
    let mut deadline = WakeDeadline::default();
    assert!(deadline.after(&signal, Duration::from_millis(1)));
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!deadline.pending.as_ref().unwrap().task.is_finished());
    let rearmed = deadline.after(&signal, Duration::from_millis(5));
    release.wait();
    assert!(rearmed);
    tokio::time::timeout(Duration::from_secs(2), async {
        while count.load(Ordering::Relaxed) != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn requests_without_a_runtime_do_not_arm_a_deadline() {
    let mut deadline = WakeDeadline::default();
    assert!(!deadline.after(&WakeSignal::new(|| panic!("No runtime")), Duration::ZERO));
    assert!(deadline.pending.is_none());
}
