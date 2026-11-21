//! Consequence values, their ordered list, and the operations that inspect them.

use std::slice::Iter;

use serde::{Deserialize, Serialize};

use crate::{
    DecimalValue, component::ComponentState, error::NucleusError, transfer::karma::AutomationStage,
};

use super::DurationMs;

mod validation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum Consequence {
    CaptureEntry {
        amount: DecimalValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concept: Option<String>,
    },
    SetQuantity {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<DecimalValue>,
    },
    SetQuantityWhere {
        assertion: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<DecimalValue>,
    },
    AddQuantity {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<DecimalValue>,
    },
    SetConcept {
        concept: String,
    },
    AddConcept {
        concept: String,
    },
    RemoveConcept {
        concept: String,
    },

    EmitPromise {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<DecimalValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_end: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        party: Option<String>,
    },
    Ask {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        question: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
    },
    Notify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    InvokeCommand {
        command: String,
    },
    RunCommand {
        command: String,
    },
    ShowComponent {
        component: ComponentState,
    },
    ActivateFiote,
    RunQuery {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<String>,
    },
    RunAction {
        action: String,
    },
    SetVisibility {
        #[serde(default = "public_subject")]
        subject_kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<String>,
    },
    AdvanceAudience {
        plan: String,
        commitment: String,
    },
    RepeatTransfer {
        transfer: String,
        person: String,
        stage: AutomationStage,
    },
    ReopenTransferNeed {
        transfer: String,
        person: String,
        target: Option<DecimalValue>,
    },
    SetTransferAgreement {
        transfer: String,
        person: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<DecimalValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_ms: Option<DurationMs>,
    },
    PublishTransfer {
        transfer: String,
        person: String,
    },
    ActivateTransferFulfillment {
        transfer: String,
        person: String,
        promise: String,
        fulfillment: String,
    },
}

fn public_subject() -> String {
    "public".into()
}

impl Consequence {
    /// Returns the stable kebab-case wire tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CaptureEntry { .. } => "capture-entry",
            Self::SetQuantity { .. } => "set-quantity",
            Self::SetQuantityWhere { .. } => "set-quantity-where",
            Self::AddQuantity { .. } => "add-quantity",
            Self::SetConcept { .. } => "set-concept",
            Self::AddConcept { .. } => "add-concept",
            Self::RemoveConcept { .. } => "remove-concept",
            Self::EmitPromise { .. } => "emit-promise",
            Self::Ask { .. } => "ask",
            Self::Notify { .. } => "notify",
            Self::InvokeCommand { .. } => "invoke-command",
            Self::RunCommand { .. } => "run-command",
            Self::ShowComponent { .. } => "show-component",
            Self::ActivateFiote => "activate-fiote",
            Self::RunQuery { .. } => "run-query",
            Self::RunAction { .. } => "run-action",
            Self::SetVisibility { .. } => "set-visibility",
            Self::AdvanceAudience { .. } => "advance-audience",
            Self::RepeatTransfer { .. } => "repeat-transfer",
            Self::ReopenTransferNeed { .. } => "reopen-transfer-need",
            Self::SetTransferAgreement { .. } => "set-transfer-agreement",
            Self::PublishTransfer { .. } => "publish-transfer",
            Self::ActivateTransferFulfillment { .. } => "activate-transfer-fulfillment",
        }
    }

    /// Reports whether the effect uses Karma’s outward execution path.
    pub fn is_outward(&self) -> bool {
        matches!(
            self,
            Self::EmitPromise { .. }
                | Self::Ask { .. }
                | Self::Notify { .. }
                | Self::InvokeCommand { .. }
                | Self::RunCommand { .. }
                | Self::ShowComponent { .. }
                | Self::ActivateFiote
                | Self::RunQuery { .. }
                | Self::RunAction { .. }
                | Self::SetVisibility { .. }
                | Self::AdvanceAudience { .. }
                | Self::ReopenTransferNeed { .. }
                | Self::RepeatTransfer { .. }
        )
    }

    /// Reports whether a capture, assignment, or addition moves a quantity.
    pub fn moves_quantity(&self) -> bool {
        matches!(
            self,
            Self::CaptureEntry { .. } | Self::SetQuantity { .. } | Self::AddQuantity { .. }
        )
    }

    /// Borrows the target for a Transfer consequence.
    pub fn transfer_target(&self) -> Option<&str> {
        self.transfer_references().map(|(transfer, _)| transfer)
    }

    /// Borrows the acting Person for a Transfer consequence.
    pub fn transfer_person(&self) -> Option<&str> {
        self.transfer_references().map(|(_, person)| person)
    }

    fn transfer_references(&self) -> Option<(&str, &str)> {
        match self {
            Self::RepeatTransfer {
                transfer, person, ..
            }
            | Self::ReopenTransferNeed {
                transfer, person, ..
            }
            | Self::SetTransferAgreement {
                transfer, person, ..
            }
            | Self::PublishTransfer { transfer, person }
            | Self::ActivateTransferFulfillment {
                transfer, person, ..
            } => Some((transfer, person)),
            _ => None,
        }
    }

    /// Mutably borrows the Transfer and acting Person, in that order.
    pub fn transfer_references_mut(&mut self) -> Option<(&mut String, &mut String)> {
        match self {
            Self::RepeatTransfer {
                transfer, person, ..
            }
            | Self::ReopenTransferNeed {
                transfer, person, ..
            }
            | Self::SetTransferAgreement {
                transfer, person, ..
            }
            | Self::PublishTransfer { transfer, person }
            | Self::ActivateTransferFulfillment {
                transfer, person, ..
            } => Some((transfer, person)),
            _ => None,
        }
    }

    /// Borrows the declared capture amount or additive delta.
    pub fn delta(&self) -> Option<&DecimalValue> {
        match self {
            Self::CaptureEntry { amount, .. } => Some(amount),
            Self::AddQuantity { delta } => delta.as_ref(),
            _ => None,
        }
    }

    /// Borrows the capture amount, if present.
    pub fn capture_amount(&self) -> Option<&DecimalValue> {
        match self {
            Self::CaptureEntry { amount, .. } => Some(amount),
            _ => None,
        }
    }
}

/// An ordered list of consequences. Use [Self::new] to validate a newly authored list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Consequences(Vec<Consequence>);

impl Consequences {
    /// Takes ownership of an ordered, nonempty list of distinct consequences.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid consequence, an empty list, or a duplicate.
    pub fn new(items: Vec<Consequence>) -> Result<Self, NucleusError> {
        Self::validate(&items)?;
        Ok(Self(items))
    }

    /// Creates a rule that captures one entry.
    pub fn capture(amount: DecimalValue, concept: Option<String>) -> Self {
        Self(vec![Consequence::CaptureEntry { amount, concept }])
    }

    /// Borrows consequences in their execution order.
    pub fn as_slice(&self) -> &[Consequence] {
        &self.0
    }

    /// Iterates through consequences in their execution order.
    pub fn iter(&self) -> Iter<'_, Consequence> {
        self.0.iter()
    }

    /// Returns the number of stored consequences.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Reports whether the stored list has no consequences.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrows the first declared capture amount or additive delta.
    pub fn declared_delta(&self) -> Option<&DecimalValue> {
        self.0.iter().find_map(Consequence::delta)
    }

    /// Borrows the first capture amount, if present.
    pub fn capture_amount(&self) -> Option<&DecimalValue> {
        self.0.iter().find_map(Consequence::capture_amount)
    }

    /// Borrows the first declared capture concept.
    pub fn capture_concept(&self) -> Option<&str> {
        self.0.iter().find_map(|item| match item {
            Consequence::CaptureEntry { concept, .. } => concept.as_deref(),
            _ => None,
        })
    }

    /// Reports whether any consequence moves a quantity.
    pub fn moves_quantity(&self) -> bool {
        self.0.iter().any(Consequence::moves_quantity)
    }
}

impl<'a> IntoIterator for &'a Consequences {
    type Item = &'a Consequence;
    type IntoIter = Iter<'a, Consequence>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(text: &str) -> DecimalValue {
        DecimalValue::parse_inferred(text).expect("decimal parses")
    }

    #[test]
    fn a_rule_with_nothing_to_do_is_refused() {
        assert!(Consequences::new(Vec::new()).is_err());
    }

    #[test]
    fn the_same_consequence_twice_is_a_duplicate_not_a_stronger_intention() {
        let once = Consequence::AddConcept {
            concept: "done".into(),
        };
        assert!(Consequences::new(vec![once.clone(), once]).is_err());
    }

    #[test]
    fn moving_a_card_between_columns_is_one_rule() {
        let moved = Consequences::new(vec![
            Consequence::RemoveConcept {
                concept: "wip".into(),
            },
            Consequence::AddConcept {
                concept: "done".into(),
            },
        ])
        .expect("a pair is legal");
        assert_eq!(moved.len(), 2);
        assert!(!moved.moves_quantity());
        assert!(moved.declared_delta().is_none());
    }

    #[test]
    fn an_empty_concept_is_refused_rather_than_stored_blank() {
        assert!(
            Consequences::new(vec![Consequence::AddConcept {
                concept: "   ".into()
            }])
            .is_err()
        );
    }

    #[test]
    fn a_capture_still_reports_its_amount_and_concept() {
        let rule = Consequences::capture(dec("-10.50"), Some("food".into()));
        assert!(rule.moves_quantity());
        assert_eq!(rule.declared_delta(), Some(&dec("-10.50")));
        assert_eq!(rule.capture_amount(), Some(&dec("-10.50")));
        assert_eq!(rule.capture_concept(), Some("food"));
    }

    #[test]
    fn setting_a_quantity_is_not_cumulative_but_still_moves_one() {
        let rule = Consequences::new(vec![Consequence::SetQuantity {
            value: Some(dec("-1")),
        }])
        .expect("legal");
        assert!(rule.moves_quantity());
        assert_eq!(
            rule.declared_delta(),
            None,
            "setting a level is not a delta"
        );
        assert!(rule.capture_amount().is_none());
        assert!(rule.capture_concept().is_none());
    }

    #[test]
    fn the_wire_form_is_tagged_kebab_case() {
        let json = serde_json::to_string(&Consequence::RemoveConcept {
            concept: "wip".into(),
        })
        .expect("serializes");
        assert_eq!(json, r#"{"kind":"remove-concept","concept":"wip"}"#);
    }

    #[test]
    fn a_stored_list_round_trips() {
        let rule = Consequences::new(vec![
            Consequence::SetQuantity {
                value: Some(dec("-1")),
            },
            Consequence::AddConcept {
                concept: "today".into(),
            },
        ])
        .expect("legal");
        let json = serde_json::to_string(&rule).expect("serializes");
        let back: Consequences = serde_json::from_str(&json).expect("parses");
        assert_eq!(back, rule);
    }
}
