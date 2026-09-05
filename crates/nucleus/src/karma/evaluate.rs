use std::{cmp::Ordering, collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use super::{
    BinaryOperator, CanonicalHash, DatumState, DecimalPrecision, DecimalValue, DeclaredUnit,
    DurationMs, ExpressionAst, FailurePath, InputSource, LateEventPolicy, LiteralValue, LocalId,
    NodeOperation, OutputRef, ProgramAst, ProofStatus, RoundedDecimal, Rounding,
    ThresholdDirection, TimestampMs, UnaryOperator, ValueType, prove_program,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationErrorCode {
    ProofRejected,
    MissingInput,
    MissingParameter,
    MissingStateSource,
    RuntimeTypeMismatch,
    ArithmeticOverflow,
    DivisionByZero,
    FuelExhausted,
    ExpressionDepthExceeded,
    MissingLogicalTime,
    NonMonotonicLogicalTime,
    ReplayRequired,
    InvariantViolation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationError {
    pub code: EvaluationErrorCode,
    pub message: String,
    pub path: FailurePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<LocalId>,
    pub fuel_used: u64,
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at {}: {}", self.code, self.path, self.message)
    }
}

impl std::error::Error for EvaluationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationLimits {
    pub fuel: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FrozenEvaluationContext {
    #[serde(default)]
    pub boundary_values: BTreeMap<LocalId, LiteralValue>,
    #[serde(default)]
    pub parameter_values: BTreeMap<LocalId, LiteralValue>,
    #[serde(default)]
    pub delay_state: BTreeMap<LocalId, LiteralValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_at: Option<TimestampMs>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub control_state: BTreeMap<LocalId, ControlState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControlState {
    Threshold {
        active: bool,
    },
    Debounce {
        stable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pending_since: Option<TimestampMs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_observed_at: Option<TimestampMs>,
    },
    Cooldown {
        #[serde(skip_serializing_if = "Option::is_none")]
        last_allowed_at: Option<TimestampMs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_observed_at: Option<TimestampMs>,
    },
    RateLimit {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        accepted_at: Vec<TimestampMs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_observed_at: Option<TimestampMs>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeTrace {
    pub node: LocalId,
    pub inputs: BTreeMap<LocalId, LiteralValue>,
    pub outputs: BTreeMap<LocalId, LiteralValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_before: Option<LiteralValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staged_state_update: Option<LiteralValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_state_before: Option<ControlState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_control_state_update: Option<ControlState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundingNote {
    pub node: LocalId,
    pub path: FailurePath,
    pub operator: BinaryOperator,
    pub scale: u8,
    pub rounding: Rounding,
    pub result: DecimalValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub revision_hash: CanonicalHash,
    pub outputs: BTreeMap<LocalId, LiteralValue>,
    pub trace: Vec<NodeTrace>,
    pub state_updates: BTreeMap<LocalId, LiteralValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub control_state_updates: BTreeMap<LocalId, ControlState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounding: Vec<RoundingNote>,
    pub fuel_used: u64,
}

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
    let mut values = BTreeMap::<OutputRef, LiteralValue>::new();
    let mut trace = Vec::with_capacity(proof.evaluation_order.len());
    let mut trace_index = BTreeMap::<LocalId, usize>::new();
    let mut control_state_updates = BTreeMap::new();

    for node_id in &proof.evaluation_order {
        meter.consume(node_id, &node_path(node_id))?;
        let node = program
            .nodes
            .get(node_id)
            .expect("Proof order contains only known nodes");
        let mut inputs = BTreeMap::new();
        if !node.operation.is_state_boundary() {
            for (name, binding) in &node.inputs {
                let value = values.get(&binding.source).cloned().ok_or_else(|| {
                    meter.error(
                        EvaluationErrorCode::InvariantViolation,
                        node_id,
                        &format!(
                            "{}/inputs/{}/source",
                            node_path(node_id),
                            pointer_segment(name.as_str())
                        ),
                        "Proof order did not make an input source available",
                    )
                })?;
                ensure_type(&value, &binding.expected_type, node_id, &mut meter)?;
                inputs.insert(name.clone(), value);
            }
        }
        let evaluated = evaluate_node(program, node_id, node, &inputs, context, &mut meter)?;
        let outputs = evaluated.outputs;
        for (name, value) in &outputs {
            let contract = node.outputs.get(name).expect("Proof checked output shape");
            ensure_type(value, &contract.value_type, node_id, &mut meter)?;
            values.insert(
                OutputRef {
                    node: node_id.clone(),
                    port: name.clone(),
                },
                value.clone(),
            );
        }
        trace_index.insert(node_id.clone(), trace.len());
        if let Some(update) = &evaluated.control_state_update {
            control_state_updates.insert(node_id.clone(), update.clone());
        }
        trace.push(NodeTrace {
            node: node_id.clone(),
            inputs,
            outputs,
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
        let binding = node
            .inputs
            .get(input)
            .expect("Proof checked delay input shape");
        let update = values.get(&binding.source).cloned().ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::MissingStateSource,
                node_id,
                &format!("{}/operation/input", node_path(node_id)),
                "delay update source did not produce a value",
            )
        })?;
        ensure_type(&update, &binding.expected_type, node_id, &mut meter)?;
        state_updates.insert(node_id.clone(), update.clone());
        let item = trace
            .get_mut(*trace_index.get(node_id).expect("delay was traced"))
            .expect("trace index is valid");
        item.inputs.insert(input.clone(), update.clone());
        item.staged_state_update = Some(update);
    }

    let mut outputs = BTreeMap::new();
    for (name, source) in &program.outputs {
        let value = values.get(source).cloned().ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::InvariantViolation,
                &source.node,
                &format!("/outputs/{}", pointer_segment(name.as_str())),
                "declared Program output was not evaluated",
            )
        })?;
        outputs.insert(name.clone(), value);
    }
    Ok(EvaluationResult {
        revision_hash,
        outputs,
        trace,
        state_updates,
        control_state_updates,
        rounding: meter.rounding.take(),
        fuel_used: meter.used,
    })
}

struct EvaluatedNode {
    outputs: BTreeMap<LocalId, LiteralValue>,
    delay_state_before: Option<LiteralValue>,
    control_state_before: Option<ControlState>,
    control_state_update: Option<ControlState>,
}

impl EvaluatedNode {
    fn pure(outputs: BTreeMap<LocalId, LiteralValue>) -> Self {
        Self {
            outputs,
            delay_state_before: None,
            control_state_before: None,
            control_state_update: None,
        }
    }

    fn control(
        outputs: BTreeMap<LocalId, LiteralValue>,
        before: ControlState,
        update: Option<ControlState>,
    ) -> Self {
        Self {
            outputs,
            delay_state_before: None,
            control_state_before: Some(before),
            control_state_update: update,
        }
    }
}

fn evaluate_node(
    program: &ProgramAst,
    node_id: &LocalId,
    node: &super::NodeAst,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    context: &FrozenEvaluationContext,
    meter: &mut FuelMeter,
) -> Result<EvaluatedNode, EvaluationError> {
    match &node.operation {
        NodeOperation::Trigger { output, .. } => {
            let value = boundary_value(context, node_id, meter)?;
            Ok(EvaluatedNode::pure(BTreeMap::from([(
                output.clone(),
                value,
            )])))
        }
        NodeOperation::Input { source, output } => {
            let value = match source {
                InputSource::Parameter { parameter } => context
                    .parameter_values
                    .get(parameter)
                    .cloned()
                    .or_else(|| {
                        program
                            .parameters
                            .get(parameter)
                            .map(|definition| definition.default.clone())
                    })
                    .ok_or_else(|| {
                        meter.error(
                            EvaluationErrorCode::MissingParameter,
                            node_id,
                            &format!("{}/operation/source/parameter", node_path(node_id)),
                            "parameter has no frozen value or default",
                        )
                    })?,
                InputSource::RecordQuantity { .. }
                | InputSource::SavedProtein { .. }
                | InputSource::Signal { .. }
                | InputSource::SecretMetadata { .. }
                | InputSource::CapturedFact { .. } => boundary_value(context, node_id, meter)?,
            };
            Ok(EvaluatedNode::pure(BTreeMap::from([(
                output.clone(),
                value,
            )])))
        }
        NodeOperation::Derive { expressions } => {
            let mut outputs = BTreeMap::new();
            for (name, expression) in expressions {
                let path = format!(
                    "{}/operation/expressions/{}",
                    node_path(node_id),
                    pointer_segment(name.as_str())
                );
                outputs.insert(
                    name.clone(),
                    evaluate_expression(expression, inputs, node_id, &path, 0, meter)?,
                );
            }
            Ok(EvaluatedNode::pure(outputs))
        }
        NodeOperation::Delay {
            output, initial, ..
        } => {
            let state = context
                .delay_state
                .get(node_id)
                .cloned()
                .unwrap_or_else(|| initial.clone());
            Ok(EvaluatedNode {
                outputs: BTreeMap::from([(output.clone(), state.clone())]),
                delay_state_before: Some(state),
                control_state_before: None,
                control_state_update: None,
            })
        }
        NodeOperation::Threshold {
            input,
            active,
            entered,
            left,
            direction,
            enter,
            exit,
            initial_active,
            ..
        } => evaluate_threshold(
            node_id,
            inputs,
            context,
            input,
            active,
            entered,
            left,
            *direction,
            enter,
            exit,
            *initial_active,
            meter,
        ),
        NodeOperation::Debounce {
            input,
            stable,
            entered,
            left,
            for_at_least,
            initial,
            state,
        } => evaluate_debounce(
            node_id,
            inputs,
            context,
            input,
            stable,
            entered,
            left,
            *for_at_least,
            *initial,
            state.late_event,
            meter,
        ),
        NodeOperation::Cooldown {
            input,
            allowed,
            cooldown,
            state,
        } => evaluate_cooldown(
            node_id,
            inputs,
            context,
            input,
            allowed,
            *cooldown,
            state.late_event,
            meter,
        ),
        NodeOperation::RateLimit {
            input,
            allowed,
            max,
            window,
            state,
        } => evaluate_rate_limit(
            node_id,
            inputs,
            context,
            input,
            allowed,
            *max,
            *window,
            state.late_event,
            meter,
        ),
        NodeOperation::RouteCandidate {
            condition,
            output,
            route,
            template,
            fields,
        } => {
            let condition = bool_input(inputs, condition, node_id, meter)?;
            let candidate_type = ValueType::Candidate {
                route: *route,
                template: template.clone(),
                fields: fields
                    .iter()
                    .map(|(field, input)| {
                        let value = inputs.get(input).expect("Proof checked candidate input");
                        Ok((
                            field.clone(),
                            value.value_type().map_err(|error| {
                                meter.error(
                                    EvaluationErrorCode::InvariantViolation,
                                    node_id,
                                    &node_path(node_id),
                                    error.to_string(),
                                )
                            })?,
                        ))
                    })
                    .collect::<Result<_, EvaluationError>>()?,
            };
            let value = condition.then(|| {
                Box::new(LiteralValue::Candidate {
                    route: *route,
                    template: template.clone(),
                    fields: fields
                        .iter()
                        .map(|(field, input)| {
                            (
                                field.clone(),
                                inputs
                                    .get(input)
                                    .expect("Proof checked candidate input")
                                    .clone(),
                            )
                        })
                        .collect(),
                })
            });
            Ok(EvaluatedNode::pure(BTreeMap::from([(
                output.clone(),
                LiteralValue::Datum {
                    value_type: Box::new(candidate_type),
                    state: if condition {
                        DatumState::Value
                    } else {
                        DatumState::Missing
                    },
                    value,
                },
            )])))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_threshold(
    node_id: &LocalId,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    context: &FrozenEvaluationContext,
    input: &LocalId,
    active_output: &LocalId,
    entered_output: &LocalId,
    left_output: &LocalId,
    direction: ThresholdDirection,
    enter: &LiteralValue,
    exit: &LiteralValue,
    initial_active: bool,
    meter: &FuelMeter,
) -> Result<EvaluatedNode, EvaluationError> {
    let before = match context.control_state.get(node_id) {
        None => ControlState::Threshold {
            active: initial_active,
        },
        Some(state @ ControlState::Threshold { .. }) => state.clone(),
        Some(_) => return Err(control_state_type_error(node_id, "threshold", meter)),
    };
    let ControlState::Threshold { active: was_active } = before else {
        unreachable!()
    };
    let value = inputs.get(input).expect("Proof checked threshold input");
    let next_active = match (direction, was_active) {
        (ThresholdDirection::Above, false) => value
            .semantic_cmp(enter)
            .is_some_and(|ordering| ordering != Ordering::Less),
        (ThresholdDirection::Above, true) => !value
            .semantic_cmp(exit)
            .is_some_and(|ordering| ordering != Ordering::Greater),
        (ThresholdDirection::Below, false) => value
            .semantic_cmp(enter)
            .is_some_and(|ordering| ordering != Ordering::Greater),
        (ThresholdDirection::Below, true) => !value
            .semantic_cmp(exit)
            .is_some_and(|ordering| ordering != Ordering::Less),
    };
    let update = ControlState::Threshold {
        active: next_active,
    };
    Ok(EvaluatedNode::control(
        bool_transition_outputs(
            active_output,
            entered_output,
            left_output,
            next_active,
            !was_active && next_active,
            was_active && !next_active,
        ),
        ControlState::Threshold { active: was_active },
        Some(update),
    ))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_debounce(
    node_id: &LocalId,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    context: &FrozenEvaluationContext,
    input: &LocalId,
    stable_output: &LocalId,
    entered_output: &LocalId,
    left_output: &LocalId,
    for_at_least: DurationMs,
    initial: bool,
    late_event: LateEventPolicy,
    meter: &FuelMeter,
) -> Result<EvaluatedNode, EvaluationError> {
    let before = match context.control_state.get(node_id) {
        None => ControlState::Debounce {
            stable: initial,
            pending: None,
            pending_since: None,
            last_observed_at: None,
        },
        Some(state @ ControlState::Debounce { .. }) => state.clone(),
        Some(_) => return Err(control_state_type_error(node_id, "debounce", meter)),
    };
    let ControlState::Debounce {
        stable: was_stable,
        pending,
        pending_since,
        last_observed_at,
    } = before.clone()
    else {
        unreachable!()
    };
    let pending_shape_valid = pending.is_some() == pending_since.is_some()
        && pending != Some(was_stable)
        && match (pending_since, last_observed_at) {
            (Some(since), Some(last)) => since <= last,
            (Some(_), None) => false,
            _ => true,
        };
    if !pending_shape_valid {
        return Err(meter.error(
            EvaluationErrorCode::RuntimeTypeMismatch,
            node_id,
            &node_path(node_id),
            "debounce control state is internally inconsistent",
        ));
    }
    let now = logical_time(context, node_id, meter)?;
    if late_disposition(now, last_observed_at, late_event, node_id, meter)?
        == LateDisposition::Ignore
    {
        return Ok(EvaluatedNode::control(
            bool_transition_outputs(
                stable_output,
                entered_output,
                left_output,
                was_stable,
                false,
                false,
            ),
            before,
            None,
        ));
    }
    let observed = bool_input(inputs, input, node_id, meter)?;
    let (stable, pending, pending_since) = if observed == was_stable {
        (was_stable, None, None)
    } else if pending != Some(observed) {
        if for_at_least == DurationMs::ZERO {
            (observed, None, None)
        } else {
            (was_stable, Some(observed), Some(now))
        }
    } else {
        let since = pending_since.ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::RuntimeTypeMismatch,
                node_id,
                &node_path(node_id),
                "debounce pending state has no pending_since timestamp",
            )
        })?;
        if elapsed_since(now, since, node_id, meter)? >= for_at_least.get() {
            (observed, None, None)
        } else {
            (was_stable, pending, Some(since))
        }
    };
    let update = ControlState::Debounce {
        stable,
        pending,
        pending_since,
        last_observed_at: Some(now),
    };
    Ok(EvaluatedNode::control(
        bool_transition_outputs(
            stable_output,
            entered_output,
            left_output,
            stable,
            !was_stable && stable,
            was_stable && !stable,
        ),
        before,
        Some(update),
    ))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_cooldown(
    node_id: &LocalId,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    context: &FrozenEvaluationContext,
    input: &LocalId,
    allowed_output: &LocalId,
    cooldown: DurationMs,
    late_event: LateEventPolicy,
    meter: &FuelMeter,
) -> Result<EvaluatedNode, EvaluationError> {
    let before = match context.control_state.get(node_id) {
        None => ControlState::Cooldown {
            last_allowed_at: None,
            last_observed_at: None,
        },
        Some(state @ ControlState::Cooldown { .. }) => state.clone(),
        Some(_) => return Err(control_state_type_error(node_id, "cooldown", meter)),
    };
    let ControlState::Cooldown {
        last_allowed_at,
        last_observed_at,
    } = before.clone()
    else {
        unreachable!()
    };
    if match (last_allowed_at, last_observed_at) {
        (Some(allowed), Some(observed)) => allowed > observed,
        (Some(_), None) => true,
        _ => false,
    } {
        return Err(meter.error(
            EvaluationErrorCode::RuntimeTypeMismatch,
            node_id,
            &node_path(node_id),
            "cooldown control state is internally inconsistent",
        ));
    }
    let now = logical_time(context, node_id, meter)?;
    if late_disposition(now, last_observed_at, late_event, node_id, meter)?
        == LateDisposition::Ignore
    {
        return Ok(EvaluatedNode::control(
            BTreeMap::from([(allowed_output.clone(), LiteralValue::Bool { value: false })]),
            before,
            None,
        ));
    }
    let fired = bool_input(inputs, input, node_id, meter)?;
    let eligible = match last_allowed_at {
        None => true,
        Some(previous) => elapsed_since(now, previous, node_id, meter)? >= cooldown.get(),
    };
    let allowed = fired && eligible;
    let update = ControlState::Cooldown {
        last_allowed_at: if allowed { Some(now) } else { last_allowed_at },
        last_observed_at: Some(now),
    };
    Ok(EvaluatedNode::control(
        BTreeMap::from([(
            allowed_output.clone(),
            LiteralValue::Bool { value: allowed },
        )]),
        before,
        Some(update),
    ))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_rate_limit(
    node_id: &LocalId,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    context: &FrozenEvaluationContext,
    input: &LocalId,
    allowed_output: &LocalId,
    max: u32,
    window: DurationMs,
    late_event: LateEventPolicy,
    meter: &FuelMeter,
) -> Result<EvaluatedNode, EvaluationError> {
    let before = match context.control_state.get(node_id) {
        None => ControlState::RateLimit {
            accepted_at: Vec::new(),
            last_observed_at: None,
        },
        Some(state @ ControlState::RateLimit { .. }) => state.clone(),
        Some(_) => return Err(control_state_type_error(node_id, "rate-limit", meter)),
    };
    let ControlState::RateLimit {
        accepted_at,
        last_observed_at,
    } = before.clone()
    else {
        unreachable!()
    };
    if accepted_at.len() > max as usize
        || accepted_at.windows(2).any(|pair| pair[0] > pair[1])
        || match (accepted_at.last(), last_observed_at) {
            (Some(accepted), Some(observed)) => accepted > &observed,
            (Some(_), None) => true,
            _ => false,
        }
    {
        return Err(meter.error(
            EvaluationErrorCode::RuntimeTypeMismatch,
            node_id,
            &node_path(node_id),
            "rate-limit control state is internally inconsistent",
        ));
    }
    let now = logical_time(context, node_id, meter)?;
    if late_disposition(now, last_observed_at, late_event, node_id, meter)?
        == LateDisposition::Ignore
    {
        return Ok(EvaluatedNode::control(
            BTreeMap::from([(allowed_output.clone(), LiteralValue::Bool { value: false })]),
            before,
            None,
        ));
    }
    let mut retained = Vec::with_capacity(accepted_at.len().min(max as usize));
    for accepted in accepted_at {
        if elapsed_since(now, accepted, node_id, meter)? < window.get() {
            retained.push(accepted);
        }
    }
    let fired = bool_input(inputs, input, node_id, meter)?;
    let allowed = fired && retained.len() < max as usize;
    if allowed {
        retained.push(now);
    }
    let update = ControlState::RateLimit {
        accepted_at: retained,
        last_observed_at: Some(now),
    };
    Ok(EvaluatedNode::control(
        BTreeMap::from([(
            allowed_output.clone(),
            LiteralValue::Bool { value: allowed },
        )]),
        before,
        Some(update),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LateDisposition {
    Proceed,
    Ignore,
}

fn late_disposition(
    now: TimestampMs,
    previous: Option<TimestampMs>,
    policy: LateEventPolicy,
    node: &LocalId,
    meter: &FuelMeter,
) -> Result<LateDisposition, EvaluationError> {
    if previous.is_none_or(|previous| now >= previous) {
        return Ok(LateDisposition::Proceed);
    }
    match policy {
        LateEventPolicy::Ignore => Ok(LateDisposition::Ignore),
        LateEventPolicy::Reject => Err(meter.error(
            EvaluationErrorCode::NonMonotonicLogicalTime,
            node,
            &node_path(node),
            "logical time precedes the control state's last observation",
        )),
        LateEventPolicy::Recompute | LateEventPolicy::Compensate => Err(meter.error(
            EvaluationErrorCode::ReplayRequired,
            node,
            &node_path(node),
            "late-event policy requires ordered occurrence replay",
        )),
    }
}

fn logical_time(
    context: &FrozenEvaluationContext,
    node: &LocalId,
    meter: &FuelMeter,
) -> Result<TimestampMs, EvaluationError> {
    context.logical_at.ok_or_else(|| {
        meter.error(
            EvaluationErrorCode::MissingLogicalTime,
            node,
            &node_path(node),
            "temporal control requires a frozen logical timestamp",
        )
    })
}

fn elapsed_since(
    now: TimestampMs,
    previous: TimestampMs,
    node: &LocalId,
    meter: &FuelMeter,
) -> Result<i64, EvaluationError> {
    now.as_millis()
        .checked_sub(previous.as_millis())
        .filter(|elapsed| *elapsed >= 0)
        .ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::RuntimeTypeMismatch,
                node,
                &node_path(node),
                "control state contains a future or unrepresentable timestamp",
            )
        })
}

fn bool_input(
    inputs: &BTreeMap<LocalId, LiteralValue>,
    input: &LocalId,
    node: &LocalId,
    meter: &FuelMeter,
) -> Result<bool, EvaluationError> {
    match inputs.get(input) {
        Some(LiteralValue::Bool { value }) => Ok(*value),
        _ => Err(meter.error(
            EvaluationErrorCode::InvariantViolation,
            node,
            &node_path(node),
            "Proof accepted a missing or non-Boolean control input",
        )),
    }
}

fn bool_transition_outputs(
    stable: &LocalId,
    entered: &LocalId,
    left: &LocalId,
    stable_value: bool,
    entered_value: bool,
    left_value: bool,
) -> BTreeMap<LocalId, LiteralValue> {
    BTreeMap::from([
        (
            stable.clone(),
            LiteralValue::Bool {
                value: stable_value,
            },
        ),
        (
            entered.clone(),
            LiteralValue::Bool {
                value: entered_value,
            },
        ),
        (left.clone(), LiteralValue::Bool { value: left_value }),
    ])
}

fn control_state_type_error(node: &LocalId, expected: &str, meter: &FuelMeter) -> EvaluationError {
    meter.error(
        EvaluationErrorCode::RuntimeTypeMismatch,
        node,
        &node_path(node),
        format!("control state does not match {expected} operation"),
    )
}

fn boundary_value(
    context: &FrozenEvaluationContext,
    node_id: &LocalId,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    context
        .boundary_values
        .get(node_id)
        .cloned()
        .ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::MissingInput,
                node_id,
                &node_path(node_id),
                "frozen boundary value is missing",
            )
        })
}

fn evaluate_expression(
    expression: &ExpressionAst,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    node_id: &LocalId,
    path: &str,
    depth: u16,
    meter: &mut FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    if depth >= meter.limits.max_expression_depth {
        return Err(meter.error(
            EvaluationErrorCode::ExpressionDepthExceeded,
            node_id,
            path,
            "expression exceeds deterministic depth limit",
        ));
    }
    meter.consume(node_id, path)?;
    match expression {
        ExpressionAst::Literal { value } => Ok(value.clone()),
        ExpressionAst::Input { input } => inputs.get(input).cloned().ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::InvariantViolation,
                node_id,
                path,
                "Proof accepted an unavailable expression input",
            )
        }),
        ExpressionAst::Unary { operator, value } => {
            let value = evaluate_expression(value, inputs, node_id, path, depth + 1, meter)?;
            evaluate_unary(*operator, value, node_id, path, meter)
        }
        ExpressionAst::Binary {
            operator,
            left,
            right,
            precision,
        } => {
            let left = evaluate_expression(left, inputs, node_id, path, depth + 1, meter)?;
            let right = evaluate_expression(right, inputs, node_id, path, depth + 1, meter)?;
            evaluate_binary(
                *operator,
                left,
                right,
                precision.as_ref(),
                node_id,
                path,
                meter,
            )
        }
        ExpressionAst::If {
            condition,
            then_value,
            else_value,
        } => {
            let condition =
                evaluate_expression(condition, inputs, node_id, path, depth + 1, meter)?;
            match condition {
                LiteralValue::Bool { value: true } => {
                    evaluate_expression(then_value, inputs, node_id, path, depth + 1, meter)
                }
                LiteralValue::Bool { value: false } => {
                    evaluate_expression(else_value, inputs, node_id, path, depth + 1, meter)
                }
                _ => Err(meter.error(
                    EvaluationErrorCode::InvariantViolation,
                    node_id,
                    path,
                    "Proof accepted a non-Boolean if condition",
                )),
            }
        }
    }
}

fn evaluate_unary(
    operator: UnaryOperator,
    value: LiteralValue,
    node: &LocalId,
    path: &str,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    let overflow = || {
        meter.error(
            EvaluationErrorCode::ArithmeticOverflow,
            node,
            path,
            "exact unary arithmetic overflowed",
        )
    };
    match (operator, value) {
        (UnaryOperator::Not, LiteralValue::Bool { value }) => {
            Ok(LiteralValue::Bool { value: !value })
        }
        (UnaryOperator::Negate, LiteralValue::I64 { value }) => value
            .checked_neg()
            .map(|value| LiteralValue::I64 { value })
            .ok_or_else(overflow),
        (UnaryOperator::Negate, LiteralValue::Decimal { value }) => value
            .checked_neg()
            .map(|value| LiteralValue::Decimal { value })
            .ok_or_else(overflow),
        (UnaryOperator::Negate, LiteralValue::Duration { value }) => value
            .get()
            .checked_neg()
            .map(|value| LiteralValue::Duration {
                value: DurationMs::new(value),
            })
            .ok_or_else(overflow),
        (UnaryOperator::Negate, LiteralValue::Quantity { amount, unit }) => amount
            .checked_neg()
            .map(|amount| LiteralValue::Quantity { amount, unit })
            .ok_or_else(overflow),
        _ => Err(meter.error(
            EvaluationErrorCode::InvariantViolation,
            node,
            path,
            "Proof accepted an invalid unary expression",
        )),
    }
}

fn evaluate_binary(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: Option<&DecimalPrecision>,
    node: &LocalId,
    path: &str,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    match operator {
        BinaryOperator::Equal => Ok(LiteralValue::Bool {
            value: semantic_equal(&left, &right),
        }),
        BinaryOperator::NotEqual => Ok(LiteralValue::Bool {
            value: !semantic_equal(&left, &right),
        }),
        BinaryOperator::Less
        | BinaryOperator::LessOrEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterOrEqual => {
            let ordering = semantic_cmp(&left, &right).ok_or_else(|| {
                meter.error(
                    EvaluationErrorCode::InvariantViolation,
                    node,
                    path,
                    "Proof accepted values without a semantic ordering",
                )
            })?;
            let value = match operator {
                BinaryOperator::Less => ordering == Ordering::Less,
                BinaryOperator::LessOrEqual => ordering != Ordering::Greater,
                BinaryOperator::Greater => ordering == Ordering::Greater,
                BinaryOperator::GreaterOrEqual => ordering != Ordering::Less,
                _ => unreachable!(),
            };
            Ok(LiteralValue::Bool { value })
        }
        BinaryOperator::And | BinaryOperator::Or => match (left, right) {
            (LiteralValue::Bool { value: left }, LiteralValue::Bool { value: right }) => {
                Ok(LiteralValue::Bool {
                    value: if operator == BinaryOperator::And {
                        left && right
                    } else {
                        left || right
                    },
                })
            }
            _ => invariant_expression(node, path, meter),
        },
        BinaryOperator::Add | BinaryOperator::Subtract => {
            evaluate_add_sub(operator, left, right, node, path, meter)
        }
        BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Remainder => {
            evaluate_product(operator, left, right, precision, node, path, meter)
        }
    }
}

fn evaluate_add_sub(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    node: &LocalId,
    path: &str,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    let add = operator == BinaryOperator::Add;
    let overflow = || arithmetic_overflow(node, path, meter);
    match (left, right) {
        (LiteralValue::I64 { value: left }, LiteralValue::I64 { value: right }) => if add {
            left.checked_add(right)
        } else {
            left.checked_sub(right)
        }
        .map(|value| LiteralValue::I64 { value })
        .ok_or_else(overflow),
        (LiteralValue::Decimal { value: left }, LiteralValue::Decimal { value: right }) => if add {
            left.checked_add(right)
        } else {
            left.checked_sub(right)
        }
        .map(|value| LiteralValue::Decimal { value })
        .ok_or_else(overflow),
        (LiteralValue::Duration { value: left }, LiteralValue::Duration { value: right }) => {
            let value = if add {
                left.get().checked_add(right.get())
            } else {
                left.get().checked_sub(right.get())
            };
            value
                .map(|value| LiteralValue::Duration {
                    value: DurationMs::new(value),
                })
                .ok_or_else(overflow)
        }
        (
            LiteralValue::Quantity { amount: left, unit },
            LiteralValue::Quantity { amount: right, .. },
        ) => if add {
            left.checked_add(right)
        } else {
            left.checked_sub(right)
        }
        .map(|amount| LiteralValue::Quantity { amount, unit })
        .ok_or_else(overflow),
        (
            LiteralValue::Timestamp { value: timestamp },
            LiteralValue::Duration { value: duration },
        ) => {
            let duration = if add {
                Some(duration)
            } else {
                duration.get().checked_neg().map(DurationMs::new)
            }
            .ok_or_else(overflow)?;
            timestamp
                .checked_add(duration)
                .map(|value| LiteralValue::Timestamp { value })
                .ok_or_else(overflow)
        }
        (
            LiteralValue::Duration { value: duration },
            LiteralValue::Timestamp { value: timestamp },
        ) if add => timestamp
            .checked_add(duration)
            .map(|value| LiteralValue::Timestamp { value })
            .ok_or_else(overflow),
        (LiteralValue::Timestamp { value: left }, LiteralValue::Timestamp { value: right })
            if !add =>
        {
            left.as_millis()
                .checked_sub(right.as_millis())
                .map(|value| LiteralValue::Duration {
                    value: DurationMs::new(value),
                })
                .ok_or_else(overflow)
        }
        _ => invariant_expression(node, path, meter),
    }
}

fn evaluate_product(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: Option<&DecimalPrecision>,
    node: &LocalId,
    path: &str,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    if let Some(precision) = precision {
        return evaluate_exact_product(operator, left, right, precision, node, path, meter);
    }
    match (left, right) {
        (LiteralValue::I64 { value: left }, LiteralValue::I64 { value: right }) => {
            if matches!(operator, BinaryOperator::Divide | BinaryOperator::Remainder) && right == 0
            {
                return Err(meter.error(
                    EvaluationErrorCode::DivisionByZero,
                    node,
                    path,
                    "integer division or remainder by zero",
                ));
            }
            let value = match operator {
                BinaryOperator::Multiply => left.checked_mul(right),
                BinaryOperator::Divide => left.checked_div(right),
                BinaryOperator::Remainder => left.checked_rem(right),
                _ => unreachable!(),
            };
            value
                .map(|value| LiteralValue::I64 { value })
                .ok_or_else(|| arithmetic_overflow(node, path, meter))
        }
        (LiteralValue::Duration { value: left }, LiteralValue::Duration { value: right })
            if operator == BinaryOperator::Remainder =>
        {
            if right.get() == 0 {
                return Err(meter.error(
                    EvaluationErrorCode::DivisionByZero,
                    node,
                    path,
                    "duration remainder by zero",
                ));
            }
            left.get()
                .checked_rem(right.get())
                .map(|value| LiteralValue::Duration {
                    value: DurationMs::new(value),
                })
                .ok_or_else(|| arithmetic_overflow(node, path, meter))
        }
        _ => invariant_expression(node, path, meter),
    }
}

fn evaluate_exact_product(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: &DecimalPrecision,
    node: &LocalId,
    path: &str,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    let (Some(left_amount), Some(right_amount)) = (exact_amount(&left), exact_amount(&right))
    else {
        return invariant_expression(node, path, meter);
    };
    if operator == BinaryOperator::Remainder {
        return invariant_expression(node, path, meter);
    }
    if operator == BinaryOperator::Divide && right_amount.is_zero() {
        return Err(meter.error(
            EvaluationErrorCode::DivisionByZero,
            node,
            path,
            "division by zero",
        ));
    }

    let RoundedDecimal { value, exact } = if operator == BinaryOperator::Multiply {
        left_amount.mul_exact(right_amount, precision.scale, precision.rounding)
    } else {
        left_amount.div_exact(right_amount, precision.scale, precision.rounding)
    }
    .ok_or_else(|| arithmetic_overflow(node, path, meter))?;

    if !exact {
        meter.note_rounding(node, path, operator, precision, value);
    }
    Ok(exact_literal(&left, &right, operator, precision, value))
}

fn exact_amount(value: &LiteralValue) -> Option<DecimalValue> {
    match value {
        LiteralValue::Decimal { value } => Some(*value),
        LiteralValue::Quantity { amount, .. } => Some(*amount),
        _ => None,
    }
}

fn exact_literal(
    left: &LiteralValue,
    right: &LiteralValue,
    operator: BinaryOperator,
    precision: &DecimalPrecision,
    amount: DecimalValue,
) -> LiteralValue {
    let multiply = operator == BinaryOperator::Multiply;
    match (left, right) {
        (LiteralValue::Quantity { unit, .. }, LiteralValue::Decimal { .. }) => {
            LiteralValue::Quantity {
                amount,
                unit: unit.clone(),
            }
        }
        (LiteralValue::Decimal { .. }, LiteralValue::Quantity { unit, .. }) if multiply => {
            LiteralValue::Quantity {
                amount,
                unit: unit.clone(),
            }
        }
        _ => match &precision.result_unit {
            Some(DeclaredUnit::Unit { unit }) => LiteralValue::Quantity {
                amount,
                unit: unit.clone(),
            },
            _ => LiteralValue::Decimal { value: amount },
        },
    }
}

fn semantic_equal(left: &LiteralValue, right: &LiteralValue) -> bool {
    match (left, right) {
        (LiteralValue::Reference { value: left }, LiteralValue::Reference { value: right }) => {
            left.target == right.target
        }
        (
            LiteralValue::Datum {
                state: left_state,
                value: left_value,
                value_type: left_type,
            },
            LiteralValue::Datum {
                state: right_state,
                value: right_value,
                value_type: right_type,
            },
        ) => {
            left_state == right_state
                && left_type == right_type
                && match (left_value, right_value) {
                    (Some(left), Some(right)) => semantic_equal(left, right),
                    (None, None) => true,
                    _ => false,
                }
        }
        _ => left == right,
    }
}

fn semantic_cmp(left: &LiteralValue, right: &LiteralValue) -> Option<Ordering> {
    match (left, right) {
        (LiteralValue::I64 { value: left }, LiteralValue::I64 { value: right }) => {
            Some(left.cmp(right))
        }
        (LiteralValue::Decimal { value: left }, LiteralValue::Decimal { value: right }) => {
            Some(left.cmp(right))
        }
        (LiteralValue::Probability { value: left }, LiteralValue::Probability { value: right }) => {
            Some(left.cmp(right))
        }
        (LiteralValue::Confidence { value: left }, LiteralValue::Confidence { value: right }) => {
            Some(left.cmp(right))
        }
        (LiteralValue::Text { value: left }, LiteralValue::Text { value: right }) => {
            Some(left.cmp(right))
        }
        (LiteralValue::Duration { value: left }, LiteralValue::Duration { value: right }) => {
            Some(left.cmp(right))
        }
        (LiteralValue::Timestamp { value: left }, LiteralValue::Timestamp { value: right }) => {
            Some(left.cmp(right))
        }
        (
            LiteralValue::Quantity { amount: left, .. },
            LiteralValue::Quantity { amount: right, .. },
        ) => Some(left.cmp(right)),
        (LiteralValue::Reference { value: left }, LiteralValue::Reference { value: right }) => {
            Some(left.target.cmp(&right.target))
        }
        _ => None,
    }
}

fn ensure_type(
    value: &LiteralValue,
    expected: &ValueType,
    node: &LocalId,
    meter: &mut FuelMeter,
) -> Result<(), EvaluationError> {
    match value.value_type() {
        Ok(actual) if &actual == expected => Ok(()),
        Ok(actual) => Err(meter.error(
            EvaluationErrorCode::RuntimeTypeMismatch,
            node,
            &node_path(node),
            format!("frozen value has type {actual:?}, expected {expected:?}"),
        )),
        Err(error) => Err(meter.error(
            EvaluationErrorCode::RuntimeTypeMismatch,
            node,
            &node_path(node),
            error.to_string(),
        )),
    }
}

fn invariant_expression(
    node: &LocalId,
    path: &str,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    Err(meter.error(
        EvaluationErrorCode::InvariantViolation,
        node,
        path,
        "Proof accepted an invalid expression operation",
    ))
}

fn arithmetic_overflow(node: &LocalId, path: &str, meter: &FuelMeter) -> EvaluationError {
    meter.error(
        EvaluationErrorCode::ArithmeticOverflow,
        node,
        path,
        "exact arithmetic overflowed",
    )
}

struct FuelMeter {
    limits: EvaluationLimits,
    used: u64,
    rounding: std::cell::RefCell<Vec<RoundingNote>>,
}

impl FuelMeter {
    const fn new(limits: EvaluationLimits) -> Self {
        Self {
            limits,
            used: 0,
            rounding: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn note_rounding(
        &self,
        node: &LocalId,
        path: &str,
        operator: BinaryOperator,
        precision: &DecimalPrecision,
        result: DecimalValue,
    ) {
        self.rounding.borrow_mut().push(RoundingNote {
            node: node.clone(),
            path: FailurePath::new(path).expect("evaluator emits valid JSON pointers"),
            operator,
            scale: precision.scale,
            rounding: precision.rounding,
            result,
        });
    }

    fn consume(&mut self, node: &LocalId, path: &str) -> Result<(), EvaluationError> {
        if self.used >= self.limits.fuel {
            return Err(self.error(
                EvaluationErrorCode::FuelExhausted,
                node,
                path,
                "deterministic evaluation fuel exhausted",
            ));
        }
        self.used += 1;
        Ok(())
    }

    fn error(
        &self,
        code: EvaluationErrorCode,
        node: &LocalId,
        path: &str,
        message: impl Into<String>,
    ) -> EvaluationError {
        EvaluationError {
            code,
            message: message.into(),
            path: FailurePath::new(path).expect("evaluator emits valid JSON pointers"),
            node: Some(node.clone()),
            fuel_used: self.used,
        }
    }
}

fn node_path(node: &LocalId) -> String {
    format!("/nodes/{}", pointer_segment(node.as_str()))
}

fn pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
