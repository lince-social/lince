// non-lean-yet
// Scope: struct AreaChoice, struct Snapshot, enum Key, struct Rows.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 63, 108.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (105).

use super::*;
use crate::native::{
    retained::{Children as RetainedChildren, Presentation},
    workspace_config::RuleSettings,
};

#[derive(Clone, PartialEq, Eq)]
struct AreaChoice {
    entity: Entity,
    name: String,
    paused: bool,
}

struct Snapshot {
    root: Entity,
    workspace: u64,
    rules: RuleSettings,
    areas: Vec<AreaChoice>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Workspace,
    Pending,
    Repeats,
    Area(Entity),
}

#[derive(Component, Default)]
struct Rows(RetainedChildren<Key>);

enum Part<'a> {
    Workspace(bool),
    Pending(u16),
    Repeats(u16),
    Area(&'a AreaChoice),
}

#[derive(PartialEq, Eq)]
enum Content {
    Workspace(bool),
    Pending(u16),
    Repeats(u16),
    Area(AreaChoice),
}

impl Part<'_> {
    fn matches(&self, content: &Content) -> bool {
        match (self, content) {
            (Self::Workspace(value), Content::Workspace(old)) => value == old,
            (Self::Pending(value), Content::Pending(old)) => value == old,
            (Self::Repeats(value), Content::Repeats(old)) => value == old,
            (Self::Area(value), Content::Area(old)) => *value == old,
            _ => false,
        }
    }

    fn owned(&self) -> Content {
        match *self {
            Self::Workspace(value) => Content::Workspace(value),
            Self::Pending(value) => Content::Pending(value),
            Self::Repeats(value) => Content::Repeats(value),
            Self::Area(value) => Content::Area(value.clone()),
        }
    }
}

struct RowState {
    root: Entity,
    workspace: u64,
    content: Content,
}

pub(super) fn update(
    world: &mut World,
    parent: Entity,
    root: Entity,
    workspace: u64,
    rules: RuleSettings,
    areas: &[Entity],
) {
    let _controls = nucleus::performance::Span::new("influence-control-presentation");
    if Presentation::matches(world, parent, |old: &Snapshot| {
        old.root == root
            && old.workspace == workspace
            && old.rules == rules
            && old.areas.len() == areas.len()
            && old.areas.iter().zip(areas).all(|(old, entity)| {
                old.entity == *entity
                    && world
                        .get::<InfluenceArea>(*entity)
                        .is_some_and(|area| old.name == area.name && old.paused == area.paused)
            })
    }) {
        nucleus::performance::add("influence-controls-retained", 1);
        return;
    }
    let snapshot = Snapshot {
        root,
        workspace,
        rules,
        areas: areas
            .iter()
            .map(|entity| {
                let area = world.get::<InfluenceArea>(*entity).unwrap();
                AreaChoice {
                    entity: *entity,
                    name: area.name.clone(),
                    paused: area.paused,
                }
            })
            .collect(),
    };
    let parts = [
        (Key::Workspace, Part::Workspace(rules.paused)),
        (Key::Pending, Part::Pending(rules.max_pending)),
        (Key::Repeats, Part::Repeats(rules.max_repeats)),
    ]
    .into_iter()
    .chain(
        snapshot
            .areas
            .iter()
            .map(|area| (Key::Area(area.entity), Part::Area(area))),
    );
    let mut rows = world.entity_mut(parent).take::<Rows>().unwrap_or_default();
    rows.0.reconcile(
        world,
        parent,
        parts,
        |world, parent, part| {
            let row = world
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(parent),
                ))
                .id();
            draw(world, row, root, workspace, part);
            row
        },
        |world, row, part| draw(world, row, root, workspace, part),
    );
    world.entity_mut(parent).insert(rows);
    Presentation::remember(world, parent, snapshot);
}

fn draw(world: &mut World, parent: Entity, root: Entity, workspace: u64, part: &Part<'_>) {
    if Presentation::matches(world, parent, |old: &RowState| {
        old.root == root && old.workspace == workspace && part.matches(&old.content)
    }) {
        nucleus::performance::add("influence-control-parts-retained", 1);
        return;
    }
    world.entity_mut(parent).despawn_children();
    match *part {
        Part::Workspace(paused) => button(
            world,
            parent,
            root,
            Control::Workspace,
            if paused {
                "Resume workspace rules"
            } else {
                "Pause workspace rules"
            },
        ),
        Part::Pending(value) => {
            label(
                world,
                parent,
                &format!("In-flight Record changes: {value} maximum"),
                14.0,
            );
            for limit in [8, 32, 64] {
                button(
                    world,
                    parent,
                    root,
                    Control::Pending(limit),
                    &format!("Allow up to {limit} requests"),
                );
            }
        }
        Part::Repeats(value) => {
            label(
                world,
                parent,
                &format!("Cycle limit: {value} identical crossings per Record in 10 seconds"),
                14.0,
            );
            for limit in [2, 4, 8] {
                button(
                    world,
                    parent,
                    root,
                    Control::Repeats(limit),
                    &format!("Pause after {limit} repetitions"),
                );
            }
        }
        Part::Area(area) => button(
            world,
            parent,
            root,
            Control::Area(area.entity),
            &format!(
                "{} {}",
                if area.paused { "Resume" } else { "Pause" },
                area.name
            ),
        ),
    }
    Presentation::remember(
        world,
        parent,
        RowState {
            root,
            workspace,
            content: part.owned(),
        },
    );
    nucleus::performance::add("influence-control-parts-built", 1);
}
