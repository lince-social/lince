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
    time::{Duration, Instant, SystemTime},
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
    NativeOutputGeometry => "native-output-geometry",
    NativeOutputItemGeometry => "native-output-item-geometry",
    NativeOutputSynchronization => "native-output-synchronization",
    NativeOutputViewPreparation => "native-output-view-preparation",
    NativeOutputFillPreparation => "native-output-fill-preparation",
    NativeOutputSurfacePreparation => "native-output-surface-preparation",
    NativeOutputFillPublication => "native-output-fill-publication",
    NativeStackingInputs => "native-stacking-inputs",
    NativeTransparentOrder => "native-transparent-order",
    NativeCoplanarOrder => "native-coplanar-order",
    NativeDecorationOrder => "native-decoration-order",
    CanvasBackgroundPresentation => "canvas-background-presentation",
    NativeStaticTextEmpty => "native-static-text-measure-empty",
    NativeStaticTextShort => "native-static-text-measure-1-to-64-bytes",
    NativeStaticTextMedium => "native-static-text-measure-65-to-512-bytes",
    NativeStaticTextLong => "native-static-text-measure-over-512-bytes",
    NativeStaticTextNoWrap => "native-static-text-measure-no-wrap",
    NativeStaticTextMultiSpan => "native-static-text-measure-multiple-spans",
    NativeAreaFields => "native-area-fields",
    NativeAreaValidation => "native-area-validation",
    AreaEditorMaintenance => "area-editor-maintenance",
    AreaChangeEditorMaintenance => "area-change-editor-maintenance",
    CommandDefinitionReadAwait => "command-definition-read-await",
    CommandInspectionReadAwait => "command-inspection-read-await",
    NativeAreaEffects => "native-area-effects",
    NativeAreaSandEffects => "native-area-sand-effects",
    ProteinOrderRead => "protein-order-read",
    NativeSpatialForces => "native-spatial-forces",
    NativeHoleLayers => "native-hole-layers",
    NativeTerrainPrepare => "native-terrain-prepare",
    TerrainTilePreparation => "terrain-tile-preparation",
    TerrainPreviewPreparation => "terrain-preview-preparation",
    TerrainGroundPresentation => "terrain-ground-presentation",
    NativeHoleBackgrounds => "native-hole-backgrounds",
    NativeBodiesSynchronize => "native-bodies-synchronize",
    NativeProteinMotionPrepare => "native-protein-motion-prepare",
    NativePixels => "native-pixels",
    NativeStyles => "native-styles",
    NativeTokenPreparation => "native-token-prepare-and-collection",
    NativeDescriptionMarkup => "native-description-markup",
    NativeDescriptionEditors => "native-description-editor-scan",
    DescriptionRasterCompletion => "description-raster-completion",
    DescriptionRasterAdmission => "description-raster-admission",
    DescriptionPictureLoad => "description-picture-load",
    DescriptionDiagramRasterize => "description-diagram-rasterize",
    DescriptionDiagramFontPreparation => "description-diagram-font-preparation",
    NativeTerminalPresentation => "native-terminal-presentation",
    NativeTerminalSnapshot => "native-terminal-vt-snapshot",
    TerminalReplyApplication => "terminal-reply-application",
    TerminalOwnerService => "terminal-owner-service",
    TransferListOrderPreparation => "transfer-list-order-preparation",
    TransferPagePreparation => "transfer-page-preparation",
    TransferListPresentation => "transfer-list-presentation",
    TransferActionInputPreparation => "transfer-action-input-preparation",
    TransferFormViewPreparation => "transfer-form-view-preparation",
    TransferFormChoicePreparation => "transfer-form-choice-preparation",
    TransferReviewSummary => "transfer-review-summary",
    TransferPreviewViewPreparation => "transfer-preview-view-preparation",
    TransferReviewedActionPreparation => "transfer-reviewed-action-preparation",
    TransferReceiptListPreparation => "transfer-receipt-list-preparation",
    ReceiptCandidateRead => "receipt-candidate-read",
    ReceiptIdentityRead => "receipt-identity-read",
    ReceiptArtifactIdentities => "receipt-artifact-identities",
    ReceiptArtifactHistoryProjection => "receipt-artifact-history-projection",
    ReceiptEventProjectionPreparation => "receipt-event-projection-preparation",
    ReceiptReviewValidation => "receipt-review-validation",
    ReceiptModelMerge => "receipt-model-merge",
    ReceiptDocumentVerification => "receipt-document-verification",
    ReceiptSignatureVerification => "receipt-signature-verification",
    ReceiptHeadProjection => "receipt-head-projection",
    ReceiptPersonApprovalVerification => "receipt-person-approval-verification",
    ReceiptSourceOptionFilter => "receipt-source-option-filter",
    ReceiptSourceMeasurePreparation => "receipt-source-measure-preparation",
    SupplierTrustQueryPreparation => "supplier-trust-query-preparation",
    TrustQueryEvaluation => "trust-query-evaluation",
    TrustQueryContextPreparation => "trust-query-context-preparation",
    OperationReplyDelivery => "operation-reply-delivery",
    OperationCatalogPreparation => "operation-catalog-preparation",
    OperationSuggestionRefresh => "operation-suggestion-refresh",
    OperationSuggestionPresentation => "operation-suggestion-presentation",
    SpeechReplyDelivery => "speech-reply-delivery",
    NativeCommandHistory => "native-command-history",
    NativeExplorerListingSort => "native-explorer-listing-sort",
    NativeExplorerRows => "native-explorer-row-model",
    NativeExplorerPresentation => "native-explorer-presentation",
    NativeMediaFit => "native-media-image-fit",
    NativeVideoPresentation => "native-video-presentation",
    WorkTimerLogPreparation => "work-timer-log-preparation",
    WorkTimerUpdate => "work-timer-update",
    WorkTimerFormReconciliation => "work-timer-form-reconciliation",
    WorkTimerRowPresentation => "work-timer-row-presentation",
    TimeFeedUpdate => "time-feed-update",
    TimePresentationUpdate => "time-presentation-update",
    TimeSettingsPreparation => "time-settings-preparation",
    TimeSourcePreparation => "time-source-preparation",
    TimeSubscriptionPreparation => "time-subscription-preparation",
    TimeGeometryPreparation => "time-geometry-preparation",
    TimeScenePreparation => "time-scene-preparation",
    TimeHitPickPreparation => "time-hit-pick-preparation",
    TimeOccurrencePreparation => "time-occurrence-preparation",
    TimeDetailPreparation => "time-detail-preparation",
    TimeDetailPresentation => "time-detail-presentation",
    TimeMotionUpdate => "time-motion-update",
    TimeLabelPreparation => "time-label-preparation",
    SourceViewUpdate => "source-view-update",
    SourceViewMaintenance => "source-view-maintenance",
    SourceCommandPreparation => "source-command-preparation",
    SourceBodyPreparation => "source-body-preparation",
    SourceBodyRulePreparation => "source-body-rule-preparation",
    SourcePreviewPresentation => "source-preview-presentation",
    SourceOutlinePreparation => "source-outline-preparation",
    SourceOutlineRowPreparation => "source-outline-row-preparation",
    FrequencyFieldCapture => "frequency-field-capture",
    FrequencyRowPreparation => "frequency-row-preparation",
    FrequencyCountdownPresentation => "frequency-countdown-presentation",
    FrequencyCountdownPublication => "frequency-countdown-publication",
    FrequencyRowConstruction => "frequency-row-construction",
    FrequencyFormPresentation => "frequency-form-presentation",
    FrequencyEditDatePreparation => "frequency-edit-date-preparation",
    FrequencyListPreparation => "frequency-list-preparation",
    KarmaListPreparation => "karma-list-preparation",
    KarmaRuleRowConstruction => "karma-rule-row-construction",
    KarmaControlPresentation => "karma-control-presentation",
    KarmaFormPresentation => "karma-form-presentation",
    KarmaColumnFitting => "karma-column-fitting",
    KarmaFieldCapture => "karma-field-capture",
    KarmaRowControls => "karma-row-controls",
    KarmaReadingPresentation => "karma-reading-presentation",
    KarmaReadingPreparation => "karma-reading-preparation",
    KarmaSuggestionSourcePreparation => "karma-suggestion-source-preparation",
    KarmaSuggestionQuery => "karma-suggestion-query",
    KarmaChoicePreparation => "karma-choice-preparation",
    KarmaInputPreparation => "karma-input-preparation",
    KarmaTransferChoicePreparation => "karma-transfer-choice-preparation",
    KarmaChoicePresentation => "karma-choice-presentation",
    CalendarProjectionUpdate => "calendar-projection-update",
    CalendarProjectionPreparation => "calendar-projection-preparation",
    TodoUpdate => "todo-update",
    TodoReplyRouting => "todo-reply-routing",
    OntologyReplyRouting => "ontology-reply-routing",
    CellReplyIndexPreparation => "cell-reply-index-preparation",
    CellReplyWaveCapture => "cell-reply-wave-capture",
    FrequencyReplyProcessing => "frequency-reply-processing",
    FrequencyReplySelection => "frequency-reply-selection",
    FrequencyMaintenance => "frequency-maintenance",
    KarmaReplyProcessing => "karma-reply-processing",
    KarmaReplySelection => "karma-reply-selection",
    KarmaMaintenance => "karma-maintenance",
    KarmaPreviewMaintenance => "karma-preview-maintenance",
    KarmaScheduleMaintenance => "karma-schedule-maintenance",
    KarmaPreviewFieldCapture => "karma-preview-field-capture",
    KarmaScheduleFieldCapture => "karma-schedule-field-capture",
    KarmaSchedulePresentation => "karma-schedule-presentation",
    KarmaScheduleFormPresentation => "karma-schedule-form-presentation",
    KarmaScheduleOutcomePartPresentation => "karma-schedule-outcome-part-presentation",
    KarmaScheduleOutcomePreparation => "karma-schedule-outcome-preparation",
    KarmaScheduleChoicePreparation => "karma-schedule-choice-preparation",
    KarmaScheduleDefaultTimezone => "karma-schedule-default-timezone",
    KarmaCommandFieldCapture => "karma-command-field-capture",
    KarmaCommandPresentation => "karma-command-presentation",
    KarmaCommandFormPresentation => "karma-command-form-presentation",
    AilurosFieldCapture => "ailuros-field-capture",
    AilurosRowPublication => "ailuros-row-publication",
    AilurosPresentation => "ailuros-presentation",
    AilurosFormPresentation => "ailuros-form-presentation",
    KarmaPreviewFingerprint => "karma-preview-fingerprint",
    KarmaPreviewReplyPreparation => "karma-preview-reply-preparation",
    KarmaHistoryDescription => "karma-history-description",
    KarmaEvidenceEncoding => "karma-evidence-encoding",
    KarmaHistoryEffectRead => "karma-history-effect-read",
    KarmaHistoryApplicationRead => "karma-history-application-read",
    TodoResultPreparation => "todo-result-preparation",
    TodoListPresentation => "todo-list-presentation",
    FioteLiveChoicesPresentation => "fiote-live-choices-presentation",
    FioteOptionAvailabilityPreparation => "fiote-option-availability-preparation",
    KanbanMaintenance => "kanban-maintenance",
    KanbanCardMaintenance => "kanban-card-maintenance",
    KanbanReplyProcessing => "kanban-reply-processing",
    KanbanHeaderPreparation => "kanban-header-preparation",
    KanbanOrderPreparation => "kanban-order-preparation",
    MapRequestPreparation => "map-request-preparation",
    MapPageApplication => "map-page-application",
    MapRecordListPreparation => "map-record-list-preparation",
    MapLiveFeedUpdate => "map-live-feed-update",
    CalendarPickerSync => "calendar-picker-sync",
    CalendarPickerPreparation => "calendar-picker-preparation",
    ShaderCastlePresentation => "shader-castle-presentation",
    CastleCardProjection => "castle-card-projection",
    NativeVideoImageUpdate => "native-video-image-update",
    NativeGifFramePrepare => "native-gif-frame-prepare",
    NativeCanvasSync => "native-canvas-host-synchronize",
    NativeCanvasSnapshot => "native-canvas-host-snapshot",
    NativeCanvasContent => "native-canvas-content-preparation",
    NativeCanvasRegistry => "native-canvas-registry-preparation",
    NativeCanvasApply => "native-canvas-placement-application",
    WorkspaceSnapshotPreparation => "workspace-snapshot-preparation",
    WorkspaceRootDiscovery => "workspace-root-discovery",
    WorkspacePersistenceOwner => "workspace-persistence-owner",
    WorkspaceHistorySelection => "workspace-history-selection",
    WorkspaceTrashRetirement => "workspace-trash-retirement",
    LayoutRecordRemember => "layout-record-remember",
    LayoutRecordForget => "layout-record-forget",
    LayoutRecordRestore => "layout-record-restore",
    LayoutRecordSnapshot => "layout-record-snapshot",
    RecordWorkLogPreparation => "record-work-log-preparation",
    WorkMetadataPreparation => "work-metadata-preparation",
    WorkLogValidation => "work-log-validation",
    SyncLocalContextRead => "sync-local-context-read-await",
    SyncLocalOperationInsert => "sync-local-operation-insert-await",
    NotificationPresentation => "notification-presentation",
    NotificationSnapshotPreparation => "notification-snapshot-preparation",
    NotificationBadgePresentation => "notification-badge-presentation",
    NotificationPanelPresentation => "notification-panel-presentation",
    NotificationToastPresentation => "notification-toast-presentation",
    ScrollbarPresentation => "scrollbar-presentation",
    FioteStatusPresentation => "fiote-status-presentation",
    FioteStatusComparison => "fiote-status-comparison",
    MessageCommandPresentation => "message-command-presentation",
    MessageCommandSelection => "message-command-selection",
    MessageCommandCatalogPreparation => "message-command-catalog-preparation",
    OperationUsagePresentation => "operation-usage-presentation",
    ShaderRenderOrder => "shader-render-order",
    ShaderDefinitionPreparation => "shader-definition-preparation",
    ShaderCandidateReadiness => "shader-candidate-readiness",
    ShaderFrameReadings => "shader-frame-readings",
    ShaderFrameInputPreparation => "shader-frame-input-preparation",
    ShaderPipelineRequests => "shader-pipeline-requests",
    ShaderPipelinePreparation => "shader-pipeline-preparation",
    ShaderSpatialDrawPublication => "shader-spatial-draw-publication",
    ShaderSpatialPlacementPublication => "shader-spatial-placement-publication",
    ShaderSpatialPipelineRequests => "shader-spatial-pipeline-requests",
    ShaderSpatialPipelinePreparation => "shader-spatial-pipeline-preparation",
    ShaderGeometryRequests => "shader-geometry-requests",
    ShaderGeometryPreparation => "shader-geometry-preparation",
    ShaderGeometryDispatch => "shader-geometry-dispatch",
    ShaderLightingPublication => "shader-lighting-publication",
    ShaderVolumePrepassMaintenance => "shader-volume-prepass-maintenance",
    ShaderInlineUpdate => "shader-inline-update",
    ShaderUpdatePreparation => "shader-update-preparation",
    ShaderUpdateRetirement => "shader-update-retirement",
    ShaderInstancePublication => "shader-instance-publication",
    ShaderInstanceMaintenance => "shader-instance-maintenance",
    ShaderFloatingPlacement => "shader-floating-placement",
    ShaderSurfacePlacement => "shader-surface-placement",
    ShaderFloatingPresentation => "shader-floating-presentation",
    ShaderSurfacePresentation => "shader-surface-presentation",
    ShaderFloatingStatusPreparation => "shader-floating-status-preparation",
    EffectTextProtectionPreparation => "effect-text-protection-preparation",
    EffectTextProtectionStyle => "effect-text-protection-style",
    ShaderExtentMeasurement => "shader-extent-measurement",
    ShaderExtentRequests => "shader-extent-requests",
    ShaderUpdateInputDispatch => "shader-update-input-dispatch",
    ShaderFeedDemandPreparation => "shader-feed-demand-preparation",
    ShaderFeedMaintenance => "shader-feed-maintenance",
    ShaderFeedCatalogPreparation => "shader-feed-catalog-preparation",
    ShaderFeedCatalogValidation => "shader-feed-catalog-validation",
    ShaderEditorFeedPublication => "shader-editor-feed-publication",
    ShaderPresentationPublication => "shader-presentation-publication",
    ShaderAuthoringCapture => "shader-authoring-capture",
    ShaderAuthoringFieldCapture => "shader-authoring-field-capture",
    ShaderAuthoringPreviewPublication => "shader-authoring-preview-publication",
    ShaderAuthoringStatusPreparation => "shader-authoring-status-preparation",
    ShaderNumericUpdate => "shader-numeric-update",
    ShaderNumericInputPreparation => "shader-numeric-input-preparation",
    ShaderNumericDemandAdmission => "shader-numeric-demand-admission",
    ShaderNumericFeedMaintenance => "shader-numeric-feed-maintenance",
    ShaderNumericValuePublication => "shader-numeric-value-publication",
    NumericProjectionValidation => "numeric-projection-validation",
    AppearanceFieldMaintenance => "appearance-field-maintenance",
    AppearanceFadeMaintenance => "appearance-fade-maintenance",
    AppearancePresentation => "appearance-presentation",
    AppearanceBackgroundExtraction => "appearance-background-extraction",
    AppearanceLighting => "appearance-lighting",
    AppearanceTextureService => "appearance-texture-service",
    AppearanceGeometry => "appearance-geometry",
    AppearanceImageExtraction => "appearance-image-extraction",
    AppearanceOverlayPresentation => "appearance-overlay-presentation",
    AppearanceRoomExtraction => "appearance-room-extraction",
    AppearanceRoomOrdering => "appearance-room-ordering",
    CompositorEffectPresentation => "compositor-effect-presentation",
    ShadowShapePreparation => "shadow-shape-preparation",
    BlurPresentation => "blur-presentation",
    MentionInputPreparation => "mention-input-preparation",
    MentionCandidateSelection => "mention-candidate-selection",
    MentionCatalogPreparation => "mention-catalog-preparation",
    MentionPopupPresentation => "mention-popup-presentation",
    MentionLinkPresentation => "mention-link-presentation",
    MentionLinkGeometryPreparation => "mention-link-geometry-preparation",
    MessageDraftSaveAdmission => "message-draft-save-admission",
    MessageDraftFileWrite => "message-draft-file-write",
    MessageDraftContentValidation => "message-draft-content-validation",
    MessageDraftPresentation => "message-draft-presentation",
    MessageStoredContentPresentation => "message-stored-content-presentation",
    MessageContentActionPreparation => "message-content-action-preparation",
    MessageStepListPresentation => "message-step-list-presentation",
    MessageAgentProgressPresentation => "message-agent-progress-presentation",
    ExtensionFieldCapture => "extension-field-capture",
    ExtensionFeedPreparation => "extension-feed-preparation",
    ExtensionReplyPreparation => "extension-reply-preparation",
    ExtensionViewPresentation => "extension-view-presentation",
    ExtensionInspection => "extension-inspection",
    ExtensionSchemaValidation => "extension-schema-validation",
    ExtensionSchemaPreservation => "extension-schema-preservation",
    ExtensionReceiptPreparation => "extension-receipt-preparation",
    ExtensionValuesPreparation => "extension-values-preparation",
    ExtensionPresetSelection => "extension-preset-selection",
    ExtensionPresetPreparation => "extension-preset-preparation-await",
    ExtensionAssertionOwnership => "extension-assertion-ownership",
    OntologyCatalogReply => "ontology-catalog-reply",
    OntologyCatalogIndex => "ontology-catalog-index",
    OntologyPayloadPreparation => "ontology-payload-preparation",
    OntologyPresentation => "ontology-presentation",
    OntologySubscriptionIdentity => "ontology-subscription-identity",
    OrganSubscriptionMaintenance => "organ-subscription-maintenance",
    VisibilityReplyApplication => "visibility-reply-application",
    LocationReplyApplication => "location-reply-application",
    SavedLocationReplyApplication => "saved-location-reply-application",
    MapLiveReplyApplication => "map-live-reply-application",
    AccessPolicyControls => "access-policy-controls",
    SimulationInputCapture => "simulation-input-capture",
    TransferInputCapture => "transfer-input-capture",
    SoundCrossingSample => "sound-crossing-sample",
    SoundControlCapture => "sound-control-capture",
    RecordingChoicePresentation => "recording-choice-presentation",
    RecorderListPresentation => "recorder-list-presentation",
    RecorderEffectPresentation => "recorder-effect-presentation",
    RecorderControlAction => "recorder-control-action",
    RecorderInputCapture => "recorder-input-capture",
    RecorderLibraryRefresh => "recorder-library-refresh",
    InfluenceInspectionTracking => "influence-inspection-tracking",
    InfluencePanelUpdate => "influence-panel-update",
    InfluenceDescriptionPresentation => "influence-description-presentation",
    InfluenceControlPresentation => "influence-control-presentation",
    AreaBlockerPreparation => "area-blocker-preparation",
    AreaMutationSnapshot => "area-mutation-snapshot",
    AreaMutationMaintenance => "area-mutation-maintenance",
    AreaMutationReplyProcessing => "area-mutation-reply-processing",
    AreaTransitionStateReadAwait => "area-transition-state-read-await",
    AccessAuthorityReadAwait => "access-authority-read-await",
    AccessUnrestrictedGrantPreparation => "access-unrestricted-grant-preparation",
    AccessGraphProjection => "access-graph-projection",
    RecordPolicyCeilingPreparation => "record-policy-ceiling-preparation",
    RoleAuthorityPolicyReadAwait => "role-authority-policy-read-await",
    AuthorityGraphPreparation => "authority-graph-preparation",
    AuthorityEvaluationPreparation => "authority-evaluation-preparation",
    AuthorityPreparedRead => "authority-prepared-read",
    AuthorityReadPolicyPreparation => "authority-read-policy-preparation",
    RecordPolicyRoleRead => "record-policy-role-read",
    AuthoritySelectorMembership => "authority-selector-membership",
    EditWorkspaceNameCapture => "edit-workspace-name-capture",
    EditWorkspacePresentation => "edit-workspace-presentation",
    EditPanelLayout => "edit-panel-layout",
    EditStoreFilter => "edit-store-filter",
    PointerHitQuery => "pointer-hit-query",
    ManipulationHitPreparation => "manipulation-hit-preparation",
    StorePreviewRefresh => "store-preview-refresh",
    StoreEntryPreparation => "store-entry-preparation",
    ProteinPlacementPreparation => "protein-placement-preparation",
    ProteinDraftStoragePreparation => "protein-draft-storage-preparation",
    ProteinDraftDecoding => "protein-draft-decoding",
    ProteinDraftValidation => "protein-draft-validation",
    ProteinResultPreparation => "protein-result-preparation",
    ProteinResultPresentation => "protein-result-presentation",
    ProteinLibraryPreparation => "protein-library-preparation",
    ProteinEditorPresentation => "protein-editor-presentation",
    TransferAudienceCapture => "transfer-audience-capture",
    TransferSnapshotPreparation => "transfer-snapshot-preparation",
    TransferFormPreparation => "transfer-form-preparation",
    VisibilityPolicyPreparation => "visibility-policy-preparation",
    SimulationScenarioParse => "simulation-scenario-parse",
    SimulationBaseStamp => "simulation-base-stamp",
    RecordRevisionObservation => "record-revision-observation-await",
    OptimizationPoll => "optimization-poll-await",
    OptimizationRunTransitionPreparation => "optimization-run-transition-preparation",
    DemeterInputCapture => "demeter-input-capture",
    DemeterCatalogIndex => "demeter-catalog-index",
    DemeterReplyApplication => "demeter-reply-application",
    DemeterViewPresentation => "demeter-view-presentation",
    DemeterWorkspaceCapture => "demeter-workspace-capture",
    TrustInputCapture => "trust-input-capture",
    TrustReplyApplication => "trust-reply-application",
    TrustPayloadPreparation => "trust-payload-preparation",
    TrustJsonPresentation => "trust-json-presentation",
    TrustViewPresentation => "trust-view-presentation",
    TrustClaimCachePreparation => "trust-claim-cache-preparation",
    SyncCatchupIntervalRead => "sync-catchup-interval-read-await",
    ThreadPageReconciliation => "thread-page-reconciliation",
    RecordOpeningReplies => "record-opening-replies",
    ThreadScrollAnchoring => "thread-scroll-anchoring",
    ThreadLoadErrors => "thread-load-errors",
    ThreadEditPreview => "thread-edit-preview",
    MessageTranscriptPresentation => "message-transcript-presentation",
    TranscriptSubscriptionMaintenance => "transcript-subscription-maintenance",
    MessageContentDescription => "message-content-description",
    MessageAttachmentChunks => "store-message-attachment-chunks-await",
    MessageAttachmentRestoration => "message-attachment-restoration",
    MessageContentRestoration => "message-content-restoration",
    FioteAvatarPreparation => "fiote-avatar-preparation",
    FioteAvatarResidency => "fiote-avatar-residency",
    FioteAvatarSceneSelection => "fiote-avatar-scene-selection",
    FioteAvatarPortraitPreparation => "fiote-avatar-portrait-preparation",
    FioteAvatarAnimation => "fiote-avatar-animation",
    FioteAvatarMotionEffects => "fiote-avatar-motion-effects",
    RecordWorkProjectionRead => "record-work-projection-read-await",
    RecordWorkProjectionWrite => "record-work-projection-write-await",
    RecordWorkLogsRead => "record-work-logs-read-await",
    WorkspaceSaveAdmission => "workspace-save-admission-await",
    WorkspaceSaveFlush => "workspace-save-flush-await",
    WorkspaceStorageSave => "workspace-storage-save",
    WorkspaceStorageValidation => "workspace-storage-validation",
    WorkspaceStorageEncoding => "workspace-storage-encoding",
    WorkspaceStorageCommit => "workspace-storage-commit",
    WorkspaceStoragePrune => "workspace-storage-prune",
    NativeCanvasAuthoringMaintenance => "native-canvas-authoring-maintenance",
    NativeCanvasAuthoringIndex => "native-canvas-authoring-index",
    NativeCanvasAuthoringReconciliation => "native-canvas-authoring-reconciliation",
    NativeCanvasAuthoringRoutePreparation => "native-canvas-authoring-route-preparation",
    NativeCanvasAuthoringEvent => "native-canvas-authoring-event",
    NativeCanvasClipping => "native-canvas-clipping",
    NativeTextClipping => "native-text-clipping",
    NativeWorkspaceRecordPresentation => "native-workspace-record-presentation",
    NativeCanvasSelectionMaintenance => "native-canvas-selection-maintenance",
    NativeCanvasSelectionDrawing => "native-canvas-selection-drawing",
    NativeCanvasSelectionMenu => "native-canvas-selection-menu",
    NativeCanvasSelectionMarquee => "native-canvas-selection-marquee",
    InspectionInput => "inspection-input",
    InspectionOverlayPreparation => "inspection-overlay-preparation",
    InspectionOverlayApplication => "inspection-overlay-application",
    NativeToolbarMaintenance => "native-toolbar-maintenance",
    NativeToolbarCorners => "native-toolbar-corners",
    ShortcutSettingsPreparation => "shortcut-settings-preparation",
    ShortcutKeyMatching => "shortcut-key-matching",
    ShortcutPointerMatching => "shortcut-pointer-matching",
    ShortcutBindingSnapshot => "shortcut-binding-snapshot",
    KeyboardPreparation => "keyboard-preparation",
    KeyboardHostPreparation => "keyboard-host-preparation",
    KeyboardPointerPreparation => "keyboard-pointer-preparation",
    KeyboardCapture => "keyboard-capture",
    KeyboardDispatch => "keyboard-dispatch",
    NativeManipulationInput => "native-manipulation-input",
    NativeManipulationBatch => "native-manipulation-batch",
    NativeManipulationRewrite => "native-manipulation-rewrite",
    ShortcutPointerCapture => "shortcut-pointer-capture",
    ShortcutConflictPreparation => "shortcut-conflict-preparation",
    ShortcutEditorPreparation => "shortcut-editor-preparation",
    ShortcutEditorPresentation => "shortcut-editor-presentation",
    ConfigurationReviewSnapshot => "configuration-review-snapshot",
    ConfigurationReviewPresentation => "configuration-review-presentation",
    ConfigurationReviewSource => "configuration-review-source",
    SettingsFieldPreparation => "settings-field-preparation",
    OverviewSettingsPresentation => "overview-settings-presentation",
    GestureSettingsPresentation => "gesture-settings-presentation",
    WidthSettingsPresentation => "width-settings-presentation",
    KeyboardSettingsPresentation => "keyboard-settings-presentation",
    FocusSettingsPresentation => "focus-settings-presentation",
    OpeningSettingsPresentation => "opening-settings-presentation",
    OpeningEditorPresentation => "opening-editor-presentation",
    OpeningRuleListPreparation => "opening-rule-list-preparation",
    OpeningRulePagePresentation => "opening-rule-page-presentation",
    OpeningWindowPresentation => "opening-window-presentation",
    OpeningEditorValidity => "opening-editor-validity",
    OpeningEditorDraft => "opening-editor-draft",
    OpeningEditorContent => "opening-editor-content",
    OpeningReferenceSelection => "opening-reference-selection",
    ExternalSettingsPresentation => "external-settings-presentation",
    ExternalSettingsDraft => "external-settings-draft",
    AbsoluteSettingsPresentation => "absolute-settings-presentation",
    DesktopGeometryPublish => "desktop-geometry-publish",
    DesktopLayoutResolve => "desktop-layout-resolve",
    DesktopInfluenceStamp => "desktop-influence-stamp",
    DesktopLayoutPreferences => "desktop-layout-preferences",
    DesktopTransientInheritance => "desktop-transient-inheritance",
    DesktopModelValidation => "desktop-model-validation",
    DesktopRowValidation => "desktop-row-validation",
    DesktopFocusTargetLookup => "desktop-focus-target-lookup",
    DesktopFocusResolve => "desktop-focus-resolve",
    DesktopFocusRestoration => "desktop-focus-restoration",
    DesktopTransientPlacement => "desktop-transient-placement",
    DesktopInfluenceOrder => "desktop-influence-order",
    DesktopHiddenMembership => "desktop-hidden-membership",
    DesktopRowSolve => "desktop-row-solve",
    DesktopMaximizedPlacement => "desktop-maximized-placement",
    DesktopManipulationPreview => "desktop-manipulation-preview",
    DesktopManipulationCommit => "desktop-manipulation-commit",
    DesktopManipulationProposal => "desktop-manipulation-proposal",
    DesktopManipulationEntities => "desktop-manipulation-entities",
    DesktopManipulationBaseline => "desktop-manipulation-baseline",
    DesktopManipulationComparison => "desktop-manipulation-comparison",
    DesktopManipulationPreviewMembership => "desktop-manipulation-preview-membership",
    DesktopControlMaintenance => "desktop-control-maintenance",
    DesktopControlPresentation => "desktop-control-presentation",
    AutomationStatusPresentation => "automation-status-presentation",
    OpeningRequestValidation => "opening-request-validation",
    AccessCatalogSnapshot => "access-catalog-snapshot",
    AccessCatalogIndex => "access-catalog-index",
    AccessCatalogList => "access-catalog-list",
    AccessRolePermissions => "access-role-permissions",
    AccessRoleChoices => "access-role-choices",
    NativeWorkspaceSnapshotDecoding => "native-workspace-snapshot-decoding",
    NativeWorkspaceReplicaPreparation => "native-workspace-replica-preparation",
    NativeWorkspaceReplicaApplication => "native-workspace-replica-application",
    NativeWorkspaceReplicaLabels => "native-workspace-replica-labels",
    NativeWorkspaceMaintenance => "native-workspace-maintenance",
    WorkspaceLayoutValidation => "workspace-layout-validation",
    WorkspaceReferencePreparation => "workspace-reference-preparation",
    WorkspaceControlVerification => "workspace-control-verification-await",
    JsonSizePreparation => "json-size-preparation",
    JsonHashPreparation => "json-hash-preparation",
    CanvasMutation => "canvas-mutation",
    CanvasCommandPreparation => "canvas-command-preparation-await",
    ExecutionScheduleAdmission => "execution-schedule-admission",
    ExecutionScheduleDrain => "execution-schedule-drain",
    NativeAccessibleInputs => "native-accessible-input-values",
    NativeAccessibleTextProjection => "native-accessible-text-projection",
    NativeAccessiblePublicationDiff => "native-accessible-publication-diff",
    NativeAccessibleTreeBuild => "native-accessible-tree-build",
    NativeAccessibleComposition => "native-accessible-output-composition",
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
    RecordProjectionPrepare => "record-projection-prepare",
    RecordProjectionMessages => "record-projection-messages",
    RecordProjectionRouting => "record-projection-routing",
    RecordProjectionRoutingPrepare => "record-projection-routing-prepare",
    RecordProjectionEditor => "record-projection-editor",
    RecordDateOrder => "record-date-order",
    RecordGroupLayout => "record-group-layout",
    RecordGroupPrepare => "record-group-layout-prepare",
    RecordGroupApply => "record-group-layout-apply",
    RecordRowLayout => "record-row-layout",
    RecordRowPrepare => "record-row-layout-prepare",
    PropertyFormCommit => "property-form-commit",
    PropertyFormCommand => "property-form-command",
    PropertyFormReply => "property-form-reply",
    RecordStatusPresentation => "record-status-presentation",
    RecordLoginSync => "record-login-sync",
    RecordSectionsUpdate => "record-sections-update",
    RecordSectionContents => "record-section-contents",
    RecordSectionArrange => "record-section-arrange",
    RecordToolbarActivity => "record-toolbar-activity",
    AssertionEditorReceive => "assertion-editor-receive",
    AssertionEditorUpdate => "assertion-editor-update",
    AssertionEditorPrepare => "assertion-editor-prepare",
    AssertionEditorKeys => "assertion-editor-keys",
    AssertionChipPresentation => "assertion-chip-presentation",
    AssertionChipDraw => "assertion-chip-draw",
    RecordCreationFit => "record-creation-fit",
    RecordCreationReceive => "record-creation-receive",
    RecordCreationSearchUpdate => "record-creation-search-update",
    RecordCreationFilterPreparation => "record-creation-filter-preparation",
    RecordCreationReferencePreparation => "record-creation-reference-preparation",
    RecordCreationSearchMatching => "record-creation-search-matching",
    RecordCreationSearchDraw => "record-creation-search-draw",
    RecordCreationSearchSelection => "record-creation-search-selection",
    RecordDraftSelectorEvaluation => "record-draft-selector-evaluation",
    RecordDraftRoleProperties => "record-draft-role-properties",
    RelationLinkReconciliation => "relation-link-reconciliation",
    RelationStyleApplication => "relation-style-application",
    RelationStyleRulePreparation => "relation-style-rule-preparation",
    ArrowPresentation => "arrow-presentation",
    ArrowAttachmentPreparation => "arrow-attachment-preparation",
    ArrowAttachmentSolve => "arrow-attachment-solve",
    AssigneePickerPresentation => "assignee-picker-presentation",
    AssigneePickerUpdate => "assignee-picker-update",
    AssigneePickerReceive => "assignee-picker-receive",
    AssigneePickerPrepare => "assignee-picker-prepare",
    AssigneePickerFilter => "assignee-picker-filter",
    AssigneePickerDraw => "assignee-picker-draw",
    AssertionCompletion => "assertion-completion",
    RecordProjectionDispatch => "record-projection-dispatch",
    NativeRecordPresentation => "native-record-presentation",
    RecordPresentationAvailability => "record-presentation-availability",
    RecordPresentationProbe => "record-presentation-probe",
    RecordPresentationInputs => "record-presentation-inputs",
    RecordHistoryCapture => "record-history-capture",
    RecordTextHistory => "record-text-history",
    RecordBindingUpdate => "record-binding-update",
    RecordBindingPreparation => "record-binding-preparation",
    RecordBindingGrouping => "record-binding-grouping",
    RecordBindingGate => "record-binding-gate",
    RecordBindingIncoming => "record-binding-incoming",
    RecordBindingIncomingSelect => "record-binding-incoming-select",
    RemoteCursorPaint => "remote-cursor-paint",
    RemoteCursorPrepare => "remote-cursor-prepare",
    RemoteCursorBytes => "remote-cursor-bytes",
    SaveFeedback => "save-feedback",
    SaveFeedbackPrepare => "save-feedback-prepare",
    SaveFeedbackPublish => "save-feedback-publish",
    SaveFeedbackStyle => "save-feedback-style",
    LegacyRecordSavePreparation => "legacy-record-save-preparation",
    RecordQueryProjection => "record-query-projection",
    RecordAvailabilityTarget => "record-availability-target",
    RecordAvailability => "record-availability",
    RecordAvailabilityPreparation => "record-availability-preparation",
    RecordAvailabilityVisibility => "record-availability-visibility",
    AreaFilterMaintenance => "area-filter-maintenance",
    AreaFilterPublication => "area-filter-publication",
    ItemPropertyRefresh => "item-property-refresh",
    ItemPropertyMetadata => "item-property-metadata",
    ItemPropertyFeedback => "item-property-feedback",
    WindowMetadataHeaders => "window-metadata-headers",
    AttachmentSynchronize => "attachment-synchronize",
    AttachmentPreparation => "attachment-preparation",
    AttachmentSubscriptions => "attachment-subscriptions",
    AttachmentMessages => "attachment-messages",
    AttachmentProperties => "attachment-properties",
    RecordEditQueued => "record-edit-queued",
    RecordEditDraftQueued => "record-edit-draft-queued",
    RecordEditDraftStarted => "record-edit-draft-started",
    RecordEditDraftDurable => "record-edit-draft-durable",
    RecordEditDraftObserved => "record-edit-draft-observed",
    RecordEditDispatched => "record-edit-dispatched",
    RecordEditServiceStarted => "record-edit-service-started",
    RecordEditAccessGranted => "record-edit-access-granted",
    RecordEditTransactionQueued => "record-edit-transaction-queued",
    RecordEditTransactionGranted => "record-edit-transaction-granted",
    RecordEditCommitStarted => "record-edit-commit-started",
    RecordEditCommitted => "record-edit-committed",
    RecordEditServiceFinished => "record-edit-service-finished",
    ActionResponseQueued => "action-response-queued",
    ActionResponseSent => "action-response-sent",
    RecordEditAckRouted => "record-edit-ack-routed",
    RecordEditAckObserved => "record-edit-ack-observed",
    EditProbeSetText => "edit-probe-set-text",
    EditProbeMirror => "edit-probe-mirror-observed-await",
    EditProbeDurable => "edit-probe-durable-recovery-observed-await",
    EditProbeAcknowledged => "edit-probe-acknowledgement-observed-await",
    EditProbeSettled => "edit-probe-refresh-settled-observed-await",
    EditProbeReopen => "edit-probe-cold-reopen-await",
    RecordEditSplice => "record-edit-splice",
    RecordSelectionCapture => "record-selection-capture",
    RecordSelectionRestore => "record-selection-restore",
    RecordEditAdmission => "record-edit-admission",
    RecordDraftPrepare => "record-draft-prepare",
    RecordDraftQueueWait => "record-draft-queue-wait",
    RecordDraftWrite => "record-draft-write-await",
    RecordRecoverySavePrepare => "record-recovery-save-prepare",
    RecordRecoverySaveTransaction => "record-recovery-save-transaction-await",
    RecordRecoveryMetadataRead => "record-recovery-metadata-read-await",
    RecordRecoveryCompactionRead => "record-recovery-compaction-read-await",
    RecordRecoveryReconstruction => "record-recovery-reconstruction",
    RecordRecoveryCompaction => "record-recovery-compaction",
    RecordDocMetadataRead => "record-doc-metadata-read",
    RecordDocSnapshotRead => "record-doc-snapshot-read",
    RecordDocRetirement => "record-doc-retirement",
    PersonExtensionRead => "person-extension-read",
    PrivateContactLoginRead => "private-contact-login-read",
    PrivateContactBindingRead => "private-contact-binding-read",
    SessionGrantedLoginRead => "session-granted-login-read",
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
    RecordTextRead => "record-text-read-await",
    RecordHeadRead => "record-head-read-await",
    RecordLiveSlugsRead => "record-live-slugs-read-await",
    RecordBodyRead => "record-body-read-await",
    AssertionObjectExists => "assertion-object-exists-await",
    AssertionSubjectExists => "assertion-subject-exists-await",
    AssertionRelatedIdentifiers => "assertion-related-identifiers-await",
    AssertionRelatedKinds => "assertion-related-kinds-await",
    AssertionRelatedHeads => "assertion-related-heads-await",
    TransferExists => "transfer-exists-await",
    ComponentReferenceResolution => "component-reference-resolution-await",
    ComponentAuthorization => "component-authorization-await",
    RecordDeltaExport => "record-canonical-delta-export",
    RecordSignerWait => "record-signer-wait",
    RecordTransactionBegin => "record-canonical-begin-await",
    RecordTransactionBody => "record-canonical-transaction-body-await",
    RecordTransactionCommit => "record-canonical-commit-await",
    RecordObserve => "record-observe-state-await",
    RecordReact => "record-reactions-await",
    RecordReactionMembership => "record-reaction-membership-await",
    RecordValueDependencies => "record-value-dependencies",
    RecordValueDependencyGraph => "record-value-dependency-graph",
    RuleConditionPreparation => "rule-condition-preparation",
    KarmaRuleRowDecode => "karma-rule-row-decode",
    KarmaRuleRevisionInsert => "karma-rule-revision-insert-await",
    KarmaFieldRuleSave => "karma-field-rule-save-await",
    KarmaConditionParse => "karma-condition-parse",
    KarmaConditionBind => "karma-condition-bind",
    CellQuery => "cell-query-execute-await",
    CellQueryBatch => "cell-query-batch-await",
    CellQueryInvalidation => "cell-query-invalidation",
    CellQueryResultComparison => "cell-query-result-comparison",
    CellQuerySignerContext => "cell-query-signer-context-await",
    ProteinConjunctionPreparation => "protein-conjunction-preparation",
    PresencePreparation => "presence-preparation",
    PresenceSelection => "presence-selection",
    CellCursorRefresh => "cell-cursor-refresh-await",
    PresenceWirePreparation => "presence-wire-preparation-await",
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
    ScheduleProjectionCalculation => "schedule-projection-calculation-await",
    ScheduleProjectionSnapshot => "schedule-projection-snapshot-await",
    StoreSnapshotCopyWorker => "store-snapshot-copy-worker-await",
    ScheduleProjectionOpenSnapshot => "schedule-projection-open-snapshot-await",
    ScheduleProjectionRuntimeSetup => "schedule-projection-runtime-setup-await",
    ScheduleProjectionPrepare => "schedule-projection-prepare-await",
    ScheduleProjectionSimulationStep => "schedule-projection-simulation-step-await",
    ScheduleProjectionCloseSnapshot => "schedule-projection-close-snapshot-await",
    ProteinRecordSelection => "protein-record-selection-await",
    ProteinRecordPredicatePrepare => "protein-record-predicate-prepare-await",
    ProteinRecordPredicateMatching => "protein-record-predicate-matching",
    ProteinRecordOrder => "protein-record-order-await",
    ProteinRecordOrderKeys => "protein-record-order-keys",
    ProteinRecordOrderIndices => "protein-record-order-indices",
    ProteinRecordOrderApply => "protein-record-order-apply",
    ProteinRecordLinkOrder => "protein-record-link-order",
    ProteinRecordMaterialization => "protein-record-materialization-await",
    ProteinRecordFieldPublish => "protein-record-field-publish",
    ProteinRecordAggregate => "protein-record-aggregate",
    RecordGroupingRead => "record-grouping-read-await",
    RecordNamedLevelsRead => "record-named-levels-read-await",
    PromiseProjectionRead => "promise-projection-read-await",
    ImaginationSnapshot => "imagination-snapshot-await",
    ImaginationDemand => "imagination-demand-await",
    ImaginationCrossings => "imagination-crossings-await",
    ImaginationProject => "imagination-project",
    ImaginationRuleIndex => "imagination-rule-index",
    ImaginationCrossingIndex => "imagination-crossing-index",
    ProjectionCacheRead => "projection-cache-read-await",
    FactLedgerRead => "fact-ledger-read-await",
    ProteinFactDimensions => "protein-fact-dimensions",
    ProteinFactAggregate => "protein-fact-aggregate",
    ProteinTimelinePublication => "protein-timeline-publication",
    ProteinPromiseSelection => "protein-promise-selection",
    ProteinCalendarOccurrenceIndex => "protein-calendar-occurrence-index",
    ProteinCalendarWorkLinksRead => "protein-calendar-work-links-read-await",
    ProteinCalendarProjection => "protein-calendar-projection",
    ProteinScheduleManualPublication => "protein-schedule-manual-publication",
    ProteinScheduleRecordIndex => "protein-schedule-record-index",
    ProteinScheduleAdmittedPublication => "protein-schedule-admitted-publication",
    ProteinScheduleProjectedPublication => "protein-schedule-projected-publication",
    ProteinScheduleLinkCatalog => "protein-schedule-link-catalog",
    ProteinScheduleMaterializedIndex => "protein-schedule-materialized-index",
    ProteinSchedulePreviewPrepare => "protein-schedule-preview-prepare",
    ProjectionWindowDatePreparation => "projection-window-date-preparation",
    ScheduleEndpointPreparation => "schedule-endpoint-preparation",
    EngineScheduleOccurrencePublication => "engine-schedule-occurrence-publication",
    CalendarMonthRecords => "calendar-month-records",
    CalendarPresentation => "calendar-presentation",
    CalendarMonthPublication => "calendar-month-publication",
    CalendarProjectionReceive => "calendar-projection-receive",
    ProteinTransferRecordIndex => "protein-transfer-record-index",
    ProteinTransferRecordsRead => "protein-transfer-records-read-await",
    ProteinTransferFilterPrepare => "protein-transfer-filter-prepare-await",
    ProteinTransferFilterProject => "protein-transfer-filter-project",
    ProteinTransferFilterMatch => "protein-transfer-filter-match",
    ProteinTransferProjectionIndex => "protein-transfer-projection-index",
    ProteinTransferBlockerGroups => "protein-transfer-blocker-groups",
    ProteinTransferLoanProjection => "protein-transfer-loan-projection-await",
    ProteinTransferRevisionEvidence => "protein-transfer-revision-evidence-await",
    ProteinTransferTimeline => "protein-transfer-timeline-await",
    ProteinTransferProof => "protein-transfer-proof-await",
    ProteinTransferRoles => "protein-transfer-roles",
    ProteinTransferRolePeople => "protein-transfer-role-people",
    TransferAgreementPathIndex => "transfer-agreement-path-index",
    TransferAgreementDirectReadiness => "transfer-agreement-direct-readiness",
    TransferAgreementDependencyIndex => "transfer-agreement-dependency-index",
    TransferAgreementEvaluate => "transfer-agreement-evaluate-await",
    ProteinTransferAgreementOptions => "protein-transfer-agreement-options",
    ProteinRecordSchema => "protein-record-schema-await",
    ProteinRecordWorkPublish => "protein-record-work-publish",
    ProteinRecordNames => "protein-record-names-await",
    ProteinRecordWork => "protein-record-work-await",
    ProteinRecordIncludes => "protein-record-includes-await",
    ProteinRecordIncludePublish => "protein-record-include-publish",
    StoreContactPresentation => "store-contact-presentation-await",
    StoreConversationPresentation => "store-conversation-presentation-await",
    StoreNearbyPresentation => "store-nearby-presentation-await",
    ProteinNearbyPreparation => "protein-nearby-preparation",
    ProteinNearbyPublish => "protein-nearby-publish",
    StoreOpenDecisionPresentation => "store-open-decision-presentation-await",
    ProteinDecisionPublish => "protein-decision-publish",
    ProteinWorkLogIds => "protein-work-log-ids-await",
    ProteinLinkGraphLoad => "protein-link-graph-load-await",
    ProteinLinkGraphIndex => "protein-link-graph-index",
    ProteinLinkProjection => "protein-link-projection",
    ProteinRelationContext => "protein-relation-context-await",
    ProteinRelationContextIndex => "protein-relation-context-index",
    ProteinRelationContextPublish => "protein-relation-context-publish",
    ProteinAssertionOrder => "protein-assertion-order-await",
    ProteinAssertionOrderPublish => "protein-assertion-order-publish",
    StoreConceptHierarchy => "store-concept-hierarchy-await",
    StoreConceptNames => "store-concept-names-await",
    StoreConceptIdentities => "store-concept-identities-await",
    StoreConceptNameMatch => "store-concept-name-match-await",
    StoreTerminologyPreferences => "store-terminology-preferences-await",
    StoreVocabularyDisplay => "store-vocabulary-display-await",
    StoreVocabularyDisplaySelect => "store-vocabulary-display-select",
    FileSyncExportPrepare => "file-sync-export-prepare-await",
    FileSyncExportIndex => "file-sync-export-index",
    FileSyncLinguaRender => "file-sync-lingua-render",
    FileSyncLinguaRecordPreparation => "file-sync-lingua-record-preparation",
    FileSyncLinguaValidation => "file-sync-lingua-validation",
    FileSyncLinguaProjectionValidation => "file-sync-lingua-projection-validation",
    FileSyncLinguaFormat => "file-sync-lingua-format",
    RecordLiveIdentifiersRead => "record-live-identifiers-read-await",
    FileSyncLinguaInputPreparation => "file-sync-lingua-input-preparation-await",
    FileSyncLinguaInputValidation => "file-sync-lingua-input-validation-await",
    FileSyncLinkValidation => "file-sync-link-validation-await",
    RecordDraftValidation => "record-draft-validation",
    FileSyncLinguaRecordValidation => "file-sync-lingua-record-validation",
    EffectValueDecode => "effect-value-decode",
    EffectPayloadClassification => "effect-payload-classification",
    EffectQueueRead => "effect-queue-read-await",
    EffectQueuePayloadDecode => "effect-queue-payload-decode",
    RecordReferenceKindRead => "record-reference-kind-read-await",
    KarmaConditionRecordResolution => "karma-condition-record-resolution-await",
    KarmaTransferSnapshotRead => "karma-transfer-snapshot-read-await",
    KarmaTransferSnapshotPublication => "karma-transfer-snapshot-publication",
    KarmaTransferEffectPreparation => "karma-transfer-effect-preparation-await",
    TransferAutomationEvidenceValidation => "transfer-automation-evidence-validation",
    ProteinThreadSelection => "protein-thread-selection-await",
    ProteinThreadProjection => "protein-thread-projection-await",
    ProteinThreadParents => "protein-thread-parents-await",
    ProteinThreadReferencePublish => "protein-thread-reference-publish",
    ProteinThreadMessagePublish => "protein-thread-message-publish",
    ProteinThreadPagePublish => "protein-thread-page-publish",
    ProteinThreadExtensionDecode => "protein-thread-extension-decode",
    ProteinThreadOrganName => "protein-thread-organ-name-await",
    OrganLocalHead => "organ-local-head-await",
    OrganOwnRecordPresence => "organ-own-record-presence-await",
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
    SyncPluginCatalogAwait => "sync-plugin-catalog-await",
    SyncPluginCatalogDependencies => "sync-plugin-catalog-dependencies",
    NativeBlobSnapshotPreparation => "native-blob-snapshot-preparation",
    NativeBlobPresentation => "native-blob-presentation",
    NativeSyncLibrary => "native-sync-protein-library",
    NativePluginCatalogPresentation => "native-plugin-catalog-presentation",
    NativeSyncOffersPresentation => "native-sync-offers-presentation",
    NativeRecordSaveFeedback => "native-record-save-feedback",
    NativeExtraction => "native-extraction",
    NativeCaptureSources => "native-capture-source-derivation",
    NativeCaptureNormalization => "native-capture-source-normalization",
    RenderUpdate => "render-update",
    FrameGeometry => "frame-geometry",
    FrameInput => "frame-input",
    NativeInputProjection => "native-input-projection",
    NativePointerCapture => "native-pointer-capture",
    NativePointerFiltering => "native-pointer-filtering",
    NativePointerProjection => "native-pointer-projection",
    NativePointerOwnership => "native-pointer-ownership",
    NativePointerLineage => "native-pointer-lineage",
    NativePointerResult => "native-pointer-result",
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
    StaticTextHidden => "native-static-text-without-display-box",
    StaticTextBytes => "native-static-text-measured-source-bytes",
    StaticTextSpans => "native-static-text-measured-spans",
    StaticTextFailed => "native-static-text-failed-measurements",
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
    NativeOutputGeometryQueryPlans => "native-output-geometry-query-plans",
    NativeOutputGeometryCandidates => "native-output-geometry-candidates",
    NativeOutputGeometryProjections => "native-output-geometry-projections",
    NativeOutputSyncQueryPlans => "native-output-sync-query-plans",
    NativeOutputViewsPrepared => "native-output-views-prepared",
    NativeOutputAreasPrepared => "native-output-areas-prepared",
    NativeOutputSurfacesPrepared => "native-output-surfaces-prepared",
    NativeOutputComponentWrites => "native-output-component-writes",
    NativeOutputFillMaterialWrites => "native-output-fill-material-writes",
    NativeOutputMeshUpdates => "native-output-mesh-updates",
    NativeStackingInputOwners => "native-stacking-input-owners",
    NativeStackingSiblingVisits => "native-stacking-sibling-visits",
    NativeStackingWindowVisits => "native-stacking-window-visits",
    NativeStackingFocusResolutions => "native-stacking-focus-resolutions",
    NativeStackingPreparedOrders => "native-stacking-prepared-orders",
    NativeStackingPreparedAnchors => "native-stacking-prepared-anchors",
    NativeStackingPlaneSorts => "native-stacking-plane-sorts",
    NativeStackingPhaseReorders => "native-stacking-phase-reorders",
    NativeStackingDecorationReorders => "native-stacking-decoration-reorders",
    CanvasBackgroundQueryPlans => "canvas-background-query-plans",
    CanvasBackgroundThemeResolutions => "canvas-background-theme-resolutions",
    CanvasBackgroundMaterialWrites => "canvas-background-material-writes",
    CanvasBackgroundComponentWrites => "canvas-background-component-writes",
    TerrainInputSnapshots => "terrain-input-snapshots",
    TerrainFormulaCompilations => "terrain-formula-compilations",
    TerrainPreviewWorkspaceOverrides => "terrain-preview-workspace-overrides",
    TerrainPreviewFormulaReuses => "terrain-preview-formula-reuses",
    TerrainGroundStylePreparations => "terrain-ground-style-preparations",
    TerrainGroundWorkspaceStyles => "terrain-ground-workspace-styles",
    TerrainGroundMaterialWrites => "terrain-ground-material-writes",
    TerrainGroundComponentWrites => "terrain-ground-component-writes",
    TerrainGroundVisualBuilds => "terrain-ground-visual-builds",
    NativeHoleBackgroundLayers => "native-hole-background-layers",
    NativeHoleBackgroundMeshWrites => "native-hole-background-mesh-writes",
    NativeHoleBackgroundMaterialWrites => "native-hole-background-material-writes",
    NativeUiRoots => "native-ui-roots-computed",
    NativeUiGeometryNodes => "native-ui-geometry-nodes-visited",
    NativeInputApplied => "native-input-applied",
    NativePointerCandidates => "native-pointer-candidates",
    NativePointerProjections => "native-pointer-projections",
    NativePointerOwners => "native-pointer-owner-reads",
    NativePointerLineages => "native-pointer-lineages",
    NativePointerTargets => "native-pointer-targets-built",
    NativePointerClipped => "native-pointer-clipped-nodes-skipped",
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
    TextMeasureCandidateSquare => "native-text-measure-source-square",
    TextMeasureCandidatePlain => "native-text-measure-source-plain-text",
    TextMeasureCandidateEditable => "native-text-measure-source-editable-text",
    TextMeasureCandidateOperation => "native-text-measure-source-operation",
    TextMeasureCandidateOverview => "native-text-measure-source-overview",
    TextMeasureCandidateTimer => "native-text-measure-source-work-timer",
    TextMeasureCandidateAccess => "native-text-measure-source-access-control",
    TextMeasureCandidateVisibility => "native-text-measure-source-visibility",
    TextMeasureCandidateTrust => "native-text-measure-source-trust",
    TextMeasureCandidateCamera => "native-text-measure-source-hand-camera",
    TextMeasureCandidateVideo => "native-text-measure-source-video",
    TextMeasureCandidateSync => "native-text-measure-source-sync",
    TextMeasureCandidateTerminal => "native-text-measure-source-terminal",
    TextMeasureCandidateConfiguration => "native-text-measure-source-configuration",
    TextMeasureCandidateOrgan => "native-text-measure-source-organ",
    TextMeasureCandidateTodo => "native-text-measure-source-todo",
    TextMeasureCandidateOntology => "native-text-measure-source-ontology",
    TextMeasureCandidateOther => "native-text-measure-source-other-or-unresolved",
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
    AccessibleProjectionPrepared => "accessible-projections-prepared",
    AccessibleTextClusters => "accessible-text-clusters-visited",
    AccessiblePublicationCandidates => "accessible-publication-candidate-nodes",
    AccessiblePublicationNodes => "accessible-publication-emitted-nodes",
    WindowMetadataChange => "window-metadata-changes",
    ScenePublished => "native-scenes-published",
    SceneReceived => "native-scenes-received",
    InputAccepted => "native-input-messages-accepted",
    ScrollCombined => "native-scroll-messages-combined",
    PointerPublished => "native-pointer-messages-published",
    CanvasExplicitWake => "native-explicit-wake-requests",
    NativeWake => "native-interface-wake-requests",
    NativeInlineWake => "native-worker-issued-interface-wakes",
    NativeWakeCoalesced => "native-wake-already-pending",
    SceneWakeCoalesced => "scene-wake-already-pending",
    NativeUpdateInitial => "native-full-update-initial",
    NativeUpdateWake => "native-full-update-with-wake",
    NativeUpdateScene => "native-full-update-with-scene",
    NativeUpdateInput => "native-full-update-with-input",
    NativeUpdateImport => "native-full-update-with-import-readiness",
    NativeUpdateUnavailable => "native-frame-attempt-without-exportable-frame",
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
    NativeAreaSandsPrepared => "native-area-sands-prepared",
    NativeAreaForcesPublished => "native-area-forces-published",
    NativeAreaForcesRemoved => "native-area-forces-removed",
    AreaEditProposals => "area-edit-proposals",
    AreaEditsPublished => "area-edits-published",
    AreaEditorInputsExamined => "area-editor-inputs-examined",
    AreaEditorSlidersExamined => "area-editor-sliders-examined",
    AreaEditorStatusPublished => "area-editor-status-published",
    AreaEditorValuesFormatted => "area-editor-values-formatted",
    ProteinOrderSubscriptionsExamined => "protein-order-subscriptions-examined",
    ProteinOrderIndexesPrepared => "protein-order-indexes-prepared",
    ProteinOrderSnapshotsPrepared => "protein-order-snapshots-prepared",
    RecordDraftSnapshot => "record-draft-snapshots",
    RecordDraftSnapshotBytes => "record-draft-snapshot-bytes",
    RecordDraftEncodedBytes => "record-draft-encoded-bytes",
    RecordDraftDelta => "record-draft-deltas",
    RecordDraftDeltaBytes => "record-draft-delta-bytes",
    RecordEditPendingIndexes => "record-edit-pending-indexes",
    RecordDraftCheckpointBytes => "record-draft-initial-checkpoint-bytes",
    RecordDraftWrites => "record-draft-writes",
    RecordDraftClears => "record-draft-clears",
    RecordRecoveryPendingReused => "record-recovery-pending-reused",
    RecordRecoveryPayloadCompares => "record-recovery-payload-compares",
    RecordRecoveryMetadataUpdates => "record-recovery-metadata-updates",
    RecordRecoveryCompactionInputs => "record-recovery-compaction-inputs",
    RecordDocMetadataMatches => "record-doc-metadata-matches",
    RecordDocSnapshotBytes => "record-doc-snapshot-bytes",
    RecordDocRetiredRows => "record-doc-retired-rows",
    RecordCanonicalChanges => "record-canonical-changes",
    RecordCanonicalBegins => "record-canonical-begins",
    RecordCanonicalCommits => "record-canonical-commits",
    RecordGuardHistoryBytes => "record-guard-history-bytes",
    RecordGuardHistoryValidatedBytes => "record-guard-history-validated-bytes",
    RecordGuardHistoryReusedTextBytes => "record-guard-history-reused-text-bytes",
    RecordGuardBasesReused => "record-guard-bases-reused",
    RecordCanonicalSnapshotBytes => "record-canonical-snapshot-bytes",
    RecordCanonicalDeltaBytes => "record-canonical-delta-bytes",
    RecordCanonicalFieldsReused => "record-canonical-fields-reused",
    RecordCanonicalTextBytesMaterialized => "record-canonical-text-bytes-materialized",
    RecordReactionMembershipQueries => "record-reaction-membership-queries",
    RecordReactionMembershipConceptsRead => "record-reaction-membership-concepts-read",
    RecordValueDependencyRounds => "record-value-dependency-rounds",
    RecordValueDependencyEdges => "record-value-dependency-edges",
    RecordValueDependencyAdditions => "record-value-dependency-additions",
    RuleConditionPreparations => "rule-condition-preparations",
    KarmaRuleRowDecodes => "karma-rule-row-decodes",
    KarmaRuleRevisionInserts => "karma-rule-revision-inserts",
    KarmaRuleIdentityHistoryWrites => "karma-rule-identity-history-writes",
    KarmaEvidenceEncodings => "karma-evidence-encodings",
    KarmaEvidenceEncodedBytes => "karma-evidence-encoded-bytes",
    KarmaHistoryEffectBatches => "karma-history-effect-batches",
    KarmaHistoryConsequencesRequested => "karma-history-consequences-requested",
    KarmaHistoryEffectRows => "karma-history-effect-rows",
    KarmaHistoryApplicationsRead => "karma-history-applications-read",
    ProteinPlacementTargets => "protein-placement-targets",
    ProteinPlacementOrderLookups => "protein-placement-order-lookups",
    KarmaConditionParseAttempts => "karma-condition-parse-attempts",
    KarmaConditionBindAttempts => "karma-condition-bind-attempts",
    KarmaConditionReadingsPrepared => "karma-condition-readings-prepared",
    CellQueries => "cell-queries",
    CellQuerySignerLookups => "cell-query-signer-lookups",
    ProteinConjunctionLeaves => "protein-conjunction-leaves",
    SyncHistorySubjectsEncoded => "sync-history-subjects-encoded",
    SyncActivityHistoryCopies => "sync-activity-history-copies",
    CellRefreshFacts => "cell-refresh-fact-events",
    CellRefreshFactBatches => "cell-refresh-fact-batches",
    CellRefreshGlobal => "cell-refresh-global-events",
    CellRefreshProjection => "cell-refresh-projection-events",
    CellRefreshEphemeral => "cell-refresh-ephemeral-events",
    CellRefreshPresence => "cell-refresh-presence-events",
    ScheduleProjectionSnapshots => "schedule-projection-snapshots",
    ScheduleProjectionManualReads => "schedule-projection-manual-reads",
    StoreSnapshotPages => "store-snapshot-pages",
    ScheduleProjectionPublications => "schedule-projection-publications",
    ScheduleProjectionSuperseded => "schedule-projection-superseded",
    ScheduleProjectionSnapshotSuperseded => "schedule-projection-snapshot-superseded",
    ScheduleProjectionPublicationSuperseded => "schedule-projection-publication-superseded",
    ScheduleProjectionNotifications => "schedule-projection-notifications",
    ScheduleProjectionNotificationsCoalesced => "schedule-projection-notifications-coalesced",
    CellQueryRows => "cell-query-rows",
    CellQueryBatchReuses => "cell-query-batch-reuses",
    CellQueryResultsUnchanged => "cell-query-results-unchanged",
    CellQueryResultComparisons => "cell-query-result-comparisons",
    CellQueryResultComparisonReuses => "cell-query-result-comparison-reuses",
    PresenceEntriesVisited => "presence-entries-visited",
    PresenceEntriesSelected => "presence-entries-selected",
    PresenceEntriesMaterialized => "presence-entries-materialized",
    PresenceWakeups => "presence-wakeups",
    CanvasAuthoringHosts => "native-canvas-authoring-hosts",
    CanvasAuthoringMaintainedParts => "native-canvas-authoring-maintained-parts",
    CanvasAuthoringReconciliations => "native-canvas-authoring-reconciliations",
    CanvasAuthoringRouteRebuilds => "native-canvas-authoring-route-rebuilds",
    CanvasAuthoringSnapshotsCopied => "native-canvas-authoring-snapshots-copied",
    CanvasAuthoringStyleWrites => "native-canvas-authoring-style-writes",
    CanvasClipNodes => "native-canvas-clip-nodes",
    CanvasClipAncestorVisits => "native-canvas-clip-ancestor-visits",
    CanvasClipParentsPrepared => "native-canvas-clip-parents-prepared",
    TextClipNodes => "native-text-clip-nodes",
    TextClipsPrepared => "native-text-clips-prepared",
    WorkspaceRecordLabelsUpdated => "native-workspace-record-labels-updated",
    CanvasSelectionsNormalized => "native-canvas-selections-normalized",
    CanvasSelectionMarks => "native-canvas-selection-marks",
    InspectionStateWrites => "inspection-state-writes",
    InspectionPreparedNodes => "inspection-prepared-nodes",
    InspectionMarks => "inspection-marks",
    InspectionOverlaysCreated => "inspection-overlays-created",
    InspectionOverlaysRetired => "inspection-overlays-retired",
    InspectionOverlaysUpdated => "inspection-overlays-updated",
    InspectionLabelsUpdated => "inspection-labels-updated",
    ToolbarControlsChecked => "native-toolbar-controls-checked",
    ToolbarOrderRepairs => "native-toolbar-order-repairs",
    ToolbarZoomFormats => "native-toolbar-zoom-formats",
    ToolbarDelayWrites => "native-toolbar-delay-writes",
    ToolbarCornersPrepared => "native-toolbar-corners-prepared",
    ToolbarCornersRetired => "native-toolbar-corners-retired",
    ShortcutSettingsPrepared => "shortcut-settings-prepared",
    KeyboardHeldSnapshots => "keyboard-held-snapshots",
    KeyboardLogicalWrites => "keyboard-logical-writes",
    KeyboardEventsPrepared => "keyboard-events-prepared",
    NativeManipulationWindowEvents => "native-manipulation-window-events",
    NativeManipulationButtonEvents => "native-manipulation-button-events",
    NativeManipulationCommands => "native-manipulation-commands",
    NativeManipulationPointerRetained => "native-manipulation-pointer-retained",
    ShortcutPointerEventsPrepared => "shortcut-pointer-events-prepared",
    ShortcutConflictsPrepared => "shortcut-conflicts-prepared",
    ShortcutEditorRowsPrepared => "shortcut-editor-rows-prepared",
    ShortcutEditorControlWrites => "shortcut-editor-control-writes",
    ConfigurationReviewSnapshotsPrepared => "configuration-review-snapshots-prepared",
    ConfigurationReviewValuesPrepared => "configuration-review-values-prepared",
    NativeSettingsTextWrites => "native-settings-text-writes",
    NativeSettingsLabelWrites => "native-settings-label-writes",
    NativeSettingsDisplayWrites => "native-settings-display-writes",
    SettingsFeedbackPrepared => "settings-feedback-prepared",
    OpeningRuleRowsCreated => "opening-rule-rows-created",
    OpeningRuleLabelsPrepared => "opening-rule-labels-prepared",
    OpeningWindowDescriptionsPrepared => "opening-window-descriptions-prepared",
    OpeningEditorFormLabelsPrepared => "opening-editor-form-labels-prepared",
    OpeningEditorContentWalks => "opening-editor-content-walks",
    OpeningEditorControlsDerived => "opening-editor-controls-derived",
    OpeningReferenceKeysChanged => "opening-reference-keys-changed",
    OpeningReferenceSourcesChanged => "opening-reference-sources-changed",
    ExternalSettingsSnapshotsPrepared => "external-settings-snapshots-prepared",
    ExternalSettingsBindingsPrepared => "external-settings-bindings-prepared",
    AbsoluteSettingsSnapshotsPrepared => "absolute-settings-snapshots-prepared",
    AbsoluteSettingsDetailsPrepared => "absolute-settings-details-prepared",
    AbsoluteSettingsLabelsPrepared => "absolute-settings-labels-prepared",
    DesktopGeometryRowsExamined => "desktop-geometry-rows-examined",
    DesktopGeometryRowsWritten => "desktop-geometry-rows-written",
    DesktopResolvedRowsWritten => "desktop-resolved-rows-written",
    DesktopResolvedRowsRemoved => "desktop-resolved-rows-removed",
    DesktopGeometryClipsPrepared => "desktop-geometry-clips-prepared",
    DesktopInfluenceStampsPrepared => "desktop-influence-stamps-prepared",
    DesktopSolverColumnsPrepared => "desktop-solver-columns-prepared",
    DesktopSolverIndexesPrepared => "desktop-solver-indexes-prepared",
    DesktopLayoutGapsWritten => "desktop-layout-gaps-written",
    DesktopRestoredOwnerRoots => "desktop-restored-owner-roots",
    DesktopRestoredChildrenExamined => "desktop-restored-children-examined",
    DesktopRestoredModelItemsExamined => "desktop-restored-model-items-examined",
    DesktopTransientMembershipsWritten => "desktop-transient-memberships-written",
    DesktopRootChildrenSourced => "desktop-root-children-sourced",
    DesktopModelValidations => "desktop-model-validations",
    DesktopFocusTargetChildren => "desktop-focus-target-children",
    DesktopFocusTargetEligibilityChecks => "desktop-focus-target-eligibility-checks",
    DesktopFocusRestoreRoots => "desktop-focus-restore-roots",
    DesktopFocusSavedEntriesExamined => "desktop-focus-saved-entries-examined",
    DesktopFocusSavedEntriesPrepared => "desktop-focus-saved-entries-prepared",
    DesktopFocusOwnerUpdates => "desktop-focus-owner-updates",
    DesktopFocusStatesRestored => "desktop-focus-states-restored",
    DesktopTransientAncestryChecks => "desktop-transient-ancestry-checks",
    DesktopTransientHiddenRowsRemoved => "desktop-transient-hidden-rows-removed",
    DesktopTransientSolutionRowsExamined => "desktop-transient-solution-rows-examined",
    DesktopTransientSolutionLookupIds => "desktop-transient-solution-lookup-ids",
    DesktopTransientPlacementAttempts => "desktop-transient-placement-attempts",
    DesktopTransientItemsPlaced => "desktop-transient-items-placed",
    DesktopOrderEntriesExamined => "desktop-order-entries-examined",
    DesktopOrderKeysPrepared => "desktop-order-keys-prepared",
    DesktopOrderPayloadSwaps => "desktop-order-payload-swaps",
    DesktopHiddenCompactIds => "desktop-hidden-compact-ids",
    DesktopHiddenIdsPrepared => "desktop-hidden-ids-prepared",
    DesktopRowSolveRows => "desktop-row-solve-rows",
    DesktopRowSolveItemsPrepared => "desktop-row-solve-items-prepared",
    DesktopRowSolveCompactIds => "desktop-row-solve-compact-ids",
    DesktopMaximizedCandidates => "desktop-maximized-candidates",
    DesktopMaximizedOverrides => "desktop-maximized-overrides",
    DesktopManipulationEntityMaps => "desktop-manipulation-entity-maps",
    DesktopManipulationBaselineAttempts => "desktop-manipulation-baseline-attempts",
    DesktopManipulationBaselineRowsIndexed => "desktop-manipulation-baseline-rows-indexed",
    DesktopManipulationProposedRowsCompared => "desktop-manipulation-proposed-rows-compared",
    DesktopManipulationPreviewItems => "desktop-manipulation-preview-items",
    DesktopManipulationPreviewRetirements => "desktop-manipulation-preview-retirements",
    DesktopControlOwnersPrepared => "desktop-control-owners-prepared",
    DesktopControlsCreated => "desktop-controls-created",
    DesktopControlLabelConsumers => "desktop-control-label-consumers",
    DesktopControlHelpPreparations => "desktop-control-help-preparations",
    DesktopControlTooltipWrites => "desktop-control-tooltip-writes",
    AutomationStatusConsumers => "automation-status-consumers",
    AutomationStatusMessagesPrepared => "automation-status-messages-prepared",
    SettingsFieldValuesPrepared => "settings-field-values-prepared",
    SettingsCandidatesPrepared => "settings-candidates-prepared",
    SettingsSummariesPrepared => "settings-summaries-prepared",
    WidthSettingsFocusSummariesPrepared => "width-settings-focus-summaries-prepared",
    WidthSettingsRowLabelsPrepared => "width-settings-row-labels-prepared",
    AccessCatalogSnapshotsPrepared => "access-catalog-snapshots-prepared",
    AccessCatalogIndexesPrepared => "access-catalog-indexes-prepared",
    AccessCatalogIndexEntries => "access-catalog-index-entries",
    AccessPermissionRegionsRebuilt => "access-permission-regions-rebuilt",
    AccessRoleChoosersRebuilt => "access-role-choosers-rebuilt",
    WorkspaceSnapshotsDecoded => "native-workspace-snapshots-decoded",
    WorkspaceReplicaItemsPrepared => "native-workspace-replica-items-prepared",
    WorkspaceReplicaGeometryWrites => "native-workspace-replica-geometry-writes",
    WorkspaceReplicaDifferences => "native-workspace-replica-differences",
    WorkspaceReplicaContentRebuilds => "native-workspace-replica-content-rebuilds",
    WorkspaceReplicaLabelsUpdated => "native-workspace-replica-labels-updated",
    WorkspaceDraftsPrepared => "native-workspace-drafts-prepared",
    WorkspaceControlsChecked => "workspace-controls-checked",
    JsonSizeValuesCounted => "json-size-values-counted",
    JsonSizeBytesCounted => "json-size-bytes-counted",
    JsonValuesHashed => "json-values-hashed",
    JsonBytesHashed => "json-bytes-hashed",
    CanvasHistorySizeReuses => "canvas-history-size-reuses",
    CanvasMutationSnapshotsCopied => "canvas-mutation-snapshots-copied",
    AreaMutationSourcesFormatted => "area-mutation-sources-formatted",
    AreaMutationFiltersPrepared => "area-mutation-filters-prepared",
    AreaMutationFilterPairs => "area-mutation-filter-pairs",
    AreaMutationGrantsChecked => "area-mutation-grants-checked",
    AreaMutationActivationCandidates => "area-mutation-activation-candidates",
    AreaMutationRepliesRetained => "area-mutation-replies-retained",
    InfluenceInspectedRootsPublished => "influence-inspected-roots-published",
    InfluenceTrackedSetsPublished => "influence-tracked-sets-published",
    InfluenceStaleReportsRetired => "influence-stale-reports-retired",
    InfluenceTrackedCopies => "influence-tracked-copies",
    InfluenceDescriptionsRetained => "influence-descriptions-retained",
    InfluenceDescriptionsFormatted => "influence-descriptions-formatted",
    InfluenceDescriptionLabelsPublished => "influence-description-labels-published",
    InfluenceControlsRetained => "influence-controls-retained",
    InfluenceControlPartsRetained => "influence-control-parts-retained",
    InfluenceControlPartsBuilt => "influence-control-parts-built",
    AccessRolePermissionReads => "access-role-permission-reads",
    AccessUnrestrictedGrantsBuilt => "access-unrestricted-grants-built",
    AccessRecordContentMoved => "access-record-content-moved",
    RoleAuthorityPoliciesRequested => "role-authority-policies-requested",
    RoleAuthorityRolesRead => "role-authority-roles-read",
    RoleAuthorityPolicyDecodes => "role-authority-policy-decodes",
    AuthorityConceptEdgesIndexed => "authority-concept-edges-indexed",
    AuthorityReadGraphsPrepared => "authority-read-graphs-prepared",
    AuthorityReadPolicyEvaluations => "authority-read-policy-evaluations",
    EditPanelVisibleOwners => "edit-panel-visible-owners",
    EditPanelLayoutWrites => "edit-panel-layout-writes",
    AuthorityTextNeedlesPrepared => "authority-text-needles-prepared",
    EditStoreFiltersPrepared => "edit-store-filters-prepared",
    EditStoreEntriesInspected => "edit-store-entries-inspected",
    EditStoreVisibilityWrites => "edit-store-visibility-writes",
    PointerHitNodesInspected => "pointer-hit-nodes-inspected",
    PointerHitNodesReturned => "pointer-hit-nodes-returned",
    PointerHitCameraMappings => "pointer-hit-camera-mappings",
    ManipulationHitTargets => "manipulation-hit-targets",
    ManipulationHitGeometriesCopied => "manipulation-hit-geometries-copied",
    StoreDimensionsFormatted => "store-dimensions-formatted",
    StoreDimensionsPublished => "store-dimensions-published",
    StoreTextSnapshotsPrepared => "store-text-snapshots-prepared",
    RecordingChoicePartsBuilt => "recording-choice-parts-built",
    RecordingChoicePartsRetained => "recording-choice-parts-retained",
    RecorderSearchEditorsBuilt => "recorder-search-editors-built",
    RecorderSearchEditorsRetained => "recorder-search-editors-retained",
    RecorderEffectControlsBuilt => "recorder-effect-controls-built",
    RecorderEffectControlsRetained => "recorder-effect-controls-retained",
    SyncSnapshotPublished => "sync-ui-snapshots-published",
    SyncSnapshotUnchanged => "sync-ui-snapshots-unchanged",
    SyncSnapshotReadsAdmitted => "sync-ui-snapshot-read-admissions",
    SyncProjectionNoticesSkipped => "sync-projection-only-notices-skipped",
    SyncCatalogRowsCreated => "sync-catalog-rows-created",
    SyncCatalogRowsRetained => "sync-catalog-rows-retained",
    SyncCatalogRowsRebuilt => "sync-catalog-rows-rebuilt",
    SyncOfferRowsCreated => "sync-offer-rows-created",
    SyncOfferRowsRetained => "sync-offer-rows-retained",
    SyncOfferRowsRebuilt => "sync-offer-rows-rebuilt",
    SyncPresentationWatchesStarted => "sync-presentation-watches-started",
    SyncPresentationWatchesRetired => "sync-presentation-watches-retired",
    SyncHistoryRequestsAdmitted => "sync-history-requests-admitted",
    SyncHistoryReplies => "sync-history-replies-received",
    SyncCastleRepliesRouted => "sync-castle-replies-routed",
    SyncProteinChoicesCreated => "sync-protein-choices-created",
    SyncProteinChoicesRetained => "sync-protein-choices-retained",
    RecordViewSaveRepliesRouted => "record-view-save-replies-routed",
    SyncPluginCatalogReads => "sync-plugin-catalog-reads",
    SyncPluginCatalogPackages => "sync-plugin-catalog-packages",
    SyncPluginCatalogDependencies => "sync-plugin-catalog-dependencies",
    BlobTransferModelsReused => "blob-transfer-models-reused",
    BlobTransferRowsCreated => "blob-transfer-rows-created",
    BlobTransferRowsRetained => "blob-transfer-rows-retained",
    BlobTransferRowsUpdated => "blob-transfer-rows-reconciled",
    BlobFileLabelsCreated => "blob-file-labels-created",
    ProteinLinkGraphLoads => "protein-link-graph-loads",
    ProteinLinkEdgesVisited => "protein-link-edges-visited",
    ProteinRelationContextAssertions => "protein-relation-context-assertions",
    ProteinRelationContextFanouts => "protein-relation-context-fanouts",
    ProteinRelationContextFamilyLookups => "protein-relation-context-family-lookups",
    ProteinAssertionOrderPredicates => "protein-assertion-order-predicates",
    StoreConceptHierarchyBatches => "store-concept-hierarchy-batches",
    StoreConceptHierarchyNodes => "store-concept-hierarchy-nodes",
    StoreVocabularyDisplayTermsRead => "store-vocabulary-display-terms-read",
    StoreVocabularyDisplayTermsConsidered => "store-vocabulary-display-terms-considered",
    StoreVocabularyDisplayLabels => "store-vocabulary-display-labels",
    ProteinTransferFilterProjectedPromises => "protein-transfer-filter-projected-promises",
    ProteinTransferFilterWindowParses => "protein-transfer-filter-window-parses",
    ProteinTransferHiddenEvidence => "protein-transfer-hidden-evidence",
    ProteinTransferIndexSourceRows => "protein-transfer-index-source-rows",
    ProteinTransferBlockerReferences => "protein-transfer-blocker-references",
    ProteinTransferLoanReadSkipped => "protein-transfer-loan-read-skipped",
    ProteinTransferRoleResults => "protein-transfer-role-results",
    TransferAgreementPathIndexedPromises => "transfer-agreement-path-indexed-promises",
    TransferAgreementPathLookups => "transfer-agreement-path-lookups",
    TransferAgreementPathCandidates => "transfer-agreement-path-candidates",
    TransferAgreementDependencySourceRows => "transfer-agreement-dependency-source-rows",
    TransferAgreementCacheHits => "transfer-agreement-cache-hits",
    TransferAgreementGraphNodes => "transfer-agreement-graph-nodes",
    TransferAgreementRootCopyFallbacks => "transfer-agreement-root-copy-fallbacks",
    ProteinTransferAgreementOptionChecks => "protein-transfer-agreement-option-checks",
    ProteinThreadSelections => "protein-thread-selections",
    ProteinRecordReferenceLookups => "protein-record-reference-lookups",
    ProteinRecordTextPredicates => "protein-record-text-predicates",
    ProteinRecordMatchHeadFolds => "protein-record-match-head-folds",
    ProteinRecordMatchBodyFolds => "protein-record-match-body-folds",
    ProteinRecordOrderKeys => "protein-record-order-keys",
    ProteinRecordOrderPayloadSwaps => "protein-record-order-payload-swaps",
    ProteinRecordLinkCandidates => "protein-record-link-candidates",
    ProteinRecordLinkDuplicates => "protein-record-link-duplicates",
    ContactPresentationRows => "contact-presentation-rows",
    ContactPresentationQuarantineRows => "contact-presentation-quarantine-rows",
    ConversationPresentationRows => "conversation-presentation-rows",
    NearbyPresentationKnownPeers => "nearby-presentation-known-peers",
    NearbyPresentationSelectedPeers => "nearby-presentation-selected-peers",
    OpenDecisionPresentationRows => "open-decision-presentation-rows",
    ProteinThreadExtensionsDecoded => "protein-thread-extensions-decoded",
    ProteinThreadOrganNameReads => "protein-thread-organ-name-reads",
    ProteinThreadSocialLabelPreparations => "protein-thread-social-label-preparations",
    ProteinRecordOwnedFieldRows => "protein-record-owned-field-rows",
    ProteinRecordAggregateGroups => "protein-record-aggregate-groups",
    RecordRelatedSubjectBatches => "record-related-subject-batches",
    RecordReadBodyBytes => "record-read-body-bytes",
    RecordGroupingRows => "record-grouping-rows",
    RecordGroupingReadSkipped => "record-grouping-read-skipped",
    RecordNamedLevelRows => "record-named-level-rows",
    PromiseProjectionRows => "promise-projection-rows",
    ImaginationEvents => "imagination-events",
    ImaginationTimelinePoints => "imagination-timeline-points",
    ImaginationRuleIndexRows => "imagination-rule-index-rows",
    ImaginationCrossingPoints => "imagination-crossing-points",
    ProjectionSpanReads => "projection-span-reads",
    ProjectionScheduleReads => "projection-schedule-reads",
    ProjectionSpanRows => "projection-span-rows",
    ProjectionScheduleRows => "projection-schedule-rows",
    FactLedgerDetailRows => "fact-ledger-detail-rows",
    FactLedgerAggregateRows => "fact-ledger-aggregate-rows",
    ProteinTimelinePublishedBuckets => "protein-timeline-published-buckets",
    ProteinCalendarOccurrenceLinks => "protein-calendar-occurrence-links",
    ProteinCalendarWorkRows => "protein-calendar-work-rows",
    ProteinCalendarOccurrenceLookups => "protein-calendar-occurrence-lookups",
    ProteinScheduleManualRowCopies => "protein-schedule-manual-row-copies",
    ProteinScheduleRecordIndexRows => "protein-schedule-record-index-rows",
    ProteinScheduleLinkValues => "protein-schedule-link-values",
    ProteinScheduleLinkDecodes => "protein-schedule-link-decodes",
    ProteinScheduleOccurrenceLookups => "protein-schedule-occurrence-lookups",
    ProteinSchedulePreviewNeedles => "protein-schedule-preview-needles",
    ProteinSchedulePreviewHeads => "protein-schedule-preview-heads",
    ProjectionWindowDatePreparations => "projection-window-date-preparations",
    ScheduleEndpointParses => "schedule-endpoint-parses",
    ProteinScheduleEndpointReuses => "protein-schedule-endpoint-reuses",
    EngineScheduleEndpointReuses => "engine-schedule-endpoint-reuses",
    CalendarMonthInputRows => "calendar-month-input-rows",
    CalendarMonthTimezones => "calendar-month-timezones",
    CalendarFramesRetained => "calendar-frames-retained",
    CalendarFramesBuilt => "calendar-frames-built",
    CalendarDaysRetained => "calendar-days-retained",
    CalendarDaysRebuilt => "calendar-days-rebuilt",
    CalendarFootersRebuilt => "calendar-footers-rebuilt",
    CalendarProjectionReplies => "calendar-projection-replies",
    CalendarProjectionUnchangedReplies => "calendar-projection-unchanged-replies",
    CalendarProjectionHiddenRetirements => "calendar-projection-hidden-retirements",
    RecordBindingViews => "record-binding-views-compared",
    RecordBindingBorrowedKeys => "record-binding-borrowed-keys",
    RecordBindingSharedSnapshots => "record-binding-shared-snapshots",
    RecordBindingGroupsCreated => "record-binding-groups-created",
    RecordBindingPreparedGroups => "record-binding-prepared-groups",
    RecordBindingDeniedPastes => "record-binding-denied-pastes",
    RecordBindingDeniedEdits => "record-binding-denied-edits",
    RecordBindingMessagesRouted => "record-binding-messages-routed",
    RecordBindingMessagesSelected => "record-binding-messages-selected",
    RecordBindingReplyDocuments => "record-binding-reply-documents",
    RemoteCursorSnapshotsPrepared => "remote-cursor-snapshots-prepared",
    RemoteCursorDecodes => "remote-cursor-decodes",
    RemoteCursorPositions => "remote-cursor-positions",
    RemoteCursorCharacters => "remote-cursor-characters",
    RemoteCursorCandidates => "remote-cursor-candidates",
    RemoteCursorGeometryWrites => "remote-cursor-geometry-writes",
    RemoteCursorCreated => "remote-cursor-created",
    RemoteCursorRetired => "remote-cursor-retired",
    SaveFeedbackFieldsPrepared => "save-feedback-fields-prepared",
    SaveFeedbackStateWrites => "save-feedback-state-writes",
    SaveFeedbackBorderWrites => "save-feedback-border-writes",
    SaveFeedbackBordersStaged => "save-feedback-borders-staged",
    SaveFeedbackWakeWrites => "save-feedback-wake-writes",
    LegacyRecordSavesPrepared => "legacy-record-saves-prepared",
    LegacyRecordSaveBackpressure => "legacy-record-save-backpressure",
    RecordQueriesProjected => "record-queries-projected",
    RecordAvailabilityPanels => "record-availability-panels",
    RecordAvailabilityMessages => "record-availability-messages-materialized",
    RecordAvailabilityChildren => "record-availability-children-visited",
    RecordAvailabilityChildrenUpdated => "record-availability-children-updated",
    AreaFilterSubscriptionsPrepared => "area-filter-subscriptions-prepared",
    AreaFilterPresenceLookups => "area-filter-presence-lookups",
    AreaFilterRowsInspected => "area-filter-rows-inspected",
    AreaFilterMemberCopies => "area-filter-member-copies",
    AreaFilterMatchWrites => "area-filter-match-writes",
    RecordProjectionRemoteDeliveries => "record-projection-remote-deliveries",
    RecordProjectionLocalDeliveries => "record-projection-local-deliveries",
    RecordProjectionRoutingIds => "record-projection-routing-ids",
    RecordProjectionRoutingLookups => "record-projection-routing-lookups",
    RecordProjectionInitialMirrors => "record-projection-initial-mirrors",
    RecordProjectionEditorPublications => "record-projection-editor-publications",
    RecordDateOrderReuses => "record-date-order-reuses",
    RecordDateKeysPrepared => "record-date-keys-prepared",
    RecordDateDataReorders => "record-date-data-reorders",
    RecordDateOrderIdsCopied => "record-date-order-ids-copied",
    RecordGroupRowsPrepared => "record-group-rows-prepared",
    RecordGroupTargetMaps => "record-group-target-maps-materialized",
    RecordGroupTargetsCopied => "record-group-targets-copied",
    RecordRowsPrepared => "record-rows-prepared",
    PropertyFieldValuesCopied => "property-field-values-copied",
    PropertyFormsRebuilt => "property-forms-rebuilt",
    RecordStatusLabelsInspected => "record-status-labels-inspected",
    RecordStatusWrites => "record-status-writes",
    RecordLoginFormsPrepared => "record-login-forms-prepared",
    RecordLoginFormsCreated => "record-login-forms-created",
    RecordLoginLabelsUpdated => "record-login-labels-updated",
    RecordToolbarsPrepared => "record-toolbars-prepared",
    RecordSectionContentChecks => "record-section-content-checks",
    RecordSectionPropertyNamesCopied => "record-section-property-names-copied",
    RecordSectionNodeWrites => "record-section-node-writes",
    RecordSectionLabelWrites => "record-section-label-writes",
    RecordSectionChildOrdersChanged => "record-section-child-orders-changed",
    RecordToolbarActivityPreparations => "record-toolbar-activity-preparations",
    RecordToolbarAncestorsPrepared => "record-toolbar-ancestors-prepared",
    AssertionReplySubscriptions => "assertion-reply-subscriptions-inspected",
    AssertionReplyRecipients => "assertion-reply-recipients",
    AssertionCompletionInputsCopied => "assertion-completion-inputs-copied",
    AssertionCompletionRefreshes => "assertion-completion-refreshes",
    AssertionCompletionRedraws => "assertion-completion-redraws",
    AssertionSubscriptionPayloads => "assertion-subscription-payloads",
    AssertionMessagesPrepared => "assertion-messages-prepared",
    AssertionFieldsPrepared => "assertion-fields-prepared",
    AssertionChipsPrepared => "assertion-chips-prepared",
    AssertionHoverAncestors => "assertion-hover-ancestors",
    AssertionRedrawValuesShared => "assertion-redraw-values-shared",
    RecordCreationFiltersPrepared => "record-creation-filters-prepared",
    RecordCreationReferenceLabelsPrepared => "record-creation-reference-labels-prepared",
    RecordCreationSearchRowsInspected => "record-creation-search-rows-inspected",
    RecordCreationSearchRowsCreated => "record-creation-search-rows-created",
    RecordCreationFitTargets => "record-creation-fit-targets",
    RecordCreationSizePublications => "record-creation-size-publications",
    RecordCreationActionPayloads => "record-creation-action-payloads",
    RecordCreationSearchPayloads => "record-creation-search-payloads",
    RelationLinksPrepared => "relation-links-prepared",
    RelationStyleRulesPrepared => "relation-style-rules-prepared",
    RelationStyleRecordsIndexed => "relation-style-records-indexed",
    RelationStyleAssertionsIndexed => "relation-style-assertions-indexed",
    ArrowTargetsPrepared => "arrow-targets-prepared",
    ArrowLabelPublications => "arrow-label-publications",
    ArrowAttachmentsReused => "arrow-attachments-reused",
    ArrowAttachmentsSolved => "arrow-attachments-solved",
    AssigneePickerFiltersPrepared => "assignee-picker-filters-prepared",
    AssigneePickerRowsInspected => "assignee-picker-rows-inspected",
    AssigneePickerRowsCreated => "assignee-picker-rows-created",
    AssigneePickerLabelsUpdated => "assignee-picker-labels-updated",
    AssigneePickerMessagesPrepared => "assignee-picker-messages-prepared",
    AssigneePickersPrepared => "assignee-pickers-prepared",
    RecordPresentationRowsCaptured => "record-presentation-rows-captured",
    RecordPresentationFillChanges => "record-presentation-fill-changes",
    RecordPresentationPreviewsPrepared => "record-presentation-previews-prepared",
    RecordPresentationPreviewsUpdated => "record-presentation-previews-updated",
    RecordHistoryCaptures => "record-history-captures",
    RecordHistorySourceReads => "record-history-source-reads",
    RecordHistoryReplayCopies => "record-history-replay-copies",
    RecordTextHistoryCandidates => "record-text-history-candidates",
    RecordTextHistoryCaptures => "record-text-history-captures",
    RecordTextHistoryRestores => "record-text-history-restores",
    RecordProjectionEditorsPrepared => "record-projection-editors-prepared",
    RecordProjectionConfigurationsPrepared => "record-projection-configurations-prepared",
    RecordProjectionMessagesPrepared => "record-projection-messages-prepared",
    RecordProjectionRequestsMoved => "record-projection-requests-moved",
    RecordProjectionRequestDeferrals => "record-projection-request-deferrals",
    ItemPropertyMetadataPrepared => "item-property-metadata-prepared",
    ItemPropertyLabelsPrepared => "item-property-labels-prepared",
    WindowMetadataCaptionsPrepared => "window-metadata-captions-prepared",
    AttachmentEligibleItems => "attachment-eligible-items",
    AttachmentCandidatesPrepared => "attachment-candidates-prepared",
    AttachmentMessagesSelected => "attachment-messages-selected",
    AttachmentSubscriptionsPrepared => "attachment-subscriptions-prepared",
    AttachmentSubscriptionDeferrals => "attachment-subscription-deferrals",
    AttachmentPropertyComparisons => "attachment-property-comparisons",
    AttachmentPropertyCopies => "attachment-property-copies",
    AttachmentPropertiesWritten => "attachment-properties-written",
    AttachmentPropertiesRemoved => "attachment-properties-removed",
    AttachmentBindingsChanged => "attachment-bindings-changed",
    AttachmentRepliesWithoutPropertyOrBindingWrites => "attachment-replies-without-property-or-binding-writes",
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
    DescriptionBlocksRetained => "description-markup-blocks-retained",
    DescriptionBlocksPatched => "description-markup-blocks-patched",
    DescriptionBlocksRebuilt => "description-markup-blocks-rebuilt",
    TerminalRowsRetained => "terminal-rows-retained",
    ExplorerListingSorts => "explorer-listing-sorts",
    ExplorerEntriesSorted => "explorer-entries-sorted",
    ExplorerDirectoryRequests => "explorer-directory-requests-admitted",
    ExplorerDirectoryCancellations => "explorer-directory-requests-cancelled",
    ExplorerRowsDerived => "explorer-rows-derived",
    ExplorerSearchModelsRetained => "explorer-search-models-retained",
    ExplorerButtonsCreated => "explorer-buttons-created",
    ExplorerButtonsRetained => "explorer-buttons-retained",
    MediaFitWrites => "media-image-fit-writes",
    VideoBindingWrites => "video-image-binding-writes",
    VideoFrameImagesUpdated => "video-frame-images-updated",
    VideoFrameImageBytes => "video-frame-image-bytes",
    GifFrameBytesMoved => "gif-frame-bytes-moved",
    TerminalRowsReconciled => "terminal-rows-reconciled",
    TerminalRunsCreated => "terminal-runs-created",
    TerminalRunsRetained => "terminal-runs-retained",
    TerminalFramesPublished => "terminal-frames-published",
    TerminalFramesUnchanged => "terminal-frames-unchanged",
    TerminalInputPrefixCopiedBytes => "terminal-input-prefix-copied-bytes",
    TerminalVtUnchangedResizes => "terminal-vt-unchanged-resizes",
    TerminalVtSnapshotFreeBatches => "terminal-vt-snapshot-free-batches",
    TerminalReplyRecipients => "terminal-reply-recipients",
    TransferListAdmittedRows => "transfer-list-admitted-rows",
    TransferTreeIndexEntries => "transfer-tree-index-entries",
    TransferPageCardsPrepared => "transfer-page-cards-prepared",
    TransferRegionsRetained => "transfer-regions-retained",
    TransferRegionsRebuilt => "transfer-regions-rebuilt",
    TransferRowsRetained => "transfer-rows-retained",
    TransferRowsRebuilt => "transfer-rows-rebuilt",
    TransferSelectionWrites => "transfer-selection-writes",
    TransferChoiceTitlesPrepared => "transfer-choice-titles-prepared",
    TransferChoiceValuesEncoded => "transfer-choice-values-encoded",
    TransferReviewLinesPrepared => "transfer-review-lines-prepared",
    TransferReviewedPayloadsPrepared => "transfer-reviewed-payloads-prepared",
    TransferReceiptListEntriesPrepared => "transfer-receipt-list-entries-prepared",
    ReceiptCandidateQueries => "receipt-candidate-queries",
    ReceiptStateOnlyProjections => "receipt-state-only-projections",
    ReceiptArtifactIdentitiesPrepared => "receipt-artifact-identities-prepared",
    ReceiptArtifactIdentitiesReused => "receipt-artifact-identities-reused",
    ReceiptIdenticalEventCopiesAvoided => "receipt-identical-event-copies-avoided",
    ReceiptFullCandidatesValidated => "receipt-full-candidates-validated",
    ReceiptValidityOnlyVerifications => "receipt-validity-only-verifications",
    ReceiptHeadEventsProjected => "receipt-head-events-projected",
    ReceiptSourceOptionsConsidered => "receipt-source-options-considered",
    ReceiptSourceOptionsRetained => "receipt-source-options-retained",
    ReceiptSourceMeasuresProjected => "receipt-source-measures-projected",
    SupplierMeasureComparisonsPrepared => "supplier-measure-comparisons-prepared",
    SupplierTrustQueriesPrepared => "supplier-trust-queries-prepared",
    TrustQueryContextsMoved => "trust-query-contexts-moved",
    OperationCatalogEntriesPrepared => "operation-catalog-entries-prepared",
    OperationCatalogPublicationsRetained => "operation-catalog-publications-retained",
    OperationRepliesRetained => "operation-replies-retained",
    OperationAcknowledgements => "operation-acknowledgements",
    OperationSuggestionsRefreshed => "operation-suggestions-refreshed",
    OperationSuggestionRowsPrepared => "operation-suggestion-rows-prepared",
    SpeechRepliesRetained => "speech-replies-retained",
    DescriptionEditorsVisited => "description-editors-compared",
    DescriptionRefreshes => "description-previews-refreshed",
    DescriptionRasterJobsAdmitted => "description-raster-jobs-admitted",
    DescriptionRasterOrphanedCompletions => "description-raster-orphaned-completions",
    DescriptionRasterPixelBytesMoved => "description-raster-pixel-bytes-moved",
    DescriptionRasterViewsPublished => "description-raster-views-published",
    DescriptionDiagramFontDatabasesPrepared => "description-diagram-font-databases-prepared",
    NativeSurfaceRequests => "native-surface-requests-considered",
    NativeSurfacesVisible => "native-surface-visible-requests",
    NativeSurfacesCreated => "native-surface-targets-created",
    NativeSurfacesReused => "native-surface-targets-reused",
    NativeSurfacesResized => "native-surface-targets-resized",
    SurfaceDormantImageReferences => "surface-dormant-image-references",
    SurfaceDynamicImageReferences => "surface-dynamic-image-references",
    SurfaceCapturePassesReused => "surface-capture-passes-reused",
    SurfaceCapturePassesEncoded => "surface-capture-passes-encoded",
    NativeLayoutRuntimeChanged => "native-layout-runtimes-changed",
    NativeCanvasSnapshots => "native-canvas-host-snapshots",
    NativeCanvasRegistryBuilds => "native-canvas-registry-builds",
    NativeCanvasRegistryReuses => "native-canvas-registry-reuses",
    NativeCanvasPlacementValuesUpdated => "native-canvas-placement-values-updated",
    WorkspaceCapturesDeferred => "workspace-captures-deferred",
    WorkspaceCapturesAdmitted => "workspace-captures-admitted",
    WorkspaceSnapshotsPrepared => "workspace-snapshots-prepared",
    WorkspaceQueryPlansCreated => "workspace-query-plans-created",
    WorkspaceHistoryRetentionReads => "workspace-history-retention-reads",
    WorkspaceHistoryRetirementReads => "workspace-history-retirement-reads",
    WorkspaceTrashRecordsRetired => "workspace-trash-records-retired",
    LayoutRecordSourceEncodings => "layout-record-source-encodings",
    LayoutRecordSavedUpdates => "layout-record-saved-updates",
    LayoutRecordSavedRemovals => "layout-record-saved-removals",
    LayoutRecordRestoreCandidates => "layout-record-restore-candidates",
    LayoutRecordSnapshotRows => "layout-record-snapshot-rows",
    LayoutRecordSnapshotPublications => "layout-record-snapshot-publications",
    WorkTimerLogsPrepared => "work-timer-logs-prepared",
    NativeToolbarMetadataUpdates => "native-toolbar-metadata-updates",
    TimeSettingsPrepared => "time-settings-prepared",
    TimeSourcesPrepared => "time-sources-prepared",
    TimeSubscriptionQueriesPrepared => "time-subscription-queries-prepared",
    TimeGeometriesPrepared => "time-geometries-prepared",
    TimeGeometryRequestedSamples => "time-geometry-requested-samples",
    TimeHitCandidatesPrepared => "time-hit-candidates-prepared",
    TimeOccurrenceRowsPrepared => "time-occurrence-rows-prepared",
    TimeDetailRowsPrepared => "time-detail-rows-prepared",
    TimeDetailRowsRetained => "time-detail-rows-retained",
    TimeDetailRowsChanged => "time-detail-rows-changed",
    TimeMotionEntriesCopied => "time-motion-entries-copied",
    TimeLabelsPrepared => "time-labels-prepared",
    FileSyncLinguaDocumentParses => "file-sync-lingua-document-parses",
    FileSyncLinguaRecordsRewritten => "file-sync-lingua-records-rewritten",
    RecordLiveIdentifiersRead => "record-live-identifiers-read",
    FileSyncDeclarationSetsPrepared => "file-sync-declaration-sets-prepared",
    RecordDraftValidations => "record-draft-validations",
    FileSyncLinguaValidationRecords => "file-sync-lingua-validation-records",
    EffectValueDecodes => "effect-value-decodes",
    EffectQueueRowsRead => "effect-queue-rows-read",
    EffectQueuePayloadDecodes => "effect-queue-payload-decodes",
    RecordReferenceKindReads => "record-reference-kind-reads",
    KarmaTransferSnapshotsShared => "karma-transfer-snapshots-shared",
    KarmaTransferSnapshotReuses => "karma-transfer-snapshot-reuses",
    KarmaTransferSnapshotMapEntriesCopied => "karma-transfer-snapshot-map-entries-copied",
    TransferAutomationEvidenceValidations => "transfer-automation-evidence-validations",
    TransferAutomationParticipantsValidated => "transfer-automation-participants-validated",
    SourceRepliesRetained => "source-replies-retained",
    SourceOutlineRowsPrepared => "source-outline-rows-prepared",
    SourceBodyLinksPrepared => "source-body-links-prepared",
    SourceBodyMatchedRules => "source-body-matched-rules",
    FrequencyRowsPrepared => "frequency-rows-prepared",
    CalendarProjectionQueriesPrepared => "calendar-projection-queries-prepared",
    TodoResultRowsCopied => "todo-result-rows-copied",
    TodoRowsRetained => "todo-rows-retained",
    TodoRowsRebuilt => "todo-rows-rebuilt",
    FioteLiveChoicesRetained => "fiote-live-choices-retained",
    FioteLiveChoicesRebuilt => "fiote-live-choices-rebuilt",
    FioteOptionClassifications => "fiote-option-classifications",
    MessageCommandCatalogsRetained => "message-command-catalogs-retained",
    TodoSelectionHighlightsChanged => "todo-selection-highlights-changed",
    TodoReplyCandidates => "todo-reply-candidates",
    OntologyReplyCandidates => "ontology-reply-candidates",
    CellReplyIndexedEntries => "cell-reply-indexed-entries",
    CellReplyIndexedIds => "cell-reply-indexed-ids",
    CellReplyIdLookups => "cell-reply-id-lookups",
    CellReplyLinearMessages => "cell-reply-linear-messages",
    CellReplyWaveMessages => "cell-reply-wave-messages",
    CellReplyWaveSequences => "cell-reply-wave-sequences",
    CellReplyOwnersExamined => "cell-reply-owners-examined",
    FrequencyRepliesRetained => "frequency-replies-retained",
    KarmaRepliesRetained => "karma-replies-retained",
    FrequencyMaintenanceOwners => "frequency-maintenance-owners",
    KarmaMaintenanceOwners => "karma-maintenance-owners",
    KarmaPreviewFieldsPublished => "karma-preview-fields-published",
    KarmaScheduleFieldsPublished => "karma-schedule-fields-published",
    KarmaSchedulePartsCreated => "karma-schedule-parts-created",
    KarmaScheduleOutcomesRetained => "karma-schedule-outcomes-retained",
    KarmaScheduleOutcomesUpdated => "karma-schedule-outcomes-updated",
    KarmaScheduleOutcomePartsBuilt => "karma-schedule-outcome-parts-built",
    KarmaScheduleFormsBuilt => "karma-schedule-forms-built",
    KarmaScheduleBoundariesBuilt => "karma-schedule-boundaries-built",
    KarmaSchedulePoliciesBuilt => "karma-schedule-policies-built",
    KarmaScheduleEditorsPublished => "karma-schedule-editors-published",
    KarmaScheduleChoicesPrepared => "karma-schedule-choices-prepared",
    KarmaScheduleChoiceButtonsBuilt => "karma-schedule-choice-buttons-built",
    KarmaScheduleDefaultTimezonesPrepared => "karma-schedule-default-timezones-prepared",
    KarmaPreviewFingerprintsPrepared => "karma-preview-fingerprints-prepared",
    KarmaReadingCandidates => "karma-reading-candidates",
    KarmaReadingHintsPrepared => "karma-reading-hints-prepared",
    KarmaReadingHintsPublished => "karma-reading-hints-published",
    KarmaReadingFragmentsPrepared => "karma-reading-fragments-prepared",
    KarmaSuggestionFieldsPrepared => "karma-suggestion-fields-prepared",
    KarmaSuggestionElementsPrepared => "karma-suggestion-elements-prepared",
    KarmaSuggestionsSelected => "karma-suggestions-selected",
    KarmaTransferChoicesPrepared => "karma-transfer-choices-prepared",
    KarmaChoiceButtonsCreated => "karma-choice-buttons-created",
    KarmaChoiceButtonsRetained => "karma-choice-buttons-retained",
    KarmaChoiceActionsUpdated => "karma-choice-actions-updated",
    KarmaChoiceLabelsPublished => "karma-choice-labels-published",
    KarmaRowControlsVisited => "karma-row-controls-visited",
    KarmaColumnRows => "karma-column-rows",
    KarmaColumnResets => "karma-column-resets",
    KarmaFieldsPublished => "karma-fields-published",
    KarmaListRowsCreated => "karma-list-rows-created",
    KarmaListRowsRetained => "karma-list-rows-retained",
    KarmaListRowsRebuilt => "karma-list-rows-rebuilt",
    KarmaControlsRebuilt => "karma-controls-rebuilt",
    KarmaFormsRebuilt => "karma-forms-rebuilt",
    FrequencyFieldsPublished => "frequency-fields-published",
    FrequencyListPartsCreated => "frequency-list-parts-created",
    FrequencyListPartsRetained => "frequency-list-parts-retained",
    FrequencyListPartsRebuilt => "frequency-list-parts-rebuilt",
    FrequencyControlsRebuilt => "frequency-controls-rebuilt",
    FrequencyFormsRebuilt => "frequency-forms-rebuilt",
    FrequencyFormFieldsPublished => "frequency-form-fields-published",
    FrequencyWeekdaysPublished => "frequency-weekdays-published",
    FrequencyCountdownCandidates => "frequency-countdown-candidates",
    FrequencyCountdownsPrepared => "frequency-countdowns-prepared",
    FrequencyCountdownsPublished => "frequency-countdowns-published",
    KarmaPreviewNamesBorrowed => "karma-preview-names-borrowed",
    KarmaScheduleRowsPublished => "karma-schedule-rows-published",
    KarmaCommandRowsPublished => "karma-command-rows-published",
    KarmaCommandFieldsCaptured => "karma-command-fields-captured",
    KarmaCommandPartsCreated => "karma-command-parts-created",
    KarmaCommandResultsRetained => "karma-command-results-retained",
    KarmaCommandResultsBuilt => "karma-command-results-built",
    KarmaCommandEditorsPublished => "karma-command-editors-published",
    KarmaCommandFormsBuilt => "karma-command-forms-built",
    AilurosRowsPublished => "ailuros-rows-published",
    AilurosFieldsCaptured => "ailuros-fields-captured",
    AilurosPartsCreated => "ailuros-parts-created",
    AilurosDevicesRetained => "ailuros-devices-retained",
    AilurosDevicesBuilt => "ailuros-devices-built",
    AilurosEditorsPublished => "ailuros-editors-published",
    AilurosFormsBuilt => "ailuros-forms-built",
    KarmaTransferRowsPublished => "karma-transfer-rows-published",
    KanbanRanksPrepared => "kanban-ranks-prepared",
    KanbanOwnersPrepared => "kanban-owners-prepared",
    KanbanCardCandidates => "kanban-card-candidates",
    KanbanCardPublications => "kanban-card-publications",
    KanbanCellMessagesExamined => "kanban-cell-messages-examined",
    KanbanReplyCandidates => "kanban-reply-candidates",
    KanbanRepliesRetained => "kanban-replies-retained",
    KanbanHeaderPartsExamined => "kanban-header-parts-examined",
    KanbanHeaderLabelCandidates => "kanban-header-label-candidates",
    KanbanHeaderCaptionsPrepared => "kanban-header-captions-prepared",
    KanbanHeaderLabelsWritten => "kanban-header-labels-written",
    KanbanAttachmentUpdates => "kanban-attachment-updates",
    WorkMetadataParseRequests => "work-metadata-parse-requests",
    WorkMetadataValidationRequests => "work-metadata-validation-requests",
    WorkLogValidationRequests => "work-log-validation-requests",
    SyncLocalContextReads => "sync-local-context-reads",
    SyncLocalOperationInsertAttempts => "sync-local-operation-insert-attempts",
    NotificationSnapshotsPrepared => "notification-snapshots-prepared",
    NotificationCardsCreated => "notification-cards-created",
    NotificationSnapshotEntriesIndexed => "notification-snapshot-entries-indexed",
    NotificationToastRootsPrepared => "notification-toast-roots-prepared",
    ScrollbarTargetsPrepared => "scrollbar-targets-prepared",
    ScrollbarOffsetPublications => "scrollbar-offset-publications",
    FioteStatusSnapshotsPrepared => "fiote-status-snapshots-prepared",
    FioteStatusComparisons => "fiote-status-comparisons",
    MessageCommandSelectionsPrepared => "message-command-selections-prepared",
    MessageCommandCatalogsPrepared => "message-command-catalogs-prepared",
    OperationUsageReportsPrepared => "operation-usage-reports-prepared",
    ShaderAuthoringFieldsCaptured => "shader-authoring-fields-captured",
    ShaderAuthoringEffectCopies => "shader-authoring-effect-copies",
    ShaderAuthoringDraftCopies => "shader-authoring-draft-copies",
    ShaderCatalogDraftCopies => "shader-catalog-draft-copies",
    ShaderCatalogWorkingCopies => "shader-catalog-working-copies",
    ShaderQuantityValidations => "shader-quantity-validations",
    ShaderDefinitionSnapshots => "shader-definition-snapshots",
    ShaderFrameReadingsResolved => "shader-frame-readings-resolved",
    ShaderPipelineUniqueRequests => "shader-pipeline-unique-requests",
    ShaderInlineInputUpdates => "shader-inline-input-updates",
    ShaderSpatialInputUpdates => "shader-spatial-input-updates",
    ShaderSpatialBodyUpdates => "shader-spatial-body-updates",
    ShaderSpatialTransformUpdates => "shader-spatial-transform-updates",
    ShaderGeometryUniformWrites => "shader-geometry-uniform-writes",
    ShaderGeometryBindingsPrepared => "shader-geometry-bindings-prepared",
    ShaderGeometryDispatches => "shader-geometry-dispatches",
    ShaderGeometryDispatchesRetained => "shader-geometry-dispatches-retained",
    ShaderLightingOwnerPreparations => "shader-lighting-owner-preparations",
    ShaderLightingMaterialCopies => "shader-lighting-material-copies",
    ShaderLightingBindings => "shader-lighting-bindings",
    ShaderAssetsRetired => "shader-assets-retired",
    ShaderInstancesPrepared => "shader-instances-prepared",
    ShaderInstancesRetired => "shader-instances-retired",
    ShaderPlacementPublications => "shader-placement-publications",
    EffectTextProtectionVisits => "effect-text-protection-visits",
    ShaderStatusLabelsBuilt => "shader-status-labels-built",
    ShaderNumericStatusFormats => "shader-numeric-status-formats",
    ShaderMappingRangesPrepared => "shader-mapping-ranges-prepared",
    ShaderMappingLabelFormats => "shader-mapping-label-formats",
    ShaderExtentConsumerVisits => "shader-extent-consumer-visits",
    ShaderRecordFeedsCreated => "shader-record-feeds-created",
    ShaderRecordSubscriptionsPrepared => "shader-record-subscriptions-prepared",
    ShaderFeedCatalogValidations => "shader-feed-catalog-validations",
    ShaderEditorCatalogLoads => "shader-editor-catalog-loads",
    ShaderEditorStatusCopies => "shader-editor-status-copies",
    ShaderFloatingDefinitionCopies => "shader-floating-definition-copies",
    ShaderSurfaceShadowUpdates => "shader-surface-shadow-updates",
    ShaderNumericInputsPrepared => "shader-numeric-inputs-prepared",
    ShaderNumericSubscriptionsPrepared => "shader-numeric-subscriptions-prepared",
    ShaderNumericValueResolutions => "shader-numeric-value-resolutions",
    ShaderNumericPreviewUpdates => "shader-numeric-preview-updates",
    NumericProjectionValidationAttempts => "numeric-projection-validation-attempts",
    AppearanceFieldStateUpdates => "appearance-field-state-updates",
    AppearanceStatusBuilds => "appearance-status-builds",
    AppearanceFadeStateUpdates => "appearance-fade-state-updates",
    AppearanceVisibleSurfaces => "appearance-visible-surfaces",
    AppearanceBackgroundOwners => "appearance-background-owners",
    AppearanceLightCandidates => "appearance-light-candidates",
    AppearanceLightsAdmitted => "appearance-lights-admitted",
    AppearanceLightComponentWrites => "appearance-light-component-writes",
    AppearanceTextureSettlements => "appearance-texture-settlements",
    AppearanceGeometryBuilds => "appearance-geometry-builds",
    AppearanceMeshPublications => "appearance-mesh-publications",
    AppearanceLitOutputs => "appearance-lit-outputs",
    AppearanceOverlayMaterialWrites => "appearance-overlay-material-writes",
    AppearanceOverlayMeshWrites => "appearance-overlay-mesh-writes",
    AppearanceRoomIdentities => "appearance-room-identities",
    AppearanceRoomClouds => "appearance-room-clouds",
    AppearanceRoomCloudViews => "appearance-room-cloud-views",
    ShadowShapesReused => "shadow-shapes-reused",
    ShadowShapeBuilds => "shadow-shape-builds",
    BlurParameterUpdates => "blur-parameter-updates",
    BlurRequestReplacements => "blur-request-replacements",
    MentionCandidatesMaterialized => "mention-candidates-materialized",
    MentionQueryScans => "mention-query-scans",
    MentionCatalogRowsPrepared => "mention-catalog-rows-prepared",
    MentionPopupRowsBuilt => "mention-popup-rows-built",
    MentionPopupSelectionUpdates => "mention-popup-selection-updates",
    MentionLinkGeometryUpdates => "mention-link-geometry-updates",
    MentionInlineLinksCreated => "mention-inline-links-created",
    MentionEditorCloneFallbacks => "mention-editor-clone-fallbacks",
    MessageDraftSnapshotsPrepared => "message-draft-snapshots-prepared",
    MessageDraftContentValidations => "message-draft-content-validations",
    MessageDraftContentCopies => "message-draft-content-copies",
    MessageDraftRowsCreated => "message-draft-rows-created",
    MessageContentSnapshotsPrepared => "message-content-snapshots-prepared",
    MessageContentReplacementsPrepared => "message-content-replacements-prepared",
    MessageStoredPartsRendered => "message-stored-parts-rendered",
    MessageStepRowsCreated => "message-step-rows-created",
    MessageStepControlsUpdated => "message-step-controls-updated",
    MessageAgentProgressDecodes => "message-agent-progress-decodes",
    MessageAgentProgressTextUpdates => "message-agent-progress-text-updates",
    ExtensionFieldValuesPrepared => "extension-field-values-prepared",
    ExtensionFeedKeysPrepared => "extension-feed-keys-prepared",
    ExtensionFeedSchemaListsUpdated => "extension-feed-schema-lists-updated",
    ExtensionInspectionSnapshotsPrepared => "extension-inspection-snapshots-prepared",
    ExtensionInspectionConceptRows => "extension-inspection-concept-rows",
    ExtensionDraftDataCopies => "extension-draft-data-copies",
    ExtensionSchemaIndexesPrepared => "extension-schema-indexes-prepared",
    ExtensionReceiptPayloadsPrepared => "extension-receipt-payloads-prepared",
    ExtensionChoiceIndexesPrepared => "extension-choice-indexes-prepared",
    ExtensionChoicesIndexed => "extension-choices-indexed",
    ExtensionPresetSelectionsPrepared => "extension-preset-selections-prepared",
    ExtensionPresetsResolved => "extension-presets-resolved",
    ExtensionAssertionReceiptsVisited => "extension-assertion-receipts-visited",
    OntologyCatalogIndexesPrepared => "ontology-catalog-indexes-prepared",
    OntologyCatalogIndexEntries => "ontology-catalog-index-entries",
    OntologyPayloadSnapshotsPrepared => "ontology-payload-snapshots-prepared",
    OntologyViewsRendered => "ontology-views-rendered",
    OntologyFilterValuesPrepared => "ontology-filter-values-prepared",
    OrganSubscriptionIdentitiesPrepared => "organ-subscription-identities-prepared",
    VisibilityRepliesAccepted => "visibility-replies-accepted",
    LocationRepliesAccepted => "location-replies-accepted",
    SavedLocationRepliesAccepted => "saved-location-replies-accepted",
    MapLiveRepliesAccepted => "map-live-replies-accepted",
    AccessPolicyDraftsParsed => "access-policy-drafts-parsed",
    PasswordMaskTextUpdates => "password-mask-text-updates",
    PasswordExportEditsRejected => "password-export-edits-rejected",
    SimulationFieldValuesPrepared => "simulation-field-values-prepared",
    TransferFieldValuesPrepared => "transfer-field-values-prepared",
    SoundCrossingItemsPrepared => "sound-crossing-items-prepared",
    SoundCrossingMembersEvaluated => "sound-crossing-members-evaluated",
    SoundCrossingPlaybackRequests => "sound-crossing-playback-requests",
    SoundStatusLabelUpdates => "sound-status-label-updates",
    SoundInputPathWrites => "sound-input-path-writes",
    AreaSourceBlockersPrepared => "area-source-blockers-prepared",
    ProteinDraftStoragePreparations => "protein-draft-storage-preparations",
    ProteinDraftsDecoded => "protein-drafts-decoded",
    ProteinDraftValidations => "protein-draft-validations",
    ProteinResultRowSetsPrepared => "protein-result-row-sets-prepared",
    ProteinResultColumnNamesPrepared => "protein-result-column-names-prepared",
    ProteinResultStatusUpdates => "protein-result-status-updates",
    TransferSnapshotDuplicates => "transfer-snapshot-duplicates",
    TransferSnapshotRowsPrepared => "transfer-snapshot-rows-prepared",
    TransferAudienceDraftsUpdated => "transfer-audience-drafts-updated",
    TransferFormUpdates => "transfer-form-updates",
    VisibilityPoliciesPrepared => "visibility-policies-prepared",
    SimulationScenarioParses => "simulation-scenario-parses",
    SimulationBaseStampsPrepared => "simulation-base-stamps-prepared",
    SimulationLiveStatusUpdates => "simulation-live-status-updates",
    SimulationLiveFactsCoalesced => "simulation-live-facts-coalesced",
    RecordRevisionObservationBatches => "record-revision-observation-batches",
    RecordRevisionObservationRows => "record-revision-observation-rows",
    OptimizationRunTransitionsPrepared => "optimization-run-transitions-prepared",
    DemeterStatusOnlyReplies => "demeter-status-only-replies",
    DemeterFieldValuesPrepared => "demeter-field-values-prepared",
    DemeterCatalogIndexesPrepared => "demeter-catalog-indexes-prepared",
    DemeterCatalogIndexEntries => "demeter-catalog-index-entries",
    TrustFieldValuesPrepared => "trust-field-values-prepared",
    TrustInputIndexesPrepared => "trust-input-indexes-prepared",
    TrustRepliesMatched => "trust-replies-matched",
    TrustPayloadSnapshotsPrepared => "trust-payload-snapshots-prepared",
    TrustJsonPresentationsPrepared => "trust-json-presentations-prepared",
    TrustClaimCacheEntriesPrepared => "trust-claim-cache-entries-prepared",
    GeneralQueryChangesDelivered => "general-query-changes-delivered",
    GeneralQueryProjectionOnlySkips => "general-query-projection-only-skips",
    ThreadMessageOrdersUpdated => "thread-message-orders-updated",
    ThreadMessageOrderIdsPrepared => "thread-message-order-ids-prepared",
    RecordOpeningReplyPayloads => "record-opening-reply-payloads",
    ThreadScrollOffsetPublications => "thread-scroll-offset-publications",
    ThreadPreviewUpdatesPrepared => "thread-preview-updates-prepared",
    TranscriptReplyTargets => "transcript-reply-targets",
    MessageDescriptionAttachmentsDecoded => "message-description-attachments-decoded",
    MessageDescriptionBytesDecoded => "message-description-bytes-decoded",
    MessageAttachmentChunkRows => "message-attachment-chunk-rows",
    MessageAttachmentEncodedBytes => "message-attachment-encoded-bytes",
    MessageRestorationAttachmentsDecoded => "message-restoration-attachments-decoded",
    MessageRestorationBytesDecoded => "message-restoration-bytes-decoded",
    MessageRestorationLengthsReused => "message-restoration-lengths-reused",
    MessageTranscriptTextUpdates => "message-transcript-text-updates",
    MessageTranscriptTextBytesPrepared => "message-transcript-text-bytes-prepared",
    FioteAvatarInputsPrepared => "fiote-avatar-inputs-prepared",
    FioteAvatarDensityPreparations => "fiote-avatar-density-preparations",
    FioteAvatarSceneReconciliations => "fiote-avatar-scene-reconciliations",
    FioteAvatarPortraitsCreated => "fiote-avatar-portraits-created",
    FioteAvatarCameraUpdates => "fiote-avatar-camera-updates",
    FioteAvatarTransformUpdates => "fiote-avatar-transform-updates",
    FioteAvatarMaterialUpdates => "fiote-avatar-material-updates",
    MapRequestIdentitiesPrepared => "map-request-identities-prepared",
    MapLiveRecordsPrepared => "map-live-records-prepared",
    CalendarPickerFieldsPrepared => "calendar-picker-fields-prepared",
    CastleCardLayoutUpdates => "castle-card-layout-updates",
    CastleCardScaleUpdates => "castle-card-scale-updates",
    RecordWorkProjectionWrites => "record-work-projection-writes",
    RecordWorkLogProjections => "record-work-log-projections",
    RecordPropertyRegisterWrites => "record-property-register-writes",
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
    pub capture_started_unix_ns: String,
    pub revision: &'static str,
    pub optimized: bool,
    pub workload: Option<&'static str>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub park_frames: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped_park_frames: Option<u64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<u64>,
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
const PARK_FRAME_TRACE_LIMIT: usize = 1024;

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
    workload: Mutex<Option<&'static str>>,
    outputs: Mutex<BTreeMap<u64, Output>>,
    started: Instant,
    capture_started_unix_ns: String,
    reported_at_us: AtomicU64,
    dropped_reports: AtomicU64,
    sender: Option<SyncSender<Report>>,
    counters: Vec<AtomicU64>,
    input_trace: ArrayQueue<InputTrace>,
    dropped_input_trace: AtomicU64,
    park_frames: Option<ArrayQueue<serde_json::Value>>,
    dropped_park_frames: AtomicU64,
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
            engine::performance::Reading::Mark(label, operation) => {
                if let Some(stage) = Stage::from_label(label) {
                    let now = self.timestamp(Instant::now());
                    let thread = thread_id();
                    self.trace_span(SpanReading {
                        stage: stage.label(),
                        started_us: now,
                        ended_us: now,
                        start_thread: thread,
                        end_thread: thread,
                        operation: Some(operation),
                    });
                }
            }
        }
    }

    fn timestamp(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.started)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64
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
            workload: Mutex::new(None),
            started: Instant::now(),
            capture_started_unix_ns: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string(),
            reported_at_us: AtomicU64::new(0),
            dropped_reports: AtomicU64::new(0),
            sender,
            counters: Count::ALL.iter().map(|_| AtomicU64::new(0)).collect(),
            input_trace: ArrayQueue::new(INPUT_TRACE_LIMIT),
            dropped_input_trace: AtomicU64::new(0),
            park_frames: std::env::var_os("LINCE_PARK_TRACE_FRAMES")
                .map(|_| ArrayQueue::new(PARK_FRAME_TRACE_LIMIT)),
            dropped_park_frames: AtomicU64::new(0),
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
        if !accepts_system_name(name) {
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
            capture_started_unix_ns: self.capture_started_unix_ns.clone(),
            revision: utils::build_info::revision(),
            optimized: !cfg!(debug_assertions),
            workload: *self.workload.lock().unwrap_or_else(|error| error.into_inner()),
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
            park_frames: self.park_frames.as_ref().map(|queue| {
                std::iter::from_fn(|| queue.pop()).take(PARK_FRAME_TRACE_LIMIT).collect()
            }).unwrap_or_default(),
            dropped_park_frames: self.park_frames.as_ref().map(|_| self.dropped_park_frames.swap(0, Ordering::Relaxed)),
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

pub fn timestamp(at: Instant) -> Option<u64> {
    monitor().map(|monitor| monitor.timestamp(at))
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

pub fn accepts_system_name(name: &str) -> bool {
    name.len() <= 512
        && (name.starts_with("bevy_")
            || name.starts_with("avian3d::")
            || name.starts_with("lince_interface::")
            || name.starts_with("lince_park::"))
}

pub fn record_system(name: &str, wall: Duration, cpu: Duration, exclusive_cpu: Duration) {
    if let Some(monitor) = monitor() {
        monitor.record_system(name, wall, cpu, exclusive_cpu);
    }
}

pub fn record_park_frame(mut sample: serde_json::Value) -> Result<(), serde_json::Value> {
    let Some(monitor) = monitor() else {
        return Err(sample);
    };
    let Some(queue) = &monitor.park_frames else {
        return Err(sample);
    };
    if monitor.finalized.load(Ordering::Acquire) {
        return Ok(());
    }
    if sample["event"] == "render-start" {
        let workload = monitor.workload.lock().unwrap_or_else(|error| error.into_inner());
        sample["workload"] = serde_json::json!(*workload);
        sample["phase"] = Phase::ALL[monitor.phase.load(Ordering::Relaxed) as usize].label().into();
    }
    if queue.push(sample).is_err() {
        monitor.dropped_park_frames.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

pub fn set_phase(phase: Phase) {
    set_context(phase, None);
}

pub fn set_workload(workload: &'static str, phase: Phase) {
    set_context(phase, Some(workload));
}

fn set_context(phase: Phase, workload: Option<&'static str>) {
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
        let mut current_workload = monitor.workload.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(workload) = workload {
            *current_workload = Some(workload);
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
                operation: None,
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
        monitor.backend(engine::performance::Reading::Mark("record-edit-queued", 7));
        monitor.backend(engine::performance::Reading::Mark("record-edit-dispatched", 7));
        monitor.backend(engine::performance::Reading::Mark("private Record contents", 99));
        let report = monitor.snapshot();
        assert_eq!(report.spans.len(), 2);
        assert_eq!(report.spans[0].stage, "record-edit-queued");
        assert_eq!(report.spans[1].stage, "record-edit-dispatched");
        assert!(report.spans[0].started_us <= report.spans[1].started_us);
        assert!(report.spans.iter().all(|span| span.operation == Some(7) && span.started_us == span.ended_us));
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
        assert!(report.spans.iter().all(|span| span.operation == Some(span.started_us)));
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
}
