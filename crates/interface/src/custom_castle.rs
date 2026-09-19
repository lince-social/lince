mod storage;
#[cfg(test)]
mod tests;
mod ui;

pub(crate) use ui::store_entries;

use crate::{
    area::InfluenceArea,
    calendar::{Calendar, CalendarSand},
    canvas::CanvasItem,
    canvas_selection::{SandGroup, SandSelection},
    layout::LayoutBox,
    protein_castle::{ProteinCastle, ProteinDraft},
    sand_placement::Placement,
    sand_store::{SandKind, StoredSand},
    sand_text::SavedText,
    tokens::TokenOverrides,
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_PARTS: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
struct CustomCastle {
    name: String,
    parts: Vec<Part>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Part {
    position: [f64; 2],
    size: [f32; 2],
    placement: Placement,
    tokens: TokenOverrides,
    content: Content,
}

#[derive(Clone, Serialize, Deserialize)]
enum Content {
    Sand {
        kind: SandKind,
        texts: Vec<SavedText>,
        #[serde(default)]
        timer: Option<crate::work_timer::LocalTimer>,
    },
    Calendar(Calendar),
    Instinct(crate::instinct::Instinct),
    Kanban(crate::kanban::Kanban),
    Protein(ProteinDraft),
    Area(InfluenceArea),
}

fn identity() -> [u8; 16] {
    let mut id = [0; 16];
    getrandom::fill(&mut id).expect("custom Castle identity");
    id
}

fn file_id() -> String {
    identity()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl CustomCastle {
    fn valid(&self) -> bool {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
            || self.parts.is_empty()
            || self.parts.len() > MAX_PARTS
        {
            return false;
        }
        let mut layouts = HashMap::new();
        let mut areas = HashSet::new();
        for part in &self.parts {
            if !DVec2::from_array(part.position).is_finite()
                || DVec2::from_array(part.position).abs().max_element() > 1_000_000.0
                || !part.size.iter().all(|v| (1.0..=100_000.0).contains(v))
                || !part.placement.valid()
                || !part.tokens.validate()
            {
                return false;
            }
            if let Some(layout) = part.placement.layout
                && layouts.insert(layout.id, layout.parent).is_some()
            {
                return false;
            }
            let valid = match &part.content {
                Content::Sand { texts, timer, kind } => {
                    texts.len() <= 256
                        && texts.iter().all(SavedText::validate)
                        && timer
                            .as_ref()
                            .is_none_or(|timer| *kind == SandKind::WorkTimer && timer.valid())
                }
                Content::Calendar(calendar) => calendar.valid(),
                Content::Instinct(instinct) => instinct.valid(),
                Content::Kanban(board) => board.valid(),
                Content::Protein(draft) => draft.valid_storage(),
                Content::Area(area) => area.validate() && areas.insert(area.id.clone()),
            };
            if !valid {
                return false;
            }
        }
        for (id, parent) in &layouts {
            let mut visited = HashSet::from([*id]);
            let mut parent = *parent;
            while let Some(id) = parent {
                if !visited.insert(id) || visited.len() > crate::layout::MAX_DEPTH {
                    return false;
                }
                let Some(next) = layouts.get(&id) else {
                    return false;
                };
                parent = *next;
            }
        }
        self.parts.iter().all(|part| match &part.content {
            Content::Calendar(calendar) => {
                calendar.area.as_ref().is_none_or(|id| areas.contains(id))
            }
            Content::Kanban(board) => board.ids().all(|id| areas.contains(id)),
            _ => true,
        })
    }

    fn capture(world: &World, root: Entity, name: &str) -> Result<Self, String> {
        let mut selection = crate::canvas_selection::selected(world, root);
        if selection.is_empty()
            && let Some(entity) = world
                .get::<crate::inspection::Inspection>(root)
                .and_then(|i| i.selected)
            && crate::canvas_selection::eligible(world, root, entity)
        {
            selection.push(entity);
        }
        if selection.is_empty() {
            return Err("Select a group on the canvas first.".into());
        }
        let mut members: HashSet<_> = selection
            .into_iter()
            .map(|entity| crate::kanban::part_owner(world, entity).unwrap_or(entity))
            .collect();
        let children: Vec<_> = world
            .get::<Children>(root)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        loop {
            let count = members.len();
            let mut linked_areas = HashSet::new();
            let mut layouts = HashSet::new();
            let mut groups = HashSet::new();
            for entity in &members {
                if let Some(calendar) = world.get::<CalendarSand>(*entity)
                    && let Some(id) = &calendar.0.area
                {
                    linked_areas.insert(id.clone());
                }
                if let Some(board) = world.get::<crate::kanban::Kanban>(*entity) {
                    linked_areas.extend(board.ids().cloned());
                }
                if let Some(layout) = world.get::<LayoutBox>(*entity) {
                    layouts.insert(layout.id);
                }
                if let Some(group) = world.get::<SandGroup>(*entity) {
                    groups.insert(*group);
                }
            }
            for entity in &children {
                if world
                    .get::<crate::protein_area::RecordBinding>(*entity)
                    .is_some()
                    || crate::kanban::part_owner(world, *entity).is_some()
                {
                    continue;
                }
                if !crate::canvas_selection::eligible(world, root, *entity) {
                    continue;
                }
                if world
                    .get::<InfluenceArea>(*entity)
                    .is_some_and(|a| linked_areas.contains(&a.id))
                    || world
                        .get::<LayoutBox>(*entity)
                        .and_then(|l| l.parent)
                        .is_some_and(|id| layouts.contains(&id))
                    || world
                        .get::<SandGroup>(*entity)
                        .is_some_and(|g| groups.contains(g))
                {
                    members.insert(*entity);
                }
            }
            if members.len() > MAX_PARTS {
                return Err(format!(
                    "A custom Castle can contain at most {MAX_PARTS} parts."
                ));
            }
            if count == members.len() {
                break;
            }
        }
        let mut members: Vec<_> = members.into_iter().collect();
        members.sort();
        let mut minimum = DVec2::splat(f64::INFINITY);
        let mut maximum = DVec2::splat(f64::NEG_INFINITY);
        for entity in &members {
            let item = world.get::<CanvasItem>(*entity).unwrap();
            minimum = minimum.min(item.position - item.size.as_dvec2() * 0.5);
            maximum = maximum.max(item.position + item.size.as_dvec2() * 0.5);
        }
        let origin = minimum + (maximum - minimum) * 0.5;
        let elevation = crate::topology::spatial(world, members[0]).elevation;
        let layout_ids: HashSet<_> = members
            .iter()
            .filter_map(|e| world.get::<LayoutBox>(*e).map(|l| l.id))
            .collect();
        let mut parts = Vec::new();
        for entity in members {
            if world
                .get::<crate::protein_area::QueryEditor>(entity)
                .is_some()
                || world.get::<crate::calendar::Picker>(entity).is_some()
                || world
                    .get::<crate::protein_area::grouping::GeneratedGroup>(entity)
                    .is_some()
            {
                return Err("Select the source Castle or Area instead of its temporary editor or generated group.".into());
            }
            let content = if let Some(sand) = world.get::<StoredSand>(entity) {
                Content::Sand {
                    kind: sand.kind,
                    texts: crate::sand_text::snapshot(world, entity),
                    timer: world.get::<crate::work_timer::LocalTimer>(entity).cloned(),
                }
            } else if let Some(calendar) = world.get::<CalendarSand>(entity) {
                Content::Calendar(calendar.0.clone())
            } else if let Some(instinct) = world.get::<crate::instinct::Instinct>(entity) {
                Content::Instinct(instinct.clone())
            } else if let Some(board) = world.get::<crate::kanban::Kanban>(entity) {
                Content::Kanban(board.clone())
            } else if let Some(protein) = world.get::<ProteinCastle>(entity) {
                Content::Protein(protein.draft.clone())
            } else if let Some(area) = world.get::<InfluenceArea>(entity) {
                Content::Area(area.clone())
            } else {
                return Err("This selection contains a Record or imported object that cannot be saved as a custom Castle. Select its Protein Area or the Sands to reuse.".into());
            };
            let item = world.get::<CanvasItem>(entity).unwrap();
            let mut placement = Placement::capture(world, entity);
            placement.group = None;
            placement.attachment = None;
            placement.group_pose = None;
            placement.spatial.elevation -= elevation;
            if let Some(layout) = &mut placement.layout {
                layout.parent = layout.parent.filter(|id| layout_ids.contains(id));
            }
            parts.push(Part {
                position: (item.position - origin).to_array(),
                size: item.size.to_array(),
                placement,
                tokens: crate::token_style::overrides(world, entity),
                content,
            });
        }
        let castle = Self {
            name: name.trim().into(),
            parts,
        };
        castle.valid().then_some(castle).ok_or_else(|| "Use a name of 1–80 characters and valid Sands with connected Areas in this workspace.".into())
    }

    fn spawn(&self, world: &mut World, root: Entity) -> Result<Vec<Entity>, String> {
        if !self.valid() {
            return Err("This custom Castle file is invalid.".into());
        }
        let workspace = world
            .get::<crate::workspace::Workspaces>(root)
            .ok_or("Open a workspace first.")?
            .active;
        let origin = world
            .get::<crate::canvas::CanvasView>(root)
            .copied()
            .unwrap_or_default()
            .center;
        let elevation = world
            .get::<crate::topology::view::View>(root)
            .map_or(0.0, |v| v.plane);
        let existing = world
            .get::<Children>(root)
            .into_iter()
            .flatten()
            .filter(|e| world.get::<InfluenceArea>(**e).is_some())
            .count();
        let added = self
            .parts
            .iter()
            .filter(|p| matches!(p.content, Content::Area(_)))
            .count();
        if existing + added > crate::area::MAX_AREAS {
            return Err("There is no room for more Areas in this workspace.".into());
        }
        if self.parts.iter().any(|p| {
            !(origin + DVec2::from_array(p.position)).is_finite()
                || !(elevation + p.placement.spatial.elevation).is_finite()
                || match &p.content {
                    Content::Area(area) => {
                        let mut area = area.clone();
                        area.center = (origin + DVec2::from_array(p.position)).to_array();
                        !area.validate()
                    }
                    _ => false,
                }
        }) {
            return Err("Move the camera to a valid position first.".into());
        }
        let layouts: HashMap<_, _> = self
            .parts
            .iter()
            .filter_map(|p| p.placement.layout.map(|l| (l.id, identity())))
            .collect();
        let areas: HashMap<_, _> = self
            .parts
            .iter()
            .filter_map(|p| match &p.content {
                Content::Area(a) => Some((a.id.clone(), file_id())),
                _ => None,
            })
            .collect();
        let group = SandGroup(identity());
        let mut entities = Vec::new();
        for part in &self.parts {
            let position = origin + DVec2::from_array(part.position);
            let entity = match &part.content {
                Content::Sand { kind, texts, timer } => {
                    let entity = crate::sand_store::spawn_sand(
                        world,
                        root,
                        workspace,
                        SandKind::Square,
                        "",
                        position,
                    );
                    let mut content = None;
                    if *kind == SandKind::Operation {
                        content = Some(crate::operation::populate(world, root, entity));
                    }
                    if *kind == SandKind::AccessControl {
                        content = Some(crate::access_control::populate(world, root, entity));
                    }
                    for text in texts {
                        let child = crate::sand_text::spawn(world, entity, text.clone());
                        content.get_or_insert(child);
                    }
                    if let Some(timer) = timer {
                        world.entity_mut(entity).insert(timer.clone());
                    }
                    world.entity_mut(entity).insert(StoredSand {
                        kind: *kind,
                        content,
                    });
                    if !matches!(
                        kind,
                        SandKind::Square
                            | SandKind::Operation
                            | SandKind::WorkTimer
                            | SandKind::AccessControl
                    ) {
                        world
                            .entity_mut(entity)
                            .remove::<(crate::sand::Square, Outline)>();
                        world
                            .entity_mut(entity)
                            .insert(BackgroundColor(Color::NONE));
                    }
                    entity
                }
                Content::Calendar(calendar) => {
                    let mut calendar = calendar.clone();
                    calendar.area = calendar.area.as_ref().map(|id| areas[id].clone());
                    crate::calendar::spawn(world, root, workspace, position, calendar)
                }
                Content::Instinct(instinct) => {
                    crate::instinct::spawn(world, root, workspace, position, instinct.clone())
                }
                Content::Kanban(board) => {
                    let mut board = board.clone();
                    board.remap(&areas);
                    crate::kanban::restore(world, root, workspace, position, board)
                }
                Content::Protein(draft) => {
                    crate::protein_castle::spawn(world, root, workspace, position, draft.clone())
                }
                Content::Area(area) => {
                    let mut area = area.clone();
                    area.id = areas[&area.id].clone();
                    if let Some(config) = &mut area.protein {
                        config.spawn_targets = config
                            .spawn_targets
                            .iter()
                            .filter_map(|id| areas.get(id).cloned())
                            .collect();
                    }
                    area.center = position.to_array();
                    crate::area::spawn_area(world, root, workspace, area).unwrap()
                }
            };
            let mut placement = part.placement.clone();
            placement.group = Some(group);
            placement.attachment = None;
            placement.group_pose = None;
            placement.spatial.elevation += elevation;
            if let Some(layout) = &mut placement.layout {
                layout.id = layouts[&layout.id];
                layout.parent = layout.parent.map(|id| layouts[&id]);
            }
            placement.restore(world, entity);
            let size = Vec2::from_array(part.size);
            world.entity_mut(entity).insert((
                CanvasItem { position, size },
                part.tokens.clone(),
                crate::token_style::AppliedSize(size),
                WorkspaceMember(workspace),
            ));
            entities.push(entity);
        }
        crate::topology::groups::attach(world, &entities);
        world
            .entity_mut(root)
            .insert(SandSelection(entities.clone()));
        if let Some(mut inspection) = world.get_mut::<crate::inspection::Inspection>(root) {
            inspection.selected = entities.first().copied();
        }
        Ok(entities)
    }
}
