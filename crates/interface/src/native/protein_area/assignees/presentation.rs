// non-lean-yet
// Scope: struct State, struct Choice, struct View, enum Key.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 122, 127, 144,
//   168, 187.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (45, 47, 88, 111, 138, 157, 168, 172, 237, 247).

use super::*;
use crate::host::performance::{self, Count, Stage};
use std::sync::Weak;

#[derive(Default)]
pub(super) struct State {
    input: String,
    query: String,
    source: Weak<Vec<Value>>,
    options: Arc<Vec<Choice>>,
    drawn: Option<View>,
    children: crate::native::retained::Children<Key>,
}

#[derive(PartialEq, Eq)]
struct Choice {
    uid: String,
    title: String,
}

struct View {
    options: Arc<Vec<Choice>>,
    checked: Vec<bool>,
    message: String,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Message,
    Empty,
    Person(String, usize),
}

#[derive(Component)]
struct Label(Entity);

enum Row<'a> {
    Message(&'a str),
    Empty,
    Person(&'a Choice, bool),
}

fn filter(world: &mut World, popup: Entity, input: Entity) {
    let _filter = performance::Span::new(Stage::AssigneePickerFilter);
    let picker = world.get::<Picker>(popup).unwrap();
    let state = &picker.presentation;
    let text = world.get::<EditableText>(input).unwrap();
    let changed = text.value() != state.input.as_str();
    let source_changed = state.source.as_ptr() != Arc::as_ptr(&picker.rows);
    if !changed && !source_changed {
        return;
    }
    let input = changed.then(|| text.value().to_string());
    let query = input.as_ref().map(|input| input.to_lowercase());
    let query_text = query.as_deref().unwrap_or(&state.query);
    let options = (source_changed || query_text != state.query).then(|| {
        performance::count(Count::AssigneePickerFiltersPrepared);
        let mut options = Vec::new();
        let mut search = String::new();
        let mut matches = 0;
        let mut inspected = 0;
        for row in picker.rows.iter() {
            inspected += 1;
            if !query_text.is_empty() {
                search.clear();
                search.push_str(row["head"].as_str().unwrap_or_default());
                search.push(' ');
                search.push_str(row["slug"].as_str().unwrap_or_default());
                if !search.to_lowercase().contains(query_text) {
                    continue;
                }
            }
            matches += 1;
            if let Some(uid) = row["uid"].as_str() {
                options.push(Choice {
                    uid: uid.into(),
                    title: row["head"].as_str().unwrap_or("Unnamed person").into(),
                });
            }
            if matches == 100 {
                break;
            }
        }
        performance::add(Count::AssigneePickerRowsInspected, inspected);
        options
    });
    let source = source_changed.then(|| Arc::downgrade(&picker.rows));
    let mut picker = world.get_mut::<Picker>(popup).unwrap();
    let state = &mut picker.presentation;
    if let Some(input) = input {
        state.input = input;
    }
    if let Some(query) = query
        && state.query != query
    {
        state.query = query;
    }
    if let Some(source) = source {
        state.source = source;
    }
    if let Some(options) = options
        && state.options.as_ref() != &options
    {
        state.options = Arc::new(options);
    }
}

pub(super) fn update(world: &mut World, popup: Entity, field: Entity, input: Entity, list: Entity) {
    let _presentation = performance::Span::new(Stage::AssigneePickerPresentation);
    filter(world, popup, input);
    let picker = world.get::<Picker>(popup).unwrap();
    let options = &picker.presentation.options;
    let selected = selection(world, field);
    let changed = picker.presentation.drawn.as_ref().is_none_or(|previous| {
        previous.options != *options
            || previous.message != picker.message
            || !previous.checked.iter().copied().eq(options
                .iter()
                .map(|choice| selected.contains(choice.uid.as_str())))
    });
    let view = changed.then(|| View {
        options: options.clone(),
        checked: options
            .iter()
            .map(|choice| selected.contains(choice.uid.as_str()))
            .collect(),
        message: picker.message.clone(),
    });
    let enter = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get())
        == Some(input)
        && world
            .get_resource::<ButtonInput<KeyCode>>()
            .is_some_and(|keys| {
                keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)
            })
        && !world.get::<EditableText>(input).unwrap().is_composing();
    let select = enter
        .then(|| {
            options
                .iter()
                .find(|choice| !selected.contains(choice.uid.as_str()))
                .map(|choice| Select(choice.uid.clone()))
        })
        .flatten();
    drop(selected);
    if let Some(select) = select {
        select.apply(world, field);
    }
    if let Some(view) = view {
        draw(world, popup, field, list, view);
    }
}

fn set_label(world: &mut World, entity: Entity, prefix: &str, value: &str) {
    let mut text = world.get_mut::<Text>(entity).unwrap();
    if text.0.strip_prefix(prefix) != Some(value) {
        text.0.clear();
        text.0.push_str(prefix);
        text.0.push_str(value);
        performance::count(Count::AssigneePickerLabelsUpdated);
    }
}

fn draw(world: &mut World, popup: Entity, field: Entity, list: Entity, view: View) {
    let _draw = performance::Span::new(Stage::AssigneePickerDraw);
    let message = world.get::<Picker>(popup).unwrap().message.clone();
    let mut children = std::mem::take(
        &mut world
            .get_mut::<Picker>(popup)
            .unwrap()
            .presentation
            .children,
    );
    let mut occurrences = HashMap::new();
    let rows = (!message.is_empty())
        .then_some((Key::Message, Row::Message(message.as_str())))
        .into_iter()
        .chain((view.options.is_empty() && message.is_empty()).then_some((Key::Empty, Row::Empty)))
        .chain(
            view.options
                .iter()
                .zip(&view.checked)
                .map(|(choice, checked)| {
                    let occurrence = occurrences.entry(choice.uid.as_str()).or_insert(0);
                    let key = Key::Person(choice.uid.clone(), *occurrence);
                    *occurrence += 1;
                    (key, Row::Person(choice, *checked))
                }),
        );
    children.reconcile(
        world,
        list,
        rows,
        |world, parent, row| {
            performance::count(Count::AssigneePickerRowsCreated);
            match row {
                Row::Message(message) => {
                    crate::native::edit_mode::label(world, parent, message, 14.0)
                }
                Row::Empty => {
                    crate::native::edit_mode::label(world, parent, "No matching people", 14.0)
                }
                Row::Person(choice, checked) => {
                    let button = world
                        .spawn((
                            crate::native::sand::button(0),
                            Node {
                                width: percent(100),
                                min_height: px(32),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            ActionButton::new(
                                field,
                                crate::native::actions![Select(choice.uid.clone())],
                            ),
                            ChildOf(parent),
                        ))
                        .id();
                    let label = crate::native::edit_mode::label(
                        world,
                        button,
                        &format!("{} {}", if *checked { "[x]" } else { "[ ]" }, choice.title),
                        14.0,
                    );
                    world.entity_mut(button).insert(Label(label));
                    button
                }
            }
        },
        |world, entity, row| match row {
            Row::Message(message) => set_label(world, entity, "", message),
            Row::Empty => set_label(world, entity, "", "No matching people"),
            Row::Person(choice, checked) => {
                let label = world.get::<Label>(entity).unwrap().0;
                set_label(
                    world,
                    label,
                    if *checked { "[x] " } else { "[ ] " },
                    &choice.title,
                );
            }
        },
    );
    let mut picker = world.get_mut::<Picker>(popup).unwrap();
    picker.presentation.children = children;
    picker.presentation.drawn = Some(view);
}
