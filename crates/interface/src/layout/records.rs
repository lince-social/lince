use super::LayoutBox;
use crate::{
    area::InfluenceArea,
    canvas::CanvasItem,
    protein_area::{RecordBinding, Source},
    workspace::WorkspaceMember,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub workspace: u64,
    owner: String,
    source: Source,
    uid: String,
    layout: LayoutBox,
}

impl Saved {
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

    fn key(&self) -> (u64, String, String, String) {
        (
            self.workspace,
            self.owner.clone(),
            serde_json::to_string(&self.source).unwrap(),
            self.uid.clone(),
        )
    }
}

#[derive(Component, Default)]
pub(crate) struct SavedLayouts(pub Vec<Saved>);

#[derive(Component)]
struct Restored;

pub(super) fn remember(world: &mut World, entity: Entity) {
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
    let saved = Saved {
        workspace: member.0,
        owner: owner.id.clone(),
        source: binding.source.clone(),
        uid: binding.uid.clone(),
        layout,
    };
    if world.get::<SavedLayouts>(root).is_none() {
        world.entity_mut(root).insert(SavedLayouts::default());
    }
    let mut layouts = world.get_mut::<SavedLayouts>(root).unwrap();
    if let Some(previous) = layouts
        .0
        .iter_mut()
        .find(|previous| previous.key() == saved.key())
    {
        *previous = saved;
    } else {
        layouts.0.push(saved);
    }
}

pub(super) fn restore(world: &mut World) {
    let rows: Vec<_> = world.query_filtered::<(Entity, &RecordBinding, &WorkspaceMember, &ChildOf), (With<CanvasItem>, Without<LayoutBox>, Without<Restored>)>()
        .iter(world).map(|(entity, binding, member, parent)| (entity, binding.clone(), member.0, parent.parent())).collect();
    for (entity, binding, workspace, root) in rows {
        let owner = world
            .get::<InfluenceArea>(binding.area)
            .map(|area| area.id.as_str());
        let saved = world
            .get::<SavedLayouts>(root)
            .and_then(|saved| {
                saved.0.iter().find(|saved| {
                    Some(saved.owner.as_str()) == owner
                        && saved.source == binding.source
                        && saved.uid == binding.uid
                        && saved.workspace == workspace
                })
            })
            .map(|saved| saved.layout);
        if let Some(layout) = saved {
            world.entity_mut(entity).insert(layout);
        }
        world.entity_mut(entity).insert(Restored);
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<Saved> {
    let mut saved: BTreeMap<_, _> = world
        .get::<SavedLayouts>(root)
        .map(|saved| {
            saved
                .0
                .iter()
                .map(|saved| (saved.key(), saved.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (layout, binding, member, parent) in world.query_filtered::<(&LayoutBox, &RecordBinding, &WorkspaceMember, &ChildOf), With<CanvasItem>>().iter(world) {
        if parent.parent() != root { continue; }
        let Some(owner) = world.get::<InfluenceArea>(binding.area) else { continue };
        let row = Saved { workspace: member.0, owner: owner.id.clone(), source: binding.source.clone(), uid: binding.uid.clone(), layout: *layout };
        saved.insert(row.key(), row);
    }
    let owners: std::collections::HashSet<_> = world
        .query::<(&InfluenceArea, &ChildOf)>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == root)
        .map(|(area, _)| area.id.clone())
        .collect();
    let saved: Vec<_> = saved
        .into_values()
        .filter(|saved| {
            owners.contains(&saved.owner)
                && world
                    .get::<crate::workspace::Workspaces>(root)
                    .is_none_or(|spaces| {
                        spaces
                            .entries
                            .iter()
                            .any(|space| space.id == saved.workspace)
                    })
        })
        .collect();
    world.entity_mut(root).insert(SavedLayouts(saved.clone()));
    saved
}
