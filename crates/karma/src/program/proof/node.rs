use std::collections::BTreeSet;

use crate::{LocalId, NodeAst, OutputRef, PortContract, ProgramAst, ProgramSchema, Sensitivity};

use super::{
    ProofIssueCode::{
        self, DefaultTypeMismatch, EmptyPurpose, InputTypeMismatch, InvalidLiteral,
        InvalidOperationShape, InvalidType, MissingNode, MissingPort, TaintDowngrade,
    },
    Validator, nodes,
};

impl Validator {
    pub(super) fn validate_program(&mut self, program: &ProgramAst) {
        match program.schema {
            ProgramSchema::V1 => {}
        }
        if program.purpose.trim().is_empty() || program.purpose.chars().any(char::is_control) {
            self.issue(
                EmptyPurpose,
                "/purpose",
                "purpose must be non-empty and contain no control characters",
                BTreeSet::new(),
            );
        }
        for (id, parameter) in &program.parameters {
            let id = id.as_str();
            if let Err(error) = parameter.value_type.validate() {
                self.issue(
                    InvalidType,
                    format!("/parameters/{id}/value_type"),
                    error.to_string(),
                    BTreeSet::new(),
                );
            }
            let (code, message) = match parameter.default.value_type() {
                Ok(actual) if actual != parameter.value_type => (
                    DefaultTypeMismatch,
                    format!(
                        "parameter default has type {actual:?}, expected {:?}",
                        parameter.value_type
                    ),
                ),
                Err(error) => (InvalidLiteral, error.to_string()),
                Ok(_) => continue,
            };
            self.issue(
                code,
                format!("/parameters/{id}/default"),
                message,
                BTreeSet::new(),
            );
        }
        for (id, node) in &program.nodes {
            let path = format!("/nodes/{}", id.as_str());
            let mut validation = NodeValidation {
                validator: self,
                id,
                node,
                path: &path,
            };
            validation.validate_ports(program);
            validation.validate_operation(program);
        }
        for (name, output) in &program.outputs {
            self.resolve_output(
                program,
                output,
                || format!("/outputs/{}", name.as_str()),
                None,
            );
        }
    }

    fn resolve_output<'a>(
        &mut self,
        program: &'a ProgramAst,
        output: &OutputRef,
        path: impl FnOnce() -> String,
        consumer: Option<&LocalId>,
    ) -> Option<&'a PortContract> {
        let Some(node) = program.nodes.get(&output.node) else {
            self.issue(
                MissingNode,
                path(),
                format!("source node `{:?}` does not exist", output.node),
                nodes(consumer),
            );
            return None;
        };
        let port = node.outputs.get(&output.port);
        if port.is_none() {
            self.issue(
                MissingPort,
                path(),
                format!(
                    "source node `{:?}` has no output `{:?}`",
                    output.node, output.port
                ),
                nodes(consumer.into_iter().chain([&output.node])),
            );
        }
        port
    }
}

pub(super) struct NodeValidation<'a> {
    validator: &'a mut Validator,
    id: &'a LocalId,
    pub(super) path: &'a str,
    pub(super) node: &'a NodeAst,
}

impl NodeValidation<'_> {
    fn validate_ports(&mut self, program: &ProgramAst) {
        let path = self.path;
        let mut sensitivity = Sensitivity::Public;
        for (name, binding) in &self.node.inputs {
            let name = name.as_str();
            if let Err(error) = binding.expected_type.validate() {
                self.issue(
                    InvalidType,
                    format!("{path}/inputs/{name}/expected_type"),
                    error.to_string(),
                );
            }
            if let Some(source) = self.validator.resolve_output(
                program,
                &binding.source,
                || format!("{path}/inputs/{name}/source"),
                Some(self.id),
            ) {
                sensitivity = sensitivity.max(source.sensitivity);
                if source.value_type != binding.expected_type {
                    self.validator.issue(
                        InputTypeMismatch,
                        format!("{path}/inputs/{name}/expected_type"),
                        format!(
                            "source has type {:?}, binding expects {:?}",
                            source.value_type, binding.expected_type
                        ),
                        nodes([self.id, &binding.source.node]),
                    );
                }
            }
        }
        for (name, output) in &self.node.outputs {
            let name = name.as_str();
            if let Err(error) = output.validate() {
                self.issue(
                    InvalidType,
                    format!("{path}/outputs/{name}/value_type"),
                    error.to_string(),
                );
            }
            if output.sensitivity < sensitivity {
                self.issue(
                    TaintDowngrade,
                    format!("{path}/outputs/{name}/sensitivity"),
                    format!("output sensitivity {:?} is below consumed input sensitivity {sensitivity:?}", output.sensitivity),
                );
            }
        }
    }

    pub(super) fn issue(
        &mut self,
        code: ProofIssueCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.validator.issue(code, path, message, nodes([self.id]));
    }

    pub(super) fn shape_issue(&mut self, message: impl Into<String>) {
        self.issue(
            InvalidOperationShape,
            format!("{}/operation", self.path),
            message,
        );
    }
}
