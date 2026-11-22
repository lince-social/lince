// non-lean-yet
// Scope: type Fingerprint, mod origins, mod order, struct CaptureCache.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 113, 197, 607,
//   644.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   290, 294, 735, 743, 752.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (64, 257, 269, 295, 478, 574, 582, 631, 647, 649, 686, 689, 763, 771).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 158, 504.

use bevy::{
    asset::AssetId,
    camera::{CameraMainTextureUsages, RenderTarget},
    core_pipeline::core_2d::{AlphaMask2d, Opaque2d, Transparent2d},
    ecs::schedule::ScheduleLabel,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        camera::{
            CameraMainPassTextureFormats, CameraRenderGraph, ExtractedCamera, extract_cameras,
        },
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_asset::{ExtractedAssets, RenderAssets},
        render_phase::{
            DrawFunctionId, DrawFunctions, SortedRenderPhase, ViewBinnedRenderPhases,
            ViewSortedRenderPhases,
        },
        render_resource::{
            LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            StoreOp, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
        renderer::RenderContext,
        sync_world::{MainEntity, MainEntityHashMap, RenderEntity},
        texture::GpuImage,
        view::{
            ExtractedView, NoIndirectDrawing, ResolvedCompositingSpace, RetainedViewEntity, ViewDepthStencilTexture, ViewTarget,
            prepare_view_targets, visibility::RenderVisibleEntities,
        },
    },
    ui_render::{
        DrawUi, ExtractedUiItem, ExtractedUiNode, ExtractedUiNodes, NodeType, TransparentUi, UiCameraView,
        extract_ui_camera_view, prepare_uinodes,
    },
};
use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

type Fingerprint = <bevy::platform::hash::FixedHasher as BuildHasher>::Hasher;

mod origins;
mod order;
mod retained_draws;
pub(crate) use origins::{CaptureOrigins, CompletedCaptures};
pub use origins::RetainCaptureTopology;
pub use retained_draws::{UiMaterialVersions, retain_ui_material};
use retained_draws::RetainedDraws;

pub fn independent_output_images(
    world: &World,
    targets: impl Iterator<Item = AssetId<Image>>,
    changed: &std::collections::BTreeSet<Entity>,
) -> std::collections::BTreeSet<AssetId<Image>> {
    world.get_resource::<CompletedCaptures>().map_or_else(Default::default, |completed| {
        completed.independent_images(targets, changed)
    })
}

#[derive(Resource, Default)]
struct CaptureCache {
    external: bool,
    pending: AtomicBool,
    fingerprints: HashMap<Entity, Fingerprint>,
    ui_owners: Option<Vec<MainEntity>>,
    ui_geometry: MainEntityHashMap<u64>,
    dirty: HashSet<Entity>,
    blocked: HashSet<Entity>,
    skipped: HashSet<Entity>,
    rendered: Mutex<HashMap<Entity, u64>>,
    targets: Mutex<HashMap<Entity, Texture>>,
}

pub(super) fn staging_textures(world: &World) -> Vec<Texture> {
    world
        .get_resource::<CaptureCache>()
        .map(|cache| cache.targets.lock().unwrap().values().cloned().collect())
        .unwrap_or_default()
}

#[derive(Component, Clone, ExtractComponent)]

#[extract_app(bevy::render::RenderApp)]
pub struct SurfaceCapture(pub Handle<Image>);

#[derive(Component, Clone, ExtractComponent)]

#[extract_app(bevy::render::RenderApp)]
pub(crate) struct CaptureReady(pub bool);

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SurfacePass;

pub struct SurfaceRenderPlugin;

pub struct ExternalCaptureCache {
    cache: CaptureCache,
    completed: CompletedCaptures,
    aliases: Option<crate::native::capture::render::aliases::ExternalAliases>,
    prepared: crate::native::capture::render::aliases::PreparedAliases,
}

pub fn begin_external_capture(world: &mut World) -> ExternalCaptureCache {
    if let Some(mut ui) = world.get_resource_mut::<origins::UiOrigins>() {
        ui.invalidate();
    }
    let cache = world.remove_resource::<CaptureCache>().unwrap_or_default();
    let completed = world.remove_resource::<CompletedCaptures>().unwrap_or_default();
    let aliases = world.remove_resource::<crate::native::capture::render::aliases::ExternalAliases>();
    let prepared = world.remove_resource::<crate::native::capture::render::aliases::PreparedAliases>().unwrap_or_default();
    world.insert_resource(CaptureCache { external: true, ..Default::default() });
    world.insert_resource(CompletedCaptures::default());
    world.insert_resource(crate::native::capture::render::aliases::PreparedAliases::default());
    ExternalCaptureCache { cache, completed, aliases, prepared }
}

pub fn end_external_capture(world: &mut World, mut saved: ExternalCaptureCache) {
    // The shared UI extractor may have changed groups while this cache was away.
    saved.cache.ui_owners = None;
    saved.cache.ui_geometry.clear();
    if let Some(mut ui) = world.get_resource_mut::<origins::UiOrigins>() {
        ui.invalidate();
    }
    world.insert_resource(saved.cache);
    world.insert_resource(saved.completed);
    if let Some(aliases) = saved.aliases { world.insert_resource(aliases); }
    else { world.remove_resource::<crate::native::capture::render::aliases::ExternalAliases>(); }
    world.insert_resource(saved.prepared);
}

impl Plugin for SurfaceRenderPlugin {
    fn build(&self, app: &mut App) {
        super::stacking::install(app);
        let completed = CompletedCaptures::default();
        app.insert_resource(completed.clone()).add_plugins((
            ExtractComponentPlugin::<SurfaceCapture>::default(),
            ExtractComponentPlugin::<CaptureReady>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        super::appearance::install_room_order(render_app);
        render_app
            .insert_resource(completed)
            .init_resource::<CaptureCache>()
            .init_resource::<RetainedDraws>()
            .init_resource::<origins::UiOrigins>()
            .init_resource::<crate::native::capture::render::aliases::PreparedAliases>()
            .add_systems(ExtractSchedule, origins::extract.after(extract_cameras))
            .add_systems(Render, crate::native::capture::render::aliases::prepare.in_set(RenderSystems::Prepare).ambiguous_with_all())
            .add_systems(Render, origins::prepare_ui.in_set(RenderSystems::PrepareBindGroups).before(prepare_uinodes))
            .add_systems(Render, origins::begin.after(RenderSystems::Prepare).before(RenderSystems::Render))
            .add_systems(Render, origins::finish.after(RenderSystems::Render).before(RenderSystems::Cleanup))
            .add_systems(bevy::core_pipeline::schedule::Core2d, origins::record_view.after(bevy::core_pipeline::upscaling::upscaling))
            .add_systems(bevy::core_pipeline::schedule::Core3d, origins::record_view.after(bevy::core_pipeline::upscaling::upscaling))
            .add_systems(
                ExtractSchedule,
                extract_captures
                    .after(extract_cameras)
                    .before(extract_ui_camera_view),
            )
            .add_systems(
                Render,
                (prepare_captures, fingerprint_captures, retained_draws::clear)
                    .in_set(RenderSystems::PrepareViews)
                    .before(prepare_view_targets),
            )
            .add_systems(
                Render,
                reuse_captures
                    .in_set(RenderSystems::PrepareBindGroups)
                    .before(prepare_uinodes),
            )
            .add_systems(SurfacePass, render_surface);
    }
}

fn extract_captures(
    mut commands: Commands,
    mut formats: ResMut<CameraMainPassTextureFormats>,
    captures: Extract<
        Query<
            (
                Entity,
                RenderEntity,
                &Camera,
                &RenderTarget,
                &CameraRenderGraph,
                &GlobalTransform,
                Option<&bevy::camera::Hdr>,
                Option<&bevy::camera::CompositingSpace>,
            ),
            With<SurfaceCapture>,
        >,
    >,
) {
    for (main_entity, entity, camera, target, graph, transform, hdr, compositing_space) in &captures {
        if !camera.is_active
            || camera
                .physical_target_size()
                .is_none_or(|size| size.min_element() == 0)
        {
            commands
                .entity(entity)
                .remove::<(ExtractedCamera, ExtractedView, ResolvedCompositingSpace)>();
            continue;
        }
        let Some(viewport) = camera.physical_viewport_rect() else {
            continue;
        };
        let format = if hdr.is_some() { TextureFormat::Rgba16Float } else { TextureFormat::Rgba8UnormSrgb };
        formats.insert(entity, format);
        commands
            .entity(entity)
            .insert((
                ExtractedCamera {
                    target: target.normalize(None),
                    physical_viewport_size: camera.physical_viewport_size(),
                    physical_target_size: camera.physical_target_size(),
                    viewport: camera.viewport.clone(),
                    schedule: graph.0,
                    order: camera.order,
                    output_mode: camera.output_mode,
                    msaa_writeback: camera.msaa_writeback,
                    clear_color: camera.clear_color,
                    sorted_camera_index_for_target: 0,
                    exposure: 1.0,
                    hdr: hdr.is_some(),
                },
                ExtractedView {
                    retained_view_entity: RetainedViewEntity::new(main_entity.into(), None, 0),
                    clip_from_view: camera.clip_from_view(),
                    world_from_view: *transform,
                    clip_from_world: None,
                    target_format: format,
                    viewport: UVec4::from((viewport.min, viewport.size())),
                    color_grading: default(),
                    invert_culling: false,
                },
                NoIndirectDrawing,
                ResolvedCompositingSpace(compositing_space.copied()),
            ))
            .remove::<RenderVisibleEntities>();
    }
}

fn floats(hash: &mut Fingerprint, values: impl IntoIterator<Item = f32>) {
    for value in values {
        hash.write_u32(value.to_bits());
    }
}

fn rectangle(hash: &mut Fingerprint, rect: Rect) {
    floats(hash, [rect.min.x, rect.min.y, rect.max.x, rect.max.y]);
}

fn hash_ui_geometry(hash: &mut Fingerprint, node: &ExtractedUiNode) {
    floats(hash, [node.z_order]);
    floats(hash, node.transform.to_cols_array());
    match node.clip.as_ref() {
        None => hash.write_u8(0),
        Some(CalculatedClip::FullyClipped) => hash.write_u8(1),
        Some(CalculatedClip::Rects(regions)) => {
            hash.write_u8(2);
            regions.len().hash(hash);
            for region in regions {
                rectangle(hash, region.rect);
                floats(hash, region.world_to_clip_local.to_cols_array());
            }
        }
    }
    match &node.item {
        ExtractedUiItem::Node {
            color,
            rect,
            atlas_scaling,
            flip_x,
            flip_y,
            border_radius,
            border,
            node_type,
        } => {
            hash.write_u8(0);
            floats(hash, color.to_f32_array());
            rectangle(hash, *rect);
            atlas_scaling.is_some().hash(hash);
            if let Some(scale) = atlas_scaling {
                floats(hash, scale.to_array());
            }
            flip_x.hash(hash);
            flip_y.hash(hash);
            floats(
                hash,
                [
                    border_radius.top_left.x,
                    border_radius.top_left.y,
                    border_radius.top_right.x,
                    border_radius.top_right.y,
                    border_radius.bottom_right.x,
                    border_radius.bottom_right.y,
                    border_radius.bottom_left.x,
                    border_radius.bottom_left.y,
                ],
            );
            floats(hash, border.min_inset.to_array());
            floats(hash, border.max_inset.to_array());
            match node_type {
                NodeType::Rect => hash.write_u8(0),
                NodeType::Inverted => hash.write_u8(1),
                NodeType::Border(flags) => {
                    hash.write_u8(2);
                    hash.write_u32(*flags);
                }
            }
        }
        ExtractedUiItem::Glyphs { glyphs } => {
            hash.write_u8(1);
            glyphs.len().hash(hash);
            for glyph in glyphs {
                floats(hash, glyph.color.to_f32_array());
                floats(hash, glyph.translation.to_array());
                rectangle(hash, glyph.rect);
            }
        }
    }
}

fn fingerprint_captures(
    mut cache: ResMut<CaptureCache>,
    captures: Query<(Entity, Option<&UiCameraView>), With<SurfaceCapture>>,
    views: Query<&ExtractedView>,
    nodes: Res<ExtractedUiNodes>,
    changed: Res<ExtractedAssets<GpuImage>>,
    images: Res<RenderAssets<GpuImage>>,
    completed: Option<Res<CompletedCaptures>>,
    materials: Option<Res<origins::MaterialInputs>>,
) {
    let _timing = crate::host::performance::Span::new(
        crate::host::performance::Stage::NativeSurfaceFingerprint,
    );
    let dormant = if cache.external {
        HashMap::new()
    } else {
        completed
            .as_ref()
            .zip(materials.as_ref())
            .map(|(completed, materials)| completed.dormant_versions(materials))
            .unwrap_or_default()
    };
    cache.targets.get_mut().unwrap().retain(|entity, _| {
        captures
            .get(*entity)
            .is_ok_and(|(_, camera)| camera.is_some_and(|camera| views.contains(camera.0)))
    });
    cache.pending.store(true, Ordering::Relaxed);
    let previous_cameras = cache.fingerprints.len();
    cache.fingerprints.retain(|entity, _| captures.contains(*entity));
    let mut owners_changed = cache.ui_owners.is_none()
        || !nodes.changed.is_empty()
        || cache.fingerprints.len() != previous_cameras;
    cache.dirty.clear();
    cache.blocked.clear();
    cache
        .rendered
        .get_mut()
        .unwrap()
        .retain(|entity, _| captures.contains(*entity));
    for (entity, camera) in &captures {
        let mut hash = bevy::platform::hash::FixedHasher.build_hasher();
        if let Some(view) = camera.and_then(|camera| views.get(camera.0).ok()) {
            floats(&mut hash, view.clip_from_view.to_cols_array());
            floats(&mut hash, view.world_from_view.to_matrix().to_cols_array());
            view.viewport.hash(&mut hash);
        }
        owners_changed |= cache.fingerprints.insert(entity, hash).is_none();
    }
    let CaptureCache {
        fingerprints,
        ui_owners,
        ui_geometry,
        dirty,
        blocked,
        ..
    } = &mut *cache;
    // The extractor marks owners whose groups were replaced or removed.
    for owner in &nodes.changed {
        ui_geometry.remove(owner);
    }
    let mut dormant_references = 0;
    let mut dynamic_references = 0;
    let owners = ui_owners.get_or_insert_with(Vec::new);
    if owners_changed {
        owners.clear();
        owners.extend(nodes.uinodes.iter().filter_map(|(owner, (camera, _))| {
            fingerprints.contains_key(camera).then_some(*owner)
        }));
        owners.sort_unstable_by_key(|owner| owner.id().to_bits());
    }
    for owner in &*owners {
        let (entity, group) = &nodes.uinodes[owner];
        let hash = fingerprints.get_mut(entity).unwrap();
        let geometry = ui_geometry.entry(*owner).or_insert_with(|| {
            let mut hash = bevy::platform::hash::FixedHasher.build_hasher();
            group.len().hash(&mut hash);
            for node in group.values() {
                hash_ui_geometry(&mut hash, node);
            }
            hash.finish()
        });
        owner.hash(hash);
        geometry.hash(hash);
        // A static node can sample a texture that changes on every render.
        for node in group.values() {
            let image = images.get(node.image);
            if node.image != AssetId::default() && image.is_none() {
                blocked.insert(*entity);
            }
            if changed.modified.contains(&node.image) || changed.added.contains(&node.image) {
                dirty.insert(*entity);
            }
            if let Some(image) = image {
                let usage = image.texture.usage();
                if usage.intersects(TextureUsages::RENDER_ATTACHMENT | TextureUsages::STORAGE_BINDING) {
                    if !usage.contains(TextureUsages::STORAGE_BINDING)
                        && let Some(version) = dormant.get(&node.image)
                    {
                        version.hash(hash);
                        dormant_references += 1;
                    } else {
                        dirty.insert(*entity);
                        dynamic_references += 1;
                    }
                }
            }
            node.image.hash(hash);
            if let Some(image) = image {
                image.texture.id().hash(hash);
            }
        }
    }
    crate::host::performance::add(
        crate::host::performance::Count::SurfaceDormantImageReferences,
        dormant_references,
    );
    crate::host::performance::add(
        crate::host::performance::Count::SurfaceDynamicImageReferences,
        dynamic_references,
    );
}

fn prepare_captures(
    mut commands: Commands,
    captures: Query<(Entity, Option<&ExtractedView>), With<SurfaceCapture>>,
    mut transparent: ResMut<ViewSortedRenderPhases<Transparent2d>>,
    mut opaque: ResMut<ViewBinnedRenderPhases<Opaque2d>>,
    mut masked: ResMut<ViewBinnedRenderPhases<AlphaMask2d>>,
) {
    for (entity, view) in &captures {
        commands.entity(entity).remove::<(
            Camera2d,
            CameraMainTextureUsages,
            ViewTarget,
            ViewDepthStencilTexture,
        )>();
        if let Some(view) = view {
            transparent.remove(&view.retained_view_entity);
            opaque.remove(&view.retained_view_entity);
            masked.remove(&view.retained_view_entity);
        }
    }
}

fn capture_hash(
    mut fingerprint: Fingerprint,
    image: &GpuImage,
    phase: &SortedRenderPhase<TransparentUi>,
    draw_ui: Option<DrawFunctionId>,
    retained: &RetainedDraws,
) -> Option<u64> {
    image.texture.id().hash(&mut fingerprint);
    for item in phase.items.values() {
        item.pipeline.hash(&mut fingerprint);
        if Some(item.draw_function) != draw_ui {
            retained.hash(item, &mut fingerprint)?;
        }
    }
    Some(fingerprint.finish())
}

fn reuse_captures(
    captures: Query<(
        Entity,
        &SurfaceCapture,
        &UiCameraView,
        Option<&CaptureReady>,
    )>,
    views: Query<&ExtractedView>,
    mut phases: ResMut<ViewSortedRenderPhases<TransparentUi>>,
    images: Res<RenderAssets<GpuImage>>,
    mut cache: ResMut<CaptureCache>,
    draws: Res<DrawFunctions<TransparentUi>>,
    pipelines: Res<PipelineCache>,
    retained: Res<RetainedDraws>,
) {
    cache.skipped.clear();
    let draw_ui = draws.read().get_id::<DrawUi>();
    for (entity, capture, ui_view, ready) in &captures {
        if ready.is_some_and(|ready| !ready.0) || cache.blocked.contains(&entity) {
            cache.skipped.insert(entity);
            continue;
        }
        if cache.dirty.contains(&entity) {
            continue;
        }
        let (Some(image), Ok(view)) = (images.get(&capture.0), views.get(ui_view.0)) else {
            continue;
        };
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        if !phase
            .items
            .values()
            .all(|item| pipelines.get_render_pipeline(item.pipeline).is_some())
        {
            continue;
        }
        let Some(fingerprint) = capture_hash(
            cache
                .fingerprints
                .get(&entity)
                .cloned()
                .unwrap_or_else(|| bevy::platform::hash::FixedHasher.build_hasher()),
            image,
            phase,
            draw_ui,
            &retained,
        ) else {
            continue;
        };
        if cache.rendered.get_mut().unwrap().get(&entity) == Some(&fingerprint) {
            phase.clear();
            cache.skipped.insert(entity);
            crate::host::performance::count(
                crate::host::performance::Count::SurfaceCapturePassesReused,
            );
        }
    }
}

fn render_surface(
    world: &World,
    captures: Query<(
        Entity,
        &SurfaceCapture,
        &UiCameraView,
        Option<&CaptureReady>,
    )>,
    views: Query<&ExtractedView>,
    phases: Res<ViewSortedRenderPhases<TransparentUi>>,
    images: Res<RenderAssets<GpuImage>>,
    cache: Res<CaptureCache>,
    draws: Res<DrawFunctions<TransparentUi>>,
    pipelines: Res<PipelineCache>,
    retained: Res<RetainedDraws>,
    completed: Res<CompletedCaptures>,
    wake: Option<Res<crate::native::wake::WakeSignal>>,
    mut context: RenderContext,
) {
    if !cache.pending.swap(false, Ordering::Relaxed) {
        return;
    }
    let mut captured_origins = std::collections::BTreeMap::new();
    let ordered = if cache.external {
        let mut producers: std::collections::BTreeMap<_, std::collections::BTreeSet<_>> = std::collections::BTreeMap::new();
        for (entity, capture, ui_view, _) in &captures {
            producers.entry(capture.0.id()).or_default().insert(entity);
            if let Ok(view) = views.get(ui_view.0) && let Some(phase) = phases.get(&view.retained_view_entity) {
                captured_origins.insert(entity, origins::surface(world, entity, phase));
            }
        }
        if let Some(aliases) = world.get_resource::<crate::native::capture::render::aliases::ExternalAliases>() {
            for (source, aliases) in &aliases.0 {
                let writers = producers.get(source).cloned().unwrap_or_default();
                for alias in aliases { producers.entry(*alias).or_default().extend(writers.iter().copied()); }
            }
        }
        let public = completed.snapshot().unwrap_or_default();
        let dependencies = captures.iter().map(|(entity, _, _, _)| {
            let dependencies = captured_origins.get(&entity).into_iter().flat_map(|origins| origins.images.iter())
                .filter(|image| public.get(*image).is_none_or(|origins| !origins.complete))
                .flat_map(|image| producers.get(image).into_iter().flatten().copied()).collect();
            (entity, dependencies)
        }).collect();
        order::sources_first(dependencies)
    } else { captures.iter().map(|(entity, _, _, _)| entity).collect() };
    let first_order = captures.iter().filter_map(|(entity, _, _, _)| world.get::<ExtractedCamera>(entity).map(|camera| camera.order)).min().unwrap_or(0);
    for (index, entity) in ordered.into_iter().enumerate() {
        let Ok((_, capture, ui_view, ready)) = captures.get(entity) else { continue; };
        if ready.is_some_and(|ready| !ready.0)
            || cache.skipped.contains(&entity)
            || cache.blocked.contains(&entity)
        {
            continue;
        }
        let (Some(image), Ok(extracted_view)) = (images.get(&capture.0), views.get(ui_view.0))
        else {
            continue;
        };
        let Some(phase) = phases.get(&extracted_view.retained_view_entity) else {
            continue;
        };
        let draw_ui = draws.read().get_id::<DrawUi>();
        if ready.is_some() && phase.items.is_empty() {
            continue;
        }
        if !phase
            .items
            .values()
            .all(|item| pipelines.get_render_pipeline(item.pipeline).is_some())
        {
            if let Some(wake) = &wake && !cache.external {
                wake.ring();
            }
            continue;
        }
        let fingerprint = capture_hash(
            cache
                .fingerprints
                .get(&entity)
                .cloned()
                .unwrap_or_else(|| bevy::platform::hash::FixedHasher.build_hasher()),
            image,
            phase,
            draw_ui,
            &retained,
        );
        if !cache.dirty.contains(&entity)
            && fingerprint.is_some_and(|fingerprint| {
                cache.rendered.lock().unwrap().get(&entity) == Some(&fingerprint)
            })
        {
            crate::host::performance::count(
                crate::host::performance::Count::SurfaceCapturePassesReused,
            );
            continue;
        }
        let target = {
            let mut targets = cache.targets.lock().unwrap();
            let target = targets.entry(entity).or_insert_with(|| {
                context.render_device().create_texture(&TextureDescriptor {
                    label: Some("sand_ui_staging"),
                    size: image.texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: image.texture.format(),
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            });
            if target.size() != image.texture.size() || target.format() != image.texture.format() {
                *target = context.render_device().create_texture(&TextureDescriptor {
                    label: Some("sand_ui_staging"),
                    size: image.texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: image.texture.format(),
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
            }
            target.clone()
        };
        let target_view = target.create_view(&default());
        let attachments = [Some(RenderPassColorAttachment {
            view: &target_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Default::default()),
                store: StoreOp::Store,
            },
        })];
        let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("sand_ui_capture"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let result = phase.render(&mut pass, world, ui_view.0);
        drop(pass);
        if let Err(error) = result {
            error!("Cannot render Sand UI: {error:?}");
            cache.rendered.lock().unwrap().remove(&entity);
        } else {
            context.command_encoder().copy_texture_to_texture(
                target.as_image_copy(),
                image.texture.as_image_copy(),
                image.texture.size(),
            );
            crate::host::performance::count(
                crate::host::performance::Count::SurfaceCapturePassesEncoded,
            );
            let order = if cache.external { (first_order, index) }
                else { world.get::<ExtractedCamera>(entity).map_or((0, 0), |camera| (camera.order, camera.sorted_camera_index_for_target)) };
            let origins = captured_origins.remove(&entity).unwrap_or_else(|| origins::surface(world, entity, phase));
            completed.queue(capture.0.id(), order, true, origins.clone());
            crate::native::capture::render::aliases::copy(world, &mut context, capture.0.id(), &origins, order);
            if let Some(fingerprint) = fingerprint {
                cache.rendered.lock().unwrap().insert(entity, fingerprint);
            } else {
                cache.rendered.lock().unwrap().remove(&entity);
            }
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::{
        math::Affine2,
        ui_render::{ExtractedGlyph, ExtractedUiNode},
    };

    fn insert_text_node(world: &mut World, source: Entity, camera: Entity) {
        let mut nodes = world.resource_mut::<ExtractedUiNodes>();
        nodes.changed.insert(source.into());
        let group = &mut nodes.uinodes.entry(source.into())
            .or_insert_with(|| (camera, Default::default())).1;
        group.insert(source, ExtractedUiNode {
            z_order: 1.0,
            image: AssetId::default(),
            clip: None,
            item: ExtractedUiItem::Glyphs {
                glyphs: vec![ExtractedGlyph {
                    color: LinearRgba::WHITE,
                    translation: Vec2::ZERO,
                    rect: Rect::from_corners(Vec2::ZERO, Vec2::splat(16.0)),
                }],
            },
            transform: Affine2::IDENTITY,
        });
    }

    fn node_mut(world: &mut World, source: Entity) -> Mut<'_, ExtractedUiNode> {
        let mut nodes = world.resource_mut::<ExtractedUiNodes>();
        nodes.changed.insert(source.into());
        nodes.map_unchanged(|nodes| {
            nodes
                .uinodes
                .get_mut(&MainEntity::from(source))
                .unwrap()
                .1
                .get_mut(&source)
                .unwrap()
        })
    }

    fn glyph_mut(world: &mut World, source: Entity) -> Mut<'_, ExtractedGlyph> {
        node_mut(world, source).map_unchanged(|node| {
            let ExtractedUiItem::Glyphs { glyphs } = &mut node.item else {
                unreachable!()
            };
            &mut glyphs[0]
        })
    }

    #[cfg_attr(test, test)]
    fn capture_fingerprint_tracks_camera_text_clipping_and_changed_images() {
        let mut app = App::new();
        app.init_resource::<CompletedCaptures>()
            .init_resource::<CaptureCache>()
            .init_resource::<ExtractedUiNodes>()
            .init_resource::<ExtractedAssets<GpuImage>>()
            .init_resource::<RenderAssets<GpuImage>>()
            .add_systems(
                Update,
                (
                    fingerprint_captures,
                    |mut nodes: ResMut<ExtractedUiNodes>| nodes.changed.clear(),
                )
                    .chain(),
            );
        let camera = app
            .world_mut()
            .spawn(SurfaceCapture(Handle::default()))
            .id();
        let view = app
            .world_mut()
            .spawn(ExtractedView {
                retained_view_entity: RetainedViewEntity::new(camera.into(), None, 1),
                clip_from_view: Mat4::IDENTITY,
                world_from_view: GlobalTransform::IDENTITY,
                clip_from_world: None,
                target_format: TextureFormat::Rgba8UnormSrgb,
                viewport: UVec4::new(0, 0, 320, 100),
                color_grading: default(),
                invert_culling: false,
            })
            .id();
        app.world_mut()
            .entity_mut(camera)
            .insert(UiCameraView(view));
        let source = app.world_mut().spawn_empty().id();
        insert_text_node(app.world_mut(), source, camera);
        app.update();
        let fingerprint =
            |app: &App| app.world().resource::<CaptureCache>().fingerprints[&camera].finish();
        let first = fingerprint(&app);
        app.update();
        assert_eq!(fingerprint(&app), first);
        let other = app.world_mut().spawn((SurfaceCapture(Handle::default()), UiCameraView(view))).id();
        let other_source = app.world_mut().spawn_empty().id();
        insert_text_node(app.world_mut(), other_source, other);
        app.update();
        let other_fingerprint = app.world().resource::<CaptureCache>().fingerprints[&other].finish();
        for (color, translation) in [(LinearRgba::BLACK, Vec2::ZERO), (LinearRgba::WHITE, Vec2::new(0.5, 1.5))] {
            {
                let mut glyph = glyph_mut(app.world_mut(), source);
                glyph.color = color;
                glyph.translation = translation;
            }
            app.update();
            assert_ne!(fingerprint(&app), first);
            assert_eq!(app.world().resource::<CaptureCache>().fingerprints[&other].finish(), other_fingerprint);
        }
        {
            let mut glyph = glyph_mut(app.world_mut(), source);
            glyph.color = LinearRgba::WHITE;
            glyph.translation = Vec2::ZERO;
        }
        app.update();
        assert_eq!(fingerprint(&app), first);
        app.world_mut()
            .get_mut::<ExtractedView>(view)
            .unwrap()
            .viewport
            .w = 800;
        app.update();
        let resized = fingerprint(&app);
        assert_ne!(resized, first);
        app.world_mut()
            .get_mut::<ExtractedView>(view)
            .unwrap()
            .clip_from_view = Mat4::from_scale(Vec3::new(1.0, 0.125, 1.0));
        app.update();
        let projected = fingerprint(&app);
        assert_ne!(projected, resized);
        glyph_mut(app.world_mut(), source).rect.max.x = 12.0;
        app.update();
        let changed_text = fingerprint(&app);
        assert_ne!(changed_text, projected);
        let mut clip = CalculatedClip::default();
        clip.push_rect(Rect::from_corners(Vec2::ZERO, Vec2::splat(8.0)), Affine2::IDENTITY);
        node_mut(app.world_mut(), source).clip = Some(clip);
        app.update();
        assert_ne!(fingerprint(&app), changed_text);
        assert!(app.world().resource::<CaptureCache>().dirty.is_empty());
        let before_image = fingerprint(&app);
        let source_owner = MainEntity::from(source);
        let geometry = app.world().resource::<CaptureCache>().ui_geometry[&source_owner];
        app.world_mut()
            .resource_mut::<ExtractedAssets<GpuImage>>()
            .modified
            .insert(AssetId::default());
        app.update();
        assert_eq!(fingerprint(&app), before_image);
        assert_eq!(
            app.world().resource::<CaptureCache>().ui_geometry[&source_owner],
            geometry
        );
        assert!(
            app.world()
                .resource::<CaptureCache>()
                .dirty
                .contains(&camera)
        );
        app.init_resource::<Assets<Image>>();
        let missing = app.world().resource::<Assets<Image>>().reserve_handle();
        node_mut(app.world_mut(), source).image = missing.id();
        app.update();
        assert!(
            app.world()
                .resource::<CaptureCache>()
                .blocked
                .contains(&camera)
        );
        let other_owner = MainEntity::from(other_source);
        {
            let mut nodes = app.world_mut().resource_mut::<ExtractedUiNodes>();
            nodes.uinodes.remove(&other_owner);
            nodes.changed.insert(other_owner);
        }
        app.update();
        assert!(
            !app.world().resource::<CaptureCache>().ui_geometry.contains_key(&other_owner)
        );
        app.world_mut().despawn(camera);
        app.world_mut().despawn(other);
        app.update();
        assert!(
            app.world()
                .resource::<CaptureCache>()
                .fingerprints
                .is_empty()
        );
    }
    crate::native::laboratory_cases! { capture_fingerprint_tracks_camera_text_clipping_and_changed_images, }

}
