use super::*;

fn context(sequence: u64) -> GestureContext {
    GestureContext {
        sequence,
        owner: 42,
        workspace: 7,
        output: OutputId(0),
        fingers: 3,
    }
}

fn pan(sequence: u64, horizontal_travel: f64) -> GestureProgress {
    GestureProgress {
        sequence,
        output: OutputId(0),
        horizontal_travel,
    }
}

fn end(sequence: u64, result: GestureResult) -> GestureCompletion {
    GestureCompletion {
        sequence,
        output: OutputId(0),
        result,
    }
}

#[test]
fn a_paused_worker_coalesces_ten_thousand_updates_and_retains_one_terminal_delivery() {
    let mut buffer = GestureDeliveryBuffer::default();
    assert!(buffer.begin(context(1)));
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Begin(context(1)))
    );
    assert!(buffer.sent(10));
    for travel in 0..10_000 {
        assert!(buffer.progress(pan(1, f64::from(travel))));
        assert_eq!(buffer.next_delivery(), None);
    }
    buffer.acknowledge(9);
    assert_eq!(buffer.next_delivery(), None);
    buffer.acknowledge(10);
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Pan(pan(1, 9999.0)))
    );
    assert!(buffer.sent(11));
    assert!(buffer.finish(end(1, GestureResult::Pan(10_000.0))));
    assert!(!buffer.finish(end(1, GestureResult::Workspace(1))));
    assert!(!buffer.progress(pan(1, 10_001.0)));
    assert!(!buffer.begin(context(2)));
    buffer.acknowledge(11);
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Finish(end(
            1,
            GestureResult::Pan(10_000.0)
        )))
    );
    assert!(buffer.sent(12));
    assert!(!buffer.begin(context(2)));
    buffer.acknowledge(12);
    assert!(!buffer.busy());
    assert_eq!(buffer.next_delivery(), None);
    assert!(!buffer.begin(context(1)));
    assert!(buffer.begin(context(2)));
}

#[test]
fn cancellation_drops_an_unsent_begin_and_replaces_a_queued_commit_without_losing_it() {
    let mut buffer = GestureDeliveryBuffer::default();
    assert!(buffer.begin(context(1)));
    buffer.cancel();
    assert!(!buffer.busy());
    assert_eq!(buffer.next_delivery(), None);
    assert!(buffer.begin(context(2)));
    assert!(buffer.sent(1));
    assert!(buffer.finish(end(2, GestureResult::Workspace(1))));
    buffer.cancel();
    buffer.acknowledge(1);
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Finish(end(2, GestureResult::Cancelled)))
    );
    assert!(buffer.sent(2));
    buffer.cancel();
    buffer.acknowledge(2);
    assert!(!buffer.busy());
}

#[test]
fn foreign_sequences_and_outputs_cannot_retarget_or_finish_a_captured_gesture() {
    let mut buffer = GestureDeliveryBuffer::default();
    assert!(buffer.begin(context(4)));
    assert!(!buffer.progress(pan(3, 10.0)));
    assert!(!buffer.progress(GestureProgress {
        output: OutputId(1),
        ..pan(4, 10.0)
    }));
    assert!(!buffer.finish(end(5, GestureResult::Workspace(1))));
    assert!(!buffer.finish(GestureCompletion {
        output: OutputId(1),
        ..end(4, GestureResult::Workspace(1))
    }));
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Begin(context(4)))
    );
    assert!(!buffer.sent(0));
    assert!(buffer.sent(7));
    assert!(!buffer.sent(8));
    buffer.acknowledge(7);
    assert_eq!(buffer.next_delivery(), None);
    assert!(buffer.finish(end(4, GestureResult::Workspace(1))));
    assert!(buffer.sent(8));
    buffer.acknowledge(100);
    assert!(!buffer.busy());
}

#[test]
fn malformed_progress_or_terminal_results_cancel_and_unmapped_begins_stay_unclaimed() {
    let mut buffer = GestureDeliveryBuffer::default();
    for context in [
        context(0),
        GestureContext {
            workspace: 0,
            ..context(1)
        },
        GestureContext {
            fingers: 2,
            ..context(1)
        },
    ] {
        assert!(!buffer.begin(context));
    }
    assert!(buffer.begin(context(1)));
    assert!(buffer.sent(1));
    assert!(!buffer.progress(pan(1, f64::NAN)));
    buffer.acknowledge(1);
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Finish(end(1, GestureResult::Cancelled)))
    );
    assert!(buffer.sent(2));
    buffer.acknowledge(2);
    assert!(buffer.begin(GestureContext {
        fingers: 4,
        ..context(2)
    }));
    assert!(buffer.sent(3));
    assert!(!buffer.progress(pan(2, 10.0)));
    assert!(buffer.finish(end(2, GestureResult::Workspace(1))));
    buffer.acknowledge(3);
    assert_eq!(
        buffer.next_delivery(),
        Some(GestureDelivery::Finish(end(2, GestureResult::Cancelled)))
    );
}
