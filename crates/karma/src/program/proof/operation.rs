use crate::{
    NodeOperation::{
        Cooldown, Debounce, Delay, Derive, Input, RateLimit, RouteCandidate, Threshold, Trigger,
    },
    ProgramAst, ValueType,
};

use super::{
    ProofIssueCode::{ExpressionTypeMismatch, InputTypeMismatch, InvalidLiteral},
    node::NodeValidation,
};

mod controls;
mod sources;

impl NodeValidation<'_> {
    pub(super) fn validate_operation(&mut self, program: &ProgramAst) {
        let node = self.node;
        let node_path = self.path;
        match &node.operation {
            Trigger { source, output } => {
                if !node.inputs.is_empty() || !node.outputs.contains_key(output) {
                    self.shape_issue("trigger must have no inputs and name one declared output");
                }
                if node
                    .outputs
                    .get(output)
                    .is_some_and(|contract| contract.value_type != ValueType::Bool)
                {
                    self.shape_issue(
                        "trigger output must be bool so occurrence pulses have one canonical type",
                    );
                }
                self.validate_trigger_source(source);
            }
            Input { source, output } => {
                if !node.inputs.is_empty() || !node.outputs.contains_key(output) {
                    self.shape_issue(
                        "input must have no input bindings and name one declared output",
                    );
                }
                self.validate_input_source(program, source, output);
            }
            Derive { expressions } => {
                if !node.outputs.keys().eq(expressions.keys()) {
                    self.shape_issue("derive expressions must exactly match declared output names");
                }
                for (output, expression) in expressions {
                    if let Some(actual) = self.infer_expression(expression, output)
                        && let Some(expected) = node.outputs.get(output)
                        && actual.as_ref() != &expected.value_type
                    {
                        self.expression_issue(
                            ExpressionTypeMismatch,
                            output,
                            format!(
                                "expression has type {actual:?}, output declares {:?}",
                                expected.value_type
                            ),
                        );
                    }
                }
            }
            Delay {
                input,
                output,
                initial,
                ..
            } => {
                let pair = if node.inputs.len() == 1 && node.outputs.len() == 1 {
                    node.inputs.get(input).zip(node.outputs.get(output))
                } else {
                    None
                };
                let Some((binding, declared_output)) = pair else {
                    self.shape_issue("delay must name its single declared input and output");
                    return;
                };
                if binding.expected_type != declared_output.value_type {
                    self.issue(
                        InputTypeMismatch,
                        format!("{node_path}/operation/input"),
                        "delay input and output types must match",
                    );
                }
                match initial.value_type() {
                    Ok(actual) if actual != declared_output.value_type => self.issue(
                        ExpressionTypeMismatch,
                        format!("{node_path}/operation/initial"),
                        "delay initial value must match its output type",
                    ),
                    Err(error) => self.issue(
                        InvalidLiteral,
                        format!("{node_path}/operation/initial"),
                        error.to_string(),
                    ),
                    Ok(_) => {}
                }
            }
            Threshold {
                input,
                active,
                entered,
                left,
                direction,
                enter,
                exit,
                ..
            } => self.validate_threshold(input, [active, entered, left], *direction, [enter, exit]),
            Debounce {
                input,
                stable,
                entered,
                left,
                for_at_least,
                ..
            } => {
                self.validate_boolean_control(input, &[stable, entered, left], "debounce");
                if for_at_least.get() < 0 {
                    self.shape_issue("debounce duration cannot be negative");
                }
            }
            Cooldown {
                input,
                allowed,
                cooldown,
                ..
            } => {
                self.validate_boolean_control(input, &[allowed], "cooldown");
                if cooldown.get() < 0 {
                    self.shape_issue("cooldown duration cannot be negative");
                }
            }
            RateLimit {
                input,
                allowed,
                max,
                window,
                ..
            } => {
                self.validate_boolean_control(input, &[allowed], "rate-limit");
                if *max == 0 {
                    self.shape_issue("rate-limit max must be non-zero");
                }
                if window.get() <= 0 {
                    self.shape_issue("rate-limit window must be positive");
                }
            }
            RouteCandidate {
                condition,
                output,
                route,
                template,
                fields,
            } => self.validate_candidate_route(condition, output, *route, template, fields),
        }
    }
}
