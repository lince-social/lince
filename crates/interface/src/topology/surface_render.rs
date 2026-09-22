use bevy::{
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
            DrawFunctions, SortedRenderPhase, ViewBinnedRenderPhases, ViewSortedRenderPhases,
        },
        render_resource::{
            LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            StoreOp, TextureFormat, TextureUsages,
        },
        renderer::RenderContext,
        sync_world::RenderEntity,
        texture::GpuImage,
        view::{
            ExtractedView, NoIndirectDrawing, RetainedViewEntity, ViewDepthTexture, ViewTarget,
            prepare_view_targets, visibility::RenderVisibleEntities,
        },
    },
    ui_render::{
        DrawUi, ExtractedUiItem, ExtractedUiNodes, NodeType, TransparentUi, UiCameraView,
        extract_ui_camera_view, prepare_uinodes,
    },
};
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Resource, Default)]
struct CaptureCache {
    pending: AtomicBool,
    fingerprints: HashMap<Entity, DefaultHasher>,
    dirty: HashSet<Entity>,
    skipped: HashSet<Entity>,
    rendered: Mutex<HashMap<Entity, u64>>,
}

#[derive(Component, Clone, ExtractComponent)]
pub struct SurfaceCapture(pub Handle<Image>);

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SurfacePass;

pub struct SurfaceRenderPlugin;

impl Plugin for SurfaceRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<SurfaceCapture>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<CaptureCache>()
            .add_systems(
                ExtractSchedule,
                extract_captures
                    .after(extract_cameras)
                    .before(extract_ui_camera_view),
            )
            .add_systems(
                Render,
                (prepare_captures, fingerprint_captures)
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
            ),
            With<SurfaceCapture>,
        >,
    >,
) {
    for (main_entity, entity, camera, target, graph, transform) in &captures {
        if !camera.is_active
            || camera
                .physical_target_size()
                .is_none_or(|size| size.min_element() == 0)
        {
            commands
                .entity(entity)
                .remove::<(ExtractedCamera, ExtractedView)>();
            continue;
        }
        let Some(viewport) = camera.physical_viewport_rect() else {
            continue;
        };
        formats.insert(entity, TextureFormat::Rgba8UnormSrgb);
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
                    hdr: false,
                    compositing_space: None,
                },
                ExtractedView {
                    retained_view_entity: RetainedViewEntity::new(main_entity.into(), None, 0),
                    clip_from_view: camera.clip_from_view(),
                    world_from_view: *transform,
                    clip_from_world: None,
                    target_format: TextureFormat::Rgba8UnormSrgb,
                    viewport: UVec4::from((viewport.min, viewport.size())),
                    color_grading: default(),
                    invert_culling: false,
                },
                NoIndirectDrawing,
            ))
            .remove::<RenderVisibleEntities>();
    }
}

fn floats(hash: &mut DefaultHasher, values: impl IntoIterator<Item = f32>) {
    for value in values {
        hash.write_u32(value.to_bits());
    }
}

fn rectangle(hash: &mut DefaultHasher, rect: Rect) {
    floats(hash, [rect.min.x, rect.min.y, rect.max.x, rect.max.y]);
}

fn fingerprint_captures(
    mut cache: ResMut<CaptureCache>,
    captures: Query<Entity, With<SurfaceCapture>>,
    nodes: Res<ExtractedUiNodes>,
    changed: Res<ExtractedAssets<GpuImage>>,
    images: Res<RenderAssets<GpuImage>>,
) {
    cache.pending.store(true, Ordering::Relaxed);
    cache.fingerprints.clear();
    cache.dirty.clear();
    cache
        .rendered
        .get_mut()
        .unwrap()
        .retain(|entity, _| captures.contains(*entity));
    for entity in &captures {
        cache.fingerprints.insert(entity, DefaultHasher::new());
    }
    for node in &nodes.uinodes {
        let entity = node.extracted_camera_entity;
        if !cache.fingerprints.contains_key(&entity) {
            continue;
        }
        if changed.modified.contains(&node.image)
            || changed.added.contains(&node.image)
            || images.get(node.image).is_some_and(|image| {
                image
                    .texture
                    .usage()
                    .intersects(TextureUsages::RENDER_ATTACHMENT | TextureUsages::STORAGE_BINDING)
            })
        {
            cache.dirty.insert(entity);
        }
        let hash = cache.fingerprints.get_mut(&entity).unwrap();
        node.main_entity.hash(hash);
        node.image.hash(hash);
        if let Some(image) = images.get(node.image) {
            image.texture.id().hash(hash);
        }
        floats(hash, [node.z_order]);
        floats(hash, node.transform.to_cols_array());
        node.clip.is_some().hash(hash);
        if let Some(clip) = node.clip {
            rectangle(hash, clip);
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
                        border_radius.top_left,
                        border_radius.top_right,
                        border_radius.bottom_right,
                        border_radius.bottom_left,
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
            ExtractedUiItem::Glyphs { range } => {
                hash.write_u8(1);
                range.len().hash(hash);
                for glyph in &nodes.glyphs[range.clone()] {
                    floats(hash, glyph.color.to_f32_array());
                    floats(hash, glyph.translation.to_array());
                    rectangle(hash, glyph.rect);
                }
            }
        }
    }
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
            ViewDepthTexture,
        )>();
        if let Some(view) = view {
            transparent.remove(&view.retained_view_entity);
            opaque.remove(&view.retained_view_entity);
            masked.remove(&view.retained_view_entity);
        }
    }
}

fn capture_hash(
    mut fingerprint: DefaultHasher,
    image: &GpuImage,
    phase: &SortedRenderPhase<TransparentUi>,
) -> u64 {
    image.texture.id().hash(&mut fingerprint);
    for item in phase.items.values() {
        item.pipeline.hash(&mut fingerprint);
    }
    fingerprint.finish()
}

fn reuse_captures(
    captures: Query<(Entity, &SurfaceCapture, &UiCameraView)>,
    views: Query<&ExtractedView>,
    mut phases: ResMut<ViewSortedRenderPhases<TransparentUi>>,
    images: Res<RenderAssets<GpuImage>>,
    mut cache: ResMut<CaptureCache>,
    draws: Res<DrawFunctions<TransparentUi>>,
    pipelines: Res<PipelineCache>,
) {
    cache.skipped.clear();
    let draw_ui = draws.read().get_id::<DrawUi>();
    for (entity, capture, ui_view) in &captures {
        if cache.dirty.contains(&entity) {
            continue;
        }
        let (Some(image), Ok(view)) = (images.get(&capture.0), views.get(ui_view.0)) else {
            continue;
        };
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        if !phase.items.values().all(|item| {
            Some(item.draw_function) == draw_ui
                && pipelines.get_render_pipeline(item.pipeline).is_some()
        }) {
            continue;
        }
        let fingerprint = capture_hash(
            cache.fingerprints.get(&entity).cloned().unwrap_or_default(),
            image,
            phase,
        );
        if cache.rendered.get_mut().unwrap().get(&entity) == Some(&fingerprint) {
            phase.clear();
            cache.skipped.insert(entity);
        }
    }
}

fn render_surface(
    world: &World,
    captures: Query<(Entity, &SurfaceCapture, &UiCameraView)>,
    views: Query<&ExtractedView>,
    phases: Res<ViewSortedRenderPhases<TransparentUi>>,
    images: Res<RenderAssets<GpuImage>>,
    cache: Res<CaptureCache>,
    draws: Res<DrawFunctions<TransparentUi>>,
    pipelines: Res<PipelineCache>,
    mut context: RenderContext,
) {
    if !cache.pending.swap(false, Ordering::Relaxed) {
        return;
    }
    for (entity, capture, ui_view) in &captures {
        if cache.skipped.contains(&entity) {
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
        let cacheable = phase.items.values().all(|item| {
            Some(item.draw_function) == draw_ui
                && pipelines.get_render_pipeline(item.pipeline).is_some()
        });
        let fingerprint = capture_hash(
            cache.fingerprints.get(&entity).cloned().unwrap_or_default(),
            image,
            phase,
        );
        if cacheable
            && !cache.dirty.contains(&entity)
            && cache.rendered.lock().unwrap().get(&entity) == Some(&fingerprint)
        {
            continue;
        }
        let attachments = [Some(RenderPassColorAttachment {
            view: &image.texture_view,
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
        if let Err(error) = phase.render(&mut pass, world, ui_view.0) {
            error!("Cannot render Sand UI: {error:?}");
            cache.rendered.lock().unwrap().remove(&entity);
        } else if cacheable {
            cache.rendered.lock().unwrap().insert(entity, fingerprint);
        } else {
            cache.rendered.lock().unwrap().remove(&entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        math::Affine2,
        ui_render::{ExtractedGlyph, ExtractedUiNode},
    };

    #[test]
    fn capture_fingerprint_tracks_text_clipping_and_changed_images() {
        let mut app = App::new();
        app.init_resource::<CaptureCache>()
            .init_resource::<ExtractedUiNodes>()
            .init_resource::<ExtractedAssets<GpuImage>>()
            .init_resource::<RenderAssets<GpuImage>>()
            .add_systems(Update, fingerprint_captures);
        let camera = app
            .world_mut()
            .spawn(SurfaceCapture(Handle::default()))
            .id();
        let source = app.world_mut().spawn_empty().id();
        let mut nodes = app.world_mut().resource_mut::<ExtractedUiNodes>();
        nodes.glyphs.push(ExtractedGlyph {
            color: LinearRgba::WHITE,
            translation: Vec2::ZERO,
            rect: Rect::from_corners(Vec2::ZERO, Vec2::splat(16.0)),
        });
        nodes.uinodes.push(ExtractedUiNode {
            z_order: 1.0,
            image: AssetId::default(),
            clip: None,
            extracted_camera_entity: camera,
            item: ExtractedUiItem::Glyphs { range: 0..1 },
            main_entity: source.into(),
            render_entity: source,
            transform: Affine2::IDENTITY,
        });
        app.update();
        let fingerprint =
            |app: &App| app.world().resource::<CaptureCache>().fingerprints[&camera].finish();
        let first = fingerprint(&app);
        app.update();
        assert_eq!(fingerprint(&app), first);
        app.world_mut().resource_mut::<ExtractedUiNodes>().glyphs[0]
            .rect
            .max
            .x = 12.0;
        app.update();
        let changed_text = fingerprint(&app);
        assert_ne!(changed_text, first);
        app.world_mut().resource_mut::<ExtractedUiNodes>().uinodes[0].clip =
            Some(Rect::from_corners(Vec2::ZERO, Vec2::splat(8.0)));
        app.update();
        assert_ne!(fingerprint(&app), changed_text);
        assert!(app.world().resource::<CaptureCache>().dirty.is_empty());
        app.world_mut()
            .resource_mut::<ExtractedAssets<GpuImage>>()
            .modified
            .insert(AssetId::default());
        app.update();
        assert!(
            app.world()
                .resource::<CaptureCache>()
                .dirty
                .contains(&camera)
        );
        app.world_mut().despawn(camera);
        app.update();
        assert!(
            app.world()
                .resource::<CaptureCache>()
                .fingerprints
                .is_empty()
        );
    }
}
