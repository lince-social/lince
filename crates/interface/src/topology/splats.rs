pub mod codec;
#[cfg(test)]
mod tests;

use super::assets::{Bounds, ImportedAsset, Ready};
use bevy::{
    asset::{AssetLoader, AsyncReadExt, LoadContext, io::Reader},
    camera::{primitives::Aabb, visibility::VisibleEntities},
    prelude::*,
    render::{
        Extract, ExtractSchedule, RenderApp, renderer::RenderDevice, sync_world::RenderEntity,
    },
};
use bevy_gaussian_splatting::{
    CloudSettings, Gaussian3d, GaussianCamera, GaussianSplattingPlugin, PlanarGaussian3d,
    PlanarGaussian3dHandle,
};
use std::{
    any::TypeId,
    collections::HashSet,
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Resource)]
struct LoadWake {
    sender: Option<mpsc::Sender<bool>>,
    task: Option<thread::JoinHandle<()>>,
    active: bool,
    frames: u8,
}

impl LoadWake {
    fn new(wake: crate::wake::WakeSignal) -> Self {
        let (sender, receiver) = mpsc::channel();
        let task = thread::Builder::new()
            .name("gaussian-load-wake".into())
            .spawn(move || {
                let mut active = false;
                loop {
                    let result = if active {
                        receiver.recv_timeout(Duration::from_millis(100))
                    } else {
                        receiver
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    };
                    match result {
                        Ok(next) => active = next,
                        Err(mpsc::RecvTimeoutError::Timeout) => wake.ring(),
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("start Gaussian load wake timer");
        Self {
            sender: Some(sender),
            task: Some(task),
            active: false,
            frames: 0,
        }
    }

    fn update(&mut self, loading: bool) {
        if loading {
            self.frames = 4;
        }
        let active = loading || self.frames > 0;
        if !loading {
            self.frames = self.frames.saturating_sub(1);
        }
        if active != self.active {
            self.active = active;
            let _ = self.sender.as_ref().unwrap().send(active);
        }
    }
}

impl Drop for LoadWake {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

pub struct SplatPlugin;

impl Plugin for SplatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(GaussianSplattingPlugin)
            .init_asset::<SplatAsset>();
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(
                ExtractSchedule,
                cull_cloud_work
                    .after(bevy_gaussian_splatting::render::extract_gaussians::<Gaussian3d>),
            );
        }
        let helpers: Handle<Shader> =
            bevy::asset::uuid_handle!("9ca57ab0-07de-4a43-94f8-547c38e292cb");
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(
                helpers.id(),
                Shader::from_wgsl(
                    include_str!("splats/helpers.wgsl"),
                    "lince/gaussian_helpers.wgsl",
                ),
            )
            .expect("Gaussian helper shader must be replaceable");
    }

    fn finish(&self, app: &mut App) {
        let limit =
            app.world()
                .get_resource::<RenderDevice>()
                .map_or(codec::MAX_SPLATS, |device| {
                    (device.limits().max_storage_buffer_binding_size as usize
                        / codec::APPEARANCE_BYTES)
                        .min(codec::MAX_SPLATS)
                });
        app.register_asset_loader(SplatLoader {
            limit,
            resident: Arc::default(),
        });
    }
}

fn cull_cloud_work(
    mut commands: Commands,
    cameras: Extract<Query<(&Camera, &VisibleEntities), With<super::presentation::WorldCamera>>>,
    clouds: Extract<Query<(Entity, RenderEntity), With<PlanarGaussian3dHandle>>>,
) {
    let class = TypeId::of::<bevy_gaussian_splatting::gaussian::cloud::CloudVisibilityClass>();
    let visible: HashSet<_> = cameras
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .flat_map(|(_, entities)| entities.get(class).iter().copied())
        .collect();
    for (entity, render_entity) in &clouds {
        if !visible.contains(&entity) {
            commands
                .entity(render_entity)
                .remove::<(CloudSettings, bevy_gaussian_splatting::render::CloudUniform)>();
        }
    }
}

#[derive(Asset, TypePath)]
pub struct SplatAsset {
    #[dependency]
    pub cloud: Handle<PlanarGaussian3d>,
    pub bounds: Bounds,
    pub count: usize,
    _reservation: Reservation,
}

struct Reservation {
    count: usize,
    resident: Arc<AtomicUsize>,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.resident.fetch_sub(self.count, Ordering::AcqRel);
    }
}

#[derive(TypePath)]
struct SplatLoader {
    limit: usize,
    resident: Arc<AtomicUsize>,
}

impl AssetLoader for SplatLoader {
    type Asset = SplatAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<SplatAsset> {
        let mut bytes = Vec::new();
        reader
            .take(codec::MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() as u64 > codec::MAX_FILE_BYTES {
            return Err(io::Error::other("Gaussian file exceeds 1 GiB"));
        }
        let count = codec::count(&bytes)?;
        if count > self.limit {
            return Err(io::Error::other(format!(
                "This GPU supports at most {} splats per cloud at the selected appearance detail; prepare a smaller cloud",
                self.limit
            )));
        }
        self.resident.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(count).filter(|total| *total <= codec::MAX_SPLATS)
        }).map_err(|_| io::Error::other("Loaded Gaussian assets exceed the 1 GiB decoded-data budget; remove another cloud or prepare less detail"))?;
        let reservation = Reservation {
            count,
            resident: self.resident.clone(),
        };
        let (cloud, bounds) = codec::decode(&bytes)?;
        drop(bytes);
        let cloud = context.add_labeled_asset("cloud", cloud);
        Ok(SplatAsset {
            cloud,
            bounds,
            count,
            _reservation: reservation,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["gcloud"]
    }
}

#[derive(Component)]
pub struct SplatHandle(pub Handle<SplatAsset>);

#[derive(Component)]
pub struct FrameOnReady;

pub fn is_splat(asset: &ImportedAsset) -> bool {
    asset.file == "source.gcloud"
}

pub fn load(world: &mut World, entity: Entity, path: String) {
    if !world.contains_resource::<LoadWake>()
        && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
    {
        world.insert_resource(LoadWake::new(wake));
    }
    if let Some(mut wake) = world.get_resource_mut::<LoadWake>() {
        wake.update(true);
    }
    let handle = world.resource::<AssetServer>().load::<SplatAsset>(path);
    world.entity_mut(entity).insert(SplatHandle(handle));
}

pub fn update(world: &mut World) {
    if !world.contains_resource::<Assets<SplatAsset>>() {
        return;
    }
    let cameras: Vec<_> = world
        .query_filtered::<Entity, (
            With<super::presentation::WorldCamera>,
            Without<GaussianCamera>,
        )>()
        .iter(world)
        .collect();
    for camera in cameras {
        world.entity_mut(camera).insert(GaussianCamera::default());
    }
    let pending: Vec<_> = world
        .query_filtered::<(Entity, &SplatHandle), Without<Ready>>()
        .iter(world)
        .map(|(e, h)| (e, h.0.clone()))
        .collect();
    if let Some(mut wake) = world.get_resource_mut::<LoadWake>() {
        wake.update(!pending.is_empty());
    }
    for (entity, handle) in pending {
        let asset = world
            .resource::<Assets<SplatAsset>>()
            .get(&handle)
            .map(|a| (a.cloud.clone(), a.bounds, a.count));
        if let Some((cloud, bounds, count)) = asset {
            let visual = world
                .spawn((
                    PlanarGaussian3dHandle(cloud),
                    CloudSettings::default(),
                    Aabb::from_min_max(bounds.min, bounds.max),
                    ChildOf(entity),
                ))
                .id();
            world
                .entity_mut(entity)
                .insert((bounds, Ready, super::assets::ImportedScene(visual)));
            if world.get::<FrameOnReady>(entity).is_some() {
                super::assets::frame(world, entity);
                world.entity_mut(entity).remove::<FrameOnReady>();
            }
            crate::notifications::report(
                world,
                "Topology",
                &format!("Imported {count} Gaussian splats. This asset is visual only."),
            );
        } else if let bevy::asset::LoadState::Failed(error) =
            world.resource::<AssetServer>().load_state(handle.id())
        {
            crate::notifications::report(
                world,
                "Topology",
                &format!("Could not load Gaussian cloud: {error}"),
            );
            super::assets::discard(world, entity);
        }
    }
}

pub fn ray_distance(bounds: &Bounds, origin: Vec3, direction: Vec3) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = f32::INFINITY;
    for i in 0..3 {
        if direction[i].abs() < 1e-8 {
            if origin[i] < bounds.min[i] || origin[i] > bounds.max[i] {
                return None;
            }
        } else {
            let a = (bounds.min[i] - origin[i]) / direction[i];
            let b = (bounds.max[i] - origin[i]) / direction[i];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    (near <= far && far.is_finite()).then_some(if near > 0.0 { near } else { far })
}
