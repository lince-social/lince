use super::*;
use std::sync::{OnceLock, Weak};

struct Element {
    value: String,
    folded: String,
    folded_label: OnceLock<String>,
}

#[derive(Default)]
pub(in crate::native::karma_castle) struct Catalog {
    ordinary: model::Suggestions,
    rules: Weak<Vec<Rule>>,
    records: Weak<Vec<Value>>,
    transfers: Weak<Vec<Value>>,
    acting: Option<String>,
    transfer_elements: [Option<Vec<Element>>; 3],
    names: model::transfers::Labels,
    shared_labels: HashMap<String, (String, Arc<str>)>,
    element_labels: HashMap<String, Arc<str>>,
}

pub(super) struct Row {
    pub(super) command: Command,
    pub(super) label: Arc<str>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Shared(String),
    Element(String, usize),
}

#[derive(Default)]
pub(super) struct State(crate::native::retained::Children<Key>);

#[derive(Component, Clone, Copy)]
struct Choice {
    field: Entity,
    label: Entity,
    slot: usize,
}

pub(super) fn draw(world: &mut World, field: Entity, rows: &[Row]) {
    let _draw = nucleus::performance::Span::new("karma-choice-presentation");
    let mut input = world.get_mut::<Input>(field).unwrap();
    let owner = input.owner;
    let parent = input.suggestions;
    let mut state = std::mem::take(&mut input.choices);
    let mut occurrences = HashMap::<&str, usize>::new();
    let keyed = rows.iter().enumerate().map(|(slot, row)| {
        let key = match &row.command {
            Command::Link(_, _, shared) => Key::Shared(shared.uid.clone()),
            Command::Insert(_, _, element, _) => {
                let occurrence = occurrences.entry(element).or_default();
                let key = Key::Element(element.clone(), *occurrence);
                *occurrence += 1;
                key
            }
            _ => unreachable!("a suggestion links or inserts a field"),
        };
        (key, (slot, row))
    });
    state.0.reconcile(
        world,
        parent,
        keyed,
        |world, parent, &(slot, row)| {
            let entity = button(world, parent, owner, row.command.clone(), &row.label);
            world.get_mut::<Node>(entity).unwrap().width = percent(100);
            let label = world.get::<Children>(entity).unwrap()[0];
            world
                .entity_mut(entity)
                .insert(Choice { field, label, slot });
            nucleus::performance::add("karma-choice-buttons-created", 1);
            entity
        },
        |world, entity, &(slot, row)| {
            let previous = world.get::<Choice>(entity).copied();
            let same_command = previous.is_some_and(|choice| {
                choice.field == field
                    && world
                        .get::<Input>(field)
                        .unwrap()
                        .options
                        .get(choice.slot)
                        .is_some_and(|(option, command)| {
                            *option == entity && *command == row.command
                        })
            });
            if !same_command
                || world
                    .get::<ActionButton>(entity)
                    .is_none_or(|button| button.target != owner)
            {
                world.entity_mut(entity).insert(ActionButton::new(
                    owner,
                    crate::native::actions![row.command.clone()],
                ));
                nucleus::performance::add("karma-choice-actions-updated", 1);
            }
            let label = previous
                .map(|choice| choice.label)
                .filter(|label| {
                    world
                        .get::<ChildOf>(*label)
                        .is_some_and(|child| child.parent() == entity)
                })
                .unwrap_or_else(|| button_label(world, entity, &row.label));
            if world
                .get::<Text>(label)
                .is_none_or(|text| text.0 != row.label.as_ref())
            {
                world
                    .entity_mut(label)
                    .insert(Text::new(row.label.as_ref()));
                nucleus::performance::add("karma-choice-labels-published", 1);
            }
            if previous.is_none_or(|choice| {
                choice.field != field || choice.label != label || choice.slot != slot
            }) {
                world
                    .entity_mut(entity)
                    .insert(Choice { field, label, slot });
            }
            nucleus::performance::add("karma-choice-buttons-retained", 1);
        },
    );
    let options: Vec<_> = world
        .get::<Children>(parent)
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            world
                .get::<Choice>(*entity)
                .filter(|choice| choice.field == field && choice.slot < rows.len())
                .map(|choice| (*entity, choice.slot))
        })
        .collect();
    let mut input = world.get_mut::<Input>(field).unwrap();
    for (index, &(entity, slot)) in options.iter().enumerate() {
        let command = &rows[slot].command;
        if let Some(option) = input.options.get_mut(index) {
            if option.0 != entity || option.1 != *command {
                *option = (entity, command.clone());
            }
        } else {
            input.options.push((entity, command.clone()));
        }
    }
    input.options.truncate(options.len());
    input.choices = state;
    let display = if rows.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    let mut node = world.get_mut::<Node>(parent).unwrap();
    if node.display != display {
        node.display = display;
    }
}

pub(super) fn prepare(
    world: &mut World,
    owner: Entity,
    row: usize,
    index: usize,
    prefix: &str,
    selection: &std::ops::Range<usize>,
) -> Vec<Row> {
    let _prepare = nucleus::performance::Span::new("karma-choice-preparation");
    let mut view = world.get_mut::<View>(owner).unwrap();
    let View {
        rules,
        records,
        frequencies,
        transfers,
        acting_person,
        completion,
        ..
    } = &mut *view;
    let transfer_changed = completion.transfers.as_ptr() != Arc::as_ptr(transfers);
    if transfer_changed {
        completion.transfers = Arc::downgrade(transfers);
        completion.names = model::transfers::Labels::new(transfers);
    }
    if transfer_changed || completion.acting.as_deref() != acting_person.as_deref() {
        completion.acting.clone_from(acting_person);
        completion.transfer_elements = Default::default();
    }
    if transfer_changed
        || completion.rules.as_ptr() != Arc::as_ptr(rules)
        || completion.records.as_ptr() != Arc::as_ptr(records)
    {
        completion.rules = Arc::downgrade(rules);
        completion.records = Arc::downgrade(records);
        completion.shared_labels.clear();
        completion.element_labels.clear();
    }
    let kind = RuleFieldKind::ALL[index];
    let mut values = completion
        .ordinary
        .select(kind, prefix, rules, records, frequencies);
    let tail = model::suggestion_tail(prefix);
    let elements = completion.transfer_elements[index].get_or_insert_with(|| {
        let _prepare = nucleus::performance::Span::new("karma-transfer-choice-preparation");
        let values: Vec<_> = model::transfers::elements(kind, transfers, acting_person.as_deref())
            .into_iter()
            .map(|value| {
                let folded = value.to_lowercase();
                Element {
                    value,
                    folded,
                    folded_label: OnceLock::new(),
                }
            })
            .collect();
        nucleus::performance::add("karma-transfer-choices-prepared", values.len() as u64);
        values
    });
    values.extend(
        elements
            .iter()
            .filter(|element| {
                element.folded.contains(&tail)
                    || element
                        .folded_label
                        .get_or_init(|| completion.names.label(&element.value).to_lowercase())
                        .contains(&tail)
            })
            .take(16)
            .map(|element| Suggestion::Element(element.value.clone())),
    );
    let shared: HashSet<_> = values
        .iter()
        .filter_map(|value| match value {
            Suggestion::Shared(field) => Some(field.uid.as_str()),
            _ => None,
        })
        .collect();
    let selected: HashSet<_> = values
        .iter()
        .filter_map(|value| match value {
            Suggestion::Element(element) => Some(element.as_str()),
            _ => None,
        })
        .collect();
    completion
        .shared_labels
        .retain(|uid, _| shared.contains(uid.as_str()));
    completion
        .element_labels
        .retain(|element, _| selected.contains(element.as_str()));
    drop((shared, selected));
    values
        .into_iter()
        .map(|value| match value {
            Suggestion::Shared(field) => {
                let label = completion
                    .shared_labels
                    .get(&field.uid)
                    .filter(|(source, _)| *source == field.source)
                    .map(|(_, label)| label.clone())
                    .unwrap_or_else(|| {
                        let label: Arc<str> = format!("Link · {}", field.source).into();
                        completion
                            .shared_labels
                            .insert(field.uid.clone(), (field.source.clone(), label.clone()));
                        label
                    });
                Row {
                    command: Command::Link(row, index, field),
                    label,
                }
            }
            Suggestion::Element(element) => {
                let label = completion
                    .element_labels
                    .get(&element)
                    .cloned()
                    .unwrap_or_else(|| {
                        let label: Arc<str> = completion
                            .names
                            .label(&model::element_label(&element, records))
                            .into();
                        completion
                            .element_labels
                            .insert(element.clone(), label.clone());
                        label
                    });
                Row {
                    command: Command::Insert(row, index, element, selection.clone()),
                    label,
                }
            }
        })
        .collect()
}
