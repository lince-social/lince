use std::collections::BTreeMap;

use crate::{
    DatumState, InputSource, LiteralValue, LocalId, NodeAst, NodeOperation, ProgramAst, ValueType,
};

use super::{
    ControlState, EvaluationError, EvaluationErrorCode, FrozenEvaluationContext, FuelMeter, Path,
    Site, control::Controls, expression::evaluate_expression,
};

pub(super) struct EvaluatedNode {
    pub(super) outputs: BTreeMap<LocalId, LiteralValue>,
    pub(super) delay_state_before: Option<LiteralValue>,
    pub(super) control_state_before: Option<ControlState>,
    pub(super) control_state_update: Option<ControlState>,
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

    fn single(port: &LocalId, value: LiteralValue) -> Self {
        Self::pure(BTreeMap::from([(port.clone(), value)]))
    }

    pub(super) fn control(
        outputs: BTreeMap<LocalId, LiteralValue>,
        before: ControlState,
        update: Option<ControlState>,
    ) -> Self {
        Self {
            control_state_before: Some(before),
            control_state_update: update,
            ..Self::pure(outputs)
        }
    }
}

pub(super) fn evaluate_node(
    program: &ProgramAst,
    node: &NodeAst,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    context: &FrozenEvaluationContext,
    site: Site<'_>,
    meter: &mut FuelMeter,
) -> Result<EvaluatedNode, EvaluationError> {
    let controls = Controls {
        inputs,
        context,
        site,
        meter,
    };
    match &node.operation {
        NodeOperation::Trigger { output, .. } => Ok(EvaluatedNode::single(
            output,
            boundary_value(context, site, meter)?,
        )),
        NodeOperation::Input { source, output } => {
            let value = match source {
                InputSource::Parameter { parameter } => context
                    .parameter_values
                    .get(parameter)
                    .or_else(|| {
                        program
                            .parameters
                            .get(parameter)
                            .map(|definition| &definition.default)
                    })
                    .ok_or_else(|| {
                        meter.error(
                            EvaluationErrorCode::MissingParameter,
                            site.at(Path::Parameter),
                            "parameter has no frozen value or default",
                        )
                    })?
                    .clone(),
                InputSource::RecordQuantity { .. }
                | InputSource::SavedProtein { .. }
                | InputSource::Signal { .. }
                | InputSource::CapturedFact { .. } => boundary_value(context, site, meter)?,
            };
            Ok(EvaluatedNode::single(output, value))
        }
        NodeOperation::Derive { expressions } => {
            let outputs = expressions
                .iter()
                .map(|(port, expression)| {
                    evaluate_expression(
                        expression,
                        inputs,
                        site.at(Path::Expression(port)),
                        0,
                        meter,
                    )
                    .map(|value| (port.clone(), value))
                })
                .collect::<Result<_, _>>()?;
            Ok(EvaluatedNode::pure(outputs))
        }
        NodeOperation::Delay {
            output, initial, ..
        } => {
            let state = context
                .delay_state
                .get(site.node)
                .unwrap_or(initial)
                .clone();
            Ok(EvaluatedNode {
                delay_state_before: Some(state.clone()),
                ..EvaluatedNode::single(output, state)
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
        } => controls.threshold(
            input,
            [active, entered, left],
            *direction,
            [enter, exit],
            *initial_active,
        ),
        NodeOperation::Debounce {
            input,
            stable,
            entered,
            left,
            for_at_least,
            initial,
            state,
        } => controls.debounce(
            input,
            [stable, entered, left],
            *for_at_least,
            *initial,
            state.late_event,
        ),
        NodeOperation::Cooldown {
            input,
            allowed,
            cooldown,
            state,
        } => controls.cooldown(input, allowed, *cooldown, state.late_event),
        NodeOperation::RateLimit {
            input,
            allowed,
            max,
            window,
            state,
        } => controls.rate_limit(input, allowed, *max, *window, state.late_event),
        NodeOperation::RouteCandidate {
            condition,
            output,
            route,
            template,
            fields,
        } => {
            let condition = bool_input(inputs, condition, site, meter)?;
            let mut field_types = BTreeMap::new();
            let mut candidate = BTreeMap::new();
            for (field, input) in fields {
                let value = inputs.get(input).ok_or_else(|| {
                    meter.invariant(site, "Proof accepted a missing candidate input")
                })?;
                let kind = value
                    .value_type()
                    .map_err(|error| meter.invariant(site, error.to_string()))?;
                field_types.insert(field.clone(), kind);
                if condition {
                    candidate.insert(field.clone(), value.clone());
                }
            }
            let value = condition.then(|| {
                Box::new(LiteralValue::Candidate {
                    route: *route,
                    template: template.clone(),
                    fields: candidate,
                })
            });
            Ok(EvaluatedNode::single(
                output,
                LiteralValue::Datum {
                    value_type: Box::new(ValueType::Candidate {
                        route: *route,
                        template: template.clone(),
                        fields: field_types,
                    }),
                    state: if condition {
                        DatumState::Value
                    } else {
                        DatumState::Missing
                    },
                    value,
                },
            ))
        }
    }
}

fn boundary_value(
    context: &FrozenEvaluationContext,
    site: Site<'_>,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    context
        .boundary_values
        .get(site.node)
        .cloned()
        .ok_or_else(|| {
            meter.error(
                EvaluationErrorCode::MissingInput,
                site,
                "frozen boundary value is missing",
            )
        })
}

pub(super) fn bool_input(
    inputs: &BTreeMap<LocalId, LiteralValue>,
    input: &LocalId,
    site: Site<'_>,
    meter: &FuelMeter,
) -> Result<bool, EvaluationError> {
    match inputs.get(input) {
        Some(LiteralValue::Bool { value }) => Ok(*value),
        _ => Err(meter.invariant(
            site,
            "Proof accepted a missing or non-Boolean control input",
        )),
    }
}
