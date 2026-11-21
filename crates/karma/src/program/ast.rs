//! Program definitions and node bindings.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{CandidateRoute, CapabilitySet, LiteralValue, LocalId, PortContract, Slug, ValueType};

mod expression;
mod operation;
mod state;

pub use expression::{
    BinaryOperator, DecimalPrecision, DeclaredUnit, ExpressionAst, UnaryOperator,
};
pub use operation::{InputSource, NodeOperation, ThresholdDirection, TriggerSource};
pub use state::{
    LateEventPolicy, SimulationStatePolicy, StateContract, StateMigrationPolicy, StatePersistence,
    StateResetPolicy,
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
                    route: CandidateRoute::Act,
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
