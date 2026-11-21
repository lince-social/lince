// non-lean-yet
// Scope: struct ComparisonInput, enum UnitInput, struct AssertionInput, enum ConditionInput.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;

pub(super) struct ComparisonInput<'a> {
    source: &'a Comparison,
    lower: Option<DecimalValue>,
    upper: Option<DecimalValue>,
}

impl<'a> ComparisonInput<'a> {
    pub(super) fn new(source: &'a Comparison) -> Self {
        let lower = decimal(&source.value);
        let upper = (lower.is_some() && source.operator == Operator::Between)
            .then(|| decimal(&source.upper))
            .flatten();
        Self {
            source,
            lower,
            upper,
        }
    }

    pub(super) fn valid(&self) -> bool {
        self.lower.is_some_and(|lower| {
            self.source.operator != Operator::Between
                || self.upper.is_some_and(|upper| {
                    let order = lower.exact_numeric_cmp(upper);
                    order.is_lt()
                        || (order.is_eq() && self.source.include_lower && self.source.include_upper)
                })
        })
    }

    pub(super) fn matches(&self, value: &Value) -> bool {
        let Some(value) = value.as_str().and_then(decimal) else {
            return false;
        };
        let Some(lower) = self.lower else {
            return false;
        };
        let order = value.exact_numeric_cmp(lower);
        match self.source.operator {
            Operator::Equal => order.is_eq(),
            Operator::NotEqual => !order.is_eq(),
            Operator::Greater => order.is_gt(),
            Operator::AtLeast => !order.is_lt(),
            Operator::Less => order.is_lt(),
            Operator::AtMost => !order.is_gt(),
            Operator::Between => self.upper.is_some_and(|upper| {
                let end = value.exact_numeric_cmp(upper);
                (order.is_gt() || (self.source.include_lower && order.is_eq()))
                    && (end.is_lt() || (self.source.include_upper && end.is_eq()))
            }),
        }
    }
}

enum UnitInput<'a> {
    Any,
    Unitless,
    Exact(&'a str),
}

impl<'a> UnitInput<'a> {
    fn new(unit: &'a Unit) -> Self {
        match unit {
            Unit::Any => Self::Any,
            Unit::Unitless => Self::Unitless,
            Unit::Exact(value) => Self::Exact(value.trim()),
        }
    }

    fn matches(&self, entry: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Unitless => entry["unit"].is_null(),
            Self::Exact(unit) => {
                entry["unit"].as_str() == Some(*unit)
                    || entry["unit_name"].as_str() == Some(*unit)
                    || entry["relation_context"]["unit_name"].as_str() == Some(*unit)
            }
        }
    }
}

struct AssertionInput<'a> {
    predicate: &'a str,
    family: bool,
    quantity: Option<ComparisonInput<'a>>,
    unit: UnitInput<'a>,
}

impl<'a> AssertionInput<'a> {
    fn new(
        predicate: &'a str,
        family: bool,
        quantity: Option<&'a Comparison>,
        unit: &'a Unit,
    ) -> Self {
        Self {
            predicate: predicate.trim(),
            family,
            quantity: quantity.map(ComparisonInput::new),
            unit: UnitInput::new(unit),
        }
    }

    fn matches(&self, assertion: &Value) -> bool {
        !assertion.is_null()
            && (self.predicate == "*"
                || if self.family {
                    assertion["families"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|value| value.as_str() == Some(self.predicate))
                } else {
                    assertion["predicate_uid"].as_str() == Some(self.predicate)
                        || assertion["predicate"].as_str() == Some(self.predicate)
                })
            && self
                .quantity
                .as_ref()
                .is_none_or(|comparison| comparison.matches(&assertion["quantity"]))
            && self.unit.matches(assertion)
    }
}

enum ConditionInput<'a> {
    Quantity(RecordSide, ComparisonInput<'a>, UnitInput<'a>),
    Assertion(RecordSide, AssertionInput<'a>, Direction, bool),
    Link(AssertionInput<'a>),
}

impl<'a> ConditionInput<'a> {
    fn new(condition: &'a Condition) -> Self {
        match condition {
            Condition::Quantity {
                record,
                comparison,
                unit,
            } => Self::Quantity(
                *record,
                ComparisonInput::new(comparison),
                UnitInput::new(unit),
            ),
            Condition::Assertion {
                record,
                predicate,
                family,
                direction,
                present,
                quantity,
                unit,
            } => Self::Assertion(
                *record,
                AssertionInput::new(predicate, *family, quantity.as_ref(), unit),
                *direction,
                *present,
            ),
            Condition::Link {
                predicate,
                family,
                quantity,
                unit,
            } => Self::Link(AssertionInput::new(
                predicate,
                *family,
                quantity.as_ref(),
                unit,
            )),
        }
    }

    fn needs_row(&self, target: Target) -> bool {
        let (Self::Quantity(side, ..) | Self::Assertion(side, ..)) = self else {
            return false;
        };
        matches!(
            (target, *side),
            (Target::Card, RecordSide::Record)
                | (Target::Link, RecordSide::Source | RecordSide::Destination)
        )
    }

    fn matches<'v>(&self, row: impl Fn(RecordSide) -> &'v Value, link: &Value) -> bool {
        match self {
            Self::Quantity(side, comparison, unit) => {
                comparison.matches(&row(*side)["quantity"]) && unit.matches(row(*side))
            }
            Self::Assertion(side, input, direction, present) => {
                let record = row(*side);
                let uid = record["uid"].as_str();
                let found = record["relation_context"]["assertions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|assertion| {
                        let incident = match direction {
                            Direction::Outgoing => {
                                uid.is_some() && assertion["from"].as_str() == uid
                            }
                            Direction::Incoming => assertion["to"].as_str() == uid && uid.is_some(),
                            Direction::Either => {
                                uid.is_some()
                                    && (assertion["from"].as_str() == uid
                                        || assertion["to"].as_str() == uid)
                            }
                        };
                        incident && input.matches(assertion)
                    });
                record["relation_context"]["assertions"].is_array() && found == *present
            }
            Self::Link(input) => input.matches(link),
        }
    }
}

struct RuleInput<'a> {
    source: &'a Rule,
    conditions: Vec<ConditionInput<'a>>,
}

pub(super) struct Rules<'a> {
    rules: Vec<RuleInput<'a>>,
    pub(super) needs_rows: bool,
    pub(super) needs_assertions: bool,
}

impl<'a> Rules<'a> {
    pub(super) fn new(settings: &'a Settings, target: Target) -> Self {
        let _preparation = nucleus::performance::Span::new("relation-style-rule-preparation");
        let rules: Vec<_> = settings
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.target == target)
            .map(|source| RuleInput {
                source,
                conditions: source.conditions.iter().map(ConditionInput::new).collect(),
            })
            .collect();
        let needs_rows = rules.iter().any(|rule| {
            rule.conditions
                .iter()
                .any(|condition| condition.needs_row(target))
        });
        let needs_assertions = target == Target::Link
            && rules.iter().any(|rule| {
                rule.conditions
                    .iter()
                    .any(|condition| matches!(condition, ConditionInput::Link(_)))
            });
        nucleus::performance::add("relation-style-rules-prepared", rules.len() as u64);
        Self {
            rules,
            needs_rows,
            needs_assertions,
        }
    }

    pub(super) fn evaluate(
        &self,
        record: &Value,
        source: &Value,
        destination: &Value,
        link: &Value,
    ) -> Colors {
        let mut colors = Colors::default();
        let row = |side| match side {
            RecordSide::Record => record,
            RecordSide::Source => source,
            RecordSide::Destination => destination,
        };
        for rule in &self.rules {
            let matches = |condition: &ConditionInput<'_>| condition.matches(row, link);
            let matched = rule.conditions.is_empty()
                || match rule.source.mode {
                    Mode::All => rule.conditions.iter().all(matches),
                    Mode::Any => rule.conditions.iter().any(matches),
                };
            if matched {
                colors.fill(&rule.source.colors);
            }
        }
        colors
    }
}
