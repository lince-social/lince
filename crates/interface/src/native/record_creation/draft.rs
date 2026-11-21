use super::Form;
use crate::native::assertion_editor;
use bevy::{prelude::*, text::EditableText};
use engine::record_creation::{Assertion, Draft};
use serde_json::{Value, json};
use std::array::from_fn;

pub(super) fn value(world: &World, entity: Entity) -> String {
    world
        .get::<EditableText>(entity)
        .map(|text| text.value().to_string())
        .unwrap_or_default()
}

fn optional(mut value: String) -> Option<String> {
    value.truncate(value.trim_end().len());
    let prefix = value.len() - value.trim_start().len();
    value.replace_range(..prefix, "");
    (!value.is_empty()).then_some(value)
}

pub(super) fn editors<'a>(world: &'a World, form: &'a Form) -> impl Iterator<Item = Entity> + 'a {
    form.fields
        .iter()
        .map(|(_, entity)| *entity)
        .chain(assertion_editor::input(world, form.assertions))
        .chain(
            form.relations
                .iter()
                .flat_map(|(_, _, fields)| fields.iter().copied()),
        )
        .chain(form.logs.iter().flat_map(|(_, start, end)| [*start, *end]))
}

pub(super) fn collect(world: &World, form: &Form) -> Result<Draft, String> {
    if editors(world, form).any(|entity| {
        world.get::<EditableText>(entity).is_some_and(|text| {
            text.is_composing() || text.pending_paste.is_some() || !text.pending_edits.is_empty()
        })
    }) {
        return Err("Finish editing before creating the Record".into());
    }
    let saved = &form.draft;
    let mut draft = Draft {
        uid: saved.uid.clone(),
        head: saved.head.clone(),
        body: saved.body.clone(),
        slug: saved.slug.clone(),
        quantity: saved.quantity.clone(),
        assertions: Vec::new(),
        work: json!({}),
        file_origin: saved.file_origin.clone(),
    };
    for (property, entity) in &form.fields {
        let value = value(world, *entity);
        match *property {
            "head" => draft.head = value,
            "body" => draft.body = value,
            "slug" => draft.slug = optional(value),
            "quantity" => draft.quantity = optional(value).unwrap_or_else(|| "0".into()),
            "start_date" | "due_date" => {
                if let Some(value) = optional(value) {
                    draft.work[if *property == "start_date" {
                        "start"
                    } else {
                        "due"
                    }] = Value::String(value);
                }
            }
            "estimate_min" => {
                if let Some(value) = optional(value) {
                    draft.work["estimate_min"] = json!(
                        value
                            .parse::<f64>()
                            .map_err(|_| "Enter an estimate in minutes")?
                    );
                }
            }
            _ => {}
        }
    }
    draft.assertions = assertion_editor::draft(world, form.assertions)?;
    for (_, assignee, fields) in &form.relations {
        let mut values = fields.iter().map(|field| value(world, *field));
        let [first, object, quantity, unit] = from_fn(|_| values.next().unwrap_or_default());
        if [&first, &object, &quantity, &unit]
            .into_iter()
            .all(|value| value.trim().is_empty())
            && values.all(|value| value.trim().is_empty())
        {
            continue;
        }
        if *assignee {
            draft.assertions.push(Assertion {
                predicate: "feature:assignment".into(),
                object: optional(first),
                quantity: None,
                unit: None,
            });
        } else {
            if fields.len() < 4 {
                return Err(
                    "Some relation fields are missing. Reopen the draft and try again.".into(),
                );
            }
            draft.assertions.push(Assertion {
                predicate: first.trim().trim_start_matches('#').into(),
                object: optional(object),
                quantity: optional(quantity),
                unit: optional(unit),
            });
        }
    }
    let logs: Vec<_> = form
        .logs
        .iter()
        .filter_map(|(_, start, end)| {
            let start = value(world, *start);
            let end = optional(value(world, *end));
            (!start.trim().is_empty() || end.is_some())
                .then(|| json!({"start":start.trim(), "end":end}))
        })
        .collect();
    if !logs.is_empty() {
        draft.work["logs"] = Value::Array(logs);
    }
    draft.validate().map_err(|error| error.to_string())?;
    Ok(draft)
}
