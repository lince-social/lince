use super::*;

type Choices = Vec<(String, Command)>;

pub(super) struct Field {
    index: usize,
    label: String,
    control: Control,
}

enum Control {
    Choice {
        selected: String,
        choices: Choices,
    },
    Input {
        value: String,
        multiline: bool,
        picker: Option<(&'static str, Choices)>,
        find: bool,
    },
}

struct Catalogue<'a> {
    records: &'a [Value],
    organ: Option<&'a str>,
    lowered: HashMap<usize, String>,
}

impl Catalogue<'_> {
    fn references(&mut self, field_index: usize, field: &model::Field, kind: &str) -> Choices {
        let query = field.value.to_lowercase();
        let mut choices = Vec::new();
        for (index, record) in self.records.iter().enumerate() {
            let matches = match kind {
                "record" => {
                    record["transfer_picker_unit"] != true
                        && (!matches!(field.path.as_str(), "/local_record" | "/record")
                            || self.organ.is_some_and(|organ| record["organ"] == organ))
                }
                "unit" => record["transfer_picker_unit"] == true,
                kind => record["kind"] == kind,
            };
            if !matches {
                continue;
            }
            let uid = model::display_ref(&record["uid"]);
            if !field.value.is_empty() && uid.as_ref() != field.value.as_str() {
                let title = self.lowered.entry(index).or_insert_with(|| {
                    nucleus::performance::add("transfer-choice-titles-prepared", 1);
                    model::title_ref(record).to_lowercase()
                });
                if !title.contains(&query) {
                    continue;
                }
            }
            choices.push((title(record), Command::Set(field_index, uid.into_owned())));
            if choices.len() == 40 {
                break;
            }
        }
        choices
    }

    fn people(&self, field_index: usize, field: &model::Field) -> Choices {
        let existing: HashSet<_> = field.value.split(',').map(str::trim).collect();
        self.records
            .iter()
            .filter(|record| {
                record["kind"] == "person"
                    && !existing.contains(model::display_ref(&record["uid"]).as_ref())
            })
            .take(40)
            .map(|record| {
                let uid = model::display_ref(&record["uid"]);
                let value = if field.value.trim().is_empty() {
                    uid.into_owned()
                } else {
                    format!("{}, {}", field.value, uid)
                };
                (title(record), Command::Set(field_index, value))
            })
            .collect()
    }
}

pub(super) fn prepare(form: &Form, view: &View) -> Vec<Field> {
    let _span = nucleus::performance::Span::new("transfer-form-choice-preparation");
    let mut catalogue = Catalogue {
        records: &view.records,
        organ: view.context["organ"].as_str(),
        lowered: HashMap::new(),
    };
    form.fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let control = match &field.kind {
                FieldKind::OptionalChoice(choices) => {
                    let selected = choices
                        .iter()
                        .find(|(value, _)| value == &field.value)
                        .map_or("None", |(_, label)| label.as_str())
                        .to_owned();
                    let choices =
                        std::iter::once(("None".into(), Command::Set(index, String::new())))
                            .chain(choices.iter().map(|(value, label)| {
                                (label.clone(), Command::Set(index, value.clone()))
                            }))
                            .collect();
                    Control::Choice { selected, choices }
                }
                FieldKind::DataChoice(choices) => {
                    let mut selected = None;
                    let choices = choices
                        .iter()
                        .map(|(value, label)| {
                            nucleus::performance::add("transfer-choice-values-encoded", 1);
                            let encoded = value.to_string();
                            if selected.is_none() && encoded == field.value {
                                selected = Some(label.clone());
                            }
                            (label.clone(), Command::Set(index, encoded))
                        })
                        .collect();
                    Control::Choice {
                        selected: selected.unwrap_or_else(|| "Choose a Need".into()),
                        choices,
                    }
                }
                FieldKind::Choice(choices) => Control::Choice {
                    selected: model::human(&field.value),
                    choices: choices
                        .iter()
                        .map(|value| (model::human(value), Command::Set(index, value.clone())))
                        .collect(),
                },
                _ => {
                    let picker = match &field.kind {
                        FieldKind::Reference(kind) => {
                            Some(("Choose by name", catalogue.references(index, field, kind)))
                        }
                        FieldKind::People => {
                            Some(("Add invited Person", catalogue.people(index, field)))
                        }
                        FieldKind::Selections(options) => {
                            let selected: Vec<_> = field
                                .value
                                .split(',')
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .collect();
                            let choices = options
                                .iter()
                                .map(|(uid, label)| {
                                    let included = selected.contains(&uid.as_str());
                                    let mut values: Vec<_> = selected
                                        .iter()
                                        .copied()
                                        .filter(|value| *value != uid)
                                        .collect();
                                    if !included {
                                        values.push(uid);
                                    }
                                    (
                                        format!("{} {label}", if included { "●" } else { "○" }),
                                        Command::Set(index, values.join(", ")),
                                    )
                                })
                                .collect();
                            Some(("Choose included items", choices))
                        }
                        _ => None,
                    };
                    Control::Input {
                        value: field.value.clone(),
                        multiline: matches!(field.path.as_str(), "/body" | "/formula")
                            || field.path.ends_with("/item/description"),
                        picker,
                        find: matches!(field.kind, FieldKind::Reference(_)),
                    }
                }
            };
            Field {
                index,
                label: field.label.clone(),
                control,
            }
        })
        .collect()
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, fields: Vec<Field>) {
    for Field {
        index,
        label,
        control,
    } in fields
    {
        match control {
            Control::Choice { selected, choices } => {
                crate::native::edit_mode::label(world, parent, &label, 12.0);
                ui::picker(world, parent, owner, &selected, choices);
            }
            Control::Input {
                value,
                multiline,
                picker: choices,
                find,
            } => {
                input(world, parent, owner, Some(index), &label, &value, multiline);
                if let Some((caption, choices)) = choices {
                    ui::picker(world, parent, owner, caption, choices);
                }
                if find {
                    button(world, parent, owner, "Find matching names", Command::Find);
                }
            }
        }
    }
}
