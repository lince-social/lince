// non-lean-yet
// Scope: struct RuleCondition, struct RuleConditionInput, struct Snapshot.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 32, 58.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 50.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 12, 14,
//   15, 19, 20, 21, 39, 50.

use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::{Arc, OnceLock},
};

use nucleus::karma::{Carry, Condition, ConditionError, Gate};

use crate::karma_bindings::ReferenceBinding;

#[derive(Clone)]
pub struct RuleCondition {
    input: Arc<Snapshot>,
    pub gate: Gate,
    pub carry: Carry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleConditionInput {
    pub source: String,
    pub bindings: Vec<ReferenceBinding>,
}

struct Snapshot {
    value: RuleConditionInput,
    parsed: OnceLock<Result<Condition, ConditionError>>,
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            parsed: OnceLock::new(),
        }
    }
}

impl RuleCondition {
    pub fn new(source: String, bindings: Vec<ReferenceBinding>, gate: Gate, carry: Carry) -> Self {
        Self {
            input: Arc::new(Snapshot {
                value: RuleConditionInput { source, bindings },
                parsed: OnceLock::new(),
            }),
            gate,
            carry,
        }
    }

    pub fn parsed(&self) -> Result<Condition, ConditionError> {
        self.input
            .parsed
            .get_or_init(|| {
                let _prepare = nucleus::performance::Span::new("rule-condition-preparation");
                nucleus::performance::add("rule-condition-preparations", 1);
                crate::karma_bindings::apply(&self.source, &self.bindings)
            })
            .clone()
    }
}

impl Deref for RuleCondition {
    type Target = RuleConditionInput;

    fn deref(&self) -> &Self::Target {
        &self.input.value
    }
}

impl DerefMut for RuleCondition {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let snapshot = Arc::make_mut(&mut self.input);
        snapshot.parsed.take();
        &mut snapshot.value
    }
}

impl PartialEq for RuleCondition {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.input, &other.input) || self.input.value == other.input.value)
            && self.gate == other.gate
            && self.carry == other.carry
    }
}

impl Eq for RuleCondition {}

impl fmt::Debug for RuleCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuleCondition")
            .field("source", &self.source)
            .field("bindings", &self.bindings)
            .field("gate", &self.gate)
            .field("carry", &self.carry)
            .finish()
    }
}
