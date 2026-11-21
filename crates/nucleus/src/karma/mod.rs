//! Karma's typed core and the Lince rule and consequence adapters.

pub mod condition;
pub mod consequence;
pub mod rule_field;
pub mod scheduled_change;
mod timezone_builtin;
pub mod transfer_consequence;

pub use condition::{Carry, Condition, ConditionError, ExactResolver, Gate, decide};
pub use consequence::{Consequence, Consequences};
pub use lince_karma::{
    ArmWindow, ArmedDeadline, ArtifactTimeZoneProvider, BinaryOperator,
    CALENDAR_COALESCED_BATCH_HASH_DOMAIN, CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN, Cadence,
    CadenceAst, CadenceBound, CadenceError, CadenceStep, CadenceStepAst, CalendarAdvance,
    CalendarBoundary, CalendarBoundaryKind, CalendarCatchUp, CalendarCatchUpPause,
    CalendarCoalescedBatch, CalendarCoalescedBatchSchema, CalendarCursor, CalendarCursorResolution,
    CalendarDiscontinuity, CalendarEmission, CalendarSchedule, CalendarScheduleOccurrence,
    CalendarScheduleOccurrenceSchema, CandidateReviewAction, CandidateReviewEvidence,
    CandidateReviewEvidenceSchema, CandidateRoute, CandidateStatus, CanonicalHash, Capability,
    CapabilityFamily, CapabilitySet, CivilDateTime, CivilTime, CivilWeekday, CompiledFrequency,
    CompiledSchedule, ConditionBinding, Confidence, ControlState, DatumState, DayOfMonth,
    DeadlineAdmission, DeadlineEntry, DeadlineIndex, DeadlinePlan, DeadlinePlanError,
    DeadlineRejectionReason, DecimalPrecision, DecimalValue, DeclaredUnit, DefinitionStatus,
    DemandResource, DemandedDeadline, DemandedDeadlinePlan, Derived, DispatchBatch,
    DispatcherResourceGrant, DslErrorKind, DurationBinding, DurationMs,
    ELAPSED_SCHEDULE_OCCURRENCE_HASH_DOMAIN, EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN,
    EVALUATION_RESULT_HASH_DOMAIN, ElapsedSchedule, ElapsedScheduleOccurrence,
    ElapsedScheduleOccurrenceSchema, EngineMode, EvaluationError, EvaluationErrorCode,
    EvaluationLimits, EvaluationReplayCapsule, EvaluationReplayCapsuleSchema, EvaluationResult,
    EvaluatorRevision, ExpressionAst, FREQUENCY_ACTIVATION_HASH_DOMAIN,
    FREQUENCY_PARAMETER_HASH_DOMAIN, FREQUENCY_REVISION_HASH_DOMAIN, FailureCode, FailurePath,
    FixedDecimal, FoldPolicy, FrequencyActivationCause, FrequencyActivationEpoch,
    FrequencyActivationEpochSchema, FrequencyAst, FrequencyCadenceAst, FrequencyCompileError,
    FrequencyCompileErrorKind, FrequencyMutationAction, FrequencyMutationEvidence,
    FrequencyMutationEvidenceSchema, FrequencyParameterDefinition, FrequencyParameterValue,
    FrequencySchema, FrequencyTimerAst, FrozenEvaluationContext, GapPolicy, HostTimerCapabilities,
    InactiveGapPolicy, InputBinding, InputSource, InvalidDay, KARMA_CANDIDATE_PROPOSAL_HASH_DOMAIN,
    KARMA_OCCURRENCE_HASH_DOMAIN, KARMA_RUN_HASH_DOMAIN, KarmaActionKind, KarmaBoundaryError,
    KarmaCandidateProposal, KarmaCandidateProposalSchema, KarmaDslError, KarmaFailure,
    KarmaObjectKind, KarmaOccurrenceEnvelope, KarmaOccurrenceSchema, KarmaOccurrenceSource,
    KarmaRun, KarmaRunOutcome, KarmaRunSchema, LateEventPolicy, LiteralValue, LocalId,
    LocalTimeResolution, MAX_DECIMAL_SCALE, MAX_DERIVED_OCCURRENCES, MAX_DSL_BYTES,
    MAX_DSL_NESTING, MAX_DSL_STRING_BYTES, MAX_DSL_TOKENS, MAX_OCCURRENCE_BATCH_PAGE_TICKS,
    MAX_TZDB_ARTIFACT_BYTES, MAX_TZDB_ARTIFACT_ZONES, MAX_TZDB_SEGMENTS_PER_ZONE, MissedPolicy,
    NoOccurrence, NodeAst, NodeOperation, NodeTrace, OCCURRENCE_BATCH_HASH_DOMAIN,
    OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN, OccurrenceBatch, OccurrenceBatchEmission,
    OccurrenceBatchSchema, OccurrenceProgramEpoch, OccurrenceProgramEpochSchema, OccurrenceRange,
    OutputRef, OverloadPolicy, PROGRAM_REVISION_HASH_DOMAIN, PROGRAM_STATE_EVENT_HASH_DOMAIN,
    ParameterDefinition, PersistedProgramNodeState, PortContract, PositiveIntegerBinding,
    Probability, ProgramAst, ProgramEpochMember, ProgramMutationAction, ProgramMutationEvidence,
    ProgramMutationEvidenceSchema, ProgramNotApplicableReason, ProgramRunBlockCode, ProgramSchema,
    ProgramStateEvent, ProgramStateEventSchema, ProgramStateResetReason, Proof, ProofIssue,
    ProofIssueCode, ProofSeverity, ProofStatus, RationalRate, ReferenceKind, RephasePolicy,
    RephasedElapsed, ReplayError, ReplayErrorCode, ResolvedReference, RetryDisposition,
    RoundedDecimal, Rounding, RoundingNote, RunStatus, SEMANTIC_CALENDAR_TICK_HASH_DOMAIN,
    SEMANTIC_SCHEDULE_TICK_HASH_DOMAIN, ScheduleAdvance, ScheduleCursor, ScheduleCursorLifecycle,
    ScheduleDemand, ScheduleDemandCapacity, ScheduleDemandRates, ScheduleEmission,
    ScheduleReactivation, ScheduleWorkloadUpperBounds, SchedulerCalibration,
    SealedEvaluationReplayCapsule, SemanticCalendarTick, SemanticCalendarTickSchema,
    SemanticScheduleTick, SemanticScheduleTickSchema, Sensitivity, SimulationStatePolicy, Slug,
    StateContract, StateMigrationPolicy, StatePersistence, StateResetPolicy,
    TZDB_ARTIFACT_HASH_DOMAIN, ThresholdDirection, TimeZoneArtifact, TimeZoneArtifactSchema,
    TimeZoneDefinition, TimeZoneId, TimeZoneProvider, TimerPolicy, TimestampMs, TriggerSource,
    TypedUid, TzdbRevision, TzdbVersion, UnaryOperator, UtcOffsetSegment, ValueType, WeekdaySet,
    WorkflowStatus, advance_calendar_cursor, ast, cadence, calendar, calendar_runtime, canonical,
    canonical_hash, canonical_json_bytes, capability, capture_evaluation_replay, dispatcher, dsl,
    durable, evaluate, evaluate_program, exact, execution, failure, format_frequency,
    format_program, frequency, occurrence, parse_frequency, parse_program, plan_deadlines,
    plan_demanded_deadlines, proof, prove_program, reference, replay, resolve_calendar_cursor,
    schedule, simple_frequency, state, time, timezone_artifact, value,
};
pub use timezone_builtin::BundledTimeZoneProvider;
