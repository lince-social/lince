use crate::native::{
    actions::Action,
    area::{AreaShape, InfluenceArea},
    protein_area::Config,
};
use bevy::{math::DVec2, prelude::*};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub mod styles;

#[derive(Component)]
pub(crate) struct RelationLink {
    pub owner: Entity,
    pub uid: String,
}

pub fn config() -> Config {
    let mut config = Config {
        hide_filled: true,
        relations: true,
        motion: Some(Default::default()),
        width: 320.0,
        max_height: Some(800.0),
        ..Config::records()
    };
    config.draft.name = "Relations".into();
    config
}

pub fn spawn(world: &mut World, root: Entity) -> Option<Entity> {
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)?
        .active;
    let center = world
        .get::<crate::native::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    let mut area = InfluenceArea::new(AreaShape::Circle, center, DVec2::splat(1200.0));
    area.name = "Relation Castle".into();
    area.protein = Some(config());
    let entity = crate::native::area::spawn_area(world, root, workspace, area)?;
    if !crate::native::workspace_config::enabled(world, root, workspace) {
        crate::native::workspace_config::set_physics(world, root, workspace, true);
    }
    Some(entity)
}

pub(crate) fn reconcile(
    world: &mut World,
    owner: Entity,
    config: &Config,
    rows: &[Value],
    records: &HashMap<String, Entity>,
) {
    let _reconcile = nucleus::performance::Span::new("relation-link-reconciliation");
    let mut query = world.query::<(Entity, &RelationLink)>();
    let mut existing: HashMap<_, _> = query
        .iter(world)
        .filter(|(_, link)| link.owner == owner)
        .map(|(entity, link)| (link.uid.as_str(), entity))
        .collect();
    let mut links = BTreeMap::new();
    if config.relations && config.enabled {
        for row in rows {
            for link in row["links"].as_array().into_iter().flatten() {
                let (Some(uid), Some(from), Some(to), Some(kind)) = (
                    link["uid"].as_str(),
                    link["from"].as_str(),
                    link["to"].as_str(),
                    link["kind"].as_str(),
                ) else {
                    continue;
                };
                if uid.is_empty() || uid.len() > 128 || kind.len() > 1024 || from == to {
                    continue;
                }
                let (Some(from), Some(to)) = (records.get(from), records.get(to)) else {
                    continue;
                };
                links.entry(uid).or_insert((*from, *to, kind, None));
                if links.len() >= 4096 {
                    break;
                }
            }
            if links.len() >= 4096 {
                break;
            }
        }
    }
    for (uid, (_, _, _, entity)) in &mut links {
        *entity = existing.remove(*uid);
    }
    let stale: Vec<_> = existing.into_values().collect();
    nucleus::performance::add("relation-links-prepared", links.len() as u64);
    for (uid, (from, to, label, entity)) in links {
        if let Some(entity) = entity {
            if let Some(mut arrow) = world.get_mut::<crate::native::arrow_sand::ArrowSand>(entity) {
                if arrow.from != from {
                    arrow.from = from;
                }
                if arrow.to != to {
                    arrow.to = to;
                }
                if arrow.label != label {
                    arrow.label.clear();
                    arrow.label.push_str(label);
                }
            }
        } else if let Some(entity) =
            crate::native::arrow_sand::spawn(world, from, to, label.to_owned())
        {
            world.entity_mut(entity).insert(RelationLink {
                owner,
                uid: uid.to_owned(),
            });
        }
    }
    for entity in stale {
        world.despawn(entity);
    }
    styles::apply(world, owner, config, rows, records);
}

#[derive(Clone)]
struct Add;

impl Action for Add {
    fn apply(&self, world: &mut World, root: Entity) {
        if let Some(entity) = spawn(world, root) {
            if let Some(mut editor) = world.get_mut::<crate::native::area_panel::AreaEditor>(root) {
                editor.selected = Some(entity);
            }
        }
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Relation Castle",
        "Connected Records pull toward one another, settle around their spawn center and repel overlapping cards.",
        Add,
        |world, _| {
            let entity = world
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(12)),
                        ..default()
                    },
                    crate::native::canvas::CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::new(320.0, 120.0),
                    },
                    crate::native::token_style::background(crate::native::tokens::Token::Surface),
                ))
                .id();
            crate::native::protein_area::preview_record(
                world,
                entity,
                &config(),
                &serde_json::json!({"head":"Connected Record", "body":"Details stay in the accordion", "quantity":"3"}),
            );
            entity
        },
    );
}
