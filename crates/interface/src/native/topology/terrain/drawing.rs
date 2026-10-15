use super::*;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::PrimitiveTopology,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, PartialEq)]
pub struct GroundMaterial {
    #[uniform(0)]
    background: Vec4,
    #[uniform(1)]
    ink: Vec4,
    #[uniform(2)]
    pattern: Vec4,
    #[uniform(3)]
    phase: Vec4,
}

impl Material for GroundMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://lince_interface/native/topology/terrain/ground.wgsl".into()
    }
}

pub fn install(app: &mut App) {
    if app.world().contains_resource::<AssetServer>() {
        embedded_asset!(app, "ground.wgsl");
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
    } else {
        app.init_resource::<Assets<GroundMaterial>>();
    }
}

fn material(world: &World, root: Entity, workspace: u64, minimum: DVec2) -> GroundMaterial {
    let colors = crate::native::canvas_background::for_workspace(world, root, workspace);
    let setting = |token, fallback| {
        world
            .get_resource::<crate::native::tokens::ThemeSettings>()
            .map_or(fallback, |settings| {
                settings
                    .resolve(token, None, &Default::default())
                    .0
                    .number()
            })
    };
    let rgb = |color: [u8; 3]| {
        let linear = Color::srgb_u8(color[0], color[1], color[2]).to_linear();
        Vec4::new(linear.red, linear.green, linear.blue, linear.alpha)
    };
    let spacing = setting(crate::native::tokens::Token::GridSpacing, 32.0).max(1.0);
    GroundMaterial {
        background: rgb(colors.background),
        ink: rgb(colors.grid),
        pattern: Vec4::new(
            spacing,
            setting(crate::native::tokens::Token::GridThickness, 1.0),
            setting(crate::native::tokens::Token::CanvasPattern, 100.0) / 100.0,
            50.0,
        ),
        phase: Vec4::new(
            minimum.x.rem_euclid(f64::from(spacing)) as f32,
            minimum.y.rem_euclid(f64::from(spacing)) as f32,
            0.0,
            0.0,
        ),
    }
}

fn visual(world: &mut World, key: Key, tile: &Tile) -> Option<Entity> {
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<GroundMaterial>>()
    {
        return None;
    }
    let origin = super::super::presentation::origin(world, key.0);
    let positions: Vec<_> = vertices(key, &tile.heights)
        .iter()
        .map(|p| {
            (*p - DVec3::new(key.minimum().x, 0.0, key.minimum().y))
                .as_vec3()
                .to_array()
        })
        .collect();
    let mut normals = Vec::with_capacity(positions.len());
    let mut uvs = Vec::with_capacity(positions.len());
    let stride = GRID + 1;
    for z in 0..=GRID {
        for x in 0..=GRID {
            let left = x.saturating_sub(1);
            let right = (x + 1).min(GRID);
            let back = z.saturating_sub(1);
            let front = (z + 1).min(GRID);
            let dx = (tile.heights[z * stride + right] - tile.heights[z * stride + left])
                / ((right - left) as f64 * TILE_SIZE / GRID as f64);
            let dz = (tile.heights[front * stride + x] - tile.heights[back * stride + x])
                / ((front - back) as f64 * TILE_SIZE / GRID as f64);
            normals.push(
                Vec3::new(-dx as f32, 1.0, -dz as f32)
                    .normalize()
                    .to_array(),
            );
            uvs.push([
                x as f32 * TILE_SIZE as f32 / GRID as f32,
                z as f32 * TILE_SIZE as f32 / GRID as f32,
            ]);
        }
    }
    let mut positions = positions;
    let mut indices: Vec<u32> = triangles().into_iter().flatten().collect();
    if tile.clipping.as_ref().is_some_and(Vec::is_empty) {
        return None;
    }
    if tile.clipping.is_some() {
        let old = positions;
        let old_normals = normals;
        positions = Vec::new();
        normals = Vec::new();
        uvs = Vec::new();
        indices.clear();
        for triangle in triangles() {
            let points = triangle.map(|i| Vec3::from_array(old[i as usize]));
            let flat = points.map(|p| DVec2::new(f64::from(p.x), f64::from(p.z)) + key.minimum());
            let denominator = (flat[1] - flat[0]).perp_dot(flat[2] - flat[0]);
            for polygon in super::super::holes::pieces(world, key.0, key.1, flat.to_vec()) {
                for index in 1..polygon.len().saturating_sub(1) {
                    for point in [polygon[0], polygon[index], polygon[index + 1]] {
                        let u = (point - flat[0]).perp_dot(flat[2] - flat[0]) / denominator;
                        let v = (flat[1] - flat[0]).perp_dot(point - flat[0]) / denominator;
                        let height = f64::from(points[0].y)
                            + f64::from(points[1].y - points[0].y) * u
                            + f64::from(points[2].y - points[0].y) * v;
                        let local = point - key.minimum();
                        positions.push([local.x as f32, height as f32, local.y as f32]);
                        let n = triangle.map(|i| Vec3::from_array(old_normals[i as usize]));
                        normals.push(
                            (n[0] + (n[1] - n[0]) * u as f32 + (n[2] - n[0]) * v as f32)
                                .normalize()
                                .to_array(),
                        );
                        uvs.push([local.x as f32, local.y as f32]);
                    }
                }
            }
        }
        indices.extend(0..positions.len() as u32);
        if positions.is_empty() {
            return None;
        }
    }
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices));
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let definition = material(world, key.0, key.1, key.minimum());
    let handle = world
        .resource_mut::<Assets<GroundMaterial>>()
        .add(definition);
    Some(
        world
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(handle),
                Transform::from_translation(
                    (DVec3::new(key.minimum().x, -(tile.depth as f64) * 0.2, key.minimum().y)
                        - origin)
                        .as_vec3(),
                ),
                Visibility::Visible,
                Pickable::IGNORE,
            ))
            .id(),
    )
}

pub fn destroy(world: &mut World, entity: Entity) {
    let mesh = world.get::<Mesh3d>(entity).map(|h| h.0.id());
    let material = world
        .get::<MeshMaterial3d<GroundMaterial>>(entity)
        .map(|h| h.0.id());
    world.despawn(entity);
    if let Some(mesh) = mesh
        && let Some(mut assets) = world.get_resource_mut::<Assets<Mesh>>()
    {
        assets.remove(mesh);
    }
    if let Some(material) = material
        && let Some(mut assets) = world.get_resource_mut::<Assets<GroundMaterial>>()
    {
        assets.remove(material);
    }
}

pub fn draw(world: &mut World) {
    if !world.contains_resource::<Terrain>() {
        return;
    }
    let roots: Vec<_> = world
        .query::<(
            Entity,
            &crate::native::workspace::Workspaces,
            &crate::native::canvas::CanvasView,
        )>()
        .iter(world)
        .flat_map(|(e, w, v)| {
            w.entries
                .iter()
                .filter(|entry| super::super::holes::live(world, e, entry.id))
                .map(move |entry| (e, entry.id, *v))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    let collisions = std::mem::take(&mut terrain.tiles);
    terrain.tiles = std::mem::take(&mut terrain.render_tiles);
    let previews: HashMap<_, _> = world
        .query::<(Entity, &Preview)>()
        .iter(world)
        .map(|(e, p)| (e, p.0.clone()))
        .collect();
    let physical_fields = if previews.is_empty() {
        None
    } else {
        let original = std::mem::take(&mut terrain.fields);
        terrain.fields = original.clone();
        for (entity, definition) in previews {
            if !definition.enabled
                || definition.amplitude == 0.0
                || world
                    .get::<Fault>(entity)
                    .is_some_and(|f| f.1.same_surface(&definition))
            {
                continue;
            }
            let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
                continue;
            };
            let Some(workspace) = world
                .get::<crate::native::workspace::WorkspaceMember>(entity)
                .map(|m| m.0)
            else {
                continue;
            };
            let Some(center) = world
                .get::<crate::native::canvas::CanvasItem>(entity)
                .map(|item| item.position)
            else {
                continue;
            };
            let Ok(formula) = definition.compile() else {
                continue;
            };
            if world
                .get::<Fault>(entity)
                .zip(world.get::<Effect>(entity))
                .is_some_and(|(fault, effect)| !fault.1.same_surface(&effect.0))
            {
                world.entity_mut(entity).remove::<Fault>();
            }
            let id = world
                .get::<crate::native::canvas_host::Identity>(entity)
                .map_or_else(|| entity.to_bits().to_string(), |id| id.0.clone());
            let heading = super::super::spatial(world, entity)
                .rotation()
                .to_euler(EulerRot::YXZ)
                .0;
            let fields = terrain.fields.entry((root, workspace)).or_default();
            fields.retain(|field| field.input.entity != entity);
            fields.push(Field {
                input: Input {
                    id,
                    entity,
                    definition,
                    center,
                    heading,
                },
                formula,
            });
            fields.sort_by(|a, b| a.input.id.cmp(&b.input.id));
        }
        Some(original)
    };
    let mut flat_layers = Vec::new();
    for (root, workspace, _) in &roots {
        if terrain.fields.keys().any(|(r, _)| r == root)
            && !terrain.fields.contains_key(&(*root, *workspace))
        {
            terrain.fields.insert((*root, *workspace), Vec::new());
            flat_layers.push((*root, *workspace));
        }
    }
    let mut visible = HashSet::new();
    for (root, workspace, canvas) in roots {
        if !terrain.fields.contains_key(&(root, workspace)) {
            continue;
        }
        let view = world
            .get::<super::super::view::View>(root)
            .copied()
            .unwrap_or_default();
        let center = if view.spatial {
            DVec2::new(view.position[0], view.position[2])
        } else {
            canvas.center
        };
        let viewport = world
            .get::<ComputedNode>(root)
            .map_or(Vec2::new(1920.0, 1080.0), |n| {
                n.size() * n.inverse_scale_factor()
            });
        let reach = if view.spatial {
            DVec2::splat(2048.0)
        } else {
            (viewport.as_dvec2() / canvas.zoom * 0.5 + DVec2::splat(TILE_SIZE))
                .min(DVec2::splat(4096.0))
        };
        let first = ((center - reach) / TILE_SIZE).floor().as_i64vec2();
        let last = ((center + reach) / TILE_SIZE).floor().as_i64vec2();
        for z in first.y..=last.y {
            for x in first.x..=last.x {
                let key = Key(root, workspace, x, z);
                tile(&mut terrain, world, key, false);
                if let Some(tile) = terrain.tiles.get_mut(&key) {
                    let minimum = key.minimum();
                    let quad = vec![
                        minimum,
                        minimum + DVec2::new(TILE_SIZE, 0.0),
                        minimum + DVec2::splat(TILE_SIZE),
                        minimum + DVec2::new(0.0, TILE_SIZE),
                    ];
                    let pieces = super::super::holes::pieces(world, root, workspace, quad.clone());
                    let clipping = (pieces != vec![quad]).then_some(pieces);
                    let depth = super::super::holes::depth(world, root, workspace);
                    if tile.clipping != clipping || tile.depth != depth {
                        if let Some(visual) = tile.visual.take() {
                            destroy(world, visual);
                        }
                        tile.clipping = clipping;
                        tile.depth = depth;
                    }
                    if tile.visual.is_none() {
                        tile.visual = visual(world, key, tile);
                    }
                    if let Some(entity) = tile.visual {
                        visible.insert(entity);
                        let origin = super::super::presentation::origin(world, root);
                        let next = Transform::from_translation(
                            (DVec3::new(
                                key.minimum().x,
                                -(tile.depth as f64) * 0.2,
                                key.minimum().y,
                            ) - origin)
                                .as_vec3(),
                        );
                        world.get_mut::<Transform>(entity).unwrap().set_if_neq(next);
                        world
                            .get_mut::<Visibility>(entity)
                            .unwrap()
                            .set_if_neq(Visibility::Visible);
                        let definition = material(world, root, workspace, key.minimum());
                        if let Some(handle) = world
                            .get::<MeshMaterial3d<GroundMaterial>>(entity)
                            .map(|h| h.0.clone())
                            && let Some(mut assets) =
                                world.get_resource_mut::<Assets<GroundMaterial>>()
                            && assets.get(&handle) != Some(&definition)
                            && let Some(mut material) = assets.get_mut(&handle)
                        {
                            *material = definition;
                        }
                    }
                }
            }
        }
    }
    for tile in terrain.tiles.values() {
        if let Some(entity) = tile.visual
            && !visible.contains(&entity)
        {
            world.entity_mut(entity).insert(Visibility::Hidden);
        }
    }
    let stale: Vec<_> = terrain
        .tiles
        .iter()
        .filter(|(_, t)| terrain.epoch.saturating_sub(t.used) > 2)
        .map(|(k, _)| *k)
        .collect();
    for key in stale {
        if let Some(tile) = terrain.tiles.remove(&key) {
            destroy_tile(world, tile);
        }
    }
    if terrain.tiles.len() > MAX_TILES {
        let mut oldest: Vec<_> = terrain
            .tiles
            .iter()
            .filter(|(_, t)| t.collider.is_none())
            .map(|(k, t)| (*k, t.used))
            .collect();
        oldest.sort_by_key(|(_, used)| *used);
        for (key, _) in oldest.into_iter().take(terrain.tiles.len() - MAX_TILES) {
            if let Some(tile) = terrain.tiles.remove(&key) {
                destroy_tile(world, tile);
            }
        }
    }
    let obsolete: Vec<_> = terrain
        .tiles
        .keys()
        .filter(|key| !terrain.fields.contains_key(&(key.0, key.1)))
        .copied()
        .collect();
    for key in obsolete {
        if let Some(tile) = terrain.tiles.remove(&key) {
            destroy_tile(world, tile);
        }
    }
    terrain.render_tiles = std::mem::take(&mut terrain.tiles);
    terrain.tiles = collisions;
    for key in flat_layers {
        terrain.fields.remove(&key);
    }
    if let Some(fields) = physical_fields {
        terrain.fields = fields;
    }
    world.insert_resource(terrain);
}
