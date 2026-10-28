use bevy::{
    asset::AssetId,
    core_pipeline::prepass::ViewPrepassTextures,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_asset::RenderAssets,
        render_resource::{Texture, TextureId},
        renderer::RenderQueue,
        texture::GpuImage,
        view::{ViewDepthTexture, ViewTarget},
    },
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

const LIMIT: usize = 16_384;
const CACHE_FRAMES: u64 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub current_bytes: u64,
    pub held_bytes: u64,
    pub cached_estimate_bytes: u64,
    pub inflight_bytes: u64,
    pub peak_bytes: u64,
    pub textures: usize,
    pub lower_bound: bool,
}

impl Snapshot {
    pub fn retained_bytes(self) -> u64 {
        self.held_bytes
            .saturating_add(self.cached_estimate_bytes)
            .saturating_add(self.inflight_bytes)
    }

    pub fn total_bytes(self) -> u64 {
        self.current_bytes.saturating_add(self.retained_bytes())
    }
}

struct Allocation {
    bytes: u64,
    seen: u64,
    current: bool,
    held: bool,
    cached: bool,
    retired: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
struct Ledger {
    frame: u64,
    allocations: HashMap<TextureId, Allocation>,
    snapshot: Snapshot,
}

impl Ledger {
    fn begin(&mut self) {
        self.frame = self.frame.saturating_add(1);
        for allocation in self.allocations.values_mut() {
            allocation.current = false;
            allocation.held = false;
        }
    }

    fn observe(&mut self, id: TextureId, bytes: u64, current: bool, cached: bool) {
        if self.allocations.len() >= LIMIT && !self.allocations.contains_key(&id) {
            self.snapshot.lower_bound = true;
            return;
        }
        let allocation = self.allocations.entry(id).or_insert(Allocation {
            bytes,
            seen: self.frame,
            current: false,
            held: false,
            cached,
            retired: None,
        });
        allocation.bytes = bytes;
        allocation.seen = self.frame;
        allocation.current |= current;
        allocation.held |= !current;
        allocation.cached |= cached;
        allocation.retired = None;
    }

    fn finish(&mut self) -> Option<Arc<AtomicBool>> {
        let mut completion = None;
        for allocation in self.allocations.values_mut() {
            if !allocation.current && !allocation.held && allocation.retired.is_none() {
                allocation.retired = Some(
                    completion
                        .get_or_insert_with(|| Arc::new(AtomicBool::new(false)))
                        .clone(),
                );
            }
        }
        let mut snapshot = Snapshot {
            peak_bytes: self.snapshot.peak_bytes,
            lower_bound: self.snapshot.lower_bound,
            ..default()
        };
        self.allocations.retain(|_, allocation| {
            let counter = if allocation.current {
                &mut snapshot.current_bytes
            } else if allocation.held {
                &mut snapshot.held_bytes
            } else if allocation
                .retired
                .as_ref()
                .is_some_and(|done| !done.load(Ordering::Acquire))
            {
                &mut snapshot.inflight_bytes
            } else if allocation.cached && self.frame.saturating_sub(allocation.seen) < CACHE_FRAMES
            {
                &mut snapshot.cached_estimate_bytes
            } else {
                return false;
            };
            *counter = counter.saturating_add(allocation.bytes);
            snapshot.textures += 1;
            true
        });
        snapshot.peak_bytes = snapshot.peak_bytes.max(snapshot.total_bytes());
        self.snapshot = snapshot;
        completion
    }
}

#[derive(Resource, Clone, Default)]
pub(crate) struct Usage(Arc<Mutex<Ledger>>);

impl Usage {
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().unwrap().snapshot
    }
}

#[derive(Component, Clone, ExtractComponent)]
pub(crate) struct Target {
    pub image: AssetId<Image>,
    pub release: bool,
}

#[derive(Resource, Default)]
struct Images(HashMap<AssetId<Image>, bool>);

pub(crate) fn install(app: &mut App) {
    if !app.is_plugin_added::<ResidencyPlugin>() {
        app.add_plugins(ResidencyPlugin);
    }
}

struct ResidencyPlugin;

impl Plugin for ResidencyPlugin {
    fn build(&self, app: &mut App) {
        let usage = Usage::default();
        app.insert_resource(usage.clone())
            .add_plugins(ExtractComponentPlugin::<Target>::default());
        app.add_observer(
            |event: On<Insert, bevy::camera::RenderTarget>,
             mut projections: Query<&mut Projection>| {
                if let Ok(mut projection) = projections.get_mut(event.entity) {
                    projection.set_changed();
                }
            },
        );
        app.add_systems(
            PostUpdate,
            refresh_targets
                .after(super::super::presentation::synchronize)
                .before(bevy::camera::CameraUpdateSystems),
        );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(usage)
                .init_resource::<Images>()
                .add_systems(ExtractSchedule, extract_images)
                .add_systems(
                    Render,
                    release_idle_targets
                        .in_set(RenderSystems::PrepareViews)
                        .before(bevy::render::view::prepare_view_targets),
                )
                .add_systems(
                    Render,
                    collect
                        .after(RenderSystems::Render)
                        .before(RenderSystems::Cleanup),
                );
        }
    }
}

fn refresh_targets(
    mut cameras: Query<(&Camera, Ref<bevy::camera::RenderTarget>, &mut Projection)>,
    images: Option<Res<Assets<Image>>>,
) {
    for (camera, target, mut projection) in &mut cameras {
        let stale = match &*target {
            bevy::camera::RenderTarget::Image(target) => images
                .as_ref()
                .and_then(|images| images.get(&target.handle))
                .is_some_and(|image| {
                    camera.physical_target_size() != Some(image.size())
                        || camera.target_scaling_factor() != Some(target.scale_factor)
                }),
            _ => false,
        };
        if target.is_changed() || stale {
            projection.set_changed();
        }
    }
}

fn extract_images(
    mut images: ResMut<Images>,
    targets: Extract<Query<&Target>>,
    displayed: Extract<
        Query<(
            &super::super::presentation::Surface,
            &super::super::presentation::DisplayedImage,
        )>,
    >,
) {
    images.0.clear();
    for target in &targets {
        images.0.insert(target.image, true);
    }
    for (surface, displayed) in &displayed {
        if surface.image != displayed.0 {
            images.0.entry(displayed.0.id()).or_insert(false);
        }
    }
}

fn release_idle_targets(
    mut commands: Commands,
    targets: Query<
        (Entity, &Target, Option<&ExtractedCamera>, Has<Camera3d>),
        Or<(
            With<ViewTarget>,
            With<ViewDepthTexture>,
            With<ViewPrepassTextures>,
        )>,
    >,
) {
    for (entity, target, camera, spatial) in &targets {
        if target.release && camera.is_none() {
            commands
                .entity(entity)
                .remove::<(ViewTarget, ViewDepthTexture, ViewPrepassTextures)>();
        } else if !spatial {
            commands
                .entity(entity)
                .remove::<(ViewDepthTexture, ViewPrepassTextures)>();
        }
    }
}

pub(super) fn texture_bytes(texture: &Texture) -> Option<u64> {
    let (block_width, block_height) = texture.format().block_dimensions();
    let block_bytes = texture.format().block_copy_size(None)?;
    let size = texture.size();
    let mut bytes = 0_u64;
    for mip in 0..texture.mip_level_count() {
        let width = (size.width >> mip.min(31)).max(1).div_ceil(block_width);
        let height = (size.height >> mip.min(31)).max(1).div_ceil(block_height);
        let depth = if texture.dimension() == bevy::render::render_resource::TextureDimension::D3 {
            (size.depth_or_array_layers >> mip.min(31)).max(1)
        } else {
            size.depth_or_array_layers
        };
        bytes = bytes.saturating_add(
            u64::from(width)
                .saturating_mul(u64::from(height))
                .saturating_mul(u64::from(depth))
                .saturating_mul(u64::from(block_bytes))
                .saturating_mul(u64::from(texture.sample_count())),
        );
    }
    Some(bytes)
}

fn collect(
    usage: Res<Usage>,
    images: Res<Images>,
    assets: Res<RenderAssets<GpuImage>>,
    views: Query<(
        &Target,
        Option<&ViewTarget>,
        Option<&ViewDepthTexture>,
        Option<&ViewPrepassTextures>,
    )>,
    queue: Res<RenderQueue>,
    external: Option<Res<crate::native::capture::render::ExternalPass>>,
    world: &World,
) {
    if external.is_some() {
        return;
    }
    let mut ledger = usage.0.lock().unwrap();
    ledger.begin();
    let mut observe = |texture: &Texture, current, cached| {
        if let Some(bytes) = texture_bytes(texture) {
            ledger.observe(texture.id(), bytes, current, cached);
        } else {
            ledger.snapshot.lower_bound = true;
        }
    };
    for (image, current) in &images.0 {
        if let Some(image) = assets.get(*image) {
            observe(&image.texture, *current, false);
        }
    }
    for (_, target, depth, prepass) in &views {
        if let Some(target) = target {
            observe(target.main_texture(), true, true);
            observe(target.main_texture_other(), true, true);
            if let Some(sampled) = target.sampled_main_texture() {
                observe(sampled, true, true);
            }
        }
        if let Some(depth) = depth {
            observe(&depth.texture, true, true);
        }
        if let Some(prepass) = prepass {
            for attachment in [
                &prepass.depth,
                &prepass.normal,
                &prepass.motion_vectors,
                &prepass.deferred,
                &prepass.deferred_lighting_pass_id,
            ]
            .into_iter()
            .flatten()
            {
                observe(&attachment.texture.texture, true, true);
                if let Some(resolve) = &attachment.resolve_target {
                    observe(&resolve.texture, true, true);
                }
                if let Some(previous) = &attachment.previous_frame_texture {
                    observe(&previous.texture, true, true);
                }
            }
        }
    }
    for texture in super::super::surface_render::staging_textures(world) {
        observe(&texture, true, false);
    }
    for (id, image) in assets.iter() {
        if !images.0.contains_key(&id) && ledger.allocations.contains_key(&image.texture.id()) {
            if let Some(bytes) = texture_bytes(&image.texture) {
                ledger.observe(image.texture.id(), bytes, false, false);
            } else {
                ledger.snapshot.lower_bound = true;
            }
        }
    }
    if let Some(done) = ledger.finish() {
        queue.on_submitted_work_done(move || done.store(true, Ordering::Release));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_consumers_count_each_texture_once_and_track_held_images_separately() {
        let mut ledger = Ledger::default();
        let source = TextureId::new();
        let displayed = TextureId::new();
        ledger.begin();
        ledger.observe(source, 800, true, false);
        ledger.observe(source, 800, true, false);
        ledger.observe(displayed, 400, false, false);
        assert!(ledger.finish().is_none());
        assert_eq!(ledger.snapshot.current_bytes, 800);
        assert_eq!(ledger.snapshot.held_bytes, 400);
        assert_eq!(ledger.snapshot.total_bytes(), 1200);
        assert_eq!(ledger.snapshot.textures, 2);
    }

    #[test]
    fn gpu_retirement_waits_for_completion_even_after_the_cache_estimate_expires() {
        let mut ledger = Ledger::default();
        ledger.begin();
        ledger.observe(TextureId::new(), 1000, true, true);
        ledger.finish();
        ledger.begin();
        let done = ledger.finish().unwrap();
        for _ in 0..20 {
            ledger.begin();
            assert!(ledger.finish().is_none());
            assert_eq!(ledger.snapshot.inflight_bytes, 1000);
        }
        done.store(true, Ordering::Release);
        ledger.begin();
        ledger.finish();
        assert_eq!(ledger.snapshot.total_bytes(), 0);
        assert_eq!(ledger.snapshot.peak_bytes, 1000);
    }

    #[test]
    fn completed_cached_textures_expire_without_extending_their_actual_lifetime() {
        let mut ledger = Ledger::default();
        let id = TextureId::new();
        ledger.begin();
        ledger.observe(id, 1000, true, true);
        ledger.finish();
        ledger.begin();
        ledger.finish().unwrap().store(true, Ordering::Release);
        ledger.begin();
        ledger.finish();
        assert_eq!(ledger.snapshot.cached_estimate_bytes, 1000);
        ledger.begin();
        ledger.finish();
        assert_eq!(ledger.snapshot.total_bytes(), 0);
        assert!(ledger.allocations.is_empty());
    }

    #[test]
    fn reusing_a_cached_texture_does_not_let_an_older_completion_retire_it() {
        let mut ledger = Ledger::default();
        let id = TextureId::new();
        ledger.begin();
        ledger.observe(id, 1000, true, true);
        ledger.finish();
        ledger.begin();
        let old = ledger.finish().unwrap();
        ledger.begin();
        ledger.observe(id, 1000, true, true);
        old.store(true, Ordering::Release);
        assert!(ledger.finish().is_none());
        assert_eq!(ledger.snapshot.current_bytes, 1000);
        assert_eq!(ledger.snapshot.inflight_bytes, 0);
    }

    #[test]
    fn accounting_stays_bounded_and_explicitly_reports_an_incomplete_measurement() {
        let mut ledger = Ledger::default();
        ledger.begin();
        for _ in 0..LIMIT + 1 {
            ledger.observe(TextureId::new(), 1, true, false);
        }
        ledger.finish();
        assert_eq!(ledger.snapshot.textures, LIMIT);
        assert_eq!(ledger.snapshot.current_bytes, LIMIT as u64);
        assert!(ledger.snapshot.lower_bound);
    }
}
