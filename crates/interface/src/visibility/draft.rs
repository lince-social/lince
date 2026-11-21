use std::sync::Arc;

use bevy::{prelude::*, text::EditableText};
use nucleus::visibility::{Bound, Condition, Policy};

use crate::controls;

use super::Panel;

#[derive(Default)]
pub(super) struct Rule {
    pub(super) organs: Vec<String>,
    pub(super) persons: Vec<String>,
    pub(super) lower: String,
    pub(super) upper: String,
    pub(super) lower_inclusive: bool,
    pub(super) upper_inclusive: bool,
    pub(super) radius: String,
}

impl From<&Condition> for Rule {
    fn from(condition: &Condition) -> Self {
        Self {
            organs: condition.organs.clone(),
            persons: condition.persons.clone(),
            lower: condition
                .lower
                .map_or_else(String::new, |bound| bound.value.to_string()),
            upper: condition
                .upper
                .map_or_else(String::new, |bound| bound.value.to_string()),
            lower_inclusive: condition.lower.is_none_or(|bound| bound.inclusive),
            upper_inclusive: condition.upper.is_none_or(|bound| bound.inclusive),
            radius: condition
                .radius_metres
                .map_or_else(String::new, |radius| radius.to_string()),
        }
    }
}

impl Rule {
    fn condition(&self) -> Result<Condition, String> {
        let bound = |text, inclusive| {
            whole_number(
                text,
                "Proximity bounds must be whole numbers from 0 to 4294967295",
            )
            .map(|value| value.map(|value| Bound { value, inclusive }))
        };
        Ok(Condition {
            organs: self.organs.clone(),
            persons: self.persons.clone(),
            lower: bound(&self.lower, self.lower_inclusive)?,
            upper: bound(&self.upper, self.upper_inclusive)?,
            radius_metres: whole_number(
                &self.radius,
                "Area radius must be a whole number of metres",
            )?,
        })
    }
}

fn whole_number(text: &str, error: &str) -> Result<Option<u32>, String> {
    let text = text.trim();
    (!text.is_empty())
        .then(|| text.parse().map_err(|_| error.to_owned()))
        .transpose()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Include,
    Exclude,
}

#[derive(Clone, Copy)]
pub(super) enum BoundEnd {
    Lower,
    Upper,
}

pub(super) fn policy(panel: &Panel) -> Result<Policy, String> {
    prepared_policy(panel)
        .as_ref()
        .map(|policy| (**policy).clone())
        .map_err(String::clone)
}

pub(super) fn prepared_policy(panel: &Panel) -> &Result<Arc<Policy>, String> {
    panel.prepared_policy.get_or_init(|| {
        let _preparation = nucleus::performance::Span::new("visibility-policy-preparation");
        nucleus::performance::add("visibility-policies-prepared", 1);
        let policy = Policy {
            include: panel
                .include
                .iter()
                .map(Rule::condition)
                .collect::<Result<_, _>>()?,
            exclude: panel
                .exclude
                .iter()
                .map(Rule::condition)
                .collect::<Result<_, _>>()?,
        };
        policy.validate_for(panel.data).map_err(str::to_owned)?;
        Ok(Arc::new(policy))
    })
}

pub(super) fn read_fields(world: &World, panel: &mut Panel) {
    let mut changed = false;
    for &(index, entity) in &panel.radius_fields {
        if let Some(rule) = panel.include.get_mut(index) {
            changed |= read_value(world, entity, &mut rule.radius);
        }
    }
    for &(side, index, end, entity) in &panel.fields {
        let rules = match side {
            Side::Include => &mut panel.include,
            Side::Exclude => &mut panel.exclude,
        };
        if let Some(rule) = rules.get_mut(index) {
            let value = match end {
                BoundEnd::Lower => &mut rule.lower,
                BoundEnd::Upper => &mut rule.upper,
            };
            changed |= read_value(world, entity, value);
        }
    }
    if changed {
        panel.prepared_policy.take();
    }
}

fn read_value(world: &World, entity: Entity, value: &mut String) -> bool {
    let Some(text) = world.get::<EditableText>(entity) else {
        return false;
    };
    if text.value() == value.as_str() {
        return false;
    }
    controls::copy_value(text, value);
    true
}
