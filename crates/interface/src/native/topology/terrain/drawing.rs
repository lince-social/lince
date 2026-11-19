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
        "embedded://lince_interface/native/topology/terrain/ground.wesl".into()
    }
}

pub fn install(app: &mut App) {
    if app.world().contains_resource::<AssetServer>() {
        embedded_asset!(app, "ground.wesl");
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
    } else {
        app.init_resource::<Assets<GroundMaterial>>();
    }
}

mod materials;
pub(super) use materials::Materials;

pub(super) struct GroundView {
    pub depth: usize,
    pub clipped: bool,
    pub origin: DVec3,
    pub workspace: u64,
}

pub(super) fn refresh_material(
    world: &mut World,
    materials: &mut Materials,
    entity: Entity,
    key: Key,
) {
    let Some(id) = world
        .get::<MeshMaterial3d<GroundMaterial>>(entity)
        .map(|h| h.0.id())
    else {
        return;
    };
    let Some(current) = world
        .get_resource::<Assets<GroundMaterial>>()
        .and_then(|assets| assets.get(id))
    else {
        return;
    };
    let definition = materials.definition(world, key);
    if *current != definition {
        *world
            .resource_mut::<Assets<GroundMaterial>>()
            .get_mut(id)
            .unwrap() = definition;
        nucleus::performance::add("terrain-ground-material-writes", 1);
    }
}

fn visual(world: &mut World, materials: &mut Materials, key: Key, tile: &Tile) -> Option<Entity> {
    if tile.clipping.as_ref().is_some_and(Vec::is_empty) {
        return None;
    }
    let view = world
        .get::<crate::native::workspace::Workspaces>(key.0)?
        .active;
    let origin = super::super::presentation::origin(world, key.0);
    visual_in(
        world,
        materials,
        key,
        &tile.heights,
        GroundView {
            depth: tile.depth,
            clipped: tile.clipping.is_some(),
            origin,
            workspace: view,
        },
    )
}

pub(super) fn visual_in(
    world: &mut World,
    materials: &mut Materials,
    key: Key,
    heights: &[f64],
    view: GroundView,
) -> Option<Entity> {
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<GroundMaterial>>()
    {
        return None;
    }
    let positions: Vec<_> = vertices(key, heights)
        .map(|p| {
            (p - DVec3::new(key.minimum().x, 0.0, key.minimum().y))
                .as_vec3()
                .to_array()
        })
        .collect();
    let mut normals = Vec::with_capacity(positions.len());
    let mut uvs = if view.clipped {
        Vec::new()
    } else {
        Vec::with_capacity(positions.len())
    };
    let stride = GRID + 1;
    for z in 0..=GRID {
        for x in 0..=GRID {
            let left = x.saturating_sub(1);
            let right = (x + 1).min(GRID);
            let back = z.saturating_sub(1);
            let front = (z + 1).min(GRID);
            let dx = (heights[z * stride + right] - heights[z * stride + left])
                / ((right - left) as f64 * TILE_SIZE / GRID as f64);
            let dz = (heights[front * stride + x] - heights[back * stride + x])
                / ((front - back) as f64 * TILE_SIZE / GRID as f64);
            normals.push(
                Vec3::new(-dx as f32, 1.0, -dz as f32)
                    .normalize()
                    .to_array(),
            );
            if !view.clipped {
                uvs.push([
                    x as f32 * TILE_SIZE as f32 / GRID as f32,
                    z as f32 * TILE_SIZE as f32 / GRID as f32,
                ]);
            }
        }
    }
    let mut positions = positions;
    let mut indices: Vec<u32> = if view.clipped {
        Vec::new()
    } else {
        let mut indices = Vec::with_capacity(GRID * GRID * 6);
        indices.extend(triangles().flatten());
        indices
    };
    if view.clipped {
        let old = positions;
        let old_normals = normals;
        positions = Vec::new();
        normals = Vec::new();
        for triangle in triangles() {
            let points = triangle.map(|i| Vec3::from_array(old[i as usize]));
            let flat = points.map(|p| DVec2::new(f64::from(p.x), f64::from(p.z)) + key.minimum());
            let denominator = (flat[1] - flat[0]).perp_dot(flat[2] - flat[0]);
            for polygon in
                super::super::holes::pieces_in(world, key.0, view.workspace, key.1, flat.to_vec())
            {
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
    nucleus::performance::add("terrain-ground-visual-builds", 1);
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices));
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let definition = materials.definition(world, key);
    let handle = world
        .resource_mut::<Assets<GroundMaterial>>()
        .add(definition);
    Some(
        world
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(handle),
                Transform::from_translation(
                    (DVec3::new(key.minimum().x, -(view.depth as f64) * 0.2, key.minimum().y)
                        - view.origin)
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
    let _presentation = nucleus::performance::Span::new("terrain-ground-presentation");
    if !world.contains_resource::<Terrain>() {
        return;
    }
    let mut roots = Vec::new();
    for (entity, workspaces, view) in world
        .query::<(
            Entity,
            &crate::native::workspace::Workspaces,
            &crate::native::canvas::CanvasView,
        )>()
        .iter(world)
    {
        roots.extend(
            workspaces
                .entries
                .iter()
                .filter(|entry| super::super::holes::live(world, entity, entry.id))
                .map(|entry| (entity, entry.id, *view)),
        );
    }
    let mut materials = Materials::default();
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    let collisions = std::mem::take(&mut terrain.tiles);
    terrain.tiles = std::mem::take(&mut terrain.render_tiles);
    let physical_fields = super::preview::prepare(world, &mut terrain);
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
                    let quad = [
                        minimum,
                        minimum + DVec2::new(TILE_SIZE, 0.0),
                        minimum + DVec2::splat(TILE_SIZE),
                        minimum + DVec2::new(0.0, TILE_SIZE),
                    ];
                    let pieces = super::super::holes::pieces(world, root, workspace, quad.to_vec());
                    let clipping = (pieces.len() != 1 || pieces[0].as_slice() != quad.as_slice())
                        .then_some(pieces);
                    let depth = super::super::holes::depth(world, root, workspace);
                    if tile.clipping != clipping || tile.depth != depth {
                        if let Some(visual) = tile.visual.take() {
                            destroy(world, visual);
                        }
                        tile.clipping = clipping;
                        tile.depth = depth;
                    }
                    if tile.visual.is_none() {
                        tile.visual = visual(world, &mut materials, key, tile);
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
                        refresh_material(world, &mut materials, entity, key);
                    }
                }
            }
        }
    }
    super::overview::draw(world, &mut terrain, &mut materials);
    for tile in terrain.tiles.values() {
        if let Some(entity) = tile.visual
            && !visible.contains(&entity)
            && world.get::<Visibility>(entity) != Some(&Visibility::Hidden)
        {
            world.entity_mut(entity).insert(Visibility::Hidden);
            nucleus::performance::add("terrain-ground-component-writes", 1);
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
    for (key, fields) in physical_fields {
        if let Some(fields) = fields {
            terrain.fields.insert(key, fields);
        } else {
            terrain.fields.remove(&key);
        }
    }
    world.insert_resource(terrain);
}
