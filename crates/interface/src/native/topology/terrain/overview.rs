use super::*;
use bevy::camera::visibility::RenderLayers;

#[derive(Component)]
struct PreviewGround {
    view: Entity,
    key: Key,
    inputs: Vec<Input>,
    clip: Vec<Vec<DVec2>>,
    depth: usize,
}

pub(super) fn draw(world: &mut World, terrain: &mut Terrain) {
    let requests = world
        .get_resource::<super::super::previews::Previews>()
        .map(|views| views.0.clone())
        .unwrap_or_default();
    let existing: HashMap<_, _> = world
        .query::<(Entity, &PreviewGround)>()
        .iter(world)
        .map(|(entity, ground)| ((ground.view, ground.key), entity))
        .collect();
    let mut living = HashSet::new();
    let mut budget = 256_usize;
    for (index, preview) in requests.iter().enumerate() {
        let Some(layer) = crate::native::render_layers::OVERVIEW.get(index) else {
            continue;
        };
        let layers = super::super::holes::layers_for(world, preview.root, preview.workspace)
            .cloned()
            .unwrap_or_default();
        let mut spaces = vec![(preview.workspace, 0)];
        spaces.extend(
            layers
                .0
                .iter()
                .filter(|layer| layer.depth > 0)
                .map(|layer| (layer.workspace, layer.depth)),
        );
        let origin =
            crate::native::output_views::canvas(world, preview.root, preview.workspace).center;
        let reach = (preview.pixels.as_dvec2() / preview.view.zoom * 0.5 + DVec2::splat(TILE_SIZE))
            .min(DVec2::splat(1536.0));
        let first = ((preview.view.center - reach) / TILE_SIZE)
            .floor()
            .as_i64vec2();
        let last = ((preview.view.center + reach) / TILE_SIZE)
            .floor()
            .as_i64vec2();
        for (workspace, depth) in spaces {
            if !terrain
                .fields
                .get(&(preview.root, workspace))
                .is_some_and(|fields| !fields.is_empty())
            {
                continue;
            }
            for z in first.y..=last.y {
                for x in first.x..=last.x {
                    if budget == 0 {
                        break;
                    }
                    let key = Key(preview.root, workspace, x, z);
                    tile(terrain, world, key, false);
                    let Some(tile) = terrain.tiles.get(&key) else {
                        continue;
                    };
                    let quad = vec![
                        key.minimum(),
                        key.minimum() + DVec2::new(TILE_SIZE, 0.0),
                        key.minimum() + DVec2::splat(TILE_SIZE),
                        key.minimum() + DVec2::new(0.0, TILE_SIZE),
                    ];
                    let clip = super::super::holes::pieces_in(
                        world,
                        preview.root,
                        preview.workspace,
                        workspace,
                        quad.clone(),
                    );
                    if clip.is_empty() {
                        continue;
                    }
                    let id = (preview.id, key);
                    let previous = existing.get(&id).copied();
                    let unchanged = previous.is_some_and(|entity| {
                        world.get::<PreviewGround>(entity).is_some_and(|ground| {
                            ground.inputs == tile.inputs
                                && ground.clip == clip
                                && ground.depth == depth
                        })
                    });
                    let entity = if unchanged {
                        previous.unwrap()
                    } else {
                        if let Some(entity) = previous {
                            super::drawing::destroy(world, entity);
                        }
                        let Some(entity) = super::drawing::visual_in(
                            world,
                            key,
                            &tile.heights,
                            depth,
                            clip != vec![quad],
                            DVec3::new(origin.x, 0.0, origin.y),
                            preview.workspace,
                        ) else {
                            continue;
                        };
                        world.entity_mut(entity).insert(PreviewGround {
                            view: preview.id,
                            key,
                            inputs: tile.inputs.clone(),
                            clip,
                            depth,
                        });
                        entity
                    };
                    world.entity_mut(entity).insert((
                        RenderLayers::none().with(layer),
                        Transform::from_xyz(
                            (key.minimum().x - origin.x) as f32,
                            -(depth as f32) * 0.2,
                            (key.minimum().y - origin.y) as f32,
                        ),
                    ));
                    super::drawing::refresh_material(world, entity, key);
                    living.insert(entity);
                    budget -= 1;
                }
            }
        }
    }
    let stale: Vec<_> = world
        .query_filtered::<Entity, With<PreviewGround>>()
        .iter(world)
        .filter(|entity| !living.contains(entity))
        .collect();
    for entity in stale {
        super::drawing::destroy(world, entity);
    }
}
