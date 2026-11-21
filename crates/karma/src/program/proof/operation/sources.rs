use crate::{InputSource, LocalId, ProgramAst, ReferenceKind, ResolvedReference, TriggerSource};

use super::super::{
    ProofIssueCode::{InputTypeMismatch, InvalidReferenceKind, MissingParameter},
    node::NodeValidation,
};

impl NodeValidation<'_> {
    pub(super) fn validate_input_source(
        &mut self,
        program: &ProgramAst,
        source: &InputSource,
        output: &LocalId,
    ) {
        let path = self.path;
        match source {
            InputSource::Parameter { parameter } => {
                let Some(definition) = program.parameters.get(parameter) else {
                    self.issue(
                        MissingParameter,
                        format!("{path}/operation/source/parameter"),
                        format!("parameter `{parameter:?}` does not exist"),
                    );
                    return;
                };
                if self
                    .node
                    .outputs
                    .get(output)
                    .is_some_and(|port| port.value_type != definition.value_type)
                {
                    self.issue(
                        InputTypeMismatch,
                        format!("{path}/outputs/{}", output.as_str()),
                        "parameter input output type does not match the parameter type",
                    );
                }
            }
            InputSource::RecordQuantity { record } => {
                self.require_reference_kind(record, ReferenceKind::Record, "record")
            }
            InputSource::SavedProtein { view } => {
                self.require_reference_kind(view, ReferenceKind::View, "view")
            }
            InputSource::Signal { signal } => {
                self.require_reference_kind(signal, ReferenceKind::Signal, "signal")
            }
            InputSource::CapturedFact { fact } => {
                self.require_reference_kind(fact, ReferenceKind::Fact, "fact")
            }
        }
    }

    pub(super) fn validate_trigger_source(&mut self, source: &TriggerSource) {
        match source {
            TriggerSource::Fact { record, concept } => {
                if let Some(record) = record {
                    self.require_reference_kind(record, ReferenceKind::Record, "record");
                }
                if let Some(concept) = concept {
                    self.require_reference_kind(concept, ReferenceKind::Concept, "concept");
                }
            }
            TriggerSource::Frequency { frequency } => {
                self.require_reference_kind(frequency, ReferenceKind::Frequency, "frequency")
            }
            TriggerSource::Signal { signal } => {
                self.require_reference_kind(signal, ReferenceKind::Signal, "signal")
            }
            TriggerSource::Decision { decision } => {
                self.require_reference_kind(decision, ReferenceKind::Decision, "decision")
            }
            TriggerSource::Receipt { receipt } => {
                self.require_reference_kind(receipt, ReferenceKind::Receipt, "receipt")
            }
            TriggerSource::Manual | TriggerSource::Sync => {}
        }
    }

    fn require_reference_kind(
        &mut self,
        reference: &ResolvedReference,
        expected: ReferenceKind,
        field: &str,
    ) {
        let kind = reference.target.kind();
        if kind != expected {
            self.issue(
                InvalidReferenceKind,
                format!("{}/operation/source/{field}", self.path),
                format!("reference has kind {kind:?}, expected {expected:?}"),
            );
        }
    }
}
