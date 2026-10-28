use super::{BlurMaterial, Item, Limitation, Outcome, Request, Runtime, Stamp, MAX_ITEMS};
use bevy::{
    app::SubApp,
    camera::{MainPassResolutionOverride, Viewport},
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d, Transparent3d},
        oit::OrderIndependentTransparencySettings,
        schedule::{Core3d, Core3dSystems},
    },
    ecs::{query::ROQueryItem, system::SystemParamItem},
    pbr::{DrawMesh, SetMeshBindGroup, SetMeshViewBindGroup, SetMeshViewBindingArrayBindGroup, main_transmissive_pass_3d},
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_phase::{
            AddRenderCommand, DrawFunctionId, DrawFunctions, PhaseItem, RenderCommand,
            RenderCommandResult, SetItemPipeline, SortedRenderPhase, TrackedRenderPass,
            ViewSortedRenderPhases,
        },
        render_resource::{
            AsBindGroup, BindGroup, BindGroupEntries, CachedPipelineState, FilterMode,
            PipelineCache, RenderPassDescriptor, Sampler, SamplerDescriptor,
            StoreOp, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
            TextureUsages, TextureView, UniformBuffer,
        },
        renderer::{RenderAdapter, RenderContext, RenderDevice, RenderQueue, ViewQuery},
        sync_world::MainEntity,
        view::{ExtractedView, RetainedViewEntity, ViewDepthTexture, ViewTarget},
        Render, RenderSystems,
    },
    shader::ShaderCacheError,
};
use std::collections::{HashMap, HashSet};

type Key = (Entity, MainEntity);
type DrawBlur = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetBackdrop,
    DrawMesh,
);

struct Group {
    inputs: UniformBuffer<Vec4>,
    binding: BindGroup,
}

#[derive(Component)]
struct Prepared {
    stamp: Stamp,
    format: TextureFormat,
    texture: Texture,
    view: TextureView,
    sampler: Sampler,
    groups: HashMap<Entity, Group>,
}

pub(crate) struct CaptureCache(Vec<(Entity, Prepared)>);

pub(crate) fn begin_capture(world: &mut World) -> CaptureCache {
    let entities = world.query_filtered::<Entity, With<Prepared>>().iter(world).collect::<Vec<_>>();
    CaptureCache(entities.into_iter().filter_map(|entity| world.entity_mut(entity).take::<Prepared>().map(|prepared| (entity, prepared))).collect())
}

pub(crate) fn end_capture(world: &mut World, saved: CaptureCache) {
    let entities = world.query_filtered::<Entity, With<Prepared>>().iter(world).collect::<Vec<_>>();
    for entity in entities { world.entity_mut(entity).remove::<Prepared>(); }
    for (entity, prepared) in saved.0 {
        if world.get::<Request>(entity).is_some_and(|request| request.stamp == prepared.stamp)
            && let Ok(mut entity) = world.get_entity_mut(entity) { entity.insert(prepared); }
    }
}

struct SetBackdrop;

impl RenderCommand<Transparent3d> for SetBackdrop {
    type Param = ();
    type ViewQuery = &'static Prepared;
    type ItemQuery = ();

    fn render<'w>(
        item: &Transparent3d,
        prepared: ROQueryItem<'w, '_, Self::ViewQuery>,
        _: Option<()>,
        _: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(group) = prepared.groups.get(&item.entity()) else { return RenderCommandResult::Skip; };
        pass.set_bind_group(3, &group.binding, &[]);
        RenderCommandResult::Success
    }
}

struct Retained {
    phase: SortedRenderPhase<Transparent3d>,
    originals: Vec<(Key, DrawFunctionId)>,
    boundaries: Vec<usize>,
}

struct Removed {
    index: usize,
    key: Key,
    item: Transparent3d,
}

#[derive(Resource, Default)]
struct Phases {
    views: HashMap<RetainedViewEntity, Retained>,
    suppressed: HashMap<RetainedViewEntity, Vec<Removed>>,
}

pub(super) fn install(render: &mut SubApp) {
    render.init_resource::<Phases>()
        .add_render_command::<Transparent3d, DrawBlur>()
        .add_systems(Render, (prepare, route).chain().after(RenderSystems::Prepare).before(RenderSystems::Render))
        .add_systems(Core3d, draw.in_set(Core3dSystems::MainPass).after(main_opaque_pass_3d)
            .after(main_transmissive_pass_3d).before(main_transparent_pass_3d))
        .add_systems(Render, restore.after(RenderSystems::Render).before(RenderSystems::Cleanup));
}

fn prepare(
    mut commands: Commands,
    mut views: Query<(
        Entity, &MainEntity, &ExtractedView, &ExtractedCamera, &ViewTarget,
        Option<&Request>, Option<&mut Prepared>, Has<OrderIndependentTransparencySettings>,
        Option<&MainPassResolutionOverride>,
    )>,
    inactive: Query<Entity, (With<Prepared>, Or<(Without<Request>, Without<ViewTarget>, Without<ExtractedView>, Without<ExtractedCamera>)>)>,
    quads: Query<(&Item, &crate::native::topology::stacking::DecorationAnchor)>,
    phases: Res<ViewSortedRenderPhases<Transparent3d>>,
    device: Res<RenderDevice>,
    adapter: Res<RenderAdapter>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    runtime: Res<Runtime>,
    wake: Option<Res<crate::native::wake::WakeSignal>>,
    external: Option<Res<crate::native::capture::render::ExternalPass>>,
) {
    for entity in &inactive { commands.entity(entity).remove::<Prepared>(); }
    let layout = BlurMaterial::bind_group_layout_descriptor(&device);
    for (entity, main, view, camera, target, request, existing, alternate, resolution_override) in &mut views {
        let Some(request) = request else {
            if existing.is_some() { commands.entity(entity).remove::<Prepared>(); }
            continue;
        };
        let report = |outcome| {
            if external.is_none() { runtime.record(main.id(), request.stamp.clone(), outcome, wake.as_deref()); }
        };
        let texture = target.main_texture();
        let format = target.main_texture_format();
        let extent = texture.size();
        let features = adapter.get_texture_format_features(format);
        let viewport = view.viewport;
        let matching = viewport == request.stamp.viewport && camera.target == request.stamp.target
            && extent.width == request.stamp.size.x && extent.height == request.stamp.size.y;
        if !matching {
            commands.entity(entity).remove::<Prepared>();
            report(Outcome::Waiting);
            continue;
        }
        let viewport_fits = viewport.z > 0 && viewport.w > 0
            && viewport.x.checked_add(viewport.z).is_some_and(|end| end <= extent.width)
            && viewport.y.checked_add(viewport.w).is_some_and(|end| end <= extent.height);
        let eligible = !alternate && resolution_override.is_none()
            && viewport_fits
            && extent.depth_or_array_layers == 1 && texture.sample_count() == 1
            && texture.dimension() == TextureDimension::D2 && texture.format() == format
            && extent.width <= device.limits().max_texture_dimension_2d
            && extent.height <= device.limits().max_texture_dimension_2d
            && format.block_dimensions() == (1, 1) && format.block_copy_size(None).is_some_and(|bytes| bytes <= 16)
            && format.sample_type(None, Some(device.features())) == Some(TextureSampleType::Float { filterable: true })
            && texture.usage().contains(TextureUsages::COPY_SRC)
            && features.allowed_usages.contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
        if !eligible {
            commands.entity(entity).remove::<Prepared>();
            report(Outcome::Unavailable(Limitation::Target));
            continue;
        }
        let Some(phase) = phases.get(&view.retained_view_entity) else {
            commands.entity(entity).remove::<Prepared>();
            report(Outcome::Waiting);
            continue;
        };
        let originals: HashMap<_, _> = phase.items.iter().map(|(key, item)| (key.1.id(), item)).collect();
        let items = phase.items.values().filter_map(|item| quads.get(item.entity()).ok().map(|quad| (item, quad))).collect::<Vec<_>>();
        if items.is_empty() {
            commands.entity(entity).remove::<Prepared>();
            report(Outcome::Ready);
            continue;
        }
        if items.iter().any(|(_, (_, anchor))| !originals.contains_key(&anchor.0)) {
            commands.entity(entity).remove::<Prepared>();
            report(Outcome::Waiting);
            continue;
        }
        let allocation = u64::from(extent.width).checked_mul(u64::from(extent.height))
            .and_then(|pixels| pixels.checked_mul(16))
            .and_then(|bytes| bytes.checked_add((items.len() as u64).saturating_mul(super::ITEM_BYTES)));
        if items.len() > MAX_ITEMS || !allocation.is_some_and(|bytes| bytes <= request.stamp.bytes) {
            commands.entity(entity).remove::<Prepared>();
            report(Outcome::Unavailable(Limitation::Budget));
            continue;
        }
        let mut outcome = Outcome::Ready;
        for pipeline in items.iter().flat_map(|(item, (_, anchor))| [item.pipeline, originals[&anchor.0].pipeline]) {
            match cache.get_render_pipeline_state(pipeline) {
                CachedPipelineState::Ok(_) => {}
                CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(_)) => {
                    if outcome == Outcome::Ready { outcome = Outcome::Waiting; }
                }
                CachedPipelineState::Err(_) => { outcome = Outcome::Unavailable(Limitation::Shader); }
                _ => { if outcome == Outcome::Ready { outcome = Outcome::Waiting; } }
            }
        }
        if outcome != Outcome::Ready {
            commands.entity(entity).remove::<Prepared>();
            report(outcome);
            continue;
        }
        let update = |prepared: &mut Prepared| {
            let alive: HashSet<_> = items.iter().map(|(item, _)| item.entity()).collect();
            prepared.groups.retain(|entity, _| alive.contains(entity));
            for (item, (quad, _)) in &items {
                if let Some(group) = prepared.groups.get_mut(&item.entity()) {
                    if *group.inputs.get() != quad.parameters {
                        group.inputs.set(quad.parameters);
                        group.inputs.write_buffer(&device, &queue);
                    }
                } else {
                    let mut inputs = UniformBuffer::from(quad.parameters);
                    inputs.write_buffer(&device, &queue);
                    let binding = device.create_bind_group("Lince backdrop", &cache.get_bind_group_layout(&layout),
                        &BindGroupEntries::sequential((&inputs, &prepared.view, &prepared.sampler)));
                    prepared.groups.insert(item.entity(), Group { inputs, binding });
                }
            }
        };
        if let Some(mut prepared) = existing.filter(|prepared| prepared.stamp == request.stamp && prepared.format == format) {
            update(&mut prepared);
        } else {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("Lince view backdrop"), size: extent, mip_level_count: 1, sample_count: 1,
                dimension: TextureDimension::D2, format,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST, view_formats: &[],
            });
            let view = texture.create_view(&default());
            let sampler = device.create_sampler(&SamplerDescriptor {
                label: Some("Lince backdrop"), mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default()
            });
            let mut prepared = Prepared { stamp: request.stamp.clone(), format, texture, view, sampler, groups: HashMap::new() };
            update(&mut prepared);
            commands.entity(entity).insert(prepared);
        }
    }
}

fn suppress(phase: &mut SortedRenderPhase<Transparent3d>, quads: &HashSet<Entity>) -> Vec<Removed> {
    let indices = phase.items.iter().enumerate().filter_map(|(index, (key, _))| quads.contains(&key.0).then_some(index)).collect::<Vec<_>>();
    let mut removed = Vec::with_capacity(indices.len());
    for index in indices.into_iter().rev() {
        if let Some((key, item)) = phase.items.shift_remove_index(index) { removed.push(Removed { index, key, item }); }
    }
    removed.reverse();
    removed
}

fn retain(mut phase: SortedRenderPhase<Transparent3d>, quads: &HashSet<Entity>, draw: DrawFunctionId) -> Retained {
    let mut originals = Vec::new();
    let mut boundaries = Vec::new();
    for (index, (key, item)) in phase.items.iter_mut().enumerate() {
        if quads.contains(&key.0) {
            originals.push((*key, item.draw_function));
            item.draw_function = draw;
            boundaries.push(index);
        }
    }
    Retained { phase, originals, boundaries }
}

fn route(
    views: Query<(&ExtractedView, &Prepared)>,
    quads: Query<Entity, With<Item>>,
    functions: Res<DrawFunctions<Transparent3d>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut retained: ResMut<Phases>,
) {
    let quads: HashSet<_> = quads.iter().collect();
    if quads.is_empty() { return; }
    let draw = functions.read().get_id::<DrawBlur>();
    let prepared: HashMap<_, _> = views.iter().map(|(view, prepared)| (view.retained_view_entity, prepared)).collect();
    let keys = phases.keys().copied().collect::<Vec<_>>();
    for key in keys {
        let Some(phase) = phases.get(&key) else { continue; };
        if !phase.items.keys().any(|key| quads.contains(&key.0)) { continue; }
        let admitted = draw.is_some() && prepared.get(&key).is_some_and(|prepared| phase.items.keys()
            .filter(|key| quads.contains(&key.0)).all(|key| prepared.groups.contains_key(&key.0)));
        if admitted {
            let phase = phases.remove(&key).unwrap();
            retained.views.insert(key, retain(phase, &quads, draw.unwrap()));
        } else if let Some(phase) = phases.get_mut(&key) {
            retained.suppressed.insert(key, suppress(phase, &quads));
        }
    }
}

fn draw(
    world: &World,
    view: ViewQuery<(Entity, &MainEntity, &Request, &ExtractedCamera, &ExtractedView, &ViewTarget, &ViewDepthTexture, &Prepared)>,
    phases: Res<Phases>,
    mut context: RenderContext,
) {
    let (entity, main, request, camera, view, target, depth, prepared) = view.into_inner();
    let Some(retained) = phases.views.get(&view.retained_view_entity) else { return; };
    let mut start = 0;
    let mut rendered = true;
    for &boundary in &retained.boundaries {
        rendered &= draw_range(world, entity, camera, target, depth, &retained.phase, start..boundary, &mut context);
        context.command_encoder().copy_texture_to_texture(target.main_texture().as_image_copy(),
            prepared.texture.as_image_copy(), target.main_texture().size());
        start = boundary;
    }
    rendered &= draw_range(world, entity, camera, target, depth, &retained.phase, start..retained.phase.items.len(), &mut context);
    if !world.contains_resource::<crate::native::capture::render::ExternalPass>() {
        world.resource::<Runtime>().record(main.id(), request.stamp.clone(),
            if rendered { Outcome::Ready } else { Outcome::Unavailable(Limitation::Shader) }, world.get_resource::<crate::native::wake::WakeSignal>());
    }
}

fn draw_range(
    world: &World,
    entity: Entity,
    camera: &ExtractedCamera,
    target: &ViewTarget,
    depth: &ViewDepthTexture,
    phase: &SortedRenderPhase<Transparent3d>,
    range: std::ops::Range<usize>,
    context: &mut RenderContext,
) -> bool {
    if range.is_empty() { return true; }
    let diagnostics = context.diagnostic_recorder();
    let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("lince_backdrop_transparent"),
        color_attachments: &[Some(target.get_color_attachment())],
        depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
        timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
    });
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.pass_span(&mut pass, "lince_backdrop_transparent");
    if let Some(viewport) = Viewport::from_viewport_and_override(camera.viewport.as_ref(), None) { pass.set_camera_viewport(&viewport); }
    let result = phase.render_range(&mut pass, world, entity, range);
    span.end(&mut pass);
    if let Err(error) = result { error!("Cannot render Lince backdrop phase: {error:?}"); return false; }
    true
}

#[cfg(test)]
mod tests;

fn restore(mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>, mut retained: ResMut<Phases>) {
    for (key, mut retained) in retained.views.drain() {
        for (key, draw) in retained.originals {
            if let Some(item) = retained.phase.items.get_mut(&key) { item.draw_function = draw; }
        }
        phases.insert(key, retained.phase);
    }
    for (key, removed) in retained.suppressed.drain() {
        if let Some(phase) = phases.get_mut(&key) {
            for removed in removed { phase.items.shift_insert(removed.index, removed.key, removed.item); }
        }
    }
}
