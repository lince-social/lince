// non-lean-yet
// Scope: enum ProgramSchema, struct ProgramAst, struct ParameterDefinition, struct OutputRef.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 240.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 23, 90, 153, 167, 193, 204, 263, 279, 287, 315, 324, 334, 344, 353.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 23, 25,
//   29, 30, 31, 32, 34, 36, 37, 39, 41, 45, 46, 47, 48, 54, 55, 56, 60, 61, 62, 66, 80, 82, 83, 84,
//   90, 91, 92, 93, 95, 96, 97, 99, 100, 102, 103, 104, 105, 106, 108, 109, 110, 111, 112, 113,
//   114, 115, 116, 117, 119, 120, 121, 122, 123, 124, 125, 126, 128, 129, 130, 131, 132, 134, 135,
//   136, 137, 138, 139, 141, 142, 143, 144, 145, 146, 153, 154, 155, 159, 167, 168, 169, 171, 173,
//   175, 176, 178, 179, 181, 182, 184, 185, 187, 193, 194, 195, 196, 197, 198, 204, 205, 206, 208,
//   209, 211, 212, 213, 215, 216, 217, 218, 220, 222, 223, 224, 225, 230, 263, 264, 265, 269, 270,
//   271, 273, 279, 280, 281, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300,
//   304, 305, 306, 307, 308, 309, 315, 316, 317, 318, 324, 325, 326, 327, 328, 334, 335, 336, 337,
//   338, 344, 345, 346, 347, 353, 354, 355.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    CandidateRoute, CapabilitySet, DurationMs, LiteralValue, LocalId, PortContract,
    ResolvedReference, Rounding, Slug, TypedUid, ValueType,
};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub enum ProgramSchema {
    #[serde(rename = "karma.program.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParameterDefinition {
    pub value_type: ValueType,
    pub default: LiteralValue,
    pub mutable: bool,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct OutputRef {
    pub node: LocalId,
    pub port: LocalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InputBinding {
    pub source: OutputRef,
    pub expected_type: ValueType,
}

impl ProgramAst {
    pub fn is_externally_observable(&self) -> bool {
        self.nodes.values().any(|node| {
            matches!(
                node.operation,
                NodeOperation::RouteCandidate {
                    route: crate::karma::CandidateRoute::Act,
                    ..
                }
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeAst {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<LocalId, InputBinding>,
    pub outputs: BTreeMap<LocalId, PortContract>,
    pub operation: NodeOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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
#[derive(schemars::JsonSchema)]
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
#[derive(schemars::JsonSchema)]
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
#[derive(schemars::JsonSchema)]
pub enum InputSource {
    Parameter { parameter: LocalId },
    RecordQuantity { record: ResolvedReference },
    SavedProtein { view: ResolvedReference },
    Signal { signal: ResolvedReference },
    CapturedFact { fact: ResolvedReference },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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
#[derive(schemars::JsonSchema)]
pub enum UnaryOperator {
    Not,
    Negate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecimalPrecision {
    pub scale: u8,
    pub rounding: Rounding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_unit: Option<DeclaredUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum DeclaredUnit {
    Dimensionless,
    Unit { unit: TypedUid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StateContract {
    pub persistence: StatePersistence,
    pub reset: StateResetPolicy,
    pub late_event: LateEventPolicy,
    pub migration: StateMigrationPolicy,
    pub simulation: SimulationStatePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum StatePersistence {
    Program,
    Workflow,
    ModelCheckpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum StateResetPolicy {
    Never,
    Manual,
    OnRevisionChange,
    OnProgramActivation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum LateEventPolicy {
    Ignore,
    Recompute,
    Compensate,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum StateMigrationPolicy {
    Reset,
    RequireExplicit,
    CompatibleTypeOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum SimulationStatePolicy {
    Clone,
    Reset,
}
