use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};
use std::{hash::Hash, sync::Weak};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Template {
    source: String,
    label: String,
}

#[derive(Default)]
pub(super) struct Templates {
    source: Weak<Vec<Value>>,
    acting: Option<String>,
    values: Arc<Vec<Template>>,
}

impl Templates {
    pub(super) fn prepare(
        &mut self,
        source: &Arc<Vec<Value>>,
        acting: Option<&str>,
    ) -> Arc<Vec<Template>> {
        if self.source.as_ptr() != Arc::as_ptr(source) || self.acting.as_deref() != acting {
            let _prepare = nucleus::performance::Span::new("karma-schedule-choice-preparation");
            let elements = model::transfers::elements(RuleFieldKind::Consequence, source, acting);
            let names = (!elements.is_empty()).then(|| model::transfers::Labels::new(source));
            let values: Vec<_> = elements
                .into_iter()
                .map(|source| Template {
                    label: names.as_ref().unwrap().label(&source),
                    source,
                })
                .collect();
            nucleus::performance::add("karma-schedule-choices-prepared", values.len() as u64);
            if self.values.as_ref() != &values {
                self.values = Arc::new(values);
            }
            self.source = Arc::downgrade(source);
            self.acting = acting.map(str::to_owned);
        }
        self.values.clone()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Resolved {
    input: nucleus::karma::scheduled_change::DateInput,
    label: String,
}

#[derive(Component, Default)]
struct Dates {
    source: Weak<Vec<Value>>,
    values: Arc<Vec<(usize, Resolved)>>,
}

pub(super) fn dates(world: &mut World, owner: Entity, parent: Entity, index: usize) {
    let source = world.get::<ScheduleView>(owner).unwrap().dates[index].clone();
    let mut state = world.entity_mut(parent).take::<Dates>().unwrap_or_default();
    if state.source.as_ptr() != Arc::as_ptr(&source) {
        let _prepare = nucleus::performance::Span::new("karma-schedule-choice-preparation");
        let values: Vec<_> = source
            .iter()
            .enumerate()
            .filter_map(|(slot, choice)| {
                let input =
                    nucleus::karma::scheduled_change::DateInput::deserialize(&choice["date"])
                        .ok()?;
                let label = choice["at_ms"]
                    .as_i64()
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map_or_else(|| "Unknown date".into(), |date| date.to_rfc3339());
                Some((
                    slot,
                    Resolved {
                        input,
                        label: format!(
                            "Use {label} · timezone rules {}",
                            choice["rules"].as_str().unwrap_or_default()
                        ),
                    },
                ))
            })
            .collect();
        nucleus::performance::add("karma-schedule-choices-prepared", values.len() as u64);
        if state.values.as_ref() != &values {
            state.values = Arc::new(values);
        }
        state.source = Arc::downgrade(&source);
    }
    draw(
        world,
        owner,
        parent,
        &state.values,
        |(slot, _)| *slot,
        |(_, choice)| choice.label.as_str(),
        |(_, choice)| Command::SelectDate(index, choice.input.clone()),
    );
    world.entity_mut(parent).insert(state);
}

pub(super) fn transfers(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    index: usize,
    values: &Arc<Vec<Template>>,
) {
    draw(
        world,
        owner,
        parent,
        values,
        |choice| choice.source.clone(),
        |choice| choice.label.as_str(),
        |choice| Command::Consequence(index, choice.source.clone()),
    );
}

#[derive(Component)]
struct Buttons<K: Send + Sync + 'static>(RetainedChildren<K>);

impl<K: Send + Sync + 'static> Default for Buttons<K> {
    fn default() -> Self {
        Self(Default::default())
    }
}

fn draw<T, K>(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    values: &Arc<Vec<T>>,
    key: impl Fn(&T) -> K,
    label: impl Fn(&T) -> &str,
    command: impl Fn(&T) -> Command,
) where
    T: Clone + PartialEq + Send + Sync + 'static,
    K: Clone + Eq + Hash + Send + Sync + 'static,
{
    let display = if values.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if world.get::<Node>(parent).unwrap().display != display {
        world.get_mut::<Node>(parent).unwrap().display = display;
    }
    if Presentation::matches(world, parent, |old: &Arc<Vec<T>>| {
        Arc::ptr_eq(old, values) || old.as_ref() == values.as_ref()
    }) {
        return;
    }
    let mut buttons = world
        .entity_mut(parent)
        .take::<Buttons<K>>()
        .unwrap_or_default();
    buttons.0.reconcile(
        world,
        parent,
        values.iter().map(|value| (key(value), value)),
        |world, parent, value| {
            let entity = button(world, parent, owner, label(value), command(value));
            Presentation::remember(world, entity, T::clone(*value));
            nucleus::performance::add("karma-schedule-choice-buttons-built", 1);
            entity
        },
        |world, entity, value| {
            if Presentation::matches(world, entity, |old: &T| old == *value) {
                return;
            }
            presentation::clear(world, entity);
            let label = label(value);
            world.entity_mut(entity).insert((
                crate::native::actions::ActionButton::new(
                    owner,
                    crate::native::actions![command(value)],
                ),
                crate::native::icons::Tooltip(label.into()),
            ));
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
                node.set_label(label);
            }
            crate::native::edit_mode::label(world, entity, label, 14.0);
            Presentation::remember(world, entity, T::clone(*value));
            nucleus::performance::add("karma-schedule-choice-buttons-built", 1);
        },
    );
    Presentation::remember(world, parent, values.clone());
    world.entity_mut(parent).insert(buttons);
}
