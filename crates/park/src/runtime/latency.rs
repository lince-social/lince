use lince_interface::host::OutputId;
use lince_interface::host::performance::{self, Count, InputStage, InputTrace, Stage};
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

thread_local! { static CURRENT: Cell<Option<Stamp>> = const { Cell::new(None) }; }
static NEXT_ORIGIN: AtomicU64 = AtomicU64::new(1);

pub(crate) fn monotonic() -> Duration {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(
        time.tv_sec.max(0) as u64,
        time.tv_nsec.clamp(0, 999_999_999) as u32,
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Stamp {
    id: u64,
    instant: Instant,
    time: Duration,
}

impl Stamp {
    pub(crate) fn earliest(left: Option<Self>, right: Option<Self>) -> Option<Self> {
        match (left, right) {
            (Some(left), Some(right)) => Some(if left.time <= right.time { left } else { right }),
            (left, right) => left.or(right),
        }
    }
    pub(crate) fn fresh() -> Option<Self> {
        performance::enabled().then(|| Self {
            id: NEXT_ORIGIN
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .unwrap_or(0),
            instant: Instant::now(),
            time: monotonic(),
        })
    }

    pub(crate) fn current() -> Option<Self> {
        if !performance::enabled() {
            return None;
        }
        CURRENT.with(Cell::get).or_else(Self::fresh)
    }

    #[cfg(all(test, feature = "nested"))]
    pub(crate) fn origin(self) -> u64 {
        self.id
    }

    pub(crate) fn record(self, stage: Stage) {
        performance::record(stage, self.instant.elapsed());
    }

    pub(crate) fn checkpoint(self, stage: InputStage, input_serial: u64, pointer_serial: u64) {
        self.trace(InputTrace {
            stage,
            origin: self.id,
            origin_us: self.time.as_micros().min(u128::from(u64::MAX)) as u64,
            observed_us: monotonic().as_micros().min(u128::from(u64::MAX)) as u64,
            input_serial,
            pointer_serial,
            frame: 0,
            output: None,
        });
    }

    fn trace(self, reading: InputTrace) {
        if self.id != 0 && reading.observed_us >= reading.origin_us {
            performance::trace_input(reading);
        }
    }

    #[cfg(feature = "nested")]
    pub(crate) fn observed(self, stage: InputStage, output: OutputId, time: Duration) {
        self.trace(InputTrace {
            stage,
            origin: self.id,
            origin_us: self.time.as_micros().min(u128::from(u64::MAX)) as u64,
            observed_us: time.as_micros().min(u128::from(u64::MAX)) as u64,
            input_serial: 0,
            pointer_serial: 0,
            frame: 0,
            output: Some(output.0),
        });
    }

    #[cfg(any(feature = "nested", test))]
    pub(crate) fn age(self, time: Duration) -> Option<Duration> {
        time.checked_sub(self.time)
    }

    #[cfg(any(feature = "nested", test))]
    pub(crate) fn record_at(self, stage: Stage, output: OutputId, time: Duration) -> bool {
        if let Some(age) = self.age(time) {
            performance::record_output(stage, output, age);
            true
        } else {
            performance::count(Count::TimingRejected);
            false
        }
    }
}

#[cfg(any(feature = "nested", test))]
pub(crate) struct Dispatch {
    previous: Option<Stamp>,
    active: bool,
}

#[cfg(any(feature = "nested", test))]
impl Dispatch {
    pub(crate) fn new() -> Self {
        Self::resume(Stamp::current())
    }

    pub(crate) fn resume(stamp: Option<Stamp>) -> Self {
        if let Some(stamp) = stamp {
            stamp.checkpoint(InputStage::Dispatch, 0, 0);
        }
        Self {
            previous: stamp.and_then(|stamp| CURRENT.with(|current| current.replace(Some(stamp)))),
            active: stamp.is_some(),
        }
    }

    #[cfg(feature = "session")]
    pub(crate) fn device(micros: u64) -> Self {
        let stamp = Stamp::fresh().and_then(|mut stamp| {
            let time = Duration::from_micros(micros);
            let Some(age) = stamp.time.checked_sub(time) else {
                performance::count(Count::TimingRejected);
                return Some(stamp);
            };
            performance::record(Stage::DeviceToDispatch, age);
            if let Some(instant) = stamp.instant.checked_sub(age) {
                stamp.instant = instant;
                stamp.time = time;
            }
            Some(stamp)
        });
        Self::resume(stamp)
    }
}

#[cfg(any(feature = "nested", test))]
impl Drop for Dispatch {
    fn drop(&mut self) {
        if self.active {
            CURRENT.with(|current| current.set(self.previous));
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub id: u64,
    pub input_serial: u64,
    pub pointer_serial: u64,
    stamp: Stamp,
}

#[derive(Default)]
pub(crate) struct Batch {
    stamp: Option<Stamp>,
    input_serial: u64,
    pointer_serial: u64,
}

impl Batch {
    pub(crate) fn accept(&mut self, stamp: Option<Stamp>, input: u64, pointer: u64) {
        if let Some(stamp) = stamp {
            stamp.checkpoint(InputStage::NativeDelivered, input, pointer);
            self.stamp = Stamp::earliest(self.stamp, Some(stamp));
            self.input_serial = self.input_serial.max(input);
            self.pointer_serial = self.pointer_serial.max(pointer);
            performance::count(Count::NativeInputApplied);
        }
    }

    pub(crate) fn finish(&mut self, sequence: u64) -> Option<Sample> {
        let batch = std::mem::take(self);
        let stamp = batch.stamp?;
        stamp.record(Stage::NativeInputToFrame);
        performance::count(Count::NativeInputBatches);
        let sample = Sample {
            id: sequence,
            stamp,
            input_serial: batch.input_serial,
            pointer_serial: batch.pointer_serial,
        };
        sample.checkpoint(InputStage::FramePrepared, None, monotonic());
        Some(sample)
    }
}

impl Sample {
    fn checkpoint(self, stage: InputStage, output: Option<OutputId>, observed: Duration) {
        self.stamp.trace(InputTrace {
            stage,
            origin: self.stamp.id,
            origin_us: self.stamp.time.as_micros().min(u128::from(u64::MAX)) as u64,
            observed_us: observed.as_micros().min(u128::from(u64::MAX)) as u64,
            input_serial: self.input_serial,
            pointer_serial: self.pointer_serial,
            frame: self.id,
            output: output.map(|output| output.0),
        });
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Frame {
    created: Stamp,
    input: Option<Sample>,
    sequence: u64,
}

impl Frame {
    pub(crate) fn new(sequence: u64, input: Option<Sample>) -> Option<Self> {
        Stamp::fresh().map(|created| Self {
            created,
            input,
            sequence,
        })
    }

    pub(crate) fn published(self) {
        self.created.record(Stage::NativeFrameToPublication);
        if let Some(input) = self.input.filter(|input| input.id == self.sequence) {
            input.stamp.record(Stage::NativeInputToPublication);
            input.checkpoint(InputStage::Published, None, monotonic());
        }
    }

    pub(crate) fn ready(self) {
        if let Some(input) = self.input.filter(|input| input.id == self.sequence) {
            input.checkpoint(InputStage::GpuReady, None, monotonic());
        }
    }

    #[cfg(feature = "nested")]
    pub(crate) fn presented(
        self,
        output: OutputId,
        previous: Option<Self>,
        time: Duration,
        hardware: bool,
    ) {
        for (stage, duration) in self
            .observations(previous, time, hardware)
            .into_iter()
            .flatten()
        {
            if let Some(duration) = duration {
                performance::record_output(stage, output, duration);
                if matches!(
                    stage,
                    Stage::NativeInputToDrm | Stage::NativeInputToObserved
                ) {
                    performance::count(Count::NativeInputPresented);
                    if let Some(input) = self.input {
                        input.checkpoint(
                            if hardware {
                                InputStage::PresentedDrm
                            } else {
                                InputStage::PresentedObserved
                            },
                            Some(output),
                            time,
                        );
                    }
                }
            } else {
                performance::count(Count::TimingRejected);
            }
        }
    }

    #[cfg(any(feature = "nested", test))]
    fn observations(
        self,
        previous: Option<Self>,
        time: Duration,
        hardware: bool,
    ) -> [Option<(Stage, Option<Duration>)>; 2] {
        if previous.is_some_and(|previous| previous.sequence >= self.sequence) {
            return [None, None];
        }
        let frame = previous
            .is_none_or(|previous| previous.sequence != self.sequence)
            .then(|| {
                (
                    if hardware {
                        Stage::NativeFrameToDrm
                    } else {
                        Stage::NativeFrameToObserved
                    },
                    self.created.age(time),
                )
            });
        let input = self
            .input
            .filter(|input| {
                previous
                    .and_then(|previous| previous.input)
                    .is_none_or(|previous| previous.id != input.id)
            })
            .map(|input| {
                (
                    if hardware {
                        Stage::NativeInputToDrm
                    } else {
                        Stage::NativeInputToObserved
                    },
                    input.stamp.age(time),
                )
            });
        [frame, input]
    }
}

#[derive(Default)]
#[cfg(any(feature = "nested", test))]
pub(crate) struct Outputs([Option<OutputId>; 16]);

#[cfg(any(feature = "nested", test))]
impl Outputs {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }
    pub(crate) fn visit(&mut self, output: OutputId) -> bool {
        if self.0.contains(&Some(output)) {
            return false;
        }
        let Some(slot) = self.0.iter_mut().find(|slot| slot.is_none()) else {
            performance::count(Count::TimingCapacity);
            return false;
        };
        *slot = Some(output);
        true
    }
}

#[cfg(feature = "nested")]
pub(crate) struct Buffer {
    stamp: Stamp,
    outputs: std::sync::Mutex<Outputs>,
}

#[cfg(feature = "nested")]
impl Buffer {
    pub(crate) fn new() -> Option<Box<Self>> {
        Stamp::fresh().map(|stamp| {
            Box::new(Self {
                stamp,
                outputs: std::sync::Mutex::new(Outputs::default()),
            })
        })
    }

    #[cfg(feature = "nested")]
    pub(crate) fn presented(&self, output: OutputId, time: Duration, hardware: bool) {
        if self.stamp.age(time).is_none() {
            performance::count(Count::TimingRejected);
            return;
        }
        if self
            .outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .visit(output)
        {
            self.stamp.record_at(
                if hardware {
                    Stage::ClientBufferToDrm
                } else {
                    Stage::ClientBufferToObserved
                },
                output,
                time,
            );
        }
    }
}

#[cfg(test)]
pub(crate) fn stamp_at(time: Duration) -> Stamp {
    Stamp {
        id: 0,
        instant: Instant::now(),
        time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "nested")]
    mod visible;

    #[test]
    fn batches_keep_oldest_time_and_independent_serial_domains() {
        let mut batch = Batch::default();
        batch.accept(Some(stamp_at(Duration::from_millis(100))), 8, 0);
        batch.accept(Some(stamp_at(Duration::from_millis(50))), 9, 0);
        batch.accept(Some(stamp_at(Duration::from_millis(80))), 0, 20);
        batch.accept(None, 99, 99);
        let sample = batch.finish(3).unwrap();
        assert_eq!(
            (sample.id, sample.input_serial, sample.pointer_serial),
            (3, 9, 20)
        );
        assert_eq!(sample.stamp.time, Duration::from_millis(50));
        assert!(batch.finish(4).is_none());
    }

    #[test]
    fn deferred_dispatch_scopes_restore_the_previous_origin() {
        assert!(CURRENT.with(Cell::get).is_none());
        {
            let _outer = Dispatch::resume(Some(stamp_at(Duration::from_millis(50))));
            {
                let _inner = Dispatch::resume(Some(stamp_at(Duration::from_millis(10))));
                assert_eq!(
                    CURRENT.with(Cell::get).unwrap().time,
                    Duration::from_millis(10)
                );
            }
            assert_eq!(
                CURRENT.with(Cell::get).unwrap().time,
                Duration::from_millis(50)
            );
        }
        assert!(CURRENT.with(Cell::get).is_none());
    }

    #[test]
    fn repeated_frames_settling_and_distinct_outputs_keep_timing_meaning() {
        let input = Sample {
            id: 2,
            input_serial: 7,
            pointer_serial: 0,
            stamp: stamp_at(Duration::from_millis(10)),
        };
        let first = Frame {
            created: stamp_at(Duration::from_millis(20)),
            input: Some(input),
            sequence: 2,
        };
        let time = Duration::from_millis(30);
        assert_eq!(
            first.observations(None, time, true),
            [
                Some((Stage::NativeFrameToDrm, Some(Duration::from_millis(10)))),
                Some((Stage::NativeInputToDrm, Some(Duration::from_millis(20))))
            ]
        );
        assert_eq!(first.observations(Some(first), time, true), [None, None]);
        let settling = Frame {
            created: stamp_at(Duration::from_millis(25)),
            sequence: 3,
            ..first
        };
        assert_eq!(
            settling.observations(Some(first), time, false),
            [
                Some((Stage::NativeFrameToObserved, Some(Duration::from_millis(5)))),
                None
            ]
        );
        assert!(first.observations(None, time, false)[1].is_some());
        assert_eq!(
            first.observations(None, Duration::ZERO, true),
            [
                Some((Stage::NativeFrameToDrm, None)),
                Some((Stage::NativeInputToDrm, None))
            ]
        );
    }

    #[test]
    fn output_receipts_are_independent_deduplicated_and_bounded() {
        let mut outputs = Outputs::default();
        assert!(outputs.is_empty());
        for id in 1..=16 {
            assert!(outputs.visit(OutputId(id)));
        }
        assert!(!outputs.visit(OutputId(1)));
        assert!(!outputs.visit(OutputId(17)));
        assert_eq!(outputs.0.iter().flatten().count(), 16);
        assert!(!outputs.is_empty());
    }

    #[test]
    fn reasoned_wakes_and_deadlines_preserve_exact_callback_delivery() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (sent, received) = tokio::sync::oneshot::channel();
        let sent = std::sync::Mutex::new(Some(sent));
        let signal = lince_interface::host::WakeSignal::new(move || {
            sent.lock().unwrap().take().unwrap().send(()).unwrap();
        });
        runtime.block_on(async {
            signal.after(Duration::from_millis(2));
            tokio::time::timeout(Duration::from_secs(1), received)
                .await
                .unwrap()
                .unwrap();
        });
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let received = observed.clone();
        let signal = lince_interface::host::WakeSignal::new(move || {
            received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        signal.ring_reason(Count::CellMessageWake);
        assert_eq!(observed.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[cfg(feature = "nested")]
    #[test]
    fn an_invalid_presentation_clock_does_not_consume_a_buffer_receipt() {
        let buffer = Buffer {
            stamp: stamp_at(Duration::from_millis(100)),
            outputs: std::sync::Mutex::new(Outputs::default()),
        };
        buffer.presented(OutputId(1), Duration::ZERO, false);
        assert!(
            !buffer
                .outputs
                .lock()
                .unwrap()
                .0
                .contains(&Some(OutputId(1)))
        );
        buffer.presented(OutputId(1), Duration::from_millis(150), false);
        assert!(
            buffer
                .outputs
                .lock()
                .unwrap()
                .0
                .contains(&Some(OutputId(1)))
        );
    }

    #[test]
    #[ignore = "requires hardware Vulkan/EGL and LINCE_TRACE_PERFORMANCE=1"]
    fn a_real_worker_keeps_input_origins_and_pointer_serials_through_publication() {
        use super::super::{CanvasWorker, Completed, Configuration, Input};
        assert!(performance::enabled());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut worker = CanvasWorker::spawn(Configuration {
            showoff: false,
            size: [640, 480],
            directory: directory.path().into(),
            runtime: runtime.handle().clone(),
            cell: None,
        })
        .unwrap();
        let wait = |serial| -> Completed {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                assert!(worker.failure().is_none());
                if let Some(frame) = worker.frame().filter(|frame| frame.input_serial >= serial) {
                    return frame;
                }
                assert!(
                    Instant::now() < deadline,
                    "The input timing frame was not published"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        let _startup = wait(0);
        let mut origin = Stamp::fresh().unwrap();
        origin.time = origin.time.saturating_sub(Duration::from_millis(40));
        origin.instant -= Duration::from_millis(40);
        let serial = {
            let _dispatch = Dispatch::resume(Some(origin));
            worker
                .input(Input::Action(
                    lince_interface::host::DesktopAction::ToggleOverview,
                ))
                .unwrap()
        };
        let frame = wait(serial);
        let timing = frame.latency.unwrap();
        let sample = timing.input.unwrap();
        assert_eq!(sample.input_serial, serial);
        assert_eq!(sample.stamp.time, origin.time);
        assert!(timing.created.time >= sample.stamp.time);
        worker.input_activity.set_active(true);
        let pointer = worker.pointer([25.0, 25.0]).unwrap();
        let barrier = worker.input(Input::Test(Box::new(|_| {}))).unwrap();
        let frame = wait(barrier);
        assert_eq!(
            frame.latency.unwrap().input.unwrap().pointer_serial,
            pointer
        );
        worker.stop().unwrap();
    }
}
