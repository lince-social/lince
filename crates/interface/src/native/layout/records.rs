// non-lean-yet
// Scope: struct Saved, struct Row, struct SavedLayouts, struct Restored.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 86, 93, 323.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (158, 199,
//   221, 250).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   140.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (84, 140).

use super::LayoutBox;
use crate::native::{
    area::InfluenceArea,
    canvas::CanvasItem,
    protein_area::{RecordBinding, Source},
    workspace::WorkspaceMember,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub workspace: u64,
    owner: String,
    source: Source,
    uid: String,
    layout: LayoutBox,
}

impl crate::native::sand_placement::scratch::Saved for Saved {
    fn normalize_scratch(&mut self, homes: &crate::native::sand_placement::scratch::Homes) {
        if let Some(home) = homes.get(&self.layout.id) {
            self.workspace = home.workspace;
            self.layout.rules = home.rules;
        }
    }
}

impl Saved {
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    pub fn valid(&self) -> bool {
        self.owner.len() == 32
            && self.owner.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !self.uid.is_empty()
            && self.uid.len() <= 128
            && self.layout.valid()
            && match &self.source {
                Source::Local => true,
                Source::Organ(organ) => !organ.is_empty() && organ.len() <= 4096,
            }
    }

    fn borrowed(&self) -> Row<'_> {
        Row {
            workspace: self.workspace,
            owner: &self.owner,
            source: &self.source,
            uid: &self.uid,
            layout: self.layout,
        }
    }
}

#[derive(Clone, Copy)]
struct Row<'a> {
    workspace: u64,
    owner: &'a str,
    source: &'a Source,
    uid: &'a str,
    layout: LayoutBox,
}

impl<'a> Row<'a> {
    fn matches(&self, saved: &Saved) -> bool {
        self.workspace == saved.workspace
            && self.owner == saved.owner
            && self.source == &saved.source
            && self.uid == saved.uid
    }

    fn key(
        &self,
        sources: &mut HashMap<&'a Source, Arc<str>>,
    ) -> (u64, &'a str, Arc<str>, &'a str) {
        let source = sources.entry(self.source).or_insert_with(|| {
            nucleus::performance::add("layout-record-source-encodings", 1);
            Arc::from(serde_json::to_string(self.source).unwrap())
        });
        (self.workspace, self.owner, source.clone(), self.uid)
    }

    fn into_owned(self) -> Saved {
        Saved {
            workspace: self.workspace,
            owner: self.owner.to_owned(),
            source: self.source.clone(),
            uid: self.uid.to_owned(),
            layout: self.layout,
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct SavedLayouts(pub Vec<Saved>);

#[derive(Component)]
struct Restored;

pub(crate) fn remember(world: &mut World, entity: Entity) {
    let _remember = nucleus::performance::Span::new("layout-record-remember");
    let Some(binding) = world.get::<RecordBinding>(entity) else {
        return;
    };
    let Some(layout) = world.get::<LayoutBox>(entity).copied() else {
        return;
    };
    let Some(owner) = world.get::<InfluenceArea>(binding.area) else {
        return;
    };
    let Some(member) = world.get::<WorkspaceMember>(entity) else {
        return;
    };
    let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
        return;
    };
    let row = Row {
        workspace: member.0,
        owner: &owner.id,
        source: &binding.source,
        uid: &binding.uid,
        layout,
    };
    let previous = world.get::<SavedLayouts>(root).and_then(|layouts| {
        layouts
            .0
            .iter()
            .enumerate()
            .find(|(_, previous)| row.matches(previous))
            .map(|(index, previous)| (index, previous.layout))
    });
    if let Some((index, previous)) = previous {
        if previous != layout {
            world.get_mut::<SavedLayouts>(root).unwrap().0[index].layout = layout;
            nucleus::performance::add("layout-record-saved-updates", 1);
        }
    } else {
        let saved = row.into_owned();
        if let Some(mut layouts) = world.get_mut::<SavedLayouts>(root) {
            layouts.0.push(saved);
        } else {
            world.entity_mut(root).insert(SavedLayouts(vec![saved]));
        }
        nucleus::performance::add("layout-record-saved-updates", 1);
    }
}

pub(crate) fn forget(world: &mut World, entity: Entity) {
    let _forget = nucleus::performance::Span::new("layout-record-forget");
    world
        .run_system_cached_with(forget_saved, entity)
        .expect("forget Record layout");
}

fn forget_saved(
    In(entity): In<Entity>,
    bindings: Query<(&RecordBinding, &ChildOf, Option<&WorkspaceMember>)>,
    areas: Query<&InfluenceArea>,
    mut layouts: Query<&mut SavedLayouts>,
) {
    let Ok((binding, parent, member)) = bindings.get(entity) else {
        return;
    };
    let Ok(owner) = areas.get(binding.area) else {
        return;
    };
    let Some(workspace) = member.map(|member| member.0) else {
        return;
    };
    let Ok(mut saved) = layouts.get_mut(parent.parent()) else {
        return;
    };
    let matches = |saved: &Saved| {
        saved.workspace == workspace
            && saved.owner == owner.id
            && saved.uid == binding.uid
            && saved.source == binding.source
    };
    if saved.0.iter().any(matches) {
        let before = saved.0.len();
        saved.0.retain(|saved| !matches(saved));
        nucleus::performance::add(
            "layout-record-saved-removals",
            (before - saved.0.len()) as u64,
        );
    }
}

pub(super) fn restore(world: &mut World) {
    let _restore = nucleus::performance::Span::new("layout-record-restore");
    world
        .run_system_cached(restore_rows)
        .expect("restore Record layouts");
}

fn restore_rows(
    world: &mut World,
    rows: &mut QueryState<
        (
            Entity,
            &'static RecordBinding,
            &'static WorkspaceMember,
            &'static ChildOf,
        ),
        (With<CanvasItem>, Without<LayoutBox>, Without<Restored>),
    >,
    mut ready: Local<Vec<Entity>>,
) {
    ready.extend(rows.iter(world).map(|(entity, ..)| entity));
    let count = ready.len();
    nucleus::performance::add("layout-record-restore-candidates", count as u64);
    for entity in ready.drain(..) {
        let (_, binding, member, parent) = rows
            .get(world, entity)
            .expect("current Record layout candidate");
        let owner = world
            .get::<InfluenceArea>(binding.area)
            .map(|area| area.id.as_str());
        let saved = world
            .get::<SavedLayouts>(parent.parent())
            .and_then(|saved| {
                saved.0.iter().find(|saved| {
                    Some(saved.owner.as_str()) == owner
                        && saved.source == binding.source
                        && saved.uid == binding.uid
                        && saved.workspace == member.0
                })
            })
            .map(|saved| saved.layout);
        if let Some(layout) = saved {
            world.entity_mut(entity).insert(layout);
        }
        world.entity_mut(entity).insert(Restored);
    }
    if count == 0 {
        *ready = Vec::new();
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<Saved> {
    let _snapshot = nucleus::performance::Span::new("layout-record-snapshot");
    world
        .run_system_cached_with(collect_snapshot, root)
        .expect("snapshot Record layouts")
}

fn collect_snapshot(
    In(root): In<Entity>,
    world: &mut World,
    layouts: &mut QueryState<
        (
            &'static LayoutBox,
            &'static RecordBinding,
            &'static WorkspaceMember,
            &'static ChildOf,
        ),
        With<CanvasItem>,
    >,
    areas: &mut QueryState<(&'static InfluenceArea, &'static ChildOf)>,
) -> Vec<Saved> {
    let saved: Vec<_> = {
        let mut sources = HashMap::new();
        let mut saved = BTreeMap::new();
        if let Some(previous) = world.get::<SavedLayouts>(root) {
            for previous in &previous.0 {
                let row = previous.borrowed();
                saved.insert(row.key(&mut sources), row);
            }
        }
        for (layout, binding, member, parent) in layouts.iter(world) {
            if parent.parent() != root {
                continue;
            }
            let Some(owner) = world.get::<InfluenceArea>(binding.area) else {
                continue;
            };
            let row = Row {
                workspace: member.0,
                owner: &owner.id,
                source: &binding.source,
                uid: &binding.uid,
                layout: *layout,
            };
            saved.insert(row.key(&mut sources), row);
        }
        let owners: HashSet<_> = areas
            .iter(world)
            .filter(|(_, parent)| parent.parent() == root)
            .map(|(area, _)| area.id.as_str())
            .collect();
        let workspaces: Option<HashSet<_>> = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map(|spaces| {
                spaces
                    .entries
                    .iter()
                    .chain(spaces.trash.iter().map(|entry| &entry.workspace))
                    .map(|space| space.id)
                    .collect()
            });
        saved
            .into_values()
            .filter(|row| {
                owners.contains(row.owner)
                    && workspaces
                        .as_ref()
                        .is_none_or(|spaces| spaces.contains(&row.workspace))
            })
            .map(Row::into_owned)
            .collect()
    };
    nucleus::performance::add("layout-record-snapshot-rows", saved.len() as u64);
    if world
        .get::<SavedLayouts>(root)
        .is_none_or(|previous| previous.0 != saved)
    {
        world.entity_mut(root).insert(SavedLayouts(saved.clone()));
        nucleus::performance::add("layout-record-snapshot-publications", 1);
    }
    saved
}
