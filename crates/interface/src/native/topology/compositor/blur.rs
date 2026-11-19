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
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn depth_bias(&self) -> f32 {
        1.95
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = None;
            }
        }
        Ok(())
    }
}

#[derive(Component)]
struct Quad {
    entity: Entity,
    material: Handle<BlurMaterial>,
}

#[derive(Component, Clone, Copy, PartialEq, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub(super) struct Item {
    parameters: Vec4,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Stamp {
    size: UVec2,
    viewport: UVec4,
    target: Option<NormalizedRenderTarget>,
    radius: u32,
    bytes: u64,
}

#[derive(Component, Clone, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub(super) struct Request {
    stamp: Stamp,
    deadline: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Waiting,
    Ready,
    Unavailable(Limitation),
}

#[derive(Resource, Clone, Default)]
pub(super) struct Runtime(Arc<Mutex<Reports>>);

#[derive(Default)]
struct Reports {
    requested: BTreeMap<Entity, Stamp>,
    outcomes: BTreeMap<Entity, (Stamp, Outcome)>,
}

impl Runtime {
    fn record(
        &self,
        entity: Entity,
        stamp: Stamp,
        outcome: Outcome,
        wake: Option<&crate::native::wake::WakeSignal>,
    ) {
        let mut reports = self.0.lock().unwrap();
        if reports.requested.get(&entity) != Some(&stamp)
            || reports
                .outcomes
                .get(&entity)
                .is_some_and(|(old, current)| old == &stamp && *current == outcome)
        {
            return;
        }
        reports.outcomes.insert(entity, (stamp, outcome));
        drop(reports);
        if let Some(wake) = wake {
            wake.after(Duration::ZERO);
        }
    }
}

pub(super) fn install(app: &mut App) {
    app.add_plugins((
        MaterialPlugin::<BlurMaterial>::default(),
        ExtractComponentPlugin::<Item>::default(),
        ExtractComponentPlugin::<Request>::default(),
    ));
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wesl(include_str!("blur.wesl"), "lince/compositor-blur.wesl"),
        )
        .unwrap();
    let runtime = Runtime::default();
    app.insert_resource(runtime.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.insert_resource(runtime);
        draw::install(render);
    }
}

fn remove(world: &mut World, face: Entity) {
    if let Some(quad) = world.entity_mut(face).take::<Quad>()
        && let Ok(entity) = world.get_entity_mut(quad.entity)
    {
        entity.despawn();
    }
}

pub(super) fn synchronize(
    world: &mut World,
    remaining: u64,
    views: &[RenderLayers],
) -> (usize, usize, BTreeSet<Limitation>) {
    let _presentation = nucleus::performance::Span::new("blur-presentation");
    if !world.contains_resource::<Runtime>() {
        return (0, 0, BTreeSet::new());
    }
    world
        .run_system_cached_with(present, (remaining, views))
        .expect("present current background blur")
}

fn spawn_quad(
    world: &mut World,
    face: Entity,
    owner: Entity,
    material: Handle<BlurMaterial>,
    parameters: Vec4,
) -> Entity {
    let mesh = world.get::<Mesh3d>(face).unwrap().0.clone();
    world
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Item { parameters },
            NoAutomaticBatching,
            Pickable::IGNORE,
            VisualOwner(owner),
            Transform::default(),
            Visibility::Inherited,
            ChildOf(face),
        ))
        .id()
}

fn present(
    (In(mut remaining), bevy::ecs::system::InRef(views)): (
        In<u64>,
        bevy::ecs::system::InRef<[RenderLayers]>,
    ),
    world: &mut World,
    all_faces: &mut QueryState<Entity, With<SurfaceFace>>,
    quads: &mut QueryState<Entity, (With<SurfaceFace>, With<Quad>)>,
    all_cameras: &mut QueryState<Entity, With<super::super::stacking::OrderedCamera>>,
    requests: &mut QueryState<Entity, (With<super::super::stacking::OrderedCamera>, With<Request>)>,
    mut faces: Local<Vec<Entity>>,
    mut cameras: Local<Vec<Entity>>,
    mut admitted: Local<Vec<RenderLayers>>,
    mut requested: Local<Vec<(Entity, Stamp)>>,
) -> (usize, usize, BTreeSet<Limitation>) {
    let settings = world
        .get_resource::<crate::native::shader_effects::ShaderSettings>()
        .copied()
        .unwrap_or_default();
    let protected = world
        .get_resource::<crate::host::control::Status>()
        .is_some_and(|status| status.available && (status.locked || !status.active));
    let radius = if settings.disabled
        || protected
        || !crate::native::shader_effects::compositor_presentation_active(world)
        || !settings.compositor.valid()
    {
        0.0
    } else {
        settings.compositor.blur_radius
    };
    faces.clear();
    cameras.clear();
    admitted.clear();
    requested.clear();
    if radius == 0.0 {
        faces.extend(quads.iter(world));
        for face in faces.drain(..) {
            remove(world, face);
        }
        cameras.extend(requests.iter(world));
        for camera in cameras.drain(..) {
            world.entity_mut(camera).remove::<Request>();
        }
        let runtime = world.resource::<Runtime>();
        let mut reports = runtime.0.lock().unwrap();
        reports.outcomes.clear();
        reports.requested.clear();
        *faces = Vec::new();
        *cameras = Vec::new();
        *admitted = Vec::new();
        *requested = Vec::new();
        return (0, 0, BTreeSet::new());
    }
    faces.extend(all_faces.iter(world));
    if faces.is_empty() {
        *faces = Vec::new();
    }
    let mut count = 0;
    let mut limited = 0;
    let mut limits = BTreeSet::new();
    for face in faces.drain(..) {
        let layer = world.get::<RenderLayers>(face).cloned().unwrap_or_default();
        let owner = world.get::<VisualOwner>(face).map(|owner| owner.0);
        let live = owner.is_some_and(|owner| {
            world
                .get::<Surface>(owner)
                .is_some_and(|surface| surface.visible)
                && super::super::presentation::displayed(world, owner)
        }) && super::visible_face(world, face)
            && views.iter().any(|view| view.intersects(&layer));
        if !live {
            remove(world, face);
            continue;
        }
        if count >= MAX_ITEMS || remaining < ITEM_BYTES {
            limited += 1;
            limits.insert(Limitation::Budget);
            remove(world, face);
            continue;
        }
        let owner = owner.unwrap();
        let Some(mesh) = world.get::<Mesh3d>(face).map(|mesh| mesh.0.id()) else {
            remove(world, face);
            continue;
        };
        let size = world.get::<Surface>(owner).unwrap().size;
        let opacity = world
            .get::<MeshMaterial3d<StandardMaterial>>(face)
            .and_then(|material| {
                world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&material.0)
            })
            .map_or(0.0, |material| material.base_color.alpha());
        if !size.is_finite() || size.min_element() <= 0.0 || !opacity.is_finite() || opacity <= 0.0
        {
            remove(world, face);
            continue;
        }
        let parameters = Vec4::new(radius, opacity.clamp(0.0, 1.0), size.x, size.y);
        let existing = world
            .get::<Quad>(face)
            .map(|quad| (quad.entity, quad.material.id()));
        let mut entity = if let Some((entity, material)) = existing {
            if world
                .resource::<Assets<BlurMaterial>>()
                .get(material)
                .is_some_and(|material| material.parameters != parameters)
            {
                world
                    .resource_mut::<Assets<BlurMaterial>>()
                    .get_mut(material)
                    .unwrap()
                    .parameters = parameters;
                nucleus::performance::add("blur-parameter-updates", 1);
            }
            entity
        } else {
            let material = world
                .resource_mut::<Assets<BlurMaterial>>()
                .add(BlurMaterial {
                    parameters,
                    texture: None,
                });
            let entity = spawn_quad(world, face, owner, material.clone(), parameters);
            world.entity_mut(face).insert(Quad { entity, material });
            entity
        };
        if world.get_entity(entity).is_err() {
            let material = world.get::<Quad>(face).unwrap().material.clone();
            entity = spawn_quad(world, face, owner, material, parameters);
            world.get_mut::<Quad>(face).unwrap().entity = entity;
        }
        if world
            .get::<Mesh3d>(entity)
            .is_none_or(|old| old.0.id() != mesh)
        {
            let mesh = world.get::<Mesh3d>(face).unwrap().0.clone();
            world.entity_mut(entity).insert(Mesh3d(mesh));
        }
        if world.get::<Item>(entity) != Some(&Item { parameters }) {
            world.entity_mut(entity).insert(Item { parameters });
        }
        if world.get::<RenderLayers>(entity) != Some(&layer) {
            world.entity_mut(entity).insert(layer.clone());
        }
        super::super::stacking::decoration(world, face, entity, -1);
        remaining -= ITEM_BYTES;
        count += 1;
        admitted.push(layer);
    }
    cameras.extend(all_cameras.iter(world));
    cameras.sort_by_key(|entity| entity.to_bits());
    if cameras.is_empty() {
        *cameras = Vec::new();
    }
    let runtime = world.resource::<Runtime>().clone();
    let now = Instant::now();
    let mut pending = 0;
    for camera in cameras.drain(..) {
        let active = world
            .get::<Camera>(camera)
            .is_some_and(|camera| camera.is_active);
        let layers = world
            .get::<RenderLayers>(camera)
            .cloned()
            .unwrap_or_default();
        let visible = admitted
            .iter()
            .filter(|layers_for_item| layers_for_item.intersects(&layers))
            .count();
        if visible == 0 || !active {
            if world.get::<Request>(camera).is_some() {
                world.entity_mut(camera).remove::<Request>();
            }
            continue;
        }
        let size = world
            .get::<Camera>(camera)
            .and_then(Camera::physical_target_size);
        let viewport = world
            .get::<Camera>(camera)
            .and_then(Camera::physical_viewport_rect)
            .map(|rect| UVec4::new(rect.min.x, rect.min.y, rect.width(), rect.height()));
        let alternate = world
            .get::<OrderIndependentTransparencySettings>(camera)
            .is_some();
        let bytes = size
            .and_then(|size| {
                u64::from(size.x)
                    .checked_mul(u64::from(size.y))?
                    .checked_mul(16)
            })
            .and_then(|bytes| bytes.checked_add((visible as u64).saturating_mul(ITEM_BYTES)));
        let target_valid = viewport.is_some()
            && size.is_some_and(|size| size.min_element() > 0 && size.x.saturating_mul(size.y) > 1);
        let eligible = !alternate
            && requested.len() < MAX_VIEWS
            && target_valid
            && bytes.is_some_and(|bytes| bytes <= remaining);
        if !eligible {
            if world.get::<Request>(camera).is_some() {
                world.entity_mut(camera).remove::<Request>();
            }
            limited += 1;
            limits.insert(if alternate || !target_valid {
                Limitation::Target
            } else {
                Limitation::Budget
            });
            continue;
        }
        let stamp = Stamp {
            size: size.unwrap(),
            viewport: viewport.unwrap(),
            target: world
                .get::<RenderTarget>(camera)
                .and_then(|target| target.normalize(None)),
            radius: radius.to_bits(),
            bytes: bytes.unwrap(),
        };
        let previous_deadline = world
            .get::<Request>(camera)
            .filter(|request| request.stamp == stamp)
            .map(|request| request.deadline);
        let deadline = previous_deadline.unwrap_or(now + Duration::from_secs(2));
        let outcome = runtime
            .0
            .lock()
            .unwrap()
            .outcomes
            .get(&camera)
            .filter(|(old, _)| old == &stamp)
            .map(|(_, outcome)| *outcome);
        match outcome {
            Some(Outcome::Ready) => {}
            Some(Outcome::Unavailable(reason)) => {
                limited += 1;
                limits.insert(reason);
            }
            Some(Outcome::Waiting) | None if now < deadline => {
                pending += 1;
                if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                    wake.after(Duration::from_millis(30));
                }
            }
            _ => {
                limited += 1;
                limits.insert(Limitation::Timeout);
            }
        }
        if previous_deadline.is_none() {
            world.entity_mut(camera).insert(Request {
                stamp: stamp.clone(),
                deadline,
            });
            nucleus::performance::add("blur-request-replacements", 1);
        }
        remaining -= stamp.bytes;
        requested.push((camera, stamp));
    }
    if admitted.is_empty() {
        *admitted = Vec::new();
    } else {
        admitted.clear();
    }
    let mut reports = runtime.0.lock().unwrap();
    reports.outcomes.retain(|camera, (stamp, _)| {
        requested
            .binary_search_by_key(&camera.to_bits(), |(camera, _)| camera.to_bits())
            .is_ok_and(|index| requested[index].1 == *stamp)
    });
    reports.requested.retain(|camera, _| {
        requested
            .binary_search_by_key(&camera.to_bits(), |(camera, _)| camera.to_bits())
            .is_ok()
    });
    if requested.is_empty() {
        *requested = Vec::new();
    }
    for (camera, stamp) in requested.drain(..) {
        if reports.requested.get(&camera) != Some(&stamp) {
            reports.requested.insert(camera, stamp);
        }
    }
    (limited, pending, limits)
}
