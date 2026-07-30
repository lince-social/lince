use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    CandidateRoute, CapabilitySet, DurationMs, LiteralValue, LocalId, PortContract,
    ResolvedReference, Rounding, Slug, TypedUid, ValueType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProgramSchema {
    #[serde(rename = "karma.program.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramAst {
    pub schema: ProgramSchema,
    pub slug: Slug,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<Slug>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<LocalId, ParameterDefinition>,
    pub nodes: BTreeMap<LocalId, NodeAst>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<LocalId, OutputRef>,
    #[serde(default)]
    pub required_capabilities: CapabilitySet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub value_type: ValueType,
    pub default: LiteralValue,
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OutputRef {
    pub node: LocalId,
    pub port: LocalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputBinding {
    pub source: OutputRef,
    pub expected_type: ValueType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAst {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<LocalId, InputBinding>,
    pub outputs: BTreeMap<LocalId, PortContract>,
    pub operation: NodeOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum NodeOperation {
    Trigger {
        source: TriggerSource,
        output: LocalId,
    },
    Input {
        source: InputSource,
        output: LocalId,
    },
    Derive {
        expressions: BTreeMap<LocalId, ExpressionAst>,
    },
    Delay {
        input: LocalId,
        output: LocalId,
        initial: LiteralValue,
        state: StateContract,
    },
    Threshold {
        input: LocalId,
        active: LocalId,
        entered: LocalId,
        left: LocalId,
        direction: ThresholdDirection,
        enter: LiteralValue,
        exit: LiteralValue,
        initial_active: bool,
        state: StateContract,
    },
    Debounce {
        input: LocalId,
        stable: LocalId,
        entered: LocalId,
        left: LocalId,
        for_at_least: DurationMs,
        initial: bool,
        state: StateContract,
    },
    Cooldown {
        input: LocalId,
        allowed: LocalId,
        cooldown: DurationMs,
        state: StateContract,
    },
    RateLimit {
        input: LocalId,
        allowed: LocalId,
        max: u32,
        window: DurationMs,
        state: StateContract,
    },
    RouteCandidate {
        condition: LocalId,
        output: LocalId,
        route: CandidateRoute,
        template: Slug,
        fields: BTreeMap<LocalId, LocalId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThresholdDirection {
    Above,
    Below,
}

impl NodeOperation {
    pub const fn is_state_boundary(&self) -> bool {
        matches!(self, Self::Delay { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TriggerSource {
    Manual,
    Fact {
        #[serde(skip_serializing_if = "Option::is_none")]
        record: Option<ResolvedReference>,
        #[serde(skip_serializing_if = "Option::is_none")]
        concept: Option<ResolvedReference>,
    },
    Frequency {
        frequency: ResolvedReference,
    },
    Signal {
        signal: ResolvedReference,
    },
    Decision {
        decision: ResolvedReference,
    },
    Receipt {
        receipt: ResolvedReference,
    },
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InputSource {
    Parameter { parameter: LocalId },
    RecordQuantity { record: ResolvedReference },
    SavedProtein { view: ResolvedReference },
    Signal { signal: ResolvedReference },
    SecretMetadata { secret: LocalId },
    CapturedFact { fact: ResolvedReference },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExpressionAst {
    Literal {
        value: LiteralValue,
    },
    Input {
        input: LocalId,
    },
    Unary {
        operator: UnaryOperator,
        value: Box<ExpressionAst>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<ExpressionAst>,
        right: Box<ExpressionAst>,
        /// Present exactly when this is a multiply or divide over exact values.
        /// Proof enforces that as an `iff`: a `precision` on an operation that
        /// cannot use one is refused rather than ignored, because a field that
        /// changes the revision hash without changing behaviour would give one
        /// program two identities.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        precision: Option<DecimalPrecision>,
    },
    If {
        condition: Box<ExpressionAst>,
        then_value: Box<ExpressionAst>,
        else_value: Box<ExpressionAst>,
    },
}

impl ExpressionAst {
    pub fn referenced_inputs(&self) -> BTreeSet<LocalId> {
        let mut inputs = BTreeSet::new();
        self.collect_referenced_inputs(&mut inputs);
        inputs
    }

    fn collect_referenced_inputs(&self, inputs: &mut BTreeSet<LocalId>) {
        match self {
            Self::Literal { .. } => {}
            Self::Input { input } => {
                inputs.insert(input.clone());
            }
            Self::Unary { value, .. } => value.collect_referenced_inputs(inputs),
            Self::Binary { left, right, .. } => {
                left.collect_referenced_inputs(inputs);
                right.collect_referenced_inputs(inputs);
            }
            Self::If {
                condition,
                then_value,
                else_value,
            } => {
                condition.collect_referenced_inputs(inputs);
                then_value.collect_referenced_inputs(inputs);
                else_value.collect_referenced_inputs(inputs);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnaryOperator {
    Not,
    Negate,
}

/// How a multiply or divide lands on a representable decimal.
///
/// Neither operation is closed over fixed-point decimals — no scale represents
/// `1/3`, and multiplying two scale-9 values needs 18 digits — so the author
/// declares where the result sits instead of the implementation guessing. This
/// rides in the AST, which means it is inside the revision hash: changing a
/// rounding mode is a visible revision, not a silent change of answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecimalPrecision {
    pub scale: u8,
    pub rounding: Rounding,
    /// Required exactly when both sides carry a dimension, and forbidden
    /// otherwise. See [`DeclaredUnit`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_unit: Option<DeclaredUnit>,
}

/// The unit an author assigns to a product or quotient of two dimensioned
/// values.
///
/// Unit algebra is declared, never inferred. Scaling a quantity by a plain
/// decimal — the percentage and rate cases, which is what most rules need —
/// keeps its unit and needs nothing here. But `kg × kg` and `kg ÷ m` have no
/// unit this system can name, and inventing `kg²` or quietly keeping the
/// left-hand unit are both worse than asking: one fabricates a dimension, the
/// other drops one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DeclaredUnit {
    /// The dimensions cancel. `total ÷ budget` is a ratio, and saying so is a
    /// real statement about the rule, not an absence of one.
    Dimensionless,
    Unit { unit: TypedUid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateContract {
    pub persistence: StatePersistence,
    pub reset: StateResetPolicy,
    pub late_event: LateEventPolicy,
    pub migration: StateMigrationPolicy,
    pub simulation: SimulationStatePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatePersistence {
    Program,
    Workflow,
    ModelCheckpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateResetPolicy {
    Never,
    Manual,
    OnRevisionChange,
    OnProgramActivation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LateEventPolicy {
    Ignore,
    Recompute,
    Compensate,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateMigrationPolicy {
    Reset,
    RequireExplicit,
    CompatibleTypeOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SimulationStatePolicy {
    Clone,
    Reset,
}
