// non-lean-yet
// Scope: enum Status, struct Source, struct Row, struct Rows.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 56, 131, 132,
//   139, 141, 142, 143, 151, 168, 265.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (128, 161, 166, 179, 187, 228).

use super::*;
use crate::native::retained;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq)]
enum Status {
    Rule { enabled: bool, revision: u64 },
    Error(String),
    Loading,
}

impl Status {
    fn enabled(&self) -> bool {
        matches!(self, Self::Rule { enabled: true, .. })
    }

    fn valid(&self) -> bool {
        matches!(self, Self::Rule { .. })
    }

    fn matches(&self, payload: Option<&Result<Payload, String>>) -> bool {
        match (self, payload) {
            (Self::Rule { enabled, revision }, Some(Ok(Payload::Rule(rule)))) => {
                *enabled == rule.enabled && *revision == rule.revision
            }
            (Self::Error(previous), Some(Err(current))) => previous == current,
            (Self::Loading, None | Some(Ok(Payload::Window(_)))) => true,
            _ => false,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Source {
    uid: String,
    title: String,
    status: Status,
}

impl Source {
    fn matches(&self, uid: &str, entry: Option<&Entry>) -> bool {
        self.uid == uid
            && self.title == entry.map_or(uid, |entry| entry.title.as_str())
            && self.status.matches(entry.map(|entry| &entry.payload))
    }

    fn read(uid: &str, entry: Option<&Entry>) -> Self {
        Self {
            uid: uid.to_owned(),
            title: entry.map_or(uid, |entry| entry.title.as_str()).to_owned(),
            status: match entry.map(|entry| &entry.payload) {
                Some(Ok(Payload::Rule(rule))) => Status::Rule {
                    enabled: rule.enabled,
                    revision: rule.revision,
                },
                Some(Err(error)) => Status::Error(error.clone()),
                _ => Status::Loading,
            },
        }
    }

    fn label(&self, index: usize) -> String {
        let status = match &self.status {
            Status::Rule { revision, .. } => {
                std::borrow::Cow::Owned(format!("revision {revision}"))
            }
            Status::Error(error) => std::borrow::Cow::Borrowed(error.as_str()),
            Status::Loading => std::borrow::Cow::Borrowed("Loading…"),
        };
        format!(
            "{}. {} · {} · {}",
            index + 1,
            self.title,
            if self.status.enabled() {
                "enabled"
            } else {
                "disabled"
            },
            status
        )
    }
}

#[derive(Component)]
struct Row {
    source: Source,
    index: usize,
    label: Entity,
    controls: [(Entity, Entity); 6],
}

#[derive(Default)]
pub(super) struct Rows {
    source: Vec<Source>,
    rows: retained::Children<(String, usize)>,
    parent: Option<Entity>,
}

struct View<'a> {
    source: &'a Source,
    index: usize,
    count: usize,
    page: Entity,
}

fn blocked(source: &Source, index: usize, count: usize, action: usize) -> bool {
    action == 3 && index == 0
        || action == 4 && index + 1 == count
        || action == 2 && !source.status.valid()
}

fn create(world: &mut World, parent: Entity, view: &View<'_>) -> Entity {
    let row = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..Default::default()
            },
            ChildOf(parent),
        ))
        .id();
    let label = wrapped(world, row, &view.source.label(view.index), 14.0);
    let actions = crate::native::sand_panel::row(world, row);
    world.get_mut::<Node>(actions).unwrap().flex_wrap = FlexWrap::Wrap;
    let uid = &view.source.uid;
    let controls = [
        ("Edit", Change::Edit(uid.clone())),
        ("Open Record", Change::Open(uid.clone())),
        (
            if view.source.status.enabled() {
                "Disable"
            } else {
                "Enable"
            },
            Change::Enable(uid.clone()),
        ),
        ("Earlier", Change::Move(uid.clone(), false)),
        ("Later", Change::Move(uid.clone(), true)),
        ("Remove from list", Change::Remove(uid.clone())),
    ]
    .into_iter()
    .enumerate()
    .map(|(kind, (caption, action))| {
        let button = crate::native::sand_panel::button(world, actions, view.page, caption, action);
        world.entity_mut(button).insert((
            RuleControl {
                uid: uid.clone(),
                action: kind,
            },
            Name::new(format!("opening.rule.{uid}.control.{kind}")),
        ));
        disable(
            world,
            button,
            blocked(view.source, view.index, view.count, kind),
        );
        let label = world.get::<Children>(button).unwrap()[0];
        (button, label)
    })
    .collect::<Vec<_>>()
    .try_into()
    .unwrap();
    world.entity_mut(row).insert(Row {
        source: view.source.clone(),
        index: view.index,
        label,
        controls,
    });
    engine::performance::add("opening-rule-rows-created", 1);
    engine::performance::add("opening-rule-labels-prepared", 1);
    row
}

fn update(world: &mut World, row: Entity, view: &View<'_>) {
    let previous = world.get::<Row>(row).unwrap();
    let (label, controls, changed) = (
        previous.label,
        previous.controls,
        previous.source != *view.source || previous.index != view.index,
    );
    if changed {
        text(world, label, view.source.label(view.index));
        let mut state = world.get_mut::<Row>(row).unwrap();
        state.source.clone_from(view.source);
        state.index = view.index;
        engine::performance::add("opening-rule-labels-prepared", 1);
    }
    let enabled = view.source.status.enabled();
    let label = if enabled { "Disable" } else { "Enable" };
    caption(world, controls[2], label);
    if let Some(mut tooltip) = world.get_mut::<crate::native::icons::Tooltip>(controls[2].0)
        && tooltip.0 != label
    {
        label.clone_into(&mut tooltip.0);
    }
    let part = if enabled {
        "button-disable"
    } else {
        "button-enable"
    };
    if let Some(mut name) =
        world.get_mut::<crate::native::canvas_host::authoring::PartName>(controls[2].0)
        && name.0 != part
    {
        part.clone_into(&mut name.0);
    }
    for (kind, (button, _)) in controls.into_iter().enumerate() {
        disable(
            world,
            button,
            blocked(view.source, view.index, view.count, kind),
        );
    }
}

impl Rows {
    pub(super) fn refresh(
        &mut self,
        world: &mut World,
        page: Entity,
        parent: Entity,
        toggle: Entity,
    ) {
        let order = &world.get::<Page>(page).unwrap().order;
        let entries = world
            .get_resource::<Catalog>()
            .map(|catalog| &catalog.rules.entries);
        let changed = self.parent != Some(parent)
            || order.len() != self.source.len()
            || order.iter().zip(&self.source).any(|(uid, previous)| {
                !previous.matches(uid, entries.and_then(|entries| entries.get(uid)))
            });
        if !changed {
            return;
        }
        let _preparation = engine::performance::Span::new("opening-rule-list-preparation");
        let previous = world
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get)
            .and_then(|entity| world.get::<RuleControl>(entity))
            .cloned();
        let previous_index = previous
            .as_ref()
            .and_then(|control| self.source.iter().position(|row| row.uid == control.uid));
        for (index, uid) in order.iter().enumerate() {
            let entry = entries.and_then(|entries| entries.get(uid));
            if let Some(previous) = self.source.get_mut(index) {
                if !previous.matches(uid, entry) {
                    *previous = Source::read(uid, entry);
                }
            } else {
                self.source.push(Source::read(uid, entry));
            }
        }
        self.source.truncate(order.len());
        self.parent = Some(parent);
        let count = self.source.len();
        let mut occurrences = HashMap::new();
        let rows = self.source.iter().enumerate().map(|(index, source)| {
            let occurrence = occurrences.entry(source.uid.as_str()).or_insert(0);
            let key = (source.uid.clone(), *occurrence);
            *occurrence += 1;
            (
                key,
                View {
                    source,
                    index,
                    count,
                    page,
                },
            )
        });
        self.rows.reconcile(world, parent, rows, create, update);
        let mut restored = None;
        let mut fallback = None;
        if let Some(children) = world.get::<Children>(parent) {
            for entity in children.iter() {
                let Some(row) = world.get::<Row>(entity) else {
                    continue;
                };
                for (kind, (button, _)) in row.controls.into_iter().enumerate() {
                    if world.get::<bevy::ui::InteractionDisabled>(button).is_none()
                        && previous.as_ref().is_some_and(|control| {
                            control.uid == row.source.uid && control.action == kind
                        })
                    {
                        restored = Some(button);
                    }
                    if kind == 0
                        && previous_index
                            .is_some_and(|old| row.index == old.min(count.saturating_sub(1)))
                    {
                        fallback = Some(button);
                    }
                }
            }
        }
        if previous.is_some()
            && let Some(mut focus) = world.get_resource_mut::<InputFocus>()
        {
            focus.set(
                restored.or(fallback).unwrap_or(toggle),
                FocusCause::Navigated,
            );
        }
    }
}
