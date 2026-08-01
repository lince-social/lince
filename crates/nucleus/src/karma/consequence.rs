//! What a rule does when one of its occurrences is applied.
//!
//! A rule used to carry exactly one thing: an amount, captured onto a Record as
//! a signed Fact. That was never a statement about rules — it was the first
//! caller's shape leaking into the model. A rule that can only add a number
//! cannot say "this task is due again", cannot move a card from `@wip` to
//! `@done`, and cannot express the single most common thing people want
//! automated: *change the state of a thing on a schedule*.
//!
//! So a rule carries a **list** of consequences. A list, not one, because the
//! useful cases are pairs: removing `@wip` and adding `@done` is one intention
//! and must be one rule, or a reader has to know that two rules are secretly
//! joined. They apply in order, in one transaction, and either all of them land
//! or none do.
//!
//! # What a consequence may not be
//!
//! Every variant here reduces to a typed Action that already exists and that a
//! person could have performed by hand. That is deliberate and it is the whole
//! safety story: a rule gets no private write path, so an automatic change is
//! auditable by exactly the same means as a manual one, and the Ledger stays
//! the only quantity truth. There is no `run arbitrary thing` variant, and
//! adding one is not a small change — it is a different capability family with
//! its own grant, its own worker and its own review.
//!
//! # Why the target is on the rule, not on the consequence
//!
//! A rule is *about* a Record. Letting each consequence name its own target
//! would make "what does this rule touch?" unanswerable without evaluating it,
//! which is exactly the question a person scanning a list of rules is asking.
//! A rule that needs to touch two Records is two rules.

use serde::{Deserialize, Serialize};

use crate::DecimalValue;
use crate::error::NucleusError;

/// One typed change a rule makes to its target Record.
///
/// Serialized tagged and kebab-case, matching every other Karma wire type, so
/// `{"kind":"add-concept","concept":"done"}` is the stored form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Consequence {
    /// Append a signed delta to the target's Fact chain, classified.
    ///
    /// The original and still the default: this is what a recurring cost, a
    /// salary, or a weekly stock count does. The sign carries direction, so an
    /// income and an expense are one shape.
    CaptureEntry {
        amount: DecimalValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concept: Option<String>,
    },
    /// Set the target's quantity to an exact value.
    ///
    /// This is how a task comes back: `set-quantity -1` makes it a Need again.
    /// Unlike a capture it is not cumulative, so applying it twice leaves the
    /// same value rather than doubling it.
    ///
    /// **`None` means the number the condition carried.** That is what makes
    /// `-1 * freq(@payday) → set-quantity` sayable: the rule computes the
    /// figure instead of restating a constant it could have worked out. A
    /// written number always wins, so a rule whose condition is only a gate —
    /// "when stock is low, set it to 10" — keeps its 10 rather than being
    /// handed the stock level.
    SetQuantity {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<DecimalValue>,
    },
    /// Move the target's quantity by an exact delta. `None` carries.
    AddQuantity {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<DecimalValue>,
    },
    /// Replace the target's *identity* concept — what the thing **is**.
    ///
    /// Rarely what you want. Reclassifying is [`Consequence::AddConcept`];
    /// this changes the answer to "what kind of thing is this?" and Transfer
    /// matching and sync resolve through it.
    SetConcept { concept: String },
    /// Add one of the target's *additional* concepts — what it **counts as**.
    ///
    /// The kanban case: a column that buckets by concept moves a card when this
    /// applies.
    AddConcept { concept: String },
    /// Remove one of the target's additional concepts.
    ///
    /// Removing the identity concept is refused at the Action boundary, because
    /// that would read like a tag edit and behave like a deletion.
    RemoveConcept { concept: String },

    // ----------------------------------------------------------- outward
    // Everything below leaves the Cell, or asks a person something, or moves
    // an obligation rather than a number. They are listed apart because they
    // are a different capability family, not because they are a different kind
    // of rule: the *when* and the *if* above them are identical.
    /// Propose an obligation on the target rather than moving it now.
    ///
    /// A promise is the honest shape for "this is expected": it projects into
    /// the future, it can be kept or broken, and until it is kept nothing has
    /// actually moved. `delta` defaults to the number the condition carried.
    EmitPromise {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<DecimalValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_end: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        party: Option<String>,
    },
    /// Ask a person a question and park the answer in the Decision Queue.
    ///
    /// The one consequence that deliberately does *not* decide. Automation that
    /// can ask is what lets a rule handle the cases it should not settle alone.
    Ask {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        question: Option<String>,
        /// Empty means yes/no, which is what almost every asked question is.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
    },
    /// Tell the person something happened.
    Notify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Run a shell command.
    RunCommand { command: String },
    /// Run a saved Protein query.
    RunQuery {
        query: String,
        /// The query's arguments as a JSON object, kept as written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<String>,
    },
    /// Perform a typed Action, through the same `act` every surface calls.
    RunAction {
        /// The Action's wire form, kept as written and typed when it runs.
        action: String,
    },
    /// Grant sight of the target to a subject.
    ///
    /// A local write, like a concept change: it decides who *may* read, and
    /// sends nothing anywhere. It sits with the outward family because it is
    /// about reaching other people, not because it is queued.
    SetVisibility {
        #[serde(default = "public_subject")]
        subject_kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<String>,
    },
}

fn public_subject() -> String {
    "public".to_string()
}

impl Consequence {
    /// A short stable word for interfaces and error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CaptureEntry { .. } => "capture-entry",
            Self::SetQuantity { .. } => "set-quantity",
            Self::AddQuantity { .. } => "add-quantity",
            Self::SetConcept { .. } => "set-concept",
            Self::AddConcept { .. } => "add-concept",
            Self::RemoveConcept { .. } => "remove-concept",
            Self::EmitPromise { .. } => "emit-promise",
            Self::Ask { .. } => "ask",
            Self::Notify { .. } => "notify",
            Self::RunCommand { .. } => "run-command",
            Self::RunQuery { .. } => "run-query",
            Self::RunAction { .. } => "run-action",
            Self::SetVisibility { .. } => "set-visibility",
        }
    }

    /// Whether this consequence acts outside the Cell's own Ledger.
    ///
    /// The split that matters for authority. Everything false here is a change
    /// the author could have typed by hand into their own Records; everything
    /// true either leaves the machine, spends someone else's attention, or
    /// binds a second party — so it is queued for a worker that can refuse it,
    /// never run inside the evaluation that decided on it.
    pub fn is_outward(&self) -> bool {
        matches!(
            self,
            Self::EmitPromise { .. }
                | Self::Ask { .. }
                | Self::Notify { .. }
                | Self::RunCommand { .. }
                | Self::RunQuery { .. }
                | Self::RunAction { .. }
                | Self::SetVisibility { .. }
        )
    }

    /// Whether this consequence appends a delta to the Ledger.
    ///
    /// The metadata variants still write a zero-delta annotation Fact, which is
    /// how live subscriptions refresh; this asks the narrower question of
    /// whether a *quantity* moved, which is what a projection needs to know.
    pub fn moves_quantity(&self) -> bool {
        matches!(
            self,
            Self::CaptureEntry { .. } | Self::SetQuantity { .. } | Self::AddQuantity { .. }
        )
    }

    /// The signed delta this consequence adds to the target's level, if it is
    /// a delta at all.
    ///
    /// `SetQuantity` is deliberately excluded: assigning `-1` is not a movement
    /// of `-1`, and folding it forward as one would draw a timeline that is
    /// wrong from the first occurrence. A projection can only sum things that
    /// are actually summable.
    pub fn delta(&self) -> Option<&DecimalValue> {
        match self {
            Self::CaptureEntry { amount, .. } => Some(amount),
            Self::AddQuantity { delta } => delta.as_ref(),
            _ => None,
        }
    }

    /// The amount a `CaptureEntry` carries, and nothing else.
    ///
    /// Narrower than [`Self::delta`] on purpose. Applying a date always writes
    /// one entry to mark it done, and that entry must carry an amount **only**
    /// when the rule genuinely captures — otherwise an `add-quantity` rule
    /// would move its delta once through the marker and once through its own
    /// consequence, doubling every application.
    pub fn capture_amount(&self) -> Option<&DecimalValue> {
        match self {
            Self::CaptureEntry { amount, .. } => Some(amount),
            _ => None,
        }
    }

    fn validate(&self) -> Result<(), NucleusError> {
        let concept = match self {
            Self::SetConcept { concept }
            | Self::AddConcept { concept }
            | Self::RemoveConcept { concept } => Some(concept),
            Self::CaptureEntry { concept, .. } => concept.as_ref(),
            _ => None,
        };
        if let Some(concept) = concept
            && concept.trim().is_empty()
        {
            return Err(NucleusError::Parse(format!(
                "`{}` needs a concept, and an empty one is not a concept",
                self.kind()
            )));
        }
        // The outward variants that name a thing to run: a blank one would be
        // stored as an active consequence that can only ever fail, at a moment
        // nobody is watching.
        let named = match self {
            Self::RunCommand { command } => Some(command),
            Self::RunQuery { query, .. } => Some(query),
            Self::RunAction { action } => Some(action),
            _ => None,
        };
        if let Some(named) = named
            && named.trim().is_empty()
        {
            return Err(NucleusError::Parse(format!(
                "`{}` needs something to run",
                self.kind()
            )));
        }
        if let Self::Ask { options, .. } = self
            && options.iter().any(|option| option.trim().is_empty())
        {
            return Err(NucleusError::Parse(
                "a question cannot offer a blank answer".into(),
            ));
        }
        Ok(())
    }
}

/// The ordered, non-empty list a rule carries.
///
/// Non-empty is enforced rather than merely expected: a rule with nothing to do
/// is not a cautious rule, it is a rule whose author lost their edit, and
/// letting it save means it sits in the list looking active forever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Consequences(Vec<Consequence>);

impl Consequences {
    pub fn new(items: Vec<Consequence>) -> Result<Self, NucleusError> {
        if items.is_empty() {
            return Err(NucleusError::Parse(
                "a rule needs at least one consequence".into(),
            ));
        }
        for item in &items {
            item.validate()?;
        }
        // Two consequences of the same kind naming the same concept is a
        // duplicate, not a stronger intention. Catching it here keeps the apply
        // path free of a "did I already do this one" check.
        for (index, item) in items.iter().enumerate() {
            if items[..index].contains(item) {
                return Err(NucleusError::Parse(format!(
                    "`{}` appears twice in one rule",
                    item.kind()
                )));
            }
        }
        Ok(Self(items))
    }

    /// A single capture, the shape every rule had before this type existed.
    pub fn capture(amount: DecimalValue, concept: Option<String>) -> Self {
        Self(vec![Consequence::CaptureEntry { amount, concept }])
    }

    pub fn as_slice(&self) -> &[Consequence] {
        &self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Consequence> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        // Never true by construction; present so clippy and callers agree.
        self.0.is_empty()
    }

    /// The signed delta a projection should fold, if this rule declares one.
    pub fn declared_delta(&self) -> Option<&DecimalValue> {
        self.0.iter().find_map(Consequence::delta)
    }

    /// The amount the entry marking an applied date should carry.
    ///
    /// `None` means zero: the entry still exists, because it is the only record
    /// that the date ran, but it moves nothing.
    pub fn capture_amount(&self) -> Option<&DecimalValue> {
        self.0.iter().find_map(Consequence::capture_amount)
    }

    /// The concept a capture classifies under, if this rule captures.
    pub fn capture_concept(&self) -> Option<&str> {
        self.0.iter().find_map(|item| match item {
            Consequence::CaptureEntry { concept, .. } => concept.as_deref(),
            _ => None,
        })
    }

    pub fn moves_quantity(&self) -> bool {
        self.0.iter().any(Consequence::moves_quantity)
    }
}

impl<'a> IntoIterator for &'a Consequences {
    type Item = &'a Consequence;
    type IntoIter = std::slice::Iter<'a, Consequence>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
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
        // Nothing about a quantity changed, so a timeline gets no point from it.
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
        let rule = Consequences::new(vec![Consequence::SetQuantity { value: Some(dec("-1")) }])
            .expect("legal");
        assert!(rule.moves_quantity());
        assert_eq!(rule.declared_delta(), None, "setting a level is not a delta");
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
            Consequence::SetQuantity { value: Some(dec("-1")) },
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
