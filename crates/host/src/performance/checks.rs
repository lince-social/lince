// non-lean-yet

use super::*;

#[cfg(feature = "backend")]
#[cfg_attr(test, test)]
pub fn backend_phases_share_the_monitor_and_only_allow_declared_numeric_readings() {
    let monitor = Monitor::new(None);
    monitor.backend(BackendReading::Elapsed(
        "record-guard-base-validation",
        Duration::from_micros(25),
    ));
    monitor.backend(BackendReading::Count("cell-query-rows", 8));
    monitor.backend(BackendReading::Elapsed(
        "cell-query-batch-await",
        Duration::from_micros(30),
    ));
    monitor.backend(BackendReading::Count("cell-query-batch-reuses", 7));
    monitor.backend(BackendReading::Elapsed(
        "private Record contents",
        Duration::from_secs(1),
    ));
    monitor.backend(BackendReading::Count("private Record contents", 99));
    monitor.backend(BackendReading::Mark("record-edit-queued", 7));
    monitor.backend(BackendReading::Mark("record-edit-dispatched", 7));
    monitor.backend(BackendReading::Mark("private Record contents", 99));
    let report = monitor.snapshot();
    assert_eq!(report.spans.len(), 2);
    assert_eq!(report.spans[0].stage, "record-edit-queued");
    assert_eq!(report.spans[1].stage, "record-edit-dispatched");
    assert!(report.spans[0].started_us <= report.spans[1].started_us);
    assert!(
        report
            .spans
            .iter()
            .all(|span| span.operation == Some(7) && span.started_us == span.ended_us)
    );
    assert_eq!(report.stages.len(), 2);
    assert_eq!(report.stages[0].stage, "record-guard-base-validation");
    assert_eq!(report.stages[0].total_us, 25);
    assert_eq!(report.stages[1].stage, "cell-query-batch-await");
    assert_eq!(report.stages[1].total_us, 30);
    assert_eq!(report.counters.len(), 2);
    assert_eq!(report.counters[0].counter, "cell-query-rows");
    assert_eq!(report.counters[0].count, 8);
    assert_eq!(report.counters[1].counter, "cell-query-batch-reuses");
    assert_eq!(report.counters[1].count, 7);
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("private Record contents")
    );
    for counter in Count::ALL {
        assert_eq!(
            Count::from_label(counter.label()).map(|value| value as usize),
            Some(*counter as usize)
        );
    }
}

#[cfg_attr(test, test)]
pub fn input_traces_are_bounded_nonblocking_and_report_loss_without_payloads() {
    let monitor = std::sync::Arc::new(Monitor::new(None));
    let reading = InputTrace {
        stage: InputStage::NativeDequeued,
        origin: 7,
        origin_us: 100,
        observed_us: 500,
        input_serial: 12,
        pointer_serial: 0,
        frame: 0,
        output: None,
    };
    monitor.trace_input(reading);
    let workers = (0..4)
        .map(|_| {
            let monitor = monitor.clone();
            std::thread::spawn(move || {
                for _ in 0..1000 {
                    monitor.trace_input(reading);
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    let report = monitor.snapshot();
    assert_eq!(report.input_trace.len(), INPUT_TRACE_LIMIT);
    assert_eq!(
        report.input_trace.len() as u64 + report.dropped_input_trace,
        4001
    );
    let serialized = serde_json::to_value(report.input_trace[0]).unwrap();
    assert_eq!(serialized["stage"], "native-dequeued");
    assert_eq!(serialized["origin"], 7);
    assert_eq!(serialized.as_object().unwrap().len(), 8);
    assert!(monitor.snapshot().input_trace.is_empty());
    assert_eq!(monitor.snapshot().dropped_input_trace, 0);
}

#[cfg_attr(test, test)]
pub fn concurrent_trace_producers_preserve_every_sample_until_capacity() {
    let monitor = std::sync::Arc::new(Monitor::new(None));
    let workers = (0..8)
        .map(|worker| {
            let monitor = monitor.clone();
            std::thread::spawn(move || {
                for sample in 0..128 {
                    let serial = worker * 128 + sample;
                    monitor.trace_input(InputTrace {
                        stage: InputStage::NativeDequeued,
                        origin: 1,
                        origin_us: serial,
                        observed_us: serial,
                        input_serial: serial,
                        pointer_serial: 0,
                        frame: 0,
                        output: None,
                    });
                    monitor.trace_span(SpanReading {
                        stage: Stage::NativeMain.label(),
                        started_us: serial,
                        ended_us: serial + 1,
                        start_thread: worker,
                        end_thread: worker,
                        operation: Some(serial),
                    });
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    let report = monitor.snapshot();
    assert_eq!(report.dropped_input_trace, 0);
    assert_eq!(report.dropped_span_trace, 0);
    assert!(
        report
            .spans
            .iter()
            .all(|span| span.operation == Some(span.started_us))
    );
    let mut inputs = report
        .input_trace
        .iter()
        .map(|reading| reading.input_serial)
        .collect::<Vec<_>>();
    let mut spans = report
        .spans
        .iter()
        .map(|reading| reading.started_us)
        .collect::<Vec<_>>();
    inputs.sort_unstable();
    spans.sort_unstable();
    assert_eq!(inputs, (0..1024).collect::<Vec<_>>());
    assert_eq!(spans, inputs);
    for serial in 0..SPAN_TRACE_LIMIT + 1 {
        monitor.trace_span(SpanReading {
            stage: Stage::NativeMain.label(),
            started_us: serial as u64,
            ended_us: serial as u64 + 1,
            start_thread: 1,
            end_thread: 1,
            operation: Some(serial as u64),
        });
    }
    let report = monitor.snapshot();
    assert_eq!(report.spans.len(), SPAN_TRACE_LIMIT);
    assert_eq!(report.dropped_span_trace, 1);
    assert!(monitor.snapshot().spans.is_empty());
    assert_eq!(monitor.snapshot().dropped_span_trace, 0);
}

#[cfg_attr(test, test)]
pub fn system_samples_survive_concurrent_snapshots_and_shard_reuse() {
    let monitor = std::sync::Arc::new(Monitor::new(None));
    let done = std::sync::Arc::new(AtomicU64::new(0));
    let reports = {
        let monitor = monitor.clone();
        let done = done.clone();
        std::thread::spawn(move || {
            let mut count = 0;
            let mut cpu = 0;
            loop {
                let finished = done.load(Ordering::Acquire) == 32;
                let report = monitor.snapshot();
                assert_eq!(report.dropped_system_samples, 0);
                for system in report.systems {
                    assert_eq!(system.system, "lince_interface::test::concurrent");
                    count += system.wall.count;
                    cpu += system.exclusive_owner_cpu.total_us;
                }
                if finished {
                    return (count, cpu);
                }
                std::thread::yield_now();
            }
        })
    };
    let workers = (0..32)
        .map(|_| {
            let monitor = monitor.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                for _ in 0..500 {
                    monitor.record_system(
                        "lince_interface::test::concurrent",
                        Duration::from_micros(12),
                        Duration::from_micros(8),
                        Duration::from_micros(5),
                    );
                }
                done.fetch_add(1, Ordering::Release);
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(reports.join().unwrap(), (16_000, 80_000));
    assert!(monitor.snapshot().systems.is_empty());
}

#[cfg_attr(test, test)]
pub fn bounded_distributions_keep_counts_tails_and_concurrent_samples() {
    let monitor = std::sync::Arc::new(Monitor::new(None));
    let workers = (0..4)
        .map(|_| {
            let monitor = monitor.clone();
            std::thread::spawn(move || {
                for _ in 0..1000 {
                    monitor.record(Stage::NativeStyles, Duration::from_micros(10));
                    monitor.counters[Count::NativeInputApplied as usize]
                        .fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    monitor.record(Stage::NativeStyles, Duration::from_secs(2));
    let report = monitor.snapshot();
    assert_eq!(report.stages.len(), 1);
    let reading = &report.stages[0];
    assert_eq!(reading.count, 4001);
    assert_eq!(reading.total_us, 2_040_000);
    assert_eq!(reading.maximum_us, 2_000_000);
    assert_eq!(reading.p50_upper_us, 10);
    assert_eq!(reading.p99_upper_us, 10);
    assert_eq!(report.counters.len(), 1);
    assert_eq!(report.counters[0].counter, "native-input-applied");
    assert_eq!(report.counters[0].count, 4000);
    assert!(monitor.snapshot().stages.is_empty());
    assert!(monitor.snapshot().counters.is_empty());
}

#[cfg_attr(test, test)]
pub fn output_intervals_remain_independent_and_observations_are_bounded() {
    let monitor = Monitor::new(None);
    for tick in 0..5 {
        monitor.presented(1, Duration::from_micros(tick * 16_667), true);
        monitor.presented(2, Duration::from_micros(tick * 33_334), true);
    }
    monitor.presented(1, Duration::from_micros(1), true);
    monitor.record_output(Stage::NativeInputToDrm, 1, Duration::from_millis(5));
    monitor.record_output(Stage::NativeInputToDrm, 2, Duration::from_millis(50));
    let report = monitor.snapshot();
    assert_eq!(report.outputs.len(), 2);
    assert_eq!(report.outputs[0].interval.count, 4);
    assert_eq!(report.outputs[0].interval.maximum_us, 16_667);
    assert_eq!(report.outputs[1].interval.maximum_us, 33_334);
    assert_eq!(report.outputs[0].stages[0].maximum_us, 5_000);
    assert_eq!(report.outputs[1].stages[0].maximum_us, 50_000);
    monitor.presented(1, Duration::from_micros(120_000), false);
    monitor.presented(1, Duration::from_micros(136_667), false);
    let fallback = monitor.snapshot();
    assert_eq!(fallback.outputs.len(), 1);
    assert_eq!(fallback.outputs[0].clock, "dispatch");
    assert_eq!(fallback.outputs[0].interval.count, 1);
    for id in 3..1000 {
        monitor.presented(id, Duration::ZERO, true);
    }
    assert_eq!(monitor.outputs.lock().unwrap().len(), 16);
    monitor.outputs.lock().unwrap().remove(&1);
    monitor.presented(1000, Duration::ZERO, true);
    assert!(monitor.outputs.lock().unwrap().contains_key(&1000));
    for stage in Stage::ALL {
        assert_eq!(Stage::from_label(stage.label()), Some(*stage));
    }
    assert!(Stage::from_label("private window contents").is_none());
}

#[cfg_attr(test, test)]
pub fn report_backpressure_does_not_block_and_is_reported() {
    let (sender, receiver) = sync_channel(1);
    let mut monitor = Monitor::new(Some(sender));
    monitor.started = Instant::now() - Duration::from_secs(2);
    monitor.record(Stage::NativeUpdate, Duration::from_millis(1));
    monitor.flush_if_due();
    monitor.reported_at_us.store(0, Ordering::Relaxed);
    monitor.flush_if_due();
    assert_eq!(monitor.dropped_reports.load(Ordering::Relaxed), 1);
    assert_eq!(receiver.try_recv().unwrap().dropped_reports, 0);
    monitor.reported_at_us.store(0, Ordering::Relaxed);
    monitor.flush_if_due();
    assert_eq!(receiver.try_recv().unwrap().dropped_reports, 1);
}
