use super::presentation::{Surface, VisualOwner};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::RenderLayers,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::Material,
    prelude::*,
    render::{RenderApp, render_resource::{AsBindGroup, ShaderType}},
    shader::ShaderRef,
};
use std::{collections::{BTreeMap, BTreeSet}, hash::{Hash, Hasher}};

#[cfg(test)]
mod tests;
mod blur;
pub(crate) use blur::{CaptureCache, begin_capture, end_capture};

const SHADOW: Handle<Shader> = bevy::asset::uuid_handle!("c1b9e2d6-04a7-42cb-a046-cf632a3d890e");
const MAX_EDGES: usize = 64;
const SHADOW_ALLOCATION_BYTES: u64 = 4096;

#[derive(Component)]
pub(super) struct SurfaceFace;

#[derive(Resource, Default, PartialEq)]
pub(crate) struct Status {
    pub shadow_limited: usize,
    pub blur_limited: usize,
    pub blur_pending: usize,
    pub shadow_limits: BTreeSet<Limitation>,
    pub blur_limits: BTreeSet<Limitation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Limitation { Budget, Outline, Target, Shader, Timeout }

impl Limitation {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Budget => "The current rendering budget is full.",
            Self::Outline => "A clipped outline is too complex or invalid for its shadow.",
            Self::Target => "This output's rendering path cannot apply the effect.",
            Self::Shader => "An effect shader could not be prepared or drawn.",
            Self::Timeout => "Effect preparation timed out; a scene change will retry it.",
        }
    }
}

#[derive(Clone, ShaderType)]
struct Parameters {
    bounds: Vec4,
    style: Vec4,
    edges: [Vec4; MAX_EDGES],
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct ShadowMaterial {
    #[uniform(0)]
    parameters: Parameters,
}

impl Material for ShadowMaterial {
    fn fragment_shader() -> ShaderRef { SHADOW.into() }
    fn alpha_mode(&self) -> AlphaMode { AlphaMode::Premultiplied }
    fn depth_bias(&self) -> f32 { 2.05 }
    fn enable_prepass() -> bool { false }
    fn enable_shadows() -> bool { false }
}

#[derive(Component)]
struct Decoration {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<ShadowMaterial>,
    fingerprint: u64,
    opacity: f32,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Status>();
    if app.get_sub_app(RenderApp).is_none() { return; }
    app.add_plugins(MaterialPlugin::<ShadowMaterial>::default());
    blur::install(app);
    app.world_mut().resource_mut::<Assets<Shader>>().insert(SHADOW.id(), Shader::from_wgsl(include_str!("compositor/shadow.wgsl"), "lince/compositor-shadow.wgsl")).unwrap();
    app.add_systems(PostUpdate, synchronize.after(super::presentation::synchronize).after(super::appearance::update)
        .before(bevy::camera::CameraUpdateSystems).before(bevy::transform::TransformSystems::Propagate));
}

fn remove(world: &mut World, face: Entity) {
    if let Some(decoration) = world.entity_mut(face).take::<Decoration>()
        && let Ok(entity) = world.get_entity_mut(decoration.entity) { entity.despawn(); }
}

fn visible_face(world: &World, face: Entity) -> bool {
    world.get::<Visibility>(face) != Some(&Visibility::Hidden)
        && world.get::<ChildOf>(face).is_none_or(|parent| world.get::<Visibility>(parent.parent()) != Some(&Visibility::Hidden))
}

fn synchronize(world: &mut World) {
    let settings = world.get_resource::<crate::native::shader_effects::ShaderSettings>().copied().unwrap_or_default();
    let protected = world.get_resource::<crate::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active));
    let radius = if settings.disabled || protected || !crate::native::shader_effects::compositor_presentation_active(world)
        || !settings.compositor.valid() { 0.0 } else { settings.compositor.shadow_radius };
    let faces = world.query_filtered::<Entity, With<SurfaceFace>>().iter(world).collect::<Vec<_>>();
    let views = world.query_filtered::<(&Camera, Option<&RenderLayers>), With<super::stacking::OrderedCamera>>()
        .iter(world).filter(|(camera, _)| camera.is_active).map(|(_, layers)| layers.cloned().unwrap_or_default()).collect::<Vec<_>>();
    let mut remaining = super::surface_budget::BYTE_BUDGET.saturating_sub(world.get_resource::<super::surface_budget::Diagnostics>().map_or(0, |diagnostics| diagnostics.granted_bytes));
    let mut limited = 0;
    let mut shadow_limits = BTreeSet::new();
    for face in faces {
        let layer = world.get::<RenderLayers>(face).cloned().unwrap_or_default();
        let owner = world.get::<VisualOwner>(face).map(|owner| owner.0);
        let live = owner.is_some_and(|owner| world.get::<Surface>(owner).is_some_and(|surface| surface.visible)
            && super::presentation::displayed(world, owner)) && visible_face(world, face) && views.iter().any(|view| view.intersects(&layer));
        if radius == 0.0 || !live { remove(world, face); continue; }
        if remaining < SHADOW_ALLOCATION_BYTES { limited += 1; shadow_limits.insert(Limitation::Budget); remove(world, face); continue; }
        let owner = owner.unwrap();
        let Some(source) = world.get::<Mesh3d>(face).map(|mesh| mesh.0.clone()) else { remove(world, face); continue; };
        let Some(size) = world.get::<Surface>(owner).map(|surface| surface.size) else { remove(world, face); continue; };
        let opacity = world.get::<MeshMaterial3d<StandardMaterial>>(face)
            .and_then(|material| world.resource::<Assets<StandardMaterial>>().get(&material.0)).map_or(0.0, |material| material.base_color.alpha());
        if !opacity.is_finite() || opacity <= 0.0 { remove(world, face); continue; }
        let shape = world.resource::<Assets<Mesh>>().get(&source).and_then(|mesh| shape(mesh, size, radius));
        let Some((mesh, parameters, fingerprint)) = shape else { limited += 1; shadow_limits.insert(Limitation::Outline); remove(world, face); continue; };
        remaining -= SHADOW_ALLOCATION_BYTES;
        let mut parameters = parameters;
        parameters.style.y = opacity;
        let mut decoration = if let Some(mut decoration) = world.entity_mut(face).take::<Decoration>() {
            if decoration.fingerprint != fingerprint {
                world.resource_mut::<Assets<Mesh>>().insert(decoration.mesh.id(), mesh).unwrap();
                decoration.fingerprint = fingerprint;
                world.resource_mut::<Assets<ShadowMaterial>>().get_mut(&decoration.material).unwrap().parameters = parameters;
                decoration.opacity = opacity;
            } else if decoration.opacity != opacity {
                world.resource_mut::<Assets<ShadowMaterial>>().get_mut(&decoration.material).unwrap().parameters.style.y = opacity;
                decoration.opacity = opacity;
            }
            decoration
        } else {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
            let material = world.resource_mut::<Assets<ShadowMaterial>>().add(ShadowMaterial { parameters });
            let entity = world.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Pickable::IGNORE,
                VisualOwner(owner), Transform::default(), Visibility::Inherited, ChildOf(face))).id();
            Decoration { entity, mesh, material, fingerprint, opacity }
        };
        if world.get_entity(decoration.entity).is_err() {
            decoration.entity = world.spawn((Mesh3d(decoration.mesh.clone()), MeshMaterial3d(decoration.material.clone()), Pickable::IGNORE,
                VisualOwner(owner), Transform::default(), Visibility::Inherited, ChildOf(face))).id();
        }
        if world.get::<RenderLayers>(decoration.entity) != Some(&layer) { world.entity_mut(decoration.entity).insert(layer); }
        super::stacking::decoration(world, face, decoration.entity, 1);
        world.entity_mut(face).insert(decoration);
    }
    let (blur_limited, blur_pending, blur_limits) = blur::synchronize(world, remaining, &views);
    let status = Status { shadow_limited: limited, blur_limited, blur_pending, shadow_limits, blur_limits };
    if world.get_resource::<Status>() != Some(&status) {
        world.insert_resource(status);
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() { wake.after(std::time::Duration::ZERO); }
    }
}

fn shape(mesh: &Mesh, size: Vec2, radius: f32) -> Option<(Mesh, Parameters, u64)> {
    if !size.is_finite() || size.min_element() <= 0.0 || mesh.primitive_topology() != PrimitiveTopology::TriangleList { return None; }
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)? else { return None; };
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)? else { return None; };
    if positions.len() != uvs.len() || positions.len() > 4096 { return None; }
    let indices = match mesh.indices() {
        Some(Indices::U16(indices)) if indices.len() <= 6144 => indices.iter().map(|index| usize::from(*index)).collect::<Vec<_>>(),
        Some(Indices::U32(indices)) if indices.len() <= 6144 => indices.iter().map(|index| *index as usize).collect(),
        None => (0..positions.len()).collect(),
        _ => return None,
    };
    if indices.is_empty() || indices.len() % 3 != 0 || indices.len() > 6144 { return None; }
    let mut hasher = std::hash::DefaultHasher::new();
    indices.hash(&mut hasher);
    radius.to_bits().hash(&mut hasher);
    size.to_array().map(f32::to_bits).hash(&mut hasher);
    let mut edges = BTreeMap::<([u32; 2], [u32; 2]), (Vec2, Vec2, usize)>::new();
    let mut basis = None;
    for triangle in indices.chunks_exact(3) {
        let vertices = [triangle[0], triangle[1], triangle[2]].map(|index| Some((Vec3::from_array(*positions.get(index)?), Vec2::from_array(*uvs.get(index)?))));
        let [Some((a, u)), Some((b, v)), Some((c, w))] = vertices else { return None; };
        if [a, b, c].iter().any(|point| !point.is_finite()) || [u, v, w].iter().any(|uv| !uv.is_finite()) { return None; }
        for (point, uv) in [(a, u), (b, v), (c, w)] {
            point.to_array().map(f32::to_bits).hash(&mut hasher);
            uv.to_array().map(f32::to_bits).hash(&mut hasher);
        }
        let delta_v = v - u;
        let delta_w = w - u;
        let determinant = delta_v.perp_dot(delta_w);
        if basis.is_none() && determinant.abs() > 1e-8 {
            let x = ((b - a) * delta_w.y - (c - a) * delta_v.y) / determinant;
            let y = ((c - a) * delta_v.x - (b - a) * delta_w.x) / determinant;
            basis = Some((a - x * u.x - y * u.y, x, y, (b - a).cross(c - a).normalize_or_zero()));
        }
        for (start, end) in [(u, v), (v, w), (w, u)] {
            if start == end { continue; }
            let key = |point: Vec2| point.to_array().map(|value| if value == 0.0 { 0 } else { value.to_bits() });
            let (a, b) = (key(start), key(end));
            let key = if a < b { (a, b) } else { (b, a) };
            let entry = edges.entry(key).or_insert((start * size, end * size, 0));
            entry.2 += 1;
        }
    }
    if edges.values().any(|(_, _, count)| *count > 2) { return None; }
    let boundary = edges.into_values().filter(|(_, _, count)| *count == 1).collect::<Vec<_>>();
    if boundary.is_empty() || boundary.len() > MAX_EDGES { return None; }
    let (origin, x, y, normal) = basis?;
    if !origin.is_finite() || !x.is_finite() || !y.is_finite() || normal.length_squared() < 0.5 { return None; }
    let mut low = Vec2::splat(f32::INFINITY);
    let mut high = Vec2::splat(f32::NEG_INFINITY);
    let mut segments = [Vec4::ZERO; MAX_EDGES];
    for (index, (start, end, _)) in boundary.iter().enumerate() {
        if !start.is_finite() || !end.is_finite() { return None; }
        low = low.min(*start).min(*end);
        high = high.max(*start).max(*end);
        segments[index] = Vec4::new(start.x, start.y, end.x, end.y);
    }
    let pad = radius * 3.0;
    low -= Vec2::splat(pad);
    high += Vec2::splat(pad);
    let quad = [low, Vec2::new(high.x, low.y), high, Vec2::new(low.x, high.y)];
    let positions = quad.map(|point| (origin + x * (point.x / size.x) + y * (point.y / size.y) + normal * 0.0001).to_array());
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.to_vec())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![normal.to_array(); 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .with_inserted_indices(Indices::U32(if x.cross(y).dot(normal) >= 0.0 { vec![0, 1, 2, 0, 2, 3] } else { vec![0, 2, 1, 0, 3, 2] }));
    Some((mesh, Parameters { bounds: Vec4::new(low.x, low.y, high.x, high.y), style: Vec4::new(radius, 1.0, boundary.len() as f32, 0.0), edges: segments }, hasher.finish()))
}
