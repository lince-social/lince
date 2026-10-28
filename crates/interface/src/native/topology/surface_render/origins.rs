use super::*;
use bevy::{
    asset::VisitAssetDependencies,
    camera::{CameraOutputMode, ClearColorConfig, NormalizedRenderTarget, visibility::VisibleEntities},
    core_pipeline::{blit::{BlitPipeline, BlitPipelineKey}, upscaling::ViewUpscalingPipeline},
    render::{
        render_resource::{BlendState, SpecializedRenderPipelines},
        renderer::ViewQuery,
    },
};
use std::{collections::{BTreeMap, BTreeSet}, sync::Arc};

const MAX_LINKS: usize = crate::host::capture::MAX_CONTRIBUTORS;

#[derive(Clone, Debug, Default)]
pub(crate) struct CaptureOrigins {
    pub contributors: BTreeSet<Entity>,
    pub images: BTreeSet<AssetId<Image>>,
    pub complete: bool,
}

impl CaptureOrigins {
    fn empty() -> Self { Self { complete: true, ..default() } }

    fn links(&self) -> usize { self.contributors.len().saturating_add(self.images.len()) }

    fn bound(&mut self) {
        if self.links() > MAX_LINKS {
            self.contributors.clear();
            self.images.clear();
            self.complete = false;
        }
    }

    fn extend(&mut self, other: &Self) {
        self.complete &= other.complete;
        self.contributors.extend(other.contributors.iter().copied());
        self.images.extend(other.images.iter().copied());
        self.bound();
    }

    fn normalize(&mut self, scopes: &BTreeMap<Entity, BTreeSet<Entity>>) {
        let mut contributors = BTreeSet::new();
        for entity in &self.contributors {
            if let Some(sources) = scopes.get(entity) { contributors.extend(sources.iter().copied()); }
            else { contributors.insert(*entity); self.complete = false; }
        }
        self.contributors = contributors;
        self.bound();
    }
}

struct Pending {
    order: (isize, usize),
    clears: bool,
    origins: CaptureOrigins,
}

#[derive(Default)]
struct State {
    images: BTreeMap<AssetId<Image>, Arc<CaptureOrigins>>,
    pending: BTreeMap<AssetId<Image>, Vec<Pending>>,
    links: usize,
    pending_links: usize,
}

#[derive(Resource, Clone, Default)]
pub(crate) struct CompletedCaptures(Arc<Mutex<State>>);

impl CompletedCaptures {
    pub(crate) fn contains(&self, image: &Handle<Image>) -> bool {
        self.0.lock().unwrap().images.contains_key(&image.id())
    }

    pub(crate) fn snapshot(&self) -> Option<BTreeMap<AssetId<Image>, Arc<CaptureOrigins>>> {
        self.0.lock().ok().map(|state| state.images.clone())
    }

    pub(crate) fn queue(&self, image: AssetId<Image>, order: (isize, usize), clears: bool, mut origins: CaptureOrigins) {
        origins.bound();
        let mut state = self.0.lock().unwrap();
        if state.pending_links.saturating_add(origins.links()) > MAX_LINKS {
            origins = CaptureOrigins::default();
        }
        state.pending_links += origins.links();
        state.pending.entry(image).or_default().push(Pending { order, clears, origins });
    }

    pub(crate) fn seed(&self, images: BTreeMap<AssetId<Image>, Arc<CaptureOrigins>>) {
        let mut state = self.0.lock().unwrap();
        state.links = 0;
        state.images.clear();
        for (image, mut origins) in images {
            if state.links.saturating_add(origins.links()) > MAX_LINKS { origins = Arc::new(CaptureOrigins::default()); }
            state.links += origins.links();
            state.images.insert(image, origins);
        }
    }
}

#[derive(Component)]
pub(super) struct Inputs(CaptureOrigins);

#[derive(Resource)]
pub(super) struct MaterialInputs(
    BTreeMap<Entity, BTreeSet<AssetId<Image>>>,
    BTreeSet<AssetId<Image>>,
    BTreeMap<Entity, BTreeSet<Entity>>,
);

pub(super) fn begin(completed: Res<CompletedCaptures>, images: Res<RenderAssets<GpuImage>>) {
    let mut state = completed.0.lock().unwrap();
    state.pending.clear();
    state.pending_links = 0;
    state.images.retain(|image, _| images.get(*image).is_some());
    state.links = state.images.values().map(|origins| origins.links()).sum();
}

pub(super) fn finish(
    completed: Res<CompletedCaptures>,
    external: Option<Res<crate::native::capture::render::ExternalPass>>,
    wake: Option<Res<crate::native::wake::WakeSignal>>,
) {
    let mut state = completed.0.lock().unwrap();
    let mut first = false;
    let mut pending: Vec<_> = std::mem::take(&mut state.pending).into_iter()
        .flat_map(|(image, pending)| pending.into_iter().map(move |pending| (image, pending))).collect();
    pending.sort_by_key(|(_, pending)| pending.order);
    for (image, pending) in pending {
        let previous = state.images.get(&image).cloned();
        let mut origins = previous.as_deref().cloned().unwrap_or_default();
        let mut incoming = pending.origins;
        for dependency in &incoming.images {
            if *dependency == image { incoming.complete = false; continue; }
            if let Some(input) = state.images.get(dependency) {
                incoming.complete &= input.complete;
                incoming.contributors.extend(input.contributors.iter().copied());
            } else { incoming.complete = false; }
        }
        incoming.bound();
        if pending.clears { origins = incoming; }
        else { origins.extend(&incoming); }
        let previous_links = previous.as_ref().map_or(0, |origins| origins.links());
        state.links = state.links.saturating_sub(previous_links);
        if state.links.saturating_add(origins.links()) > MAX_LINKS { origins = CaptureOrigins::default(); }
        state.links += origins.links();
        first |= state.images.insert(image, Arc::new(origins)).is_none();
    }
    state.pending_links = 0;
    drop(state);
    if first && external.is_none() && let Some(wake) = wake { wake.ring(); }
}

pub(super) fn extract(
    mut commands: Commands,
    world: Extract<&World>,
    cameras: Extract<Query<(Entity, RenderEntity, Option<&VisibleEntities>, Option<&RenderTarget>), (With<Camera>, bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>)>>,
    entities: Extract<Query<Entity, bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>>>,
    materials: Extract<Query<Entity, Or<(
        With<MeshMaterial3d<StandardMaterial>>,
        With<MeshMaterial3d<crate::native::description::shader::spatial::MeshMaterial>>,
        With<crate::native::description::shader::extent::SurfaceConsumer>,
    )>>>,
) {
    let mut targets: BTreeSet<_> = cameras.iter().filter_map(|(_, _, _, target)| match target {
        Some(RenderTarget::Image(image)) => Some(image.handle.id()),
        _ => None,
    }).collect();
    if let Some(history) = world.get_resource::<CompletedCaptures>().and_then(CompletedCaptures::snapshot) { targets.extend(history.into_keys()); }
    let mut material_images = BTreeMap::new();
    for entity in &materials {
        let mut images = BTreeSet::new();
        if let Some(material) = world.get::<MeshMaterial3d<StandardMaterial>>(entity)
            .and_then(|material| world.get_resource::<Assets<StandardMaterial>>()?.get(&material.0))
        {
            material.visit_dependencies(&mut |image| {
                if let Ok(image) = image.try_typed::<Image>() { images.insert(image); }
            });
        }
        if let Some(material) = world.get::<MeshMaterial3d<crate::native::description::shader::spatial::MeshMaterial>>(entity)
            .and_then(|material| world.get_resource::<Assets<crate::native::description::shader::spatial::MeshMaterial>>()?.get(&material.0))
        {
            material.visit_dependencies(&mut |image| {
                if let Ok(image) = image.try_typed::<Image>() { images.insert(image); }
            });
        }
        if let Some(consumer) = world.get::<crate::native::description::shader::extent::SurfaceConsumer>(entity) { images.insert(consumer.image.id()); }
        material_images.insert(entity, images);
    }
    for (camera, render, visible, _) in &cameras {
        let mut origins = CaptureOrigins::empty();
        origins.contributors.insert(camera);
        if let Some(visible) = visible {
            for entity in visible.entities.values().flatten().take(MAX_LINKS + 1) {
                origins.contributors.insert(*entity);
                if let Some(images) = material_images.get(entity) { origins.images.extend(images.iter().copied()); }
            }
        }
        origins.bound();
        commands.entity(render).insert(Inputs(origins));
    }
    let scopes = entities.iter().take(MAX_LINKS + 1).map(|entity| (entity, crate::native::capture::semantic_sources(&world, entity))).collect();
    commands.insert_resource(MaterialInputs(material_images, targets, scopes));
}

pub(super) fn surface(
    world: &World,
    camera: Entity,
    phase: &SortedRenderPhase<TransparentUi>,
) -> CaptureOrigins {
    let mut origins = world.get::<Inputs>(camera).map_or_else(CaptureOrigins::default, |inputs| inputs.0.clone());
    let Some(materials) = world.get_resource::<MaterialInputs>() else { origins.complete = false; return origins; };
    for item in phase.items.values().take(MAX_LINKS + 1) { origins.contributors.insert(item.entity.1.id()); }
    if let Some(nodes) = world.get_resource::<ExtractedUiNodes>() {
        for node in nodes.uinodes.iter().filter(|node| node.extracted_camera_entity == camera).take(MAX_LINKS + 1) {
            origins.contributors.insert(node.main_entity.id());
            if materials.1.contains(&node.image) { origins.images.insert(node.image); }
        }
    }
    origins.images.retain(|image| materials.1.contains(image));
    origins.normalize(&materials.2);
    origins
}

pub(super) fn record_view(
    view: ViewQuery<(Entity, &ExtractedCamera, &ViewTarget, Option<&RenderVisibleEntities>, Option<&Inputs>, &ViewUpscalingPipeline, Option<&UiCameraView>)>,
    nodes: Res<ExtractedUiNodes>,
    phases: Res<ViewSortedRenderPhases<TransparentUi>>,
    views: Query<&ExtractedView>,
    materials: Res<MaterialInputs>,
    mut cache: ResMut<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<BlitPipeline>>,
    blit: Res<BlitPipeline>,
    completed: Res<CompletedCaptures>,
) {
    let (entity, camera, target, visible, inputs, _, ui_view) = view.into_inner();
    let Some(NormalizedRenderTarget::Image(image)) = &camera.target else { return; };
    let CameraOutputMode::Write { blend_state, clear_color } = camera.output_mode else { return; };
    let blend_state = blend_state.or_else(|| (camera.sorted_camera_index_for_target > 0).then_some(BlendState::ALPHA_BLENDING));
    let Some(target_format) = target.out_texture_view_format() else { return; };
    let pipeline = pipelines.specialize(&mut cache, &blit, BlitPipelineKey { target_format, blend_state, samples: 1, source_space: target.compositing_space });
    if cache.get_render_pipeline(pipeline).is_none() || !target.needs_present() { return; }
    let mut origins = inputs.map_or_else(CaptureOrigins::default, |inputs| inputs.0.clone());
    if let Some(visible) = visible {
        for (_, main) in visible.classes.values().flat_map(|class| class.iter_visible()).take(MAX_LINKS + 1) {
            origins.contributors.insert(main.id());
            if let Some(images) = materials.0.get(&main.id()) { origins.images.extend(images.iter().copied()); }
        }
    }
    for node in nodes.uinodes.iter().filter(|node| node.extracted_camera_entity == entity).take(MAX_LINKS + 1) {
        origins.contributors.insert(node.main_entity.id());
        if materials.1.contains(&node.image) { origins.images.insert(node.image); }
    }
    if let Ok(view) = views.get(ui_view.map_or(entity, |view| view.0))
        && let Some(phase) = phases.get(&view.retained_view_entity)
    {
        for item in phase.items.values().take(MAX_LINKS + 1) { origins.contributors.insert(item.entity.1.id()); }
    }
    origins.images.retain(|image| materials.1.contains(image));
    origins.normalize(&materials.2);
    let full = camera.physical_viewport_size == camera.physical_target_size
        && camera.viewport.as_ref().is_none_or(|viewport| viewport.physical_position == UVec2::ZERO);
    let clears = !matches!(camera.clear_color, ClearColorConfig::None)
        && (!matches!(clear_color, ClearColorConfig::None) || full && blend_state.is_none());
    completed.queue(image.handle.id(), (camera.order, camera.sorted_camera_index_for_target), clears, origins);
}

#[cfg(test)]
mod tests;
