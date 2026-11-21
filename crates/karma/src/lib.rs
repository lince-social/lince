//! Typed Karma programs, exact evaluation, and schedule planning.
//!
//! These operations use their supplied inputs and need no Lince runtime or database.

mod model;
mod program;
mod scheduling;

pub use ast::{
    BinaryOperator, DecimalPrecision, DeclaredUnit, ExpressionAst, InputBinding, InputSource,
    LateEventPolicy, NodeAst, NodeOperation, OutputRef, ParameterDefinition, ProgramAst,
    ProgramSchema, SimulationStatePolicy, StateContract, StateMigrationPolicy, StatePersistence,
    StateResetPolicy, ThresholdDirection, TriggerSource, UnaryOperator,
};
pub use cadence::{
    Cadence, CadenceBound, CadenceError, CadenceStep, Derived, InvalidDay, MAX_DERIVED_OCCURRENCES,
    NoOccurrence,
};
pub use calendar::{
    CalendarAdvance, CalendarBoundary, CalendarBoundaryKind, CalendarDiscontinuity,
    CalendarSchedule, CivilDateTime, CivilTime, CivilWeekday, DayOfMonth, FoldPolicy, GapPolicy,
    LocalTimeResolution, TimeZoneId, TimeZoneProvider, TzdbRevision, TzdbVersion, WeekdaySet,
};
pub use calendar_runtime::{
    CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN, CalendarCatchUp, CalendarCatchUpPause,
    CalendarCursor, CalendarCursorResolution, CalendarEmission, CalendarScheduleOccurrence,
    CalendarScheduleOccurrenceSchema, advance_calendar_cursor, resolve_calendar_cursor,
};
pub use canonical::{CanonicalHash, canonical_hash, canonical_json_bytes};
pub use capability::{Capability, CapabilityFamily, CapabilitySet};
pub use dispatcher::{
    ArmedDeadline, DeadlineAdmission, DeadlineEntry, DeadlineIndex, DeadlinePlan,
    DeadlinePlanError, DeadlineRejectionReason, DemandResource, DemandedDeadline,
    DemandedDeadlinePlan, DispatchBatch, DispatcherResourceGrant,
    ELAPSED_SCHEDULE_OCCURRENCE_HASH_DOMAIN, ElapsedScheduleOccurrence,
    ElapsedScheduleOccurrenceSchema, HostTimerCapabilities, MAX_OCCURRENCE_BATCH_PAGE_TICKS,
    OCCURRENCE_BATCH_HASH_DOMAIN, OccurrenceBatch, OccurrenceBatchEmission, OccurrenceBatchSchema,
    SEMANTIC_SCHEDULE_TICK_HASH_DOMAIN, ScheduleCursorLifecycle, ScheduleDemand,
    ScheduleDemandCapacity, ScheduleDemandRates, ScheduleWorkloadUpperBounds, SchedulerCalibration,
    SemanticScheduleTick, SemanticScheduleTickSchema, plan_deadlines, plan_demanded_deadlines,
};
pub use dsl::{
    DslErrorKind, KarmaDslError, MAX_DSL_BYTES, MAX_DSL_NESTING, MAX_DSL_STRING_BYTES,
    MAX_DSL_TOKENS, format_frequency, format_program, parse_frequency, parse_program,
};
pub use durable::{
    FREQUENCY_ACTIVATION_HASH_DOMAIN, FrequencyActivationCause, FrequencyActivationEpoch,
    FrequencyActivationEpochSchema, FrequencyMutationAction, FrequencyMutationEvidence,
    FrequencyMutationEvidenceSchema, ProgramMutationAction, ProgramMutationEvidence,
    ProgramMutationEvidenceSchema,
};
pub use evaluate::{
    ControlState, EvaluationError, EvaluationErrorCode, EvaluationLimits, EvaluationResult,
    FrozenEvaluationContext, NodeTrace, RoundingNote, evaluate_program,
};
pub use exact::{Confidence, FixedDecimal, MAX_DECIMAL_SCALE, Probability};
pub use execution::{
    CandidateReviewAction, CandidateReviewEvidence, CandidateReviewEvidenceSchema,
    KARMA_CANDIDATE_PROPOSAL_HASH_DOMAIN, KARMA_RUN_HASH_DOMAIN, KarmaCandidateProposal,
    KarmaCandidateProposalSchema, KarmaRun, KarmaRunOutcome, KarmaRunSchema,
    OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN, OccurrenceProgramEpoch, OccurrenceProgramEpochSchema,
    PROGRAM_STATE_EVENT_HASH_DOMAIN, PersistedProgramNodeState, ProgramEpochMember,
    ProgramNotApplicableReason, ProgramRunBlockCode, ProgramStateEvent, ProgramStateEventSchema,
    ProgramStateResetReason,
};
pub use failure::{FailureCode, FailurePath, KarmaBoundaryError, KarmaFailure, RetryDisposition};
pub use frequency::{
    CadenceAst, CadenceStepAst, CompiledFrequency, CompiledSchedule, DurationBinding,
    FREQUENCY_PARAMETER_HASH_DOMAIN, FREQUENCY_REVISION_HASH_DOMAIN, FrequencyAst,
    FrequencyCadenceAst, FrequencyCompileError, FrequencyCompileErrorKind,
    FrequencyParameterDefinition, FrequencyParameterValue, FrequencySchema, FrequencyTimerAst,
    PositiveIntegerBinding,
};
pub use model::{canonical, capability, exact, failure, reference, state, time, value};
pub use occurrence::{
    CALENDAR_COALESCED_BATCH_HASH_DOMAIN, CalendarCoalescedBatch, CalendarCoalescedBatchSchema,
    KARMA_OCCURRENCE_HASH_DOMAIN, KarmaOccurrenceEnvelope, KarmaOccurrenceSchema,
    KarmaOccurrenceSource, SEMANTIC_CALENDAR_TICK_HASH_DOMAIN, SemanticCalendarTick,
    SemanticCalendarTickSchema,
};
pub use program::{ast, dsl, durable, evaluate, execution, proof, replay};
pub use proof::{Proof, ProofIssue, ProofIssueCode, ProofSeverity, ProofStatus, prove_program};
pub use reference::{ConditionBinding, LocalId, ReferenceKind, ResolvedReference, Slug, TypedUid};
pub use replay::{
    EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN, EVALUATION_RESULT_HASH_DOMAIN, EvaluationReplayCapsule,
    EvaluationReplayCapsuleSchema, EvaluatorRevision, PROGRAM_REVISION_HASH_DOMAIN, ReplayError,
    ReplayErrorCode, SealedEvaluationReplayCapsule, capture_evaluation_replay,
};
pub use schedule::{
    ArmWindow, ElapsedSchedule, InactiveGapPolicy, MissedPolicy, OccurrenceRange, OverloadPolicy,
    RationalRate, RephasePolicy, RephasedElapsed, ScheduleAdvance, ScheduleCursor,
    ScheduleEmission, ScheduleReactivation, TimerPolicy,
};
pub use scheduling::{
    cadence, calendar, calendar_runtime, dispatcher, frequency, occurrence, schedule,
    simple_frequency, timezone_artifact,
};
pub use state::{
    CandidateStatus, DatumState, DefinitionStatus, EngineMode, KarmaActionKind, KarmaObjectKind,
    RunStatus, WorkflowStatus,
};
pub use time::{DurationMs, TimestampMs};
pub use timezone_artifact::{
    ArtifactTimeZoneProvider, MAX_TZDB_ARTIFACT_BYTES, MAX_TZDB_ARTIFACT_ZONES,
    MAX_TZDB_SEGMENTS_PER_ZONE, TZDB_ARTIFACT_HASH_DOMAIN, TimeZoneArtifact,
    TimeZoneArtifactSchema, TimeZoneDefinition, UtcOffsetSegment,
};
pub use value::{
    CandidateRoute, DecimalValue, LiteralValue, PortContract, RoundedDecimal, Rounding,
    Sensitivity, ValueType,
};
