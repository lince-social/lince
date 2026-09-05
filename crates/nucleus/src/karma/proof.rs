use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    BinaryOperator, CandidateRoute, CanonicalHash, DecimalPrecision, DeclaredUnit, ExpressionAst,
    FailurePath, InputSource, LiteralValue, LocalId, MAX_DECIMAL_SCALE, NodeAst, NodeOperation,
    OutputRef, ProgramAst, ProgramSchema, ReferenceKind, Sensitivity, ThresholdDirection,
    TriggerSource, TypedUid, UnaryOperator, ValueType, canonical_hash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofIssueCode {
    CanonicalizationFailed,
    EmptyPurpose,
    InvalidType,
    InvalidLiteral,
    DefaultTypeMismatch,
    MissingParameter,
    MissingNode,
    MissingPort,
    InputTypeMismatch,
    UndeclaredExpressionInput,
    ExpressionTypeMismatch,
    InvalidOperationShape,
    InvalidPrecision,
    InvalidReferenceKind,
    TaintDowngrade,
    CombinationalCycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofIssue {
    pub severity: ProofSeverity,
    pub code: ProofIssueCode,
    pub path: FailurePath,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub related_nodes: BTreeSet<LocalId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub status: ProofStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_hash: Option<CanonicalHash>,
    pub evaluation_order: Vec<LocalId>,
    pub issues: Vec<ProofIssue>,
}

pub fn prove_program(program: &ProgramAst) -> Proof {
    let mut validator = Validator::default();
    validator.validate_program(program);
    let evaluation_order = validator.validate_graph(program);
    let revision_hash = match canonical_hash("karma.program-revision.v1", program) {
        Ok(hash) => Some(hash),
        Err(error) => {
            validator.issue(
                ProofIssueCode::CanonicalizationFailed,
                "",
                error.to_string(),
                BTreeSet::new(),
            );
            None
        }
    };
    validator.issues.sort_by(|left, right| {
        (
            left.severity,
            left.code,
            left.path.as_str(),
            left.message.as_str(),
            &left.related_nodes,
        )
            .cmp(&(
                right.severity,
                right.code,
                right.path.as_str(),
                right.message.as_str(),
                &right.related_nodes,
            ))
    });
    let status = if validator
        .issues
        .iter()
        .any(|issue| issue.severity == ProofSeverity::Error)
    {
        ProofStatus::Rejected
    } else {
        ProofStatus::Accepted
    };
    Proof {
        status,
        revision_hash,
        evaluation_order,
        issues: validator.issues,
    }
}

#[derive(Default)]
struct Validator {
    issues: Vec<ProofIssue>,
}

impl Validator {
    fn validate_program(&mut self, program: &ProgramAst) {
        match program.schema {
            ProgramSchema::V1 => {}
        }
        if program.purpose.trim().is_empty() || program.purpose.chars().any(char::is_control) {
            self.issue(
                ProofIssueCode::EmptyPurpose,
                "/purpose",
                "purpose must be non-empty and contain no control characters",
                BTreeSet::new(),
            );
        }
        for (id, parameter) in &program.parameters {
            let path = format!("/parameters/{}", pointer_segment(id.as_str()));
            if let Err(error) = parameter.value_type.validate() {
                self.issue(
                    ProofIssueCode::InvalidType,
                    &format!("{path}/value_type"),
                    error.to_string(),
                    BTreeSet::new(),
                );
            }
            match parameter.default.value_type() {
                Ok(actual) if actual != parameter.value_type => self.issue(
                    ProofIssueCode::DefaultTypeMismatch,
                    &format!("{path}/default"),
                    format!(
                        "parameter default has type {actual:?}, expected {:?}",
                        parameter.value_type
                    ),
                    BTreeSet::new(),
                ),
                Err(error) => self.issue(
                    ProofIssueCode::InvalidLiteral,
                    &format!("{path}/default"),
                    error.to_string(),
                    BTreeSet::new(),
                ),
                Ok(_) => {}
            }
        }
        for (id, node) in &program.nodes {
            self.validate_node(program, id, node);
        }
        for (name, output) in &program.outputs {
            self.resolve_output(
                program,
                output,
                &format!("/outputs/{}", pointer_segment(name.as_str())),
                None,
            );
        }
    }

    fn validate_node(&mut self, program: &ProgramAst, id: &LocalId, node: &NodeAst) {
        let node_path = format!("/nodes/{}", pointer_segment(id.as_str()));
        for (name, binding) in &node.inputs {
            let input_path = format!("{node_path}/inputs/{}", pointer_segment(name.as_str()));
            if let Err(error) = binding.expected_type.validate() {
                self.issue(
                    ProofIssueCode::InvalidType,
                    &format!("{input_path}/expected_type"),
                    error.to_string(),
                    nodes([id]),
                );
            }
            if let Some(source) = self.resolve_output(
                program,
                &binding.source,
                &format!("{input_path}/source"),
                Some(id),
            ) && source.value_type != binding.expected_type
            {
                self.issue(
                    ProofIssueCode::InputTypeMismatch,
                    &format!("{input_path}/expected_type"),
                    format!(
                        "source has type {:?}, binding expects {:?}",
                        source.value_type, binding.expected_type
                    ),
                    nodes([id, &binding.source.node]),
                );
            }
        }
        for (name, output) in &node.outputs {
            if let Err(error) = output.validate() {
                self.issue(
                    ProofIssueCode::InvalidType,
                    &format!(
                        "{node_path}/outputs/{}/value_type",
                        pointer_segment(name.as_str())
                    ),
                    error.to_string(),
                    nodes([id]),
                );
            }
        }
        self.validate_operation(program, id, node, &node_path);
        self.validate_taint(program, id, node, &node_path);
    }

    fn validate_operation(
        &mut self,
        program: &ProgramAst,
        id: &LocalId,
        node: &NodeAst,
        node_path: &str,
    ) {
        match &node.operation {
            NodeOperation::Trigger { source, output } => {
                if !node.inputs.is_empty() || !node.outputs.contains_key(output) {
                    self.shape_issue(
                        id,
                        node_path,
                        "trigger must have no inputs and name one declared output",
                    );
                }
                if node
                    .outputs
                    .get(output)
                    .is_some_and(|contract| contract.value_type != ValueType::Bool)
                {
                    self.shape_issue(
                        id,
                        node_path,
                        "trigger output must be bool so occurrence pulses have one canonical type",
                    );
                }
                self.validate_trigger_source(source, id, node_path);
            }
            NodeOperation::Input { source, output } => {
                if !node.inputs.is_empty() || !node.outputs.contains_key(output) {
                    self.shape_issue(
                        id,
                        node_path,
                        "input must have no input bindings and name one declared output",
                    );
                }
                self.validate_input_source(program, source, id, node, node_path);
            }
            NodeOperation::Derive { expressions } => {
                let output_names = node.outputs.keys().cloned().collect::<BTreeSet<_>>();
                let expression_names = expressions.keys().cloned().collect::<BTreeSet<_>>();
                if output_names != expression_names {
                    self.shape_issue(
                        id,
                        node_path,
                        "derive expressions must exactly match declared output names",
                    );
                }
                for (output, expression) in expressions {
                    let path = format!(
                        "{node_path}/operation/expressions/{}",
                        pointer_segment(output.as_str())
                    );
                    if let Some(actual) = self.infer_expression(id, node, expression, &path)
                        && let Some(expected) = node.outputs.get(output)
                        && actual != expected.value_type
                    {
                        self.issue(
                            ProofIssueCode::ExpressionTypeMismatch,
                            &path,
                            format!(
                                "expression has type {actual:?}, output declares {:?}",
                                expected.value_type
                            ),
                            nodes([id]),
                        );
                    }
                }
            }
            NodeOperation::Delay {
                input,
                output,
                initial,
                ..
            } => {
                let binding = node.inputs.get(input);
                let declared_output = node.outputs.get(output);
                if node.inputs.len() != 1
                    || node.outputs.len() != 1
                    || binding.is_none()
                    || declared_output.is_none()
                {
                    self.shape_issue(
                        id,
                        node_path,
                        "delay must name its single declared input and output",
                    );
                    return;
                }
                let binding = binding.expect("checked above");
                let declared_output = declared_output.expect("checked above");
                if binding.expected_type != declared_output.value_type {
                    self.issue(
                        ProofIssueCode::InputTypeMismatch,
                        &format!("{node_path}/operation/input"),
                        "delay input and output types must match",
                        nodes([id]),
                    );
                }
                match initial.value_type() {
                    Ok(actual) if actual != declared_output.value_type => self.issue(
                        ProofIssueCode::ExpressionTypeMismatch,
                        &format!("{node_path}/operation/initial"),
                        "delay initial value must match its output type",
                        nodes([id]),
                    ),
                    Err(error) => self.issue(
                        ProofIssueCode::InvalidLiteral,
                        &format!("{node_path}/operation/initial"),
                        error.to_string(),
                        nodes([id]),
                    ),
                    Ok(_) => {}
                }
            }
            NodeOperation::Threshold {
                input,
                active,
                entered,
                left,
                direction,
                enter,
                exit,
                ..
            } => self.validate_threshold(
                id, node, node_path, input, active, entered, left, *direction, enter, exit,
            ),
            NodeOperation::Debounce {
                input,
                stable,
                entered,
                left,
                for_at_least,
                ..
            } => {
                self.validate_boolean_control(
                    id,
                    node,
                    node_path,
                    input,
                    &[stable, entered, left],
                    "debounce",
                );
                if for_at_least.get() < 0 {
                    self.shape_issue(id, node_path, "debounce duration cannot be negative");
                }
            }
            NodeOperation::Cooldown {
                input,
                allowed,
                cooldown,
                ..
            } => {
                self.validate_boolean_control(id, node, node_path, input, &[allowed], "cooldown");
                if cooldown.get() < 0 {
                    self.shape_issue(id, node_path, "cooldown duration cannot be negative");
                }
            }
            NodeOperation::RateLimit {
                input,
                allowed,
                max,
                window,
                ..
            } => {
                self.validate_boolean_control(id, node, node_path, input, &[allowed], "rate-limit");
                if *max == 0 {
                    self.shape_issue(id, node_path, "rate-limit max must be non-zero");
                }
                if window.get() <= 0 {
                    self.shape_issue(id, node_path, "rate-limit window must be positive");
                }
            }
            NodeOperation::RouteCandidate {
                condition,
                output,
                route,
                template,
                fields,
            } => self.validate_candidate_route(
                id, node, node_path, condition, output, *route, template, fields,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_threshold(
        &mut self,
        id: &LocalId,
        node: &NodeAst,
        node_path: &str,
        input: &LocalId,
        active: &LocalId,
        entered: &LocalId,
        left: &LocalId,
        direction: ThresholdDirection,
        enter: &LiteralValue,
        exit: &LiteralValue,
    ) {
        let Some(binding) = node.inputs.get(input) else {
            self.shape_issue(
                id,
                node_path,
                "threshold must name its single declared input",
            );
            return;
        };
        let output_names = [active, entered, left]
            .into_iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if node.inputs.len() != 1
            || output_names.len() != 3
            || node.outputs.keys().cloned().collect::<BTreeSet<_>>() != output_names
        {
            self.shape_issue(
                id,
                node_path,
                "threshold must name one input and distinct active/entered/left outputs",
            );
        }
        if !binding.expected_type.is_ordered_scalar() {
            self.issue(
                ProofIssueCode::InputTypeMismatch,
                &format!("{node_path}/operation/input"),
                "threshold input must be an ordered scalar",
                nodes([id]),
            );
        }
        for output in [active, entered, left] {
            if node
                .outputs
                .get(output)
                .is_some_and(|contract| contract.value_type != ValueType::Bool)
            {
                self.issue(
                    ProofIssueCode::InputTypeMismatch,
                    &format!("{node_path}/outputs/{}", pointer_segment(output.as_str())),
                    "threshold outputs must be bool",
                    nodes([id]),
                );
            }
        }
        let enter_type = enter.value_type();
        let exit_type = exit.value_type();
        match (&enter_type, &exit_type) {
            (Ok(enter_type), Ok(exit_type))
                if enter_type == &binding.expected_type && exit_type == &binding.expected_type =>
            {
                let ordering = enter.semantic_cmp(exit);
                let valid = match direction {
                    ThresholdDirection::Above => ordering == Some(std::cmp::Ordering::Greater),
                    ThresholdDirection::Below => ordering == Some(std::cmp::Ordering::Less),
                };
                if !valid {
                    self.shape_issue(
                        id,
                        node_path,
                        match direction {
                            ThresholdDirection::Above => "above threshold requires exit < enter",
                            ThresholdDirection::Below => "below threshold requires enter < exit",
                        },
                    );
                }
            }
            (Err(error), _) | (_, Err(error)) => self.issue(
                ProofIssueCode::InvalidLiteral,
                &format!("{node_path}/operation"),
                error.to_string(),
                nodes([id]),
            ),
            _ => self.issue(
                ProofIssueCode::InputTypeMismatch,
                &format!("{node_path}/operation"),
                "threshold literals must exactly match the input type",
                nodes([id]),
            ),
        }
    }

    fn validate_boolean_control(
        &mut self,
        id: &LocalId,
        node: &NodeAst,
        node_path: &str,
        input: &LocalId,
        outputs: &[&LocalId],
        operation: &str,
    ) {
        let output_names = outputs
            .iter()
            .map(|id| (*id).clone())
            .collect::<BTreeSet<_>>();
        let shape_ok = node.inputs.len() == 1
            && node
                .inputs
                .get(input)
                .is_some_and(|binding| binding.expected_type == ValueType::Bool)
            && output_names.len() == outputs.len()
            && node.outputs.keys().cloned().collect::<BTreeSet<_>>() == output_names;
        if !shape_ok {
            self.shape_issue(
                id,
                node_path,
                format!(
                    "{operation} must name one bool input and its distinct declared bool outputs"
                ),
            );
        }
        for output in outputs {
            if node
                .outputs
                .get(*output)
                .is_some_and(|contract| contract.value_type != ValueType::Bool)
            {
                self.issue(
                    ProofIssueCode::InputTypeMismatch,
                    &format!("{node_path}/outputs/{}", pointer_segment(output.as_str())),
                    format!("{operation} outputs must be bool"),
                    nodes([id]),
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_candidate_route(
        &mut self,
        id: &LocalId,
        node: &NodeAst,
        node_path: &str,
        condition: &LocalId,
        output: &LocalId,
        route: CandidateRoute,
        template: &super::Slug,
        fields: &BTreeMap<LocalId, LocalId>,
    ) {
        let declared_inputs = fields
            .values()
            .chain(std::iter::once(condition))
            .cloned()
            .collect::<BTreeSet<_>>();
        if node.inputs.keys().cloned().collect::<BTreeSet<_>>() != declared_inputs
            || node.outputs.len() != 1
            || !node.outputs.contains_key(output)
        {
            self.shape_issue(
                id,
                node_path,
                "route-candidate must name one unique condition, every field input, and one output",
            );
            return;
        }
        if node
            .inputs
            .get(condition)
            .is_some_and(|binding| binding.expected_type != ValueType::Bool)
        {
            self.issue(
                ProofIssueCode::InputTypeMismatch,
                &format!("{node_path}/operation/condition"),
                "route-candidate condition must be bool",
                nodes([id]),
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
        if node
            .outputs
            .get(output)
            .is_some_and(|contract| contract.value_type != expected)
        {
            self.issue(
                ProofIssueCode::InputTypeMismatch,
                &format!("{node_path}/outputs/{}", pointer_segment(output.as_str())),
                "route-candidate output type must exactly match its route, template, and fields",
                nodes([id]),
            );
        }
    }

    fn validate_input_source(
        &mut self,
        program: &ProgramAst,
        source: &InputSource,
        id: &LocalId,
        node: &NodeAst,
        node_path: &str,
    ) {
        match source {
            InputSource::Parameter { parameter } => {
                let Some(definition) = program.parameters.get(parameter) else {
                    self.issue(
                        ProofIssueCode::MissingParameter,
                        &format!("{node_path}/operation/source/parameter"),
                        format!("parameter `{parameter:?}` does not exist"),
                        nodes([id]),
                    );
                    return;
                };
                if let NodeOperation::Input { output, .. } = &node.operation
                    && let Some(contract) = node.outputs.get(output)
                    && contract.value_type != definition.value_type
                {
                    self.issue(
                        ProofIssueCode::InputTypeMismatch,
                        &format!("{node_path}/outputs/{}", pointer_segment(output.as_str())),
                        "parameter input output type does not match the parameter type",
                        nodes([id]),
                    );
                }
            }
            InputSource::RecordQuantity { record } => self.require_reference_kind(
                record,
                ReferenceKind::Record,
                id,
                &format!("{node_path}/operation/source/record"),
            ),
            InputSource::SavedProtein { view } => self.require_reference_kind(
                view,
                ReferenceKind::View,
                id,
                &format!("{node_path}/operation/source/view"),
            ),
            InputSource::Signal { signal } => self.require_reference_kind(
                signal,
                ReferenceKind::Signal,
                id,
                &format!("{node_path}/operation/source/signal"),
            ),
            InputSource::CapturedFact { fact } => self.require_reference_kind(
                fact,
                ReferenceKind::Fact,
                id,
                &format!("{node_path}/operation/source/fact"),
            ),
            InputSource::SecretMetadata { .. } => {}
        }
    }

    fn validate_trigger_source(&mut self, source: &TriggerSource, id: &LocalId, node_path: &str) {
        match source {
            TriggerSource::Fact { record, concept } => {
                if let Some(record) = record {
                    self.require_reference_kind(
                        record,
                        ReferenceKind::Record,
                        id,
                        &format!("{node_path}/operation/source/record"),
                    );
                }
                if let Some(concept) = concept {
                    self.require_reference_kind(
                        concept,
                        ReferenceKind::Concept,
                        id,
                        &format!("{node_path}/operation/source/concept"),
                    );
                }
            }
            TriggerSource::Frequency { frequency } => self.require_reference_kind(
                frequency,
                ReferenceKind::Frequency,
                id,
                &format!("{node_path}/operation/source/frequency"),
            ),
            TriggerSource::Signal { signal } => self.require_reference_kind(
                signal,
                ReferenceKind::Signal,
                id,
                &format!("{node_path}/operation/source/signal"),
            ),
            TriggerSource::Decision { decision } => self.require_reference_kind(
                decision,
                ReferenceKind::Decision,
                id,
                &format!("{node_path}/operation/source/decision"),
            ),
            TriggerSource::Receipt { receipt } => self.require_reference_kind(
                receipt,
                ReferenceKind::Receipt,
                id,
                &format!("{node_path}/operation/source/receipt"),
            ),
            TriggerSource::Manual | TriggerSource::Sync => {}
        }
    }

    fn require_reference_kind(
        &mut self,
        reference: &super::ResolvedReference,
        expected: ReferenceKind,
        id: &LocalId,
        path: &str,
    ) {
        if reference.target.kind() != expected {
            self.issue(
                ProofIssueCode::InvalidReferenceKind,
                path,
                format!(
                    "reference has kind {:?}, expected {expected:?}",
                    reference.target.kind()
                ),
                nodes([id]),
            );
        }
    }

    fn validate_taint(&mut self, program: &ProgramAst, id: &LocalId, node: &NodeAst, path: &str) {
        if node.inputs.is_empty() {
            return;
        }
        let maximum_input = node
            .inputs
            .values()
            .filter_map(|binding| {
                program
                    .nodes
                    .get(&binding.source.node)
                    .and_then(|source| source.outputs.get(&binding.source.port))
                    .map(|contract| contract.sensitivity)
            })
            .max()
            .unwrap_or(Sensitivity::Public);
        for (name, output) in &node.outputs {
            if output.sensitivity < maximum_input {
                self.issue(
                    ProofIssueCode::TaintDowngrade,
                    &format!("{path}/outputs/{}/sensitivity", pointer_segment(name.as_str())),
                    format!(
                        "output sensitivity {:?} is below consumed input sensitivity {maximum_input:?}",
                        output.sensitivity
                    ),
                    nodes([id]),
                );
            }
        }
    }

    fn infer_expression(
        &mut self,
        id: &LocalId,
        node: &NodeAst,
        expression: &ExpressionAst,
        path: &str,
    ) -> Option<ValueType> {
        match expression {
            ExpressionAst::Literal { value } => match value.value_type() {
                Ok(value_type) => Some(value_type),
                Err(error) => {
                    self.issue(
                        ProofIssueCode::InvalidLiteral,
                        path,
                        error.to_string(),
                        nodes([id]),
                    );
                    None
                }
            },
            ExpressionAst::Input { input } => match node.inputs.get(input) {
                Some(binding) => Some(binding.expected_type.clone()),
                None => {
                    self.issue(
                        ProofIssueCode::UndeclaredExpressionInput,
                        path,
                        format!("expression input `{input:?}` is not declared by this node"),
                        nodes([id]),
                    );
                    None
                }
            },
            ExpressionAst::Unary { operator, value } => {
                let value_type = self.infer_expression(id, node, value, path)?;
                let result = match operator {
                    UnaryOperator::Not if value_type == ValueType::Bool => Some(ValueType::Bool),
                    UnaryOperator::Negate if value_type.is_negatable() => Some(value_type.clone()),
                    _ => None,
                };
                if result.is_none() {
                    self.expression_error(
                        id,
                        path,
                        format!("invalid {operator:?} for {value_type:?}"),
                    );
                }
                result
            }
            ExpressionAst::Binary {
                operator,
                left,
                right,
                precision,
            } => {
                let left = self.infer_expression(id, node, left, path)?;
                let right = self.infer_expression(id, node, right, path)?;
                match infer_binary(*operator, &left, &right, precision.as_ref()) {
                    Ok(value_type) => Some(value_type),
                    Err(BinaryIssue::Types) => {
                        self.expression_error(
                            id,
                            path,
                            format!("invalid {operator:?} for {left:?} and {right:?}"),
                        );
                        None
                    }
                    Err(BinaryIssue::Precision(message)) => {
                        self.issue(
                            ProofIssueCode::InvalidPrecision,
                            path,
                            format!("invalid {operator:?} precision: {message}"),
                            nodes([id]),
                        );
                        None
                    }
                }
            }
            ExpressionAst::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.infer_expression(id, node, condition, path)?;
                let then_type = self.infer_expression(id, node, then_value, path)?;
                let else_type = self.infer_expression(id, node, else_value, path)?;
                if condition == ValueType::Bool && then_type == else_type {
                    Some(then_type)
                } else {
                    self.expression_error(
                        id,
                        path,
                        "if requires a bool condition and exactly matching branch types",
                    );
                    None
                }
            }
        }
    }

    fn expression_error(&mut self, id: &LocalId, path: &str, message: impl Into<String>) {
        self.issue(
            ProofIssueCode::ExpressionTypeMismatch,
            path,
            message,
            nodes([id]),
        );
    }

    fn resolve_output<'a>(
        &mut self,
        program: &'a ProgramAst,
        output: &OutputRef,
        path: &str,
        consumer: Option<&LocalId>,
    ) -> Option<&'a super::PortContract> {
        let Some(node) = program.nodes.get(&output.node) else {
            let related = consumer.map_or_else(BTreeSet::new, |id| nodes([id]));
            self.issue(
                ProofIssueCode::MissingNode,
                path,
                format!("source node `{:?}` does not exist", output.node),
                related,
            );
            return None;
        };
        let Some(port) = node.outputs.get(&output.port) else {
            let mut related = nodes([&output.node]);
            if let Some(consumer) = consumer {
                related.insert(consumer.clone());
            }
            self.issue(
                ProofIssueCode::MissingPort,
                path,
                format!(
                    "source node `{:?}` has no output `{:?}`",
                    output.node, output.port
                ),
                related,
            );
            return None;
        };
        Some(port)
    }

    fn validate_graph(&mut self, program: &ProgramAst) -> Vec<LocalId> {
        let mut successors = program
            .nodes
            .keys()
            .cloned()
            .map(|id| (id, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        let mut indegree = program
            .nodes
            .keys()
            .cloned()
            .map(|id| (id, 0_usize))
            .collect::<BTreeMap<_, _>>();
        for (target, node) in &program.nodes {
            if node.operation.is_state_boundary() {
                continue;
            }
            for binding in node.inputs.values() {
                if !program.nodes.contains_key(&binding.source.node) {
                    continue;
                }
                let inserted = successors
                    .get_mut(&binding.source.node)
                    .expect("known source")
                    .insert(target.clone());
                if inserted {
                    *indegree.get_mut(target).expect("known target") += 1;
                }
            }
        }
        let mut ready = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::with_capacity(program.nodes.len());
        while let Some(id) = ready.pop_first() {
            order.push(id.clone());
            for successor in successors.get(&id).expect("known node") {
                let degree = indegree.get_mut(successor).expect("known successor");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(successor.clone());
                }
            }
        }
        if order.len() != program.nodes.len() {
            let remaining = indegree
                .into_iter()
                .filter_map(|(id, degree)| (degree > 0).then_some(id))
                .collect::<BTreeSet<_>>();
            self.issue(
                ProofIssueCode::CombinationalCycle,
                "/nodes",
                "combinational cycle requires an explicit delay/state boundary",
                remaining,
            );
        }
        order
    }

    fn shape_issue(&mut self, id: &LocalId, path: &str, message: impl Into<String>) {
        self.issue(
            ProofIssueCode::InvalidOperationShape,
            &format!("{path}/operation"),
            message,
            nodes([id]),
        );
    }

    fn issue(
        &mut self,
        code: ProofIssueCode,
        path: &str,
        message: impl Into<String>,
        related_nodes: BTreeSet<LocalId>,
    ) {
        self.issues.push(ProofIssue {
            severity: ProofSeverity::Error,
            code,
            path: FailurePath::new(path).expect("validator emits valid JSON pointers"),
            message: message.into(),
            related_nodes,
        });
    }
}

enum BinaryIssue {
    Types,
    Precision(String),
}

enum Dimension<'a> {
    Plain,
    Unit(&'a TypedUid),
}

fn dimension(value_type: &ValueType) -> Option<Dimension<'_>> {
    match value_type {
        ValueType::Decimal { .. } => Some(Dimension::Plain),
        ValueType::Quantity { unit, .. } => Some(Dimension::Unit(unit)),
        _ => None,
    }
}

fn infer_binary(
    operator: BinaryOperator,
    left: &ValueType,
    right: &ValueType,
    precision: Option<&DecimalPrecision>,
) -> Result<ValueType, BinaryIssue> {
    let exact_product = matches!(operator, BinaryOperator::Multiply | BinaryOperator::Divide)
        && (dimension(left).is_some() || dimension(right).is_some());
    if exact_product {
        return infer_exact_product(operator, left, right, precision);
    }
    if precision.is_some() {
        return Err(BinaryIssue::Precision(
            "only multiplication and division over exact values declare a precision".to_string(),
        ));
    }
    infer_plain_binary(operator, left, right).ok_or(BinaryIssue::Types)
}

fn infer_exact_product(
    operator: BinaryOperator,
    left: &ValueType,
    right: &ValueType,
    precision: Option<&DecimalPrecision>,
) -> Result<ValueType, BinaryIssue> {
    let Some(precision) = precision else {
        return Err(BinaryIssue::Precision(
            "multiplication and division over exact values must declare a result scale and \
             rounding, because neither operation is closed over fixed-point decimals"
                .to_string(),
        ));
    };
    if precision.scale > MAX_DECIMAL_SCALE {
        return Err(BinaryIssue::Precision(format!(
            "declared scale {} exceeds maximum {MAX_DECIMAL_SCALE}",
            precision.scale
        )));
    }
    let scale = precision.scale;
    let (Some(left_dim), Some(right_dim)) = (dimension(left), dimension(right)) else {
        return Err(BinaryIssue::Types);
    };
    let multiply = operator == BinaryOperator::Multiply;

    match (left_dim, right_dim) {
        (Dimension::Plain, Dimension::Plain) => {
            keeps_own_dimension(precision)?;
            Ok(ValueType::Decimal { scale })
        }
        (Dimension::Unit(unit), Dimension::Plain) => {
            keeps_own_dimension(precision)?;
            Ok(ValueType::Quantity {
                scale,
                unit: unit.clone(),
            })
        }
        (Dimension::Plain, Dimension::Unit(unit)) if multiply => {
            keeps_own_dimension(precision)?;
            Ok(ValueType::Quantity {
                scale,
                unit: unit.clone(),
            })
        }
        (Dimension::Unit(_), Dimension::Unit(_)) | (Dimension::Plain, Dimension::Unit(_)) => {
            match &precision.result_unit {
                Some(DeclaredUnit::Dimensionless) => Ok(ValueType::Decimal { scale }),
                Some(DeclaredUnit::Unit { unit }) => {
                    if unit.kind() == ReferenceKind::Unit {
                        Ok(ValueType::Quantity {
                            scale,
                            unit: unit.clone(),
                        })
                    } else {
                        Err(BinaryIssue::Precision(
                            "declared result unit must be a unit reference".to_string(),
                        ))
                    }
                }
                None => Err(BinaryIssue::Precision(
                    "combining two dimensioned values must declare the result unit, or declare it \
                     dimensionless"
                        .to_string(),
                )),
            }
        }
    }
}

fn keeps_own_dimension(precision: &DecimalPrecision) -> Result<(), BinaryIssue> {
    if precision.result_unit.is_some() {
        return Err(BinaryIssue::Precision(
            "scaling by a plain decimal keeps the value's own unit; remove the declared result unit"
                .to_string(),
        ));
    }
    Ok(())
}

fn infer_plain_binary(
    operator: BinaryOperator,
    left: &ValueType,
    right: &ValueType,
) -> Option<ValueType> {
    match operator {
        BinaryOperator::And | BinaryOperator::Or
            if left == &ValueType::Bool && right == &ValueType::Bool =>
        {
            Some(ValueType::Bool)
        }
        BinaryOperator::Equal | BinaryOperator::NotEqual if left == right => Some(ValueType::Bool),
        BinaryOperator::Less
        | BinaryOperator::LessOrEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterOrEqual
            if left == right && left.is_ordered_scalar() =>
        {
            Some(ValueType::Bool)
        }
        BinaryOperator::Add | BinaryOperator::Subtract if left == right && left.is_additive() => {
            Some(left.clone())
        }
        BinaryOperator::Add
            if (left == &ValueType::Timestamp && right == &ValueType::Duration)
                || (left == &ValueType::Duration && right == &ValueType::Timestamp) =>
        {
            Some(ValueType::Timestamp)
        }
        BinaryOperator::Subtract
            if left == &ValueType::Timestamp && right == &ValueType::Duration =>
        {
            Some(ValueType::Timestamp)
        }
        BinaryOperator::Subtract
            if left == &ValueType::Timestamp && right == &ValueType::Timestamp =>
        {
            Some(ValueType::Duration)
        }
        BinaryOperator::Multiply | BinaryOperator::Divide
            if left == &ValueType::I64 && right == &ValueType::I64 =>
        {
            Some(ValueType::I64)
        }
        BinaryOperator::Remainder
            if left == right && matches!(left, ValueType::I64 | ValueType::Duration) =>
        {
            Some(left.clone())
        }
        _ => None,
    }
}

fn nodes<'a>(ids: impl IntoIterator<Item = &'a LocalId>) -> BTreeSet<LocalId> {
    ids.into_iter().cloned().collect()
}

fn pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
