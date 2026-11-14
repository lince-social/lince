use crossbeam_queue::ArrayQueue;
use serde::Serialize;

mod accounting;
pub use accounting::{Coverage, Phase, SystemReading, SystemThreadReading, TimingKind};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
    NativePhaseStartup => "native-phase-startup",
    NativePhaseStartupCpu => "native-phase-startup-owner-cpu",
    NativePhaseFirst => "native-phase-first",
    NativePhaseFirstCpu => "native-phase-first-owner-cpu",
    NativePhasePreUpdate => "native-phase-pre-update",
    NativePhasePreUpdateCpu => "native-phase-pre-update-owner-cpu",
    NativePhaseFixedLoop => "native-phase-fixed-loop",
    NativePhaseFixedLoopCpu => "native-phase-fixed-loop-owner-cpu",
    NativePhaseUpdate => "native-phase-update",
    NativePhaseUpdateCpu => "native-phase-update-owner-cpu",
    NativePhaseSpawnScene => "native-phase-spawn-scene",
    NativePhaseSpawnSceneCpu => "native-phase-spawn-scene-owner-cpu",
    NativePhasePostUpdate => "native-phase-post-update",
    NativePhasePostUpdateCpu => "native-phase-post-update-owner-cpu",
    NativePhaseLast => "native-phase-last",
    NativePhaseLastCpu => "native-phase-last-owner-cpu",
    NativePhaseCustom => "native-phase-custom",
    NativePhaseCustomCpu => "native-phase-custom-owner-cpu",
    NativeUpdate => "native-update",
    NativeUiSynchronization => "native-ui-synchronization",
    NativeUiComputation => "native-ui-computation",
    NativeUiGeometry => "native-ui-geometry",
    NativeAreaFields => "native-area-fields",
    NativeAreaValidation => "native-area-validation",
    NativeAreaEffects => "native-area-effects",
    NativeSpatialForces => "native-spatial-forces",
    NativeHoleLayers => "native-hole-layers",
    NativeTerrainPrepare => "native-terrain-prepare",
    NativeBodiesSynchronize => "native-bodies-synchronize",
    NativeProteinMotionPrepare => "native-protein-motion-prepare",
    NativePixels => "native-pixels",
    NativeStyles => "native-styles",
    NativeTokenPreparation => "native-token-prepare-and-collection",
    NativeDescriptionMarkup => "native-description-markup",
    NativeDescriptionEditors => "native-description-editor-scan",
    NativeCanvasSync => "native-canvas-host-synchronize",
    NativeCanvasSnapshot => "native-canvas-host-snapshot",
    NativeAccessibleInputs => "native-accessible-input-values",
    NativeSandSizes => "native-sand-sizes",
    NativeIconColors => "native-icon-colors",
    NativeIconMetrics => "native-icon-metrics",
    NativeLayoutMetrics => "native-layout-metrics",
    NativeLayoutSolve => "native-layout-solve",
    NativeLayoutConstraints => "native-layout-constraints",
    NativeLayoutCollection => "native-layout-collection",
    NativeLayoutConstraintSolve => "native-layout-constraint-solve",
    NativeLayoutApplication => "native-layout-application",
    NativeSurfaces => "native-surfaces",
    NativeSurfaceDerivation => "native-surface-derivation",
    NativeSurfaceApplication => "native-surface-application",
    NativeRecordProjection => "native-record-projection",
    NativeRecordPresentation => "native-record-presentation",
    RecordBindingUpdate => "record-binding-update",
    EditProbeSetText => "edit-probe-set-text",
    EditProbeMirror => "edit-probe-mirror-observed-await",
    EditProbeDurable => "edit-probe-durable-recovery-observed-await",
    EditProbeAcknowledged => "edit-probe-acknowledgement-observed-await",
    EditProbeSettled => "edit-probe-refresh-settled-observed-await",
    EditProbeReopen => "edit-probe-cold-reopen-await",
    RecordEditSplice => "record-edit-splice",
    RecordDraftPrepare => "record-draft-prepare",
    RecordDraftQueueWait => "record-draft-queue-wait",
    RecordDraftWrite => "record-draft-write-await",
    RecordChangeService => "record-change-service-await",
    RecordAccessWriteWait => "record-access-write-wait",
    RecordAccessReadWait => "record-access-read-wait",
    RecordAccessWriteHold => "record-access-write-hold-await",
    RecordAccessReadHold => "record-access-read-hold-await",
    RecordAdmission => "record-admission-await",
    RecordTextPermissions => "record-text-permissions-await",
    RecordImportWait => "record-import-lock-wait",
    RecordImportHold => "record-import-lock-hold-await",
    RecordAcceptedLookup => "record-accepted-document-await",
    RecordPrepare => "record-canonical-prepare",
    RecordGuardBase => "record-guard-base-validation",
    RecordGuardCandidateImport => "record-guard-candidate-import",
    RecordGuardCandidateBuild => "record-guard-candidate-build",
    RecordGuardHistory => "record-guard-history-validation",
    RecordGuardHistoryEncode => "record-guard-history-encoding",
    RecordSnapshotExport => "record-canonical-snapshot-export",
    RecordPersistence => "record-canonical-persist-await",
    RecordCompositionCheck => "record-composition-check-await",
    RecordDeltaExport => "record-canonical-delta-export",
    RecordSignerWait => "record-signer-wait",
    RecordTransactionBegin => "record-canonical-begin-await",
    RecordTransactionBody => "record-canonical-transaction-body-await",
    RecordTransactionCommit => "record-canonical-commit-await",
    RecordObserve => "record-observe-state-await",
    RecordReact => "record-reactions-await",
    CellQuery => "cell-query-execute-await",
    CellQueryBatch => "cell-query-batch-await",
    ActionInspection => "action-inspection-await",
    CellQueryRecord => "cell-query-record-await",
    CellQueryPromise => "cell-query-promise-await",
    CellQueryDecision => "cell-query-decision-await",
    CellQueryFact => "cell-query-fact-await",
    CellQueryConcept => "cell-query-concept-await",
    CellQueryLingua => "cell-query-lingua-await",
    CellQueryAssertion => "cell-query-assertion-await",
    CellQueryTransfer => "cell-query-transfer-await",
    CellQueryTransferSettlementPreview => "cell-query-transfer-settlement-preview-await",
    CellQueryTransferBulkCompletionPreview => "cell-query-transfer-bulk-completion-preview-await",
    CellQueryAuth => "cell-query-auth-await",
    CellQueryKarma => "cell-query-karma-await",
    CellQueryKarmaRule => "cell-query-karma-rule-await",
    CellQueryTimeline => "cell-query-timeline-await",
    CellQueryCalendar => "cell-query-calendar-await",
    CellQuerySchedule => "cell-query-schedule-await",
    CellQueryEntry => "cell-query-entry-await",
    CellQueryFrequency => "cell-query-frequency-await",
    CellQueryRecurrence => "cell-query-recurrence-await",
    CellQueryNearby => "cell-query-nearby-await",
    CellQueryProjectionRequest => "cell-query-projection-request-await",
    ProteinRecordSelection => "protein-record-selection-await",
    ProteinRecordMaterialization => "protein-record-materialization-await",
    ProteinRecordSchema => "protein-record-schema-await",
    ProteinRecordNames => "protein-record-names-await",
    ProteinRecordWork => "protein-record-work-await",
    ProteinRecordIncludes => "protein-record-includes-await",
    ProteinWorkLogIds => "protein-work-log-ids-await",
    ProteinLinkGraphLoad => "protein-link-graph-load-await",
    ProteinLinkProjection => "protein-link-projection",
    ProteinThreadSelection => "protein-thread-selection-await",
    ProteinThreadProjection => "protein-thread-projection-await",
    RecordRelatedSubjects => "record-related-subjects-await",
    CellRefresh => "cell-refresh-await",
    CellSubjectValidation => "cell-subject-validation-await",
    CellRefreshCollaboration => "cell-refresh-collaboration-await",
    CollabDeltaLockWait => "collab-delta-lock-wait",
    CollabDeltaDocument => "collab-delta-document-await",
    CollabDeltaExport => "collab-delta-export",
    SyncService => "sync-service-await",
    SyncWork => "sync-work-await",
    SyncSummary => "sync-summary",
    SyncHistoryAppend => "sync-history-append-await",
    SyncHistoryPrepare => "sync-history-prepare",
    SyncHistoryBegin => "sync-history-begin-await",
    SyncHistoryInsert => "sync-history-insert-await",
    SyncHistoryPrune => "sync-history-prune-await",
    SyncHistoryCommit => "sync-history-commit-await",
    StoreWritePoolAcquire => "store-write-pool-acquire-await",
    StoreWriteBegin => "store-write-begin-await",
    StoreWriteSequence => "store-write-sequence-await",
    RecordBindingPublish => "record-binding-publish",
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
    ExportSubmit => "dmabuf-export-submit",
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
            pub fn from_label(label: &str) -> Option<Self> {
                match label { $($label => Some(Self::$name),)* _ => None }
            }
        }
    };
}

counts! {
    ActionQueryInvalidations => "action-query-invalidations",
    NativeUiNodes => "native-ui-nodes-scanned",
    NativeUiStyles => "native-ui-styles-changed",
    NativeUiMeasures => "native-ui-measures-changed",
    NativeUiEmptyMeasures => "native-ui-empty-measure-inputs",
    NativeUiImageMeasures => "native-ui-image-measure-inputs",
    NativeUiFixedMeasures => "native-ui-fixed-measure-inputs",
    NativeUiTextInputMeasures => "native-ui-text-input-measure-inputs",
    NativeUiTextMeasures => "native-ui-text-measure-inputs",
    NativeUiCustomMeasures => "native-ui-custom-measure-inputs",
    NativeUiMeasureNoops => "native-ui-measure-only-noops",
    NativeUiTargets => "native-ui-targets-changed",
    NativeUiHierarchy => "native-ui-hierarchy-updates",
    NativeUiGeometryInputs => "native-ui-geometry-inputs-changed",
    NativeUiRoots => "native-ui-roots-computed",
    NativeUiGeometryNodes => "native-ui-geometry-nodes-visited",
    NativeInputApplied => "native-input-applied",
    NativeInputRetired => "native-input-retired",
    NativeInputBatches => "native-input-frame-batches",
    NativeInputPresented => "native-input-batches-presented-per-output",
    ClientInputForwarded => "client-input-forwarded",
    ClientInputProxyPresented => "client-input-buffer-proxies-presented-per-output",
    TimingRejected => "timing-timestamps-rejected",
    TimingCapacity => "timing-capacity-exceeded",
    NativePhaseMissing => "native-phase-missing-schedules",
    TextMeasureCandidates => "native-text-measure-candidates",
    TextMeasureBytes => "native-text-measure-source-bytes",
    TextMeasureNew => "native-text-measure-new-blocks",
    TextMeasureNewSquare => "native-text-measure-new-square",
    TextMeasureNewPlain => "native-text-measure-new-plain-text",
    TextMeasureNewEditable => "native-text-measure-new-editable-text",
    TextMeasureNewOperation => "native-text-measure-new-operation",
    TextMeasureNewOverview => "native-text-measure-new-overview",
    TextMeasureNewTimer => "native-text-measure-new-work-timer",
    TextMeasureNewAccess => "native-text-measure-new-access-control",
    TextMeasureNewVisibility => "native-text-measure-new-visibility",
    TextMeasureNewTrust => "native-text-measure-new-trust",
    TextMeasureNewCamera => "native-text-measure-new-hand-camera",
    TextMeasureNewVideo => "native-text-measure-new-video",
    TextMeasureNewSync => "native-text-measure-new-sync",
    TextMeasureNewTerminal => "native-text-measure-new-terminal",
    TextMeasureNewConfiguration => "native-text-measure-new-configuration",
    TextMeasureNewOrgan => "native-text-measure-new-organ",
    TextMeasureNewTodo => "native-text-measure-new-todo",
    TextMeasureNewOntology => "native-text-measure-new-ontology",
    TextMeasureNewOther => "native-text-measure-new-other-or-unresolved",
    TextMeasureChanged => "native-text-measure-changed-text",
    TextMeasureStyle => "native-text-measure-style-or-span-changes",
    TextMeasureScale => "native-text-measure-scale-changes",
    TextMeasureUnits => "native-text-measure-viewport-or-rem-changes",
    TextMeasureInvisible => "native-text-measure-zero-size-or-invisible-before-layout",
    WindowPixelsCommit => "window-pixels-only-commits",
    WindowSemanticCommit => "window-semantic-commits",
    WindowCallbackCommit => "window-callback-only-commits",
    UnchangedFrameDeferred => "output-unchanged-frames-deferred",
    UnchangedFrameDelivered => "output-unchanged-frames-delivered",
    AccessibleInputChecked => "accessible-input-values-checked",
    AccessibleValueBytes => "accessible-input-value-bytes-materialized",
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
    NativeAreaDefinitionsReused => "native-area-definitions-reused",
    NativeAreaDefinitionsRebuilt => "native-area-definitions-rebuilt",
    RecordDraftSnapshot => "record-draft-snapshots",
    RecordDraftSnapshotBytes => "record-draft-snapshot-bytes",
    RecordDraftEncodedBytes => "record-draft-encoded-bytes",
    RecordDraftDelta => "record-draft-deltas",
    RecordDraftDeltaBytes => "record-draft-delta-bytes",
    RecordDraftCheckpointBytes => "record-draft-initial-checkpoint-bytes",
    RecordDraftWrites => "record-draft-writes",
    RecordDraftClears => "record-draft-clears",
    RecordCanonicalChanges => "record-canonical-changes",
    RecordCanonicalBegins => "record-canonical-begins",
    RecordCanonicalCommits => "record-canonical-commits",
    RecordGuardHistoryBytes => "record-guard-history-bytes",
    RecordGuardHistoryValidatedBytes => "record-guard-history-validated-bytes",
    RecordGuardHistoryReusedTextBytes => "record-guard-history-reused-text-bytes",
    RecordGuardBasesReused => "record-guard-bases-reused",
    RecordCanonicalSnapshotBytes => "record-canonical-snapshot-bytes",
    RecordCanonicalDeltaBytes => "record-canonical-delta-bytes",
    CellQueries => "cell-queries",
    CellQueryRows => "cell-query-rows",
    CellQueryBatchReuses => "cell-query-batch-reuses",
    CellQueryResultsUnchanged => "cell-query-results-unchanged",
    SyncSnapshotPublished => "sync-ui-snapshots-published",
    SyncSnapshotUnchanged => "sync-ui-snapshots-unchanged",
    ProteinLinkGraphLoads => "protein-link-graph-loads",
    ProteinLinkEdgesVisited => "protein-link-edges-visited",
    ProteinThreadSelections => "protein-thread-selections",
    RecordRelatedSubjectBatches => "record-related-subject-batches",
    RecordReadBodyBytes => "record-read-body-bytes",
    RecordBindingViews => "record-binding-views-compared",
    RecordBindingViewChanged => "record-binding-views-changed",
    RecordEditorTextCopies => "record-editor-input-text-copies",
    RecordEditorTextCopiedBytes => "record-editor-input-text-copied-bytes",
    StyleInvalidated => "style-resolver-cache-invalidations",
    StyleValuesPrepared => "style-values-derived-during-preparation",
    StyleReadCached => "style-runtime-cache-reads",
    StyleReadDirect => "style-runtime-direct-resolutions",
    NodeMetricsVisited => "node-metric-consumers-visited",
    NodeMetricsActive => "node-metric-consumers-requiring-values",
    IconMetricsVisited => "icon-metric-consumers-visited",
    SandSizesVisited => "sand-size-consumers-visited",
    SandSizesChanged => "sand-size-consumers-changed",
    DescriptionRenders => "description-markup-renders",
    DescriptionSourceBytes => "description-markup-source-bytes",
    DescriptionEditorsVisited => "description-editors-compared",
    DescriptionRefreshes => "description-previews-refreshed",
    NativeSurfaceRequests => "native-surface-requests-considered",
    NativeSurfacesVisible => "native-surface-visible-requests",
    NativeSurfacesCreated => "native-surface-targets-created",
    NativeSurfacesReused => "native-surface-targets-reused",
    NativeSurfacesResized => "native-surface-targets-resized",
    NativeLayoutRuntimeChanged => "native-layout-runtimes-changed",
    NativeCanvasSnapshots => "native-canvas-host-snapshots",
    NativeCanvasCalls => "native-canvas-host-calls-serviced",
    NativeConstraintRoots => "native-layout-constraint-roots",
    NativeConstraintItems => "native-layout-constraint-items",
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
    fn merge(&mut self, other: Self) {
        self.count = self.count.saturating_add(other.count);
        self.total_us = self.total_us.saturating_add(other.total_us);
        self.maximum_us = self.maximum_us.max(other.maximum_us);
        for (target, value) in self.buckets.iter_mut().zip(other.buckets) {
            *target = target.saturating_add(value);
        }
    }

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
            kind: TimingKind::for_label(stage),
            buckets: self.buckets,
        }
    }
}

#[derive(Serialize)]
pub struct Reading {
    pub stage: &'static str,
    pub kind: TimingKind,
    pub buckets: [u64; LIMITS_US.len()],
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
    presentations: u64,
    stages: BTreeMap<usize, Distribution>,
}

#[derive(Serialize)]
pub struct OutputReading {
    pub output: u64,
    pub presentations: u64,
    pub clock: &'static str,
    pub interval: Reading,
    pub stages: Vec<Reading>,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub counter_catalog: Vec<&'static str>,
    pub histogram_limits_us: &'static [u64],
    pub process: u32,
    pub phase: &'static str,
    pub sequence: u64,
    pub window_started_us: u64,
    pub window_ended_us: u64,
    pub final_report: bool,
    pub systems: Vec<SystemReading>,
    pub render_diagnostics: Vec<RenderDiagnostic>,
    pub spans: Vec<SpanReading>,
    pub dropped_span_trace: u64,
    pub dropped_system_samples: u64,
    pub coverage: Vec<Coverage>,
    #[serde(skip)]
    completion: Option<SyncSender<()>>,
    pub elapsed_us: u64,
    pub dropped_reports: u64,
    pub stages: Vec<Reading>,
    pub outputs: Vec<OutputReading>,
    pub counters: Vec<CountReading>,
    pub input_trace: Vec<InputTrace>,
    pub dropped_input_trace: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputStage {
    Dispatch,
    NativeEnqueued,
    NativeDequeued,
    NativeDelivered,
    FramePrepared,
    GpuReady,
    Published,
    PresentedDrm,
    PresentedObserved,
    ClientForwarded,
    ClientProxyDrm,
    ClientProxyObserved,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct InputTrace {
    pub stage: InputStage,
    pub origin: u64,
    pub origin_us: u64,
    pub observed_us: u64,
    pub input_serial: u64,
    pub pointer_serial: u64,
    pub frame: u64,
    pub output: Option<u64>,
}

#[derive(Serialize)]
pub struct SpanReading {
    pub stage: &'static str,
    pub started_us: u64,
    pub ended_us: u64,
    pub start_thread: u64,
    pub end_thread: u64,
}

#[derive(Serialize)]
pub struct RenderDiagnostic {
    pub path: String,
    pub unit: std::borrow::Cow<'static, str>,
    pub count: u64,
    pub total: f64,
    pub maximum: f64,
    pub published_us: u64,
    pub source_frame: Option<u64>,
    pub identity: &'static str,
}

const SPAN_TRACE_LIMIT: usize = 16_384;
const SYSTEM_LIMIT: usize = 2048;
const SYSTEM_SHARDS: usize = 16;

thread_local! {
    static THREAD_ID: u64 = {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    };
}

pub fn thread_id() -> u64 {
    THREAD_ID.with(|id| *id)
}

const INPUT_TRACE_LIMIT: usize = 1024;

#[derive(Serialize)]
pub struct CountReading {
    pub counter: &'static str,
    pub count: u64,
}

struct Monitor {
    stages: Vec<Mutex<Distribution>>,
    systems: [Mutex<BTreeMap<String, accounting::SystemSample>>; SYSTEM_SHARDS],
    render_diagnostics: Mutex<BTreeMap<String, RenderDiagnostic>>,
    spans: ArrayQueue<SpanReading>,
    dropped_span_trace: AtomicU64,
    dropped_system_samples: AtomicU64,
    snapshot_at_us: AtomicU64,
    sequence: AtomicU64,
    flushing: Mutex<()>,
    finalized: AtomicBool,
    phase: AtomicU64,
    outputs: Mutex<BTreeMap<u64, Output>>,
    started: Instant,
    reported_at_us: AtomicU64,
    dropped_reports: AtomicU64,
    sender: Option<SyncSender<Report>>,
    counters: Vec<AtomicU64>,
    input_trace: ArrayQueue<InputTrace>,
    dropped_input_trace: AtomicU64,
}

impl Monitor {
    #[cfg(feature = "native-presentation")]
    fn backend(&self, reading: engine::performance::Reading) {
        match reading {
            engine::performance::Reading::Elapsed(label, elapsed) => {
                if let Some(stage) = Stage::from_label(label) {
                    self.record(stage, elapsed);
                }
            }
            engine::performance::Reading::Count(label, count) => {
                if let Some(counter) = Count::from_label(label) {
                    self.counters[counter as usize].fetch_add(count, Ordering::Relaxed);
                }
            }
        }
    }

    fn new(sender: Option<SyncSender<Report>>) -> Self {
        Self {
            stages: Stage::ALL
                .iter()
                .map(|_| Mutex::new(Distribution::default()))
                .collect(),
            outputs: Mutex::new(BTreeMap::new()),
            systems: std::array::from_fn(|_| Mutex::new(BTreeMap::new())),
            render_diagnostics: Mutex::new(BTreeMap::new()),
            spans: ArrayQueue::new(SPAN_TRACE_LIMIT),
            dropped_span_trace: AtomicU64::new(0),
            dropped_system_samples: AtomicU64::new(0),
            snapshot_at_us: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            flushing: Mutex::new(()),
            finalized: AtomicBool::new(false),
            phase: AtomicU64::new(0),
            started: Instant::now(),
            reported_at_us: AtomicU64::new(0),
            dropped_reports: AtomicU64::new(0),
            sender,
            counters: Count::ALL.iter().map(|_| AtomicU64::new(0)).collect(),
            input_trace: ArrayQueue::new(INPUT_TRACE_LIMIT),
            dropped_input_trace: AtomicU64::new(0),
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
        output.presentations = output.presentations.saturating_add(1);
        output.previous = Some(timestamp);
    }

    fn record_system(&self, name: &str, wall: Duration, cpu: Duration, exclusive_cpu: Duration) {
        if name.len() > 512
            || !(name.starts_with("bevy_")
                || name.starts_with("lince_interface::")
                || name.starts_with("lince_park::"))
        {
            self.dropped_system_samples.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let mut systems = self.systems[(thread_id() % SYSTEM_SHARDS as u64) as usize]
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(sample) = systems.get_mut(name) {
            sample.record(wall, cpu, exclusive_cpu);
        } else if systems.len() < SYSTEM_LIMIT {
            let mut sample = accounting::SystemSample::default();
            sample.record(wall, cpu, exclusive_cpu);
            systems.insert(name.to_owned(), sample);
        } else {
            self.dropped_system_samples.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn snapshot(&self) -> Report {
        let ended = self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        let began = self.snapshot_at_us.swap(ended, Ordering::Relaxed);
        let mut combined = BTreeMap::<String, accounting::SystemSample>::new();
        for shard in &self.systems {
            let samples =
                std::mem::take(&mut *shard.lock().unwrap_or_else(|error| error.into_inner()));
            for (name, sample) in samples {
                combined.entry(name).or_default().merge(sample);
            }
        }
        let systems = combined
            .iter_mut()
            .filter_map(|(name, sample)| sample.take(name))
            .collect::<Vec<_>>();
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
                (output.presentations != 0
                    || output.intervals.count != 0
                    || output.stages.values().any(|stage| stage.count != 0))
                .then(|| OutputReading {
                    output: *id,
                    presentations: std::mem::take(&mut output.presentations),
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
        let render_diagnostics = std::mem::take(
            &mut *self
                .render_diagnostics
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        )
        .into_values()
        .collect::<Vec<_>>();
        let observed_gpu = render_diagnostics
            .iter()
            .any(|reading| reading.path.ends_with("/elapsed_gpu"));
        let observed_systems = !systems.is_empty();
        Report {
            schema_version: 2,
            histogram_limits_us: &LIMITS_US,
            counter_catalog: Count::ALL.iter().map(|counter| counter.label()).collect(),
            process: std::process::id(),
            phase: Phase::ALL[self.phase.load(Ordering::Relaxed) as usize].label(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            window_started_us: began,
            window_ended_us: ended,
            final_report: false,
            systems,
            render_diagnostics,
            spans: std::iter::from_fn(|| self.spans.pop())
                .take(SPAN_TRACE_LIMIT)
                .collect(),
            dropped_span_trace: self.dropped_span_trace.swap(0, Ordering::Relaxed),
            dropped_system_samples: self.dropped_system_samples.swap(0, Ordering::Relaxed),
            coverage: accounting::coverage(observed_systems, observed_gpu),
            completion: None,
            elapsed_us: ended,
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
            input_trace: std::iter::from_fn(|| self.input_trace.pop())
                .take(INPUT_TRACE_LIMIT)
                .collect(),
            dropped_input_trace: self.dropped_input_trace.swap(0, Ordering::Relaxed),
        }
    }

    fn trace_input(&self, reading: InputTrace) {
        if self.input_trace.push(reading).is_err() {
            self.dropped_input_trace.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn trace_span(&self, reading: SpanReading) {
        if self.spans.push(reading).is_err() {
            self.dropped_span_trace.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self) {
        let Ok(_flushing) = self.flushing.try_lock() else {
            return;
        };
        if self.finalized.load(Ordering::Acquire) {
            return;
        }
        if let Some(sender) = &self.sender
            && sender.try_send(self.snapshot()).is_err()
        {
            self.dropped_reports.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush_if_due(&self) {
        if self.finalized.load(Ordering::Acquire) || self.sender.is_none() {
            return;
        }
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
        self.flush();
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
            let mut output = match std::env::var_os("LINCE_PERFORMANCE_LOG") {
                Some(path) => {
                    let path = std::path::PathBuf::from(path);
                    if !path.is_absolute() {
                        eprintln!("Lince performance log needs an absolute private path");
                        return None;
                    }
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    match options.open(path) {
                        Ok(file) => Some(file),
                        Err(error) => {
                            eprintln!("Lince performance log unavailable: {error}");
                            return None;
                        }
                    }
                }
                None => None,
            };
            let (sender, receiver) = sync_channel::<Report>(4);
            let logger = std::thread::Builder::new()
                .name("lince-perf-report".into())
                .spawn(move || {
                    use std::io::Write;
                    loop {
                        let report = match receiver.recv_timeout(Duration::from_secs(1)) {
                            Ok(report) => report,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                if let Some(monitor) = monitor() {
                                    monitor.flush_if_due();
                                }
                                continue;
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        };
                        let mut bytes = b"Lince performance ".to_vec();
                        if serde_json::to_writer(&mut bytes, &report).is_err() {
                            break;
                        }
                        bytes.push(b'\n');
                        let written = if let Some(file) = &mut output {
                            file.write_all(&bytes).and_then(|()| file.flush())
                        } else {
                            let stderr = std::io::stderr();
                            let mut stderr = stderr.lock();
                            stderr.write_all(&bytes).and_then(|()| stderr.flush())
                        };
                        if written.is_err() {
                            break;
                        }
                        if let Some(completion) = report.completion {
                            let _ = completion.try_send(());
                        }
                    }
                });
            let monitor = logger.ok().map(|_| Monitor::new(Some(sender)));
            #[cfg(feature = "native-presentation")]
            if monitor.is_some() {
                engine::performance::install(backend);
            }
            monitor
        })
        .as_ref()
}

#[cfg(feature = "native-presentation")]
fn backend(reading: engine::performance::Reading) {
    if let Some(monitor) = monitor() {
        monitor.backend(reading);
    }
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
    add(counter, 1);
}

pub fn add(counter: Count, value: u64) {
    if let Some(monitor) = monitor() {
        monitor.counters[counter as usize].fetch_add(value, Ordering::Relaxed);
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

pub fn trace_input(reading: InputTrace) {
    if let Some(monitor) = monitor() {
        monitor.trace_input(reading);
    }
}

pub fn flush() {
    if let Some(monitor) = monitor() {
        monitor.flush();
    }
}

pub fn record_render_diagnostic(path: &str, unit: &std::borrow::Cow<'static, str>, value: f64) {
    let Some(monitor) = monitor() else {
        return;
    };
    if !path.starts_with("render/") || path.len() > 512 || !value.is_finite() || value < 0.0 {
        count(Count::TimingRejected);
        return;
    }
    if let Ok(mut values) = monitor.render_diagnostics.try_lock() {
        let now = monitor
            .started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        if let Some(reading) = values.get_mut(path) {
            if &reading.unit != unit {
                count(Count::TimingRejected);
                return;
            }
            reading.count = reading.count.saturating_add(1);
            reading.total += value;
            reading.maximum = reading.maximum.max(value);
            reading.published_us = now;
        } else if values.len() < 512 {
            values.insert(
                path.to_owned(),
                RenderDiagnostic {
                    path: path.to_owned(),
                    unit: unit.clone(),
                    count: 1,
                    total: value,
                    maximum: value,
                    published_us: now,
                    source_frame: None,
                    identity: "last-completed-bevy-diagnostic; publication-time-is-not-gpu-time",
                },
            );
        } else {
            count(Count::TimingCapacity);
        }
    } else {
        count(Count::TimingCapacity);
    }
}

pub fn record_system(name: &str, wall: Duration, cpu: Duration, exclusive_cpu: Duration) {
    if let Some(monitor) = monitor() {
        monitor.record_system(name, wall, cpu, exclusive_cpu);
    }
}

pub fn set_phase(phase: Phase) {
    if let Some(monitor) = monitor() {
        let _flushing = monitor
            .flushing
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if monitor.finalized.load(Ordering::Acquire) {
            return;
        }
        if let Some(sender) = &monitor.sender
            && sender.try_send(monitor.snapshot()).is_err()
        {
            monitor.dropped_reports.fetch_add(1, Ordering::Relaxed);
        }
        monitor.phase.store(phase as u64, Ordering::Relaxed);
    }
}

pub fn finish(timeout: Duration) -> bool {
    let Some(monitor) = monitor() else {
        return true;
    };
    let Some(sender) = &monitor.sender else {
        return true;
    };
    let flushing = monitor
        .flushing
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if monitor.finalized.load(Ordering::Acquire) {
        return true;
    }
    let (completion, finished) = sync_channel(1);
    let mut report = monitor.snapshot();
    report.final_report = true;
    report.completion = Some(completion);
    if sender.try_send(report).is_err() {
        monitor.dropped_reports.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    monitor.finalized.store(true, Ordering::Release);
    drop(flushing);
    finished.recv_timeout(timeout).is_ok()
}

pub struct Span {
    stage: Stage,
    started: Option<Instant>,
    thread: u64,
}

impl Span {
    pub fn new(stage: Stage) -> Self {
        Self {
            stage,
            started: start(),
            thread: if enabled() { thread_id() } else { 0 },
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        record_since(self.stage, self.started);
        if let (Some(monitor), Some(started)) = (monitor(), self.started) {
            let ended = Instant::now();
            let reading = SpanReading {
                stage: self.stage.label(),
                started_us: started
                    .saturating_duration_since(monitor.started)
                    .as_micros()
                    .min(u128::from(u64::MAX)) as u64,
                ended_us: ended
                    .saturating_duration_since(monitor.started)
                    .as_micros()
                    .min(u128::from(u64::MAX)) as u64,
                start_thread: self.thread,
                end_thread: thread_id(),
            };
            monitor.trace_span(reading);
        }
    }
}

#[cfg(any(test, feature = "behavior-checks"))]
pub mod checks {
    use super::*;

    #[cfg(feature = "native-presentation")]
    #[cfg_attr(test, test)]
    pub fn backend_phases_share_the_monitor_and_only_allow_declared_numeric_readings() {
        let monitor = Monitor::new(None);
        monitor.backend(engine::performance::Reading::Elapsed(
            "record-guard-base-validation",
            Duration::from_micros(25),
        ));
        monitor.backend(engine::performance::Reading::Count("cell-query-rows", 8));
        monitor.backend(engine::performance::Reading::Elapsed(
            "cell-query-batch-await",
            Duration::from_micros(30),
        ));
        monitor.backend(engine::performance::Reading::Count(
            "cell-query-batch-reuses",
            7,
        ));
        monitor.backend(engine::performance::Reading::Elapsed(
            "private Record contents",
            Duration::from_secs(1),
        ));
        monitor.backend(engine::performance::Reading::Count(
            "private Record contents",
            99,
        ));
        let report = monitor.snapshot();
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
}
