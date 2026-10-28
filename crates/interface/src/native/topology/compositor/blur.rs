use super::{Limitation, Surface, SurfaceFace, VisualOwner};
use bevy::{
    camera::{NormalizedRenderTarget, RenderTarget, visibility::RenderLayers},
    core_pipeline::oit::OrderIndependentTransparencySettings,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::{
        RenderApp,
        batching::NoAutomaticBatching,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    },
    shader::ShaderRef,
};
use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex}, time::{Duration, Instant}};

mod draw;
pub(crate) use draw::{CaptureCache, begin_capture, end_capture};

#[cfg(test)]
mod tests;

const SHADER: Handle<Shader> = bevy::asset::uuid_handle!("0d48678c-6c94-47c5-b1b2-b78045e39f8a");
const MAX_ITEMS: usize = 1024;
const MAX_VIEWS: usize = 128;
const ITEM_BYTES: u64 = 4096;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct BlurMaterial {
    #[uniform(0)]
    parameters: Vec4,
    #[texture(1)]
    #[sampler(2)]
    texture: Option<Handle<Image>>,
}

impl Material for BlurMaterial {
    fn fragment_shader() -> ShaderRef { SHADER.into() }
    fn alpha_mode(&self) -> AlphaMode { AlphaMode::Premultiplied }
    fn depth_bias(&self) -> f32 { 1.95 }
    fn enable_prepass() -> bool { false }
    fn enable_shadows() -> bool { false }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() { target.blend = None; }
        }
        Ok(())
    }
}

#[derive(Component)]
struct Quad { entity: Entity, material: Handle<BlurMaterial> }

#[derive(Component, Clone, Copy, PartialEq, ExtractComponent)]
pub(super) struct Item { parameters: Vec4 }

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Stamp {
    size: UVec2,
    viewport: UVec4,
    target: Option<NormalizedRenderTarget>,
    radius: u32,
    bytes: u64,
}

#[derive(Component, Clone, ExtractComponent)]
pub(super) struct Request { stamp: Stamp, deadline: Instant }

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome { Waiting, Ready, Unavailable(Limitation) }

#[derive(Resource, Clone, Default)]
pub(super) struct Runtime(Arc<Mutex<Reports>>);

#[derive(Default)]
struct Reports {
    requested: BTreeMap<Entity, Stamp>,
    outcomes: BTreeMap<Entity, (Stamp, Outcome)>,
}

impl Runtime {
    fn record(&self, entity: Entity, stamp: Stamp, outcome: Outcome, wake: Option<&crate::native::wake::WakeSignal>) {
        let mut reports = self.0.lock().unwrap();
        if reports.requested.get(&entity) != Some(&stamp)
            || reports.outcomes.get(&entity).is_some_and(|(old, current)| old == &stamp && *current == outcome) { return; }
        reports.outcomes.insert(entity, (stamp, outcome));
        drop(reports);
        if let Some(wake) = wake { wake.after(Duration::ZERO); }
    }
}

pub(super) fn install(app: &mut App) {
    app.add_plugins((MaterialPlugin::<BlurMaterial>::default(), ExtractComponentPlugin::<Item>::default(), ExtractComponentPlugin::<Request>::default()));
    app.world_mut().resource_mut::<Assets<Shader>>().insert(SHADER.id(), Shader::from_wgsl(include_str!("blur.wgsl"), "lince/compositor-blur.wgsl")).unwrap();
    let runtime = Runtime::default();
    app.insert_resource(runtime.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.insert_resource(runtime);
        draw::install(render);
    }
}

fn remove(world: &mut World, face: Entity) {
    if let Some(quad) = world.entity_mut(face).take::<Quad>()
        && let Ok(entity) = world.get_entity_mut(quad.entity) { entity.despawn(); }
}

pub(super) fn synchronize(world: &mut World, mut remaining: u64, views: &[RenderLayers]) -> (usize, usize, BTreeSet<Limitation>) {
    if !world.contains_resource::<Runtime>() { return (0, 0, BTreeSet::new()); }
    let settings = world.get_resource::<crate::native::shader_effects::ShaderSettings>().copied().unwrap_or_default();
    let protected = world.get_resource::<crate::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active));
    let radius = if settings.disabled || protected || !crate::native::shader_effects::compositor_presentation_active(world)
        || !settings.compositor.valid() { 0.0 } else { settings.compositor.blur_radius };
    let faces = world.query_filtered::<Entity, With<SurfaceFace>>().iter(world).collect::<Vec<_>>();
    let mut count = 0;
    let mut limited = 0;
    let mut limits = BTreeSet::new();
    let mut admitted = Vec::new();
    for face in faces {
        let layer = world.get::<RenderLayers>(face).cloned().unwrap_or_default();
        let owner = world.get::<VisualOwner>(face).map(|owner| owner.0);
        let live = owner.is_some_and(|owner| world.get::<Surface>(owner).is_some_and(|surface| surface.visible)
            && super::super::presentation::displayed(world, owner)) && super::visible_face(world, face)
            && views.iter().any(|view| view.intersects(&layer));
        if radius == 0.0 || !live { remove(world, face); continue; }
        if count >= MAX_ITEMS || remaining < ITEM_BYTES { limited += 1; limits.insert(Limitation::Budget); remove(world, face); continue; }
        let owner = owner.unwrap();
        let Some(mesh) = world.get::<Mesh3d>(face).map(|mesh| mesh.0.clone()) else { remove(world, face); continue; };
        let size = world.get::<Surface>(owner).unwrap().size;
        let opacity = world.get::<MeshMaterial3d<StandardMaterial>>(face)
            .and_then(|material| world.resource::<Assets<StandardMaterial>>().get(&material.0)).map_or(0.0, |material| material.base_color.alpha());
        if !size.is_finite() || size.min_element() <= 0.0 || !opacity.is_finite() || opacity <= 0.0 { remove(world, face); continue; }
        let parameters = Vec4::new(radius, opacity.clamp(0.0, 1.0), size.x, size.y);
        let mut quad = if let Some(quad) = world.entity_mut(face).take::<Quad>() {
            let mut materials = world.resource_mut::<Assets<BlurMaterial>>();
            if let Some(mut material) = materials.get_mut(&quad.material)
                && material.parameters != parameters { material.parameters = parameters; }
            quad
        } else {
            let material = world.resource_mut::<Assets<BlurMaterial>>().add(BlurMaterial { parameters, texture: None });
            let entity = world.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Item { parameters }, NoAutomaticBatching,
                Pickable::IGNORE, VisualOwner(owner), Transform::default(), Visibility::Inherited, ChildOf(face))).id();
            Quad { entity, material }
        };
        if world.get_entity(quad.entity).is_err() {
            quad.entity = world.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(quad.material.clone()), Item { parameters }, NoAutomaticBatching,
                Pickable::IGNORE, VisualOwner(owner), Transform::default(), Visibility::Inherited, ChildOf(face))).id();
        }
        if world.get::<Mesh3d>(quad.entity).is_none_or(|old| old.0 != mesh) { world.entity_mut(quad.entity).insert(Mesh3d(mesh)); }
        if world.get::<Item>(quad.entity) != Some(&Item { parameters }) { world.entity_mut(quad.entity).insert(Item { parameters }); }
        if world.get::<RenderLayers>(quad.entity) != Some(&layer) { world.entity_mut(quad.entity).insert(layer.clone()); }
        super::super::stacking::decoration(world, face, quad.entity, -1);
        world.entity_mut(face).insert(quad);
        remaining -= ITEM_BYTES;
        count += 1;
        admitted.push(layer);
    }
    let mut cameras = world.query_filtered::<Entity, With<super::super::stacking::OrderedCamera>>().iter(world).collect::<Vec<_>>();
    cameras.sort_by_key(|entity| entity.to_bits());
    let runtime = world.resource::<Runtime>().clone();
    let now = Instant::now();
    let mut pending = 0;
    let mut requested = BTreeMap::new();
    for camera in cameras {
        let active = world.get::<Camera>(camera).is_some_and(|camera| camera.is_active);
        let layers = world.get::<RenderLayers>(camera).cloned().unwrap_or_default();
        let visible = admitted.iter().filter(|layers_for_item| layers_for_item.intersects(&layers)).count();
        if visible == 0 || !active || radius == 0.0 { world.entity_mut(camera).remove::<Request>(); continue; }
        let size = world.get::<Camera>(camera).and_then(Camera::physical_target_size);
        let viewport = world.get::<Camera>(camera).and_then(Camera::physical_viewport_rect)
            .map(|rect| UVec4::new(rect.min.x, rect.min.y, rect.width(), rect.height()));
        let alternate = world.get::<OrderIndependentTransparencySettings>(camera).is_some();
        let bytes = size.and_then(|size| u64::from(size.x).checked_mul(u64::from(size.y))?.checked_mul(16))
            .and_then(|bytes| bytes.checked_add((visible as u64).saturating_mul(ITEM_BYTES)));
        let target_valid = viewport.is_some() && size.is_some_and(|size| size.min_element() > 0 && size.x.saturating_mul(size.y) > 1);
        let eligible = !alternate && requested.len() < MAX_VIEWS && target_valid
            && bytes.is_some_and(|bytes| bytes <= remaining);
        if !eligible {
            world.entity_mut(camera).remove::<Request>();
            limited += 1;
            limits.insert(if alternate || !target_valid { Limitation::Target } else { Limitation::Budget });
            continue;
        }
        let stamp = Stamp { size: size.unwrap(), viewport: viewport.unwrap(),
            target: world.get::<RenderTarget>(camera).and_then(|target| target.normalize(None)),
            radius: radius.to_bits(), bytes: bytes.unwrap() };
        let request = world.get::<Request>(camera).filter(|request| request.stamp == stamp).cloned()
            .unwrap_or(Request { stamp: stamp.clone(), deadline: now + Duration::from_secs(2) });
        let outcome = runtime.0.lock().unwrap().outcomes.get(&camera).filter(|(old, _)| old == &stamp).map(|(_, outcome)| *outcome);
        match outcome {
            Some(Outcome::Ready) => {}
            Some(Outcome::Unavailable(reason)) => { limited += 1; limits.insert(reason); }
            Some(Outcome::Waiting) | None if now < request.deadline => {
                pending += 1;
                if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() { wake.after(Duration::from_millis(30)); }
            }
            _ => { limited += 1; limits.insert(Limitation::Timeout); }
        }
        if world.get::<Request>(camera).is_none_or(|old| old.stamp != stamp) { world.entity_mut(camera).insert(request); }
        remaining -= stamp.bytes;
        requested.insert(camera, stamp);
    }
    let mut reports = runtime.0.lock().unwrap();
    reports.outcomes.retain(|camera, (stamp, _)| requested.get(camera) == Some(stamp));
    reports.requested = requested;
    (limited, pending, limits)
}
