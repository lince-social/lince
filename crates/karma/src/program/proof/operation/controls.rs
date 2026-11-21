use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use crate::{CandidateRoute, LiteralValue, LocalId, NodeAst, Slug, ThresholdDirection, ValueType};

use super::super::{
    ProofIssueCode::{InputTypeMismatch, InvalidLiteral},
    node::NodeValidation,
};

impl NodeValidation<'_> {
    pub(super) fn validate_threshold(
        &mut self,
        input: &LocalId,
        outputs: [&LocalId; 3],
        direction: ThresholdDirection,
        [enter, exit]: [&LiteralValue; 2],
    ) {
        let node = self.node;
        let node_path = self.path;
        let Some(binding) = node.inputs.get(input) else {
            self.shape_issue("threshold must name its single declared input");
            return;
        };
        if node.inputs.len() != 1 || !exact_outputs(node, &outputs) {
            self.shape_issue(
                "threshold must name one input and distinct active/entered/left outputs",
            );
        }
        if !binding.expected_type.is_ordered_scalar() {
            self.issue(
                InputTypeMismatch,
                format!("{node_path}/operation/input"),
                "threshold input must be an ordered scalar",
            );
        }
        self.validate_boolean_outputs(&outputs, "threshold");
        let enter_type = enter.value_type();
        let exit_type = exit.value_type();
        match (&enter_type, &exit_type) {
            (Ok(enter_type), Ok(exit_type))
                if enter_type == &binding.expected_type && exit_type == &binding.expected_type =>
            {
                let ordering = enter.semantic_cmp(exit);
                let valid = match direction {
                    ThresholdDirection::Above => ordering == Some(Ordering::Greater),
                    ThresholdDirection::Below => ordering == Some(Ordering::Less),
                };
                if !valid {
                    self.shape_issue(match direction {
                        ThresholdDirection::Above => "above threshold requires exit < enter",
                        ThresholdDirection::Below => "below threshold requires enter < exit",
                    });
                }
            }
            (Err(error), _) | (_, Err(error)) => self.issue(
                InvalidLiteral,
                format!("{node_path}/operation"),
                error.to_string(),
            ),
            _ => self.issue(
                InputTypeMismatch,
                format!("{node_path}/operation"),
                "threshold literals must exactly match the input type",
            ),
        }
    }

    pub(super) fn validate_boolean_control(
        &mut self,
        input: &LocalId,
        outputs: &[&LocalId],
        operation: &str,
    ) {
        let node = self.node;
        let shape_ok = node.inputs.len() == 1
            && node
                .inputs
                .get(input)
                .is_some_and(|binding| binding.expected_type == ValueType::Bool)
            && exact_outputs(node, outputs);
        if !shape_ok {
            self.shape_issue(format!(
                "{operation} must name one bool input and its distinct declared bool outputs"
            ));
        }
        self.validate_boolean_outputs(outputs, operation);
    }

    fn validate_boolean_outputs(&mut self, outputs: &[&LocalId], operation: &str) {
        let node_path = self.path;
        for output in outputs {
            if self
                .node
                .outputs
                .get(*output)
                .is_some_and(|contract| contract.value_type != ValueType::Bool)
            {
                self.issue(
                    InputTypeMismatch,
                    format!("{node_path}/outputs/{}", output.as_str()),
                    format!("{operation} outputs must be bool"),
                );
            }
        }
    }

    pub(super) fn validate_candidate_route(
        &mut self,
        condition: &LocalId,
        output: &LocalId,
        route: CandidateRoute,
        template: &Slug,
        fields: &BTreeMap<LocalId, LocalId>,
    ) {
        let node = self.node;
        let node_path = self.path;
        let declared_inputs: BTreeSet<_> = fields.values().chain([condition]).collect();
        let contract = node.outputs.get(output);
        let Some(contract) =
            contract.filter(|_| node.outputs.len() == 1 && node.inputs.keys().eq(declared_inputs))
        else {
            self.shape_issue(
                "route-candidate must name one unique condition, every field input, and one output",
            );
            return;
        };
        if node
            .inputs
            .get(condition)
            .is_some_and(|binding| binding.expected_type != ValueType::Bool)
        {
            self.issue(
                InputTypeMismatch,
                format!("{node_path}/operation/condition"),
                "route-candidate condition must be bool",
            );
        }
        let candidate_fields = fields
            .iter()
            .filter_map(|(field, input)| {
                node.inputs
                    .get(input)
                    .map(|binding| (field.clone(), binding.expected_type.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let expected = ValueType::Datum {
            value: Box::new(ValueType::Candidate {
                route,
                template: template.clone(),
                fields: candidate_fields,
            }),
        };
        if contract.value_type != expected {
            self.issue(
                InputTypeMismatch,
                format!("{node_path}/outputs/{}", output.as_str()),
                "route-candidate output type must exactly match its route, template, and fields",
            );
        }
    }
}

fn exact_outputs(node: &NodeAst, ports: &[&LocalId]) -> bool {
    node.outputs.len() == ports.len()
        && ports.iter().enumerate().all(|(index, &port)| {
            node.outputs.contains_key(port) && !ports[..index].contains(&port)
        })
}
