use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    time::{Duration, Instant},
};

const LIMITS_US: [u64; 32] = [
    10,
    25,
    50,
    100,
    250,
    500,
    750,
    1_000,
    1_500,
    2_000,
    2_500,
    3_000,
    4_000,
    5_000,
    6_000,
    8_000,
    10_000,
    12_000,
    14_000,
    16_000,
    16_667,
    20_000,
    25_000,
    33_334,
    40_000,
    50_000,
    66_667,
    100_000,
    200_000,
    500_000,
    1_000_000,
    u64::MAX,
];

macro_rules! stages {
    ($($name:ident => $label:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
        #[repr(usize)]
        pub enum Stage { $($name,)* }

        impl Stage {
            pub const ALL: &'static [Self] = &[$(Self::$name,)*];
            pub fn label(self) -> &'static str {
                match self { $(Self::$name => $label,)* }
            }
            pub fn from_label(label: &str) -> Option<Self> {
                match label { $($label => Some(Self::$name),)* _ => None }
            }
        }
    };
}

stages! {
    DesktopDispatch => "desktop-dispatch",
    EventLoopWait => "event-loop-wait",
    SceneSync => "scene-sync",
    NativeMain => "native-main",
    NativeUpdate => "native-update",
    NativePixels => "native-pixels",
    NativeStyles => "native-styles",
    NativeSandSizes => "native-sand-sizes",
    NativeIconColors => "native-icon-colors",
    NativeIconMetrics => "native-icon-metrics",
    NativeLayoutMetrics => "native-layout-metrics",
    NativeLayoutSolve => "native-layout-solve",
    NativeSurfaces => "native-surfaces",
    NativeRecordProjection => "native-record-projection",
    NativeRecordPresentation => "native-record-presentation",
    NativeSurfaceFingerprint => "native-surface-fingerprint",
    NativeExtraction => "native-extraction",
    NativeCaptureSources => "native-capture-source-derivation",
    NativeCaptureNormalization => "native-capture-source-normalization",
    RenderUpdate => "render-update",
    FrameGeometry => "frame-geometry",
    FrameInput => "frame-input",
    NativeInputProjection => "native-input-projection",
    NativePointerCapture => "native-pointer-capture",
    NativeInputControls => "native-input-controls",
    NativeInputEditor => "native-input-editor",
    NativeInputCatalog => "native-input-catalog",
    FrameConfigurations => "frame-window-configurations",
    FrameImageExport => "frame-image-export",
    Export => "export",
    ClientImport => "client-import",
    ShmRead => "shm-read",
    ShmUpload => "shm-upload",
    ImportRetained => "dmabuf-import-retained",
    ImportAllocation => "dmabuf-import-allocation",
    ImportPreserve => "dmabuf-import-preserve",
    ImportCopy => "dmabuf-import-copy",
    ExportAllocation => "dmabuf-export-allocation",
    ExportCopy => "dmabuf-export-copy",
    ExportRelease => "dmabuf-export-release",
    SharedInitialize => "shared-image-initialize",
    SharedMemory => "shared-image-memory",
    SharedExportFd => "shared-image-export-fd",
    SharedRegister => "shared-image-register",
    Composition => "composition",
    ColorEncodeSubmit => "color-encode-submit",
    NestedSubmit => "nested-submit",
    GpuSubmissionToCompletion => "gpu-submission-to-completion",
    GpuCompletionFailure => "gpu-completion-failure",
    InputQueueWait => "input-queue-wait",
    DeferredInputWait => "deferred-input-wait",
    DeviceToDispatch => "device-to-dispatch",
    NativeInputToFrame => "native-input-to-frame",
    NativeInputToPublication => "native-input-to-publication",
    NativeFrameToPublication => "native-frame-to-publication",
    NativeInputToDrm => "native-input-to-drm-presentation",
    NativeInputToObserved => "native-input-to-observed-presentation",
    NativeFrameToDrm => "native-frame-to-drm-presentation",
    NativeFrameToObserved => "native-frame-to-observed-presentation",
    ClientInputToForward => "client-input-to-forward",
    ClientInputToDrmBuffer => "client-input-to-newer-buffer-drm-proxy",
    ClientInputToObservedBuffer => "client-input-to-newer-buffer-observed-proxy",
    ClientBufferToDrm => "client-buffer-attach-to-drm-presentation",
    ClientBufferToObserved => "client-buffer-attach-to-observed-presentation",
}

macro_rules! counts {
    ($($name:ident => $label:literal),* $(,)?) => {
        #[derive(Clone, Copy)]
        #[repr(usize)]
        pub enum Count { $($name,)* }
        impl Count {
            pub const ALL: &'static [Self] = &[$(Self::$name,)*];
            fn label(self) -> &'static str { match self { $(Self::$name => $label,)* } }
        }
    };
}

counts! {
    NativeInputApplied => "native-input-applied",
    NativeInputRetired => "native-input-retired",
    NativeInputBatches => "native-input-frame-batches",
    NativeInputPresented => "native-input-batches-presented-per-output",
    ClientInputForwarded => "client-input-forwarded",
    ClientInputProxyPresented => "client-input-buffer-proxies-presented-per-output",
    TimingRejected => "timing-timestamps-rejected",
    TimingCapacity => "timing-capacity-exceeded",
    WindowPixelsCommit => "window-pixels-only-commits",
    WindowSemanticCommit => "window-semantic-commits",
    WindowCallbackCommit => "window-callback-only-commits",
    WindowMetadataChange => "window-metadata-changes",
    ScenePublished => "native-scenes-published",
    SceneReceived => "native-scenes-received",
    InputAccepted => "native-input-messages-accepted",
    ScrollCombined => "native-scroll-messages-combined",
    PointerPublished => "native-pointer-messages-published",
    CanvasExplicitWake => "native-explicit-wake-requests",
    NativeWake => "native-interface-wake-requests",
    NativeInlineWake => "native-worker-issued-interface-wakes",
    TimerScheduled => "native-timers-scheduled",
    TimerFired => "native-timers-fired",
    CellMessageWake => "cell-message-wake-requests",
    CellMessageDrained => "cell-messages-drained",
    CellDrainContinuation => "cell-drain-follow-up-wakes",
    RecordRowsReceived => "record-row-messages-received",
    RecordViewRefreshed => "record-view-derivations",
    RecordViewRetained => "record-view-derivations-retained",
    StyleInvalidated => "style-resolver-cache-invalidations",
    FramePrepared => "native-frames-prepared",
    FramePublished => "native-frames-published",
    SettlingFrame => "native-settling-frames",
    IdlePark => "native-idle-parks",
    GpuPark => "native-gpu-wait-parks",
    ClientSync => "native-client-synchronizations",
    ImportRetry => "native-client-import-retries",
    SupersededFrame => "native-frames-superseded-by-scene-revision",
    FrameAccepted => "native-frames-accepted",
    PixelFrame => "native-pixel-frames-prepared",
}

#[derive(Default)]
struct Distribution {
    count: u64,
    total_us: u64,
    maximum_us: u64,
    buckets: [u64; LIMITS_US.len()],
}

impl Distribution {
    fn record(&mut self, duration: Duration) {
        let micros = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        self.count = self.count.saturating_add(1);
        self.total_us = self.total_us.saturating_add(micros);
        self.maximum_us = self.maximum_us.max(micros);
        let bucket = LIMITS_US
            .partition_point(|limit| *limit < micros)
            .min(LIMITS_US.len() - 1);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    fn percentile(&self, percent: u64) -> u64 {
        let needed = (u128::from(self.count) * u128::from(percent)).div_ceil(100);
        let mut observed = 0_u128;
        for (limit, count) in LIMITS_US.into_iter().zip(self.buckets) {
            observed += u128::from(count);
            if observed >= needed {
                return limit.min(self.maximum_us);
            }
        }
        self.maximum_us
    }

    fn report(self, stage: &'static str) -> Reading {
        Reading {
            stage,
            count: self.count,
            total_us: self.total_us,
            maximum_us: self.maximum_us,
            p50_upper_us: self.percentile(50),
            p95_upper_us: self.percentile(95),
            p99_upper_us: self.percentile(99),
        }
    }
}

#[derive(Serialize)]
pub struct Reading {
    pub stage: &'static str,
    pub count: u64,
    pub total_us: u64,
    pub maximum_us: u64,
    pub p50_upper_us: u64,
    pub p95_upper_us: u64,
    pub p99_upper_us: u64,
}

#[derive(Default)]
struct Output {
    previous: Option<Duration>,
    hardware_clock: Option<bool>,
    intervals: Distribution,
    stages: BTreeMap<usize, Distribution>,
}

#[derive(Serialize)]
pub struct OutputReading {
    pub output: u64,
    pub clock: &'static str,
    pub interval: Reading,
    pub stages: Vec<Reading>,
}

#[derive(Serialize)]
pub struct Report {
    pub elapsed_us: u64,
    pub dropped_reports: u64,
    pub stages: Vec<Reading>,
    pub outputs: Vec<OutputReading>,
    pub counters: Vec<CountReading>,
}

#[derive(Serialize)]
pub struct CountReading {
    pub counter: &'static str,
    pub count: u64,
}

struct Monitor {
    stages: Vec<Mutex<Distribution>>,
    outputs: Mutex<BTreeMap<u64, Output>>,
    started: Instant,
    reported_at_us: AtomicU64,
    dropped_reports: AtomicU64,
    sender: Option<SyncSender<Report>>,
    counters: Vec<AtomicU64>,
}

impl Monitor {
    fn new(sender: Option<SyncSender<Report>>) -> Self {
        Self {
            stages: Stage::ALL
                .iter()
                .map(|_| Mutex::new(Distribution::default()))
                .collect(),
            outputs: Mutex::new(BTreeMap::new()),
            started: Instant::now(),
            reported_at_us: AtomicU64::new(0),
            dropped_reports: AtomicU64::new(0),
            sender,
            counters: Count::ALL.iter().map(|_| AtomicU64::new(0)).collect(),
        }
    }

    fn record(&self, stage: Stage, duration: Duration) {
        self.stages[stage as usize]
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .record(duration);
    }

    fn record_output(&self, stage: Stage, output: u64, duration: Duration) {
        self.record(stage, duration);
        let mut outputs = self
            .outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if outputs.len() >= 16 && !outputs.contains_key(&output) {
            self.counters[Count::TimingCapacity as usize].fetch_add(1, Ordering::Relaxed);
            return;
        }
        outputs
            .entry(output)
            .or_default()
            .stages
            .entry(stage as usize)
            .or_default()
            .record(duration);
    }

    fn presented(&self, output: u64, timestamp: Duration, hardware_clock: bool) {
        let mut outputs = self
            .outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if outputs.len() >= 16 && !outputs.contains_key(&output) {
            return;
        }
        let output = outputs.entry(output).or_default();
        if output.hardware_clock != Some(hardware_clock) {
            output.previous = None;
            output.intervals = Distribution::default();
            output.hardware_clock = Some(hardware_clock);
        }
        if let Some(previous) = output.previous {
            if timestamp <= previous {
                return;
            }
            output.intervals.record(timestamp - previous);
        }
        output.previous = Some(timestamp);
    }

    fn snapshot(&self) -> Report {
        let stages = Stage::ALL
            .iter()
            .filter_map(|stage| {
                let mut values = self.stages[*stage as usize]
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                (values.count != 0).then(|| std::mem::take(&mut *values).report(stage.label()))
            })
            .collect();
        let outputs = self
            .outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter_mut()
            .filter_map(|(id, output)| {
                (output.intervals.count != 0
                    || output.stages.values().any(|stage| stage.count != 0))
                .then(|| OutputReading {
                    output: *id,
                    clock: match output.hardware_clock {
                        Some(true) => "drm",
                        Some(false) => "dispatch",
                        None => "unobserved",
                    },
                    interval: std::mem::take(&mut output.intervals).report("presentation-interval"),
                    stages: output
                        .stages
                        .iter_mut()
                        .filter_map(|(stage, values)| {
                            (values.count != 0)
                                .then(|| std::mem::take(values).report(Stage::ALL[*stage].label()))
                        })
                        .collect(),
                })
            })
            .collect();
        Report {
            elapsed_us: self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            dropped_reports: self.dropped_reports.load(Ordering::Relaxed),
            stages,
            outputs,
            counters: Count::ALL
                .iter()
                .filter_map(|counter| {
                    let count = self.counters[*counter as usize].swap(0, Ordering::Relaxed);
                    (count != 0).then_some(CountReading {
                        counter: counter.label(),
                        count,
                    })
                })
                .collect(),
        }
    }

    fn flush_if_due(&self) {
        let Some(sender) = &self.sender else {
            return;
        };
        let now = self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        let previous = self.reported_at_us.load(Ordering::Relaxed);
        if now.saturating_sub(previous) < 1_000_000
            || self
                .reported_at_us
                .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return;
        }
        if sender.try_send(self.snapshot()).is_err() {
            self.dropped_reports.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn monitor() -> Option<&'static Monitor> {
    static MONITOR: OnceLock<Option<Monitor>> = OnceLock::new();
    MONITOR
        .get_or_init(|| {
            let active = std::env::var("LINCE_TRACE_PERFORMANCE")
                .is_ok_and(|value| matches!(value.as_str(), "1" | "true"));
            if !active {
                return None;
            }
            let (sender, receiver) = sync_channel::<Report>(1);
            let logger = std::thread::Builder::new()
                .name("lince-perf-report".into())
                .spawn(move || {
                    use std::io::Write;
                    while let Ok(report) = receiver.recv() {
                        let stderr = std::io::stderr();
                        let mut stderr = stderr.lock();
                        if stderr.write_all(b"Lince performance ").is_err() {
                            break;
                        }
                        if serde_json::to_writer(&mut stderr, &report).is_err() {
                            break;
                        }
                        if stderr.write_all(b"\n").is_err() {
                            break;
                        }
                    }
                });
            logger.ok().map(|_| Monitor::new(Some(sender)))
        })
        .as_ref()
}

pub fn enabled() -> bool {
    monitor().is_some()
}

pub fn start() -> Option<Instant> {
    monitor().map(|_| Instant::now())
}

pub fn record(stage: Stage, duration: Duration) {
    if let Some(monitor) = monitor() {
        monitor.record(stage, duration);
    }
}

pub fn record_output(stage: Stage, output: super::OutputId, duration: Duration) {
    if let Some(monitor) = monitor() {
        monitor.record_output(stage, output.0, duration);
    }
}

pub fn count(counter: Count) {
    if let Some(monitor) = monitor() {
        monitor.counters[counter as usize].fetch_add(1, Ordering::Relaxed);
    }
}

pub fn record_since(stage: Stage, started: Option<Instant>) {
    if let Some(started) = started {
        record(stage, started.elapsed());
    }
}

pub fn presented(output: super::OutputId, timestamp: Duration, hardware_clock: bool) {
    if let Some(monitor) = monitor() {
        monitor.presented(output.0, timestamp, hardware_clock);
    }
}

pub fn retire_output(output: super::OutputId) {
    if let Some(monitor) = monitor() {
        monitor
            .outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&output.0);
    }
}

pub fn flush_if_due() {
    if let Some(monitor) = monitor() {
        monitor.flush_if_due();
    }
}

pub struct Span {
    stage: Stage,
    started: Option<Instant>,
}

impl Span {
    pub fn new(stage: Stage) -> Self {
        Self {
            stage,
            started: start(),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        record_since(self.stage, self.started);
    }
}

#[cfg(any(test, feature = "behavior-checks"))]
pub mod checks {
    use super::*;

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
}
