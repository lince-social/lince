use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Blueprint {
    board: crate::native::kanban::Kanban,
    areas: Vec<Region>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Region {
    id: String,
    geometry: api::Geometry,
    spatial: api::Spatial,
    settings: BTreeMap<String, serde_json::Value>,
    layout: Option<crate::native::layout::Rules>,
}

pub(super) fn room_members(component: &ContentKind) -> Result<usize, String> {
    match component {
        ContentKind::Authored { base, .. } => room_members(base),
        ContentKind::Native { kind, settings, .. } if kind == "kanban" => {
            Ok(read(settings)?.map_or(8, |blueprint| blueprint.areas.len()))
        }
        _ => Ok(0),
    }
}

fn read(settings: &BTreeMap<String, serde_json::Value>) -> Result<Option<Blueprint>, String> {
    let Some(source) = settings
        .get("state")
        .and_then(serde_json::Value::as_str)
        .filter(|source| *source != "{}")
    else {
        return Ok(None);
    };
    let blueprint: Blueprint = serde_json::from_str(source).map_err(|error| error.to_string())?;
    if !blueprint.board.valid() || blueprint.areas.len() != blueprint.board.ids().count() {
        return Err("A Kanban blueprint includes every source and column region once.".into());
    }
    let mut ids = BTreeMap::new();
    for region in &blueprint.areas {
        region.geometry.validate()?;
        if !api::room::valid_pose(&api::Pose {
            position: [
                region.geometry.position[0],
                region.spatial.elevation,
                region.geometry.position[1],
            ],
            rotation: region.spatial.rotation,
        }) || region
            .spatial
            .depth
            .is_some_and(|depth| !depth.is_finite() || depth <= 0.0 || depth > 100_000.0)
        {
            return Err("Invalid Kanban region pose.".into());
        }
        if !blueprint.board.ids().any(|id| id == &region.id)
            || ids.insert(&region.id, ()).is_some()
            || region.layout.is_some_and(|layout| !layout.valid())
        {
            return Err("Invalid Kanban region or layout.".into());
        }
        super::area::parse(&region.settings, None, &region.geometry)?;
    }
    Ok(Some(blueprint))
}

pub(super) fn capture(world: &World, host: Entity) -> Option<ContentKind> {
    let board = world.get::<crate::native::kanban::Kanban>(host)?.clone();
    let origin = crate::native::topology::rooms::world_pose(world, host)?;
    let parent = world.get::<ChildOf>(host)?.parent();
    let children = world.get::<Children>(parent)?;
    let mut areas = Vec::new();
    for id in board.ids() {
        let entity = children.iter().find(|entity| {
            world
                .get::<crate::native::area::InfluenceArea>(*entity)
                .is_some_and(|area| &area.id == id)
        })?;
        let area = world.get::<crate::native::area::InfluenceArea>(entity)?;
        let point = crate::native::topology::rooms::world_pose(world, entity)?;
        let local = match (
            world.get::<crate::native::topology::rooms::Member>(host),
            world.get::<crate::native::topology::rooms::Member>(entity),
        ) {
            (Some(owner), Some(member))
                if owner.0.room == member.0.room && owner.0.parent == member.0.parent =>
            {
                api::room::relative(&owner.0.local, &member.0.local)
            }
            _ => api::room::relative(&origin, &point),
        };
        let spatial = crate::native::topology::spatial(world, entity);
        areas.push(Region {
            id: id.clone(),
            geometry: api::Geometry {
                position: [local.position[0], local.position[2]],
                size: area.size,
            },
            spatial: api::Spatial {
                elevation: local.position[1],
                rotation: local.rotation,
                depth: spatial.depth,
                world_pinned: spatial.world_pinned,
            },
            settings: super::area::settings(area),
            layout: world
                .get::<crate::native::layout::LayoutBox>(entity)
                .map(|layout| layout.rules),
        });
    }
    let value = serde_json::to_value(Blueprint { board, areas }).unwrap();
    Some(ContentKind::Native {
        kind: "kanban".into(),
        settings: BTreeMap::from([(
            "state".into(),
            serde_json::json!(serde_json::to_string(&value).unwrap()),
        )]),
        bindings: record_references(&value),
    })
}

pub(super) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    settings: &BTreeMap<String, serde_json::Value>,
    bindings: &[String],
) -> Result<Entity, String> {
    let Some(blueprint) = read(settings)? else {
        let host = crate::native::kanban::spawn(world, root, workspace, position)
            .ok_or("No room for the Kanban's source and columns.")?;
        let board = world
            .get::<crate::native::kanban::Kanban>(host)
            .unwrap()
            .clone();
        let children: Vec<_> = world
            .get::<Children>(root)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for entity in children {
            if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity)
                && board.ids().any(|id| id == &area.id)
            {
                area.enabled = false;
                area.paused = true;
            }
        }
        crate::native::kanban::authoring_activation(world, host, false);
        return Ok(host);
    };
    if record_references(&serde_json::to_value(&blueprint).unwrap())
        .iter()
        .any(|uid| !bindings.contains(uid))
    {
        return Err("List the Kanban's referenced Record UIDs in bindings.".into());
    }
    let count = world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .filter(|entity| {
            world
                .get::<crate::native::area::InfluenceArea>(**entity)
                .is_some()
        })
        .count();
    if count + blueprint.areas.len() > crate::native::area::MAX_AREAS {
        return Err("No room for the Kanban's source and columns.".into());
    }
    let mut remap = HashMap::new();
    let mut staged = Vec::new();
    for region in &blueprint.areas {
        let mut area = super::area::parse(&region.settings, None, &region.geometry)?;
        remap.insert(region.id.clone(), area.id.clone());
        area.center = (position + DVec2::from_array(region.geometry.position)).to_array();
        staged.push((area, region.layout, region.spatial.clone()));
    }
    let mut spawned = Vec::new();
    for (mut area, layout, spatial) in staged {
        for config in [&mut area.protein, &mut area.filter, &mut area.change_filter]
            .into_iter()
            .flatten()
        {
            for target in &mut config.spawn_targets {
                if let Some(replacement) = remap.get(target) {
                    *target = replacement.clone();
                }
            }
        }
        let Some(entity) = crate::native::area::spawn_area(world, root, workspace, area) else {
            for entity in spawned {
                world.despawn(entity);
            }
            return Err("Could not create a Kanban region.".into());
        };
        spawned.push(entity);
        world
            .entity_mut(entity)
            .insert(crate::native::topology::Spatial {
                elevation: spatial.elevation,
                rotation: spatial.rotation,
                depth: spatial.depth,
                world_pinned: spatial.world_pinned,
            });
        if let Some(layout) = layout {
            if let Err(error) = crate::native::layout::configure(world, entity, layout) {
                for entity in spawned {
                    world.despawn(entity);
                }
                return Err(error.into());
            }
        }
    }
    let mut board = blueprint.board;
    board.remap(&remap);
    let host = crate::native::kanban::restore(world, root, workspace, position, board);
    crate::native::kanban::authoring_activation(world, host, false);
    Ok(host)
}

pub(super) fn descriptor() -> api::Descriptor {
    api::Descriptor { kind: "kanban".into(), name: "Kanban".into(), description: "Native Kanban, including its owned source/column regions, layout rules and area behaviors. state={} creates the default with inactive regions; inspect its canonical state, then configure individual region activate flags. Reuse remaps area identities and spawn-target links. Initialization that creates Concepts remains a separate native workflow.".into(), settings: BTreeMap::from([("state".into(), api::Setting::Text { default: "{}".into(), max_bytes: 65536 })]), max_bindings: 32, placeable: true, configurable: true, composable: true, unavailable_reason: None }
}
