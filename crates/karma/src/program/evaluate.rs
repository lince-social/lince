//! Deterministic evaluation of one proven Karma program occurrence.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    BinaryOperator, CanonicalHash, DecimalValue, FailurePath, LiteralValue, LocalId, NodeOperation,
    OutputRef, ProgramAst, ProofStatus, Rounding, TimestampMs, ValueType, prove_program,
};

mod control;
mod expression;
mod meter;
mod node;

use meter::{FuelMeter, Path, Site};
use node::evaluate_node;

/// Stable categories for failures during one program occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationErrorCode {
    /// Static proof rejected the program.
    ProofRejected,
    /// A boundary node has no frozen value.
    MissingInput,
    /// A parameter has neither a frozen value nor a program default.
    MissingParameter,
    /// The source for a delay update did not produce a value.
    MissingStateSource,
    /// A frozen value or state conflicts with the expected type.
    RuntimeTypeMismatch,
    /// An exact arithmetic result could not be represented.
    ArithmeticOverflow,
    /// Division or remainder used a zero divisor.
    DivisionByZero,
    /// The deterministic work allowance was consumed.
    FuelExhausted,
    /// Evaluating the next expression would exceed the depth limit.
    ExpressionDepthExceeded,
    /// A temporal control requires a frozen logical timestamp.
    MissingLogicalTime,
    /// A control rejected an occurrence earlier than its last observation.
    NonMonotonicLogicalTime,
    /// The control's late-event policy requires replay in occurrence order.
    ReplayRequired,
    /// A fact assumed by accepted proof or evaluation did not hold.
    InvariantViolation,
}

/// A failed occurrence with its location and work consumed up to the failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationError {
    /// Machine-readable failure category.
    pub code: EvaluationErrorCode,
    /// Context for displaying the failure.
    pub message: String,
    /// JSON pointer to the affected part of the program.
    pub path: FailurePath,
    /// Node associated with the failure, when one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<LocalId>,
    /// Deterministic work consumed before the failure.
    pub fuel_used: u64,
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at {}: {}", self.code, self.path, self.message)
    }
}

impl std::error::Error for EvaluationError {}

/// Per-occurrence limits for deterministic work and expression depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationLimits {
    /// Maximum number of visited nodes and evaluated expressions combined.
    pub fuel: u64,
    /// Maximum expression depth, counting the root as one.
    pub max_expression_depth: u16,
}

impl Default for EvaluationLimits {
    fn default() -> Self {
        Self {
            fuel: 100_000,
            max_expression_depth: 128,
        }
    }
}

/// All external values, time, and prior state used to evaluate an occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FrozenEvaluationContext {
    /// Frozen input values keyed by the receiving node ID.
    #[serde(default)]
    pub boundary_values: BTreeMap<LocalId, LiteralValue>,
    /// Optional overrides keyed by the program's parameter IDs.
    #[serde(default)]
    pub parameter_values: BTreeMap<LocalId, LiteralValue>,
    /// Previous delay values keyed by node ID.
    #[serde(default)]
    pub delay_state: BTreeMap<LocalId, LiteralValue>,
    /// Occurrence timestamp for temporal controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_at: Option<TimestampMs>,
    /// Previous control states keyed by node ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub control_state: BTreeMap<LocalId, ControlState>,
}

/// Persisted state of a threshold or temporal control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControlState {
    /// Last hysteresis decision.
    Threshold { active: bool },
    /// Stable value and any pending observation.
    Debounce {
        stable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending_since: Option<TimestampMs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_observed_at: Option<TimestampMs>,
    },
    /// Timestamp of the last allowed event and the last observation.
    Cooldown {
        #[serde(skip_serializing_if = "Option::is_none")]
        last_allowed_at: Option<TimestampMs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_observed_at: Option<TimestampMs>,
    },
    /// Accepted events still in the rate window and the last observation.
    RateLimit {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        accepted_at: Vec<TimestampMs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_observed_at: Option<TimestampMs>,
    },
}

/// Values read, emitted, and staged by one node in evaluation order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeTrace {
    /// ID of the evaluated node.
    pub node: LocalId,
    /// Inputs by port, including the staged input of a delay.
    pub inputs: BTreeMap<LocalId, LiteralValue>,
    /// Outputs by port.
    pub outputs: BTreeMap<LocalId, LiteralValue>,
    /// Delay value used by this occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_before: Option<LiteralValue>,
    /// Delay value staged for the next occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staged_state_update: Option<LiteralValue>,
    /// Control state read by this occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_state_before: Option<ControlState>,
    /// Control state staged for the next occurrence, when the node advanced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_control_state_update: Option<ControlState>,
}

/// A decimal operation whose result used the declared rounding rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundingNote {
    /// Node that evaluated the operation.
    pub node: LocalId,
    /// JSON pointer to the expression on the node.
    pub path: FailurePath,
    /// Operation that produced the rounded result.
    pub operator: BinaryOperator,
    /// Requested decimal scale.
    pub scale: u8,
    /// Rule used to round the result.
    pub rounding: Rounding,
    /// Rounded decimal result.
    pub result: DecimalValue,
}

/// Successful evaluation with evidence and the state to stage for the next occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Revision accepted by static proof.
    pub revision_hash: CanonicalHash,
    /// Values exposed by the program's declared outputs.
    pub outputs: BTreeMap<LocalId, LiteralValue>,
    /// Node records in proof evaluation order.
    pub trace: Vec<NodeTrace>,
    /// Delay state to use in the next occurrence.
    pub state_updates: BTreeMap<LocalId, LiteralValue>,
    /// Control states to use in the next occurrence.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub control_state_updates: BTreeMap<LocalId, ControlState>,
    /// Inexact arithmetic operations in the order they were evaluated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounding: Vec<RoundingNote>,
    /// Deterministic work consumed by the occurrence.
    pub fuel_used: u64,
}

/// Evaluates a program against frozen inputs and stages updates for the next occurrence.
///
/// # Errors
///
/// Returns a typed failure when proof, inputs, state, arithmetic, or evaluation limits
/// prevent the occurrence from completing.
pub fn evaluate_program(
    program: &ProgramAst,
    context: &FrozenEvaluationContext,
    limits: EvaluationLimits,
) -> Result<EvaluationResult, EvaluationError> {
    let proof = prove_program(program);
    if proof.status != ProofStatus::Accepted {
        return Err(EvaluationError {
            code: EvaluationErrorCode::ProofRejected,
            message: format!("program has {} Proof issue(s)", proof.issues.len()),
            path: FailurePath::root(),
            node: None,
            fuel_used: 0,
        });
    }
    let revision_hash = proof.revision_hash.ok_or_else(|| EvaluationError {
        code: EvaluationErrorCode::InvariantViolation,
        message: "accepted Proof has no revision hash".to_string(),
        path: FailurePath::root(),
        node: None,
        fuel_used: 0,
    })?;
    let mut meter = FuelMeter::new(limits);
    let mut trace = Vec::with_capacity(proof.evaluation_order.len());
    let mut index = BTreeMap::new();
    let mut control_state_updates = BTreeMap::new();

    for node_id in proof.evaluation_order {
        let site = Site::node(&node_id);
        meter.consume(site)?;
        let (key, node) = program
            .nodes
            .get_key_value(&node_id)
            .ok_or_else(|| meter.invariant(site, "Proof order contains an unknown node"))?;
        let mut inputs = BTreeMap::new();
        if !node.operation.is_state_boundary() {
            for (name, binding) in &node.inputs {
                let value = traced_value(&trace, &index, &binding.source).ok_or_else(|| {
                    meter.invariant(
                        site.at(Path::InputSource(name)),
                        "Proof order did not make an input source available",
                    )
                })?;
                ensure_type(value, &binding.expected_type, site, &meter)?;
                inputs.insert(name.clone(), value.clone());
            }
        }
        let evaluated = evaluate_node(program, node, &inputs, context, site, &mut meter)?;
        for (name, value) in &evaluated.outputs {
            let contract = node
                .outputs
                .get(name)
                .ok_or_else(|| meter.invariant(site, "Proof accepted an undeclared node output"))?;
            ensure_type(value, &contract.value_type, site, &meter)?;
        }
        index.insert(key, trace.len());
        if let Some(update) = &evaluated.control_state_update {
            control_state_updates.insert(node_id.clone(), update.clone());
        }
        trace.push(NodeTrace {
            node: node_id,
            inputs,
            outputs: evaluated.outputs,
            state_before: evaluated.delay_state_before,
            staged_state_update: None,
            control_state_before: evaluated.control_state_before,
            staged_control_state_update: evaluated.control_state_update,
        });
    }

    let mut state_updates = BTreeMap::new();
    for (node_id, node) in &program.nodes {
        let NodeOperation::Delay { input, .. } = &node.operation else {
            continue;
        };
        let site = Site::node(node_id);
        let binding = node
            .inputs
            .get(input)
            .ok_or_else(|| meter.invariant(site, "Proof accepted a delay without an input"))?;
        let value = traced_value(&trace, &index, &binding.source).ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::MissingStateSource,
                site.at(Path::DelayInput),
                "delay update source did not produce a value",
            )
        })?;
        ensure_type(value, &binding.expected_type, site, &meter)?;
        let update = value.clone();
        let item = index
            .get(node_id)
            .and_then(|&slot| trace.get_mut(slot))
            .ok_or_else(|| meter.invariant(site, "Proof order did not include a delay"))?;
        state_updates.insert(node_id.clone(), update.clone());
        item.inputs.insert(input.clone(), update.clone());
        item.staged_state_update = Some(update);
    }

    let outputs = program
        .outputs
        .iter()
        .map(|(name, source)| {
            let value = traced_value(&trace, &index, source).ok_or_else(|| {
                meter.invariant(
                    Site::node(&source.node).at(Path::Output(name)),
                    "declared Program output was not evaluated",
                )
            })?;
            Ok((name.clone(), value.clone()))
        })
        .collect::<Result<_, EvaluationError>>()?;

    Ok(EvaluationResult {
        revision_hash,
        outputs,
        trace,
        state_updates,
        control_state_updates,
        rounding: meter.rounding,
        fuel_used: meter.used,
    })
}

fn traced_value<'a>(
    trace: &'a [NodeTrace],
    index: &BTreeMap<&LocalId, usize>,
    source: &OutputRef,
) -> Option<&'a LiteralValue> {
    trace
        .get(*index.get(&source.node)?)?
        .outputs
        .get(&source.port)
}

fn ensure_type(
    value: &LiteralValue,
    expected: &ValueType,
    site: Site<'_>,
    meter: &FuelMeter,
) -> Result<(), EvaluationError> {
    let message = match value.value_type() {
        Ok(actual) if &actual == expected => return Ok(()),
        Ok(actual) => format!("frozen value has type {actual:?}, expected {expected:?}"),
        Err(error) => error.to_string(),
    };
    Err(meter.error(EvaluationErrorCode::RuntimeTypeMismatch, site, message))
}
