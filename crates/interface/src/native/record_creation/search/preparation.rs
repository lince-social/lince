use super::*;
use crate::native::assertion_editor::{self, DraftInput};

struct AssertionInput {
    values: Arc<Vec<Value>>,
    text: String,
}

struct Inputs {
    fields: Vec<(&'static str, String)>,
    relations: Vec<(bool, Vec<String>)>,
    logs: Vec<(String, String)>,
    assertions: Option<AssertionInput>,
}

fn matches_text(world: &World, entity: Entity, previous: &str) -> bool {
    world
        .get::<EditableText>(entity)
        .map_or_else(|| previous.is_empty(), |text| text.value() == previous)
}

impl Inputs {
    fn matches(&self, world: &World, form: &Form, assertions: Option<&DraftInput<'_>>) -> bool {
        self.fields.len() == form.fields.len()
            && self
                .fields
                .iter()
                .zip(&form.fields)
                .all(|((old, value), (key, entity))| {
                    old == key && matches_text(world, *entity, value)
                })
            && self.relations.len() == form.relations.len()
            && self.relations.iter().zip(&form.relations).all(
                |((old, values), (_, assignee, fields))| {
                    old == assignee
                        && values.len() == fields.len()
                        && values
                            .iter()
                            .zip(fields)
                            .all(|(value, entity)| matches_text(world, *entity, value))
                },
            )
            && self.logs.len() == form.logs.len()
            && self
                .logs
                .iter()
                .zip(&form.logs)
                .all(|((old_start, old_end), (_, start, end))| {
                    matches_text(world, *start, old_start) && matches_text(world, *end, old_end)
                })
            && match (&self.assertions, assertions) {
                (None, None) => true,
                (Some(previous), Some(current)) => {
                    Arc::ptr_eq(&previous.values, current.values)
                        && current.text.value() == previous.text.as_str()
                }
                _ => false,
            }
    }

    fn capture(world: &World, form: &Form, assertions: Option<&DraftInput<'_>>) -> Self {
        Self {
            fields: form
                .fields
                .iter()
                .map(|(key, entity)| (*key, value(world, *entity)))
                .collect(),
            relations: form
                .relations
                .iter()
                .map(|(_, assignee, fields)| {
                    (
                        *assignee,
                        fields.iter().map(|entity| value(world, *entity)).collect(),
                    )
                })
                .collect(),
            logs: form
                .logs
                .iter()
                .map(|(_, start, end)| (value(world, *start), value(world, *end)))
                .collect(),
            assertions: assertions.map(|input| AssertionInput {
                values: Arc::clone(input.values),
                text: input.text.value().to_string(),
            }),
        }
    }

    fn filter(&self, assertions: Option<&DraftInput<'_>>) -> Filter {
        let normalize = |value: &String| value.trim().to_lowercase();
        let mut relations: Vec<_> = self
            .relations
            .iter()
            .map(|(assignee, fields)| (*assignee, fields.iter().map(normalize).collect()))
            .collect();
        relations.extend(
            assertions
                .and_then(|input| input.collect().ok())
                .unwrap_or_default()
                .into_iter()
                .map(|assertion| {
                    (
                        false,
                        vec![
                            assertion.predicate.to_lowercase(),
                            assertion.object.unwrap_or_default().to_lowercase(),
                            assertion.quantity.unwrap_or_default(),
                            assertion.unit.unwrap_or_default().to_lowercase(),
                        ],
                    )
                }),
        );
        Filter {
            fields: self
                .fields
                .iter()
                .map(|(key, value)| (*key, normalize(value)))
                .collect(),
            relations,
            logs: self
                .logs
                .iter()
                .map(|(start, end)| (normalize(start), normalize(end)))
                .collect(),
        }
    }
}

pub(super) struct PreparedFilter {
    inputs: Inputs,
    pub(super) value: Arc<Filter>,
}

impl PreparedFilter {
    pub(super) fn changed(world: &World, form: &Form, previous: Option<&Self>) -> Option<Self> {
        let _preparation = nucleus::performance::Span::new("record-creation-filter-preparation");
        let assertions = assertion_editor::draft_input(world, form.assertions).ok();
        if previous.is_some_and(|old| old.inputs.matches(world, form, assertions.as_ref())) {
            return None;
        }
        let inputs = Inputs::capture(world, form, assertions.as_ref());
        let filter = inputs.filter(assertions.as_ref());
        let value = previous
            .filter(|old| old.value.as_ref() == &filter)
            .map_or_else(|| Arc::new(filter), |old| Arc::clone(&old.value));
        nucleus::performance::add("record-creation-filters-prepared", 1);
        Some(Self { inputs, value })
    }
}

pub(super) struct References {
    rows: Arc<Vec<Value>>,
    concepts: Arc<Vec<Value>>,
    values: HashMap<String, String>,
}

impl References {
    pub(super) fn current(&self, rows: &Arc<Vec<Value>>, concepts: &Arc<Vec<Value>>) -> bool {
        Arc::ptr_eq(&self.rows, rows) && Arc::ptr_eq(&self.concepts, concepts)
    }

    pub(super) fn capture(rows: &Arc<Vec<Value>>, concepts: &Arc<Vec<Value>>) -> Self {
        let _preparation = nucleus::performance::Span::new("record-creation-reference-preparation");
        let mut values: HashMap<_, _> = rows
            .iter()
            .filter_map(|row| {
                Some((
                    row["uid"].as_str()?.to_owned(),
                    format!("{} {}", text(&row["head"]), text(&row["slug"])).to_lowercase(),
                ))
            })
            .collect();
        values.extend(concepts.iter().filter_map(|row| {
            Some((
                row["uid"].as_str()?.to_owned(),
                text(&row["name"]).to_lowercase(),
            ))
        }));
        nucleus::performance::add(
            "record-creation-reference-labels-prepared",
            values.len() as u64,
        );
        Self {
            rows: Arc::clone(rows),
            concepts: Arc::clone(concepts),
            values,
        }
    }
}

pub(super) fn options(
    filter: &Filter,
    rows: &[Value],
    references: Option<&References>,
) -> (usize, Vec<(String, String)>) {
    let _matching = nucleus::performance::Span::new("record-creation-search-matching");
    let empty;
    let references = if let Some(references) = references {
        &references.values
    } else {
        empty = HashMap::<String, String>::new();
        &empty
    };
    let mut total = 0;
    let mut options = Vec::new();
    for row in rows {
        if !filter.matches(row, references) {
            continue;
        }
        if total < 100
            && let Some(uid) = row["uid"].as_str()
        {
            let head = row["head"]
                .as_str()
                .filter(|head| !head.is_empty())
                .unwrap_or("Untitled");
            let label = row["slug"]
                .as_str()
                .map_or_else(|| head.to_string(), |slug| format!("{head}  @{slug}"));
            options.push((uid.to_owned(), label));
        }
        total += 1;
    }
    nucleus::performance::add("record-creation-search-rows-inspected", rows.len() as u64);
    (total, options)
}
