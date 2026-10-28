use super::*;
use crate::native::topology::assets::{AssetDirectory, ImportedAsset};
use bevy::{
    asset::RenderAssetUsages,
    camera::{RenderTarget, primitives::Aabb, visibility::RenderLayers},
    gltf::Gltf,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    sync::{Mutex, mpsc},
};

const MAX_FILE: u64 = 128 * 1024 * 1024;

#[derive(Component)]
struct Model {
    resource: lince_resources::Resource,
    allow_local: bool,
    viewport: Entity,
    placeholder: Entity,
    asset: Option<ImportedAsset>,
    busy: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: Vec2,
    dirty: u8,
}

struct Job {
    owner: Entity,
    resource: lince_resources::Resource,
    directory: std::path::PathBuf,
    download: bool,
    allow_local: bool,
    wake: Option<crate::native::wake::WakeSignal>,
}

#[derive(Resource)]
struct Worker {
    sender: mpsc::SyncSender<Job>,
    replies: Mutex<mpsc::Receiver<(Entity, Result<Option<ImportedAsset>, String>)>>,
}

impl Default for Worker {
    fn default() -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Job>(16);
        let (replies, receiver) = mpsc::sync_channel(16);
        std::thread::Builder::new()
            .name("embedded-models".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = engine::sources::library()
                        .map_err(|error| error.to_string())
                        .and_then(|library| {
                            prepare(
                                &library,
                                &job.resource,
                                &job.directory,
                                job.download,
                                job.allow_local,
                            )
                        });
                    if replies.send((job.owner, result)).is_err() {
                        break;
                    }
                    if let Some(wake) = job.wake {
                        wake.ring();
                    }
                }
            })
            .expect("start embedded model worker");
        Self {
            sender,
            replies: Mutex::new(receiver),
        }
    }
}

pub(super) fn supports(media_type: &str) -> bool {
    matches!(media_type, "model/gltf-binary" | "model/gltf+json")
}

fn prepare(
    library: &lince_resources::Library,
    resource: &lince_resources::Resource,
    directory: &Path,
    download: bool,
    allow_local: bool,
) -> Result<Option<ImportedAsset>, String> {
    resource.validate()?;
    let mut entry = library.find(resource)?;
    if entry.is_none() {
        let uri = lince_resources::address(&resource.uri)?;
        if uri.scheme() == "file" && allow_local {
            let path = uri
                .to_file_path()
                .map_err(|_| "Invalid model file address")?;
            if std::fs::metadata(&path)
                .map_err(|error| error.to_string())?
                .len()
                > MAX_FILE
            {
                return Err("Model exceeds 128 MiB".into());
            }
            entry = Some(library.fetch(resource)?);
        } else if download {
            let mut error = "No downloadable model address".to_owned();
            for address in std::iter::once(&resource.uri).chain(&resource.locations) {
                let uri = lince_resources::address(address)?;
                if !matches!(uri.scheme(), "https" | "http") {
                    continue;
                }
                match library
                    .http_file(address, Default::default(), MAX_FILE, false)
                    .and_then(|file| {
                        library.register(
                            &file.path,
                            &resource.uri,
                            &resource.media_type,
                            resource.hash.as_deref(),
                        )
                    }) {
                    Ok(found) => {
                        entry = Some(found);
                        break;
                    }
                    Err(failure) => error = failure,
                }
            }
            if entry.is_none() {
                return Err(error);
            }
        }
    }
    entry
        .map(|entry| package(&entry.path, directory, &resource.media_type))
        .transpose()
}

fn package(path: &Path, directory: &Path, media_type: &str) -> Result<ImportedAsset, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Choose a regular model file".into());
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_FILE + 1), &mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Model exceeds 128 MiB".into());
    }
    let binary = media_type == "model/gltf-binary";
    let value = crate::native::topology::assets::document(&bytes, binary)
        .map_err(|error| error.to_string())?;
    if value
        .get("scenes")
        .and_then(|value| value.as_array())
        .is_none_or(|scenes| scenes.is_empty())
    {
        return Err("This model has no scene to display.".into());
    }
    let mut files = BTreeSet::new();
    crate::native::topology::assets::resources(&value, &mut files)
        .map_err(|error| error.to_string())?;
    if !files.is_empty() {
        return Err(
            "Embed a self-contained GLB or glTF with its buffers and textures included.".into(),
        );
    }
    if value
        .get("extensionsRequired")
        .and_then(|value| value.as_array())
        .is_some_and(|values| {
            values.iter().any(|value| {
                value == "KHR_draco_mesh_compression" || value == "EXT_meshopt_compression"
            })
        })
    {
        return Err(
            "This model needs a compression decoder. Export an uncompressed GLB for this viewer."
                .into(),
        );
    }
    let hash = lince_resources::fingerprint(&bytes);
    let id = hash[..32].to_owned();
    let name = if binary { "source.glb" } else { "source.gltf" };
    let destination = directory.join(&id);
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    if !destination.join(name).is_file() {
        let staging = tempfile::tempdir_in(directory).map_err(|error| error.to_string())?;
        std::fs::write(staging.path().join(name), &bytes).map_err(|error| error.to_string())?;
        if let Err(error) = std::fs::rename(staging.path(), &destination)
            && !destination.join(name).is_file()
        {
            return Err(error.to_string());
        }
    }
    Ok(ImportedAsset {
        id,
        name: "Referenced model".into(),
        file: name.into(),
        scale: 1.0,
    })
}

pub(super) fn spawn(
    world: &mut World,
    frame: Frame,
    resource: lince_resources::Resource,
    allow_local: bool,
) {
    let viewport = world
        .spawn((
            ImageNode::default(),
            Node {
                width: percent(100),
                height: px(420),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(frame.content),
            super::OwnsMediaScroll,
        ))
        .observe(drag)
        .observe(scroll)
        .observe(press)
        .id();
    let placeholder = panel::column(world, frame.content);
    crate::native::edit_mode::label(world, placeholder, "Checking local model…", 14.0);
    panel::button(
        world,
        frame.controls,
        frame.owner,
        "Download / keep offline",
        Control::Download,
    );
    panel::button(
        world,
        frame.controls,
        frame.owner,
        "Reset view",
        Control::Reset,
    );
    panel::button(
        world,
        frame.controls,
        frame.owner,
        "Zoom in",
        Control::Zoom(0.8),
    );
    panel::button(
        world,
        frame.controls,
        frame.owner,
        "Zoom out",
        Control::Zoom(1.25),
    );
    panel::button(
        world,
        frame.controls,
        frame.owner,
        "Expand / compact",
        Control::Expand,
    );
    crate::native::edit_mode::label(
        world,
        frame.controls,
        "Drag to rotate; middle-drag to pan; wheel to zoom. Arrow keys rotate, +/− zoom, Home resets.",
        12.0,
    );
    panel::credits(
        world,
        frame.controls,
        frame.controls,
        crate::native::credits::ATTRIBUTIONS,
    );
    world
        .entity_mut(frame.owner)
        .insert(crate::native::sand_store::SandCredits(
            crate::native::credits::ATTRIBUTIONS,
        ));
    world
        .entity_mut(frame.owner)
        .insert(Model {
            resource,
            allow_local,
            viewport,
            placeholder,
            asset: None,
            busy: false,
            yaw: 0.0,
            pitch: 0.1,
            distance: 3.0,
            pan: Vec2::ZERO,
            dirty: 4,
        })
        .observe(orbit_keys);
    world.get_mut::<Node>(viewport).unwrap().display = Display::None;
    request(world, frame.owner, false);
}

fn request(world: &mut World, owner: Entity, download: bool) {
    let Some(model) = world.get::<Model>(owner) else {
        return;
    };
    if model.busy {
        return;
    }
    let Some(directory) = world
        .get_resource::<AssetDirectory>()
        .map(|directory| directory.0.clone())
    else {
        message(world, owner, "Model storage is unavailable.", true);
        return;
    };
    let job = Job {
        owner,
        resource: model.resource.clone(),
        directory,
        download,
        allow_local: model.allow_local,
        wake: world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned(),
    };
    world.init_resource::<Worker>();
    if world.resource::<Worker>().sender.try_send(job).is_ok() {
        world.get_mut::<Model>(owner).unwrap().busy = true;
        message(
            world,
            owner,
            if download {
                "Downloading model…"
            } else {
                "Checking local model…"
            },
            false,
        );
    } else {
        message(world, owner, "Model loader is busy. Try again.", true);
    }
}

fn message(world: &mut World, owner: Entity, text: &str, download: bool) {
    let Some(parent) = world.get::<Model>(owner).map(|model| model.placeholder) else {
        return;
    };
    panel::clear(world, parent);
    crate::native::edit_mode::label(world, parent, text, 14.0);
    if download {
        panel::button(
            world,
            parent,
            owner,
            "Download model / keep offline",
            Control::Download,
        );
    }
}

#[derive(Clone, Copy)]
enum Control {
    Download,
    Reset,
    Zoom(f32),
    Expand,
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Download) {
            request(world, owner, true);
            return;
        }
        if matches!(self, Self::Expand) {
            if let Some(viewport) = world.get::<Model>(owner).map(|model| model.viewport)
                && let Some(mut node) = world.get_mut::<Node>(viewport)
            {
                node.height = if node.height == px(420) {
                    px(720)
                } else {
                    px(420)
                };
            }
        }
        if let Some(mut model) = world.get_mut::<Model>(owner) {
            match self {
                Self::Reset => {
                    model.yaw = 0.0;
                    model.pitch = 0.1;
                    model.distance = 3.0;
                    model.pan = Vec2::ZERO;
                }
                Self::Zoom(scale) => model.distance = (model.distance * scale).clamp(0.25, 30.0),
                Self::Download | Self::Expand => {}
            }
            model.dirty = 3;
        }
        wake(world);
    }
}

fn press(mut event: On<Pointer<Press>>, parents: Query<&ChildOf>, mut focus: ResMut<InputFocus>) {
    let Ok(parent) = parents.get(event.entity) else {
        return;
    };
    let Ok(parent) = parents.get(parent.parent()) else {
        return;
    };
    focus.set(parent.parent(), bevy::input_focus::FocusCause::Pressed);
    event.propagate(false);
}

fn drag(
    mut event: On<Pointer<Drag>>,
    parents: Query<&ChildOf>,
    mut models: Query<&mut Model>,
    mut focus: ResMut<InputFocus>,
    wake_signal: Option<Res<crate::native::wake::WakeSignal>>,
) {
    if !matches!(event.button, PointerButton::Primary | PointerButton::Middle) {
        return;
    }
    let Ok(parent) = parents.get(event.entity) else {
        return;
    };
    let Ok(parent) = parents.get(parent.parent()) else {
        return;
    };
    let owner = parent.parent();
    let Ok(mut model) = models.get_mut(owner) else {
        return;
    };
    if event.button == PointerButton::Primary {
        model.yaw -= event.delta.x * 0.01;
        model.pitch = (model.pitch + event.delta.y * 0.01).clamp(-1.5, 1.5);
    } else {
        let distance = model.distance;
        model.pan += Vec2::new(-event.delta.x, event.delta.y) * distance * 0.002;
    }
    model.dirty = 3;
    focus.set(owner, bevy::input_focus::FocusCause::Pressed);
    event.propagate(false);
    if let Some(wake) = wake_signal {
        wake.ring();
    }
}

fn scroll(
    mut event: On<Pointer<Scroll>>,
    parents: Query<&ChildOf>,
    mut models: Query<&mut Model>,
    wake_signal: Option<Res<crate::native::wake::WakeSignal>>,
) {
    let Ok(parent) = parents.get(event.entity) else {
        return;
    };
    let Ok(parent) = parents.get(parent.parent()) else {
        return;
    };
    let Ok(mut model) = models.get_mut(parent.parent()) else {
        return;
    };
    let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
        0.12
    } else {
        0.003
    };
    model.distance = (model.distance * (-event.y * scale).exp()).clamp(0.25, 30.0);
    model.dirty = 3;
    event.propagate(false);
    if let Some(wake) = wake_signal {
        wake.ring();
    }
}

fn orbit_keys(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    mut models: Query<&mut Model>,
    wake_signal: Option<Res<crate::native::wake::WakeSignal>>,
) {
    if !event.input.state.is_pressed() || event.original_event_target() != event.focused_entity {
        return;
    }
    let Ok(mut model) = models.get_mut(event.focused_entity) else {
        return;
    };
    match event.input.key_code {
        KeyCode::ArrowLeft => model.yaw += 0.15,
        KeyCode::ArrowRight => model.yaw -= 0.15,
        KeyCode::ArrowUp => model.pitch = (model.pitch - 0.15).max(-1.5),
        KeyCode::ArrowDown => model.pitch = (model.pitch + 0.15).min(1.5),
        KeyCode::Equal | KeyCode::NumpadAdd => model.distance = (model.distance * 0.8).max(0.25),
        KeyCode::Minus | KeyCode::NumpadSubtract => {
            model.distance = (model.distance * 1.25).min(30.0)
        }
        KeyCode::Home => {
            model.yaw = 0.0;
            model.pitch = 0.1;
            model.distance = 3.0;
            model.pan = Vec2::ZERO;
        }
        _ => return,
    }
    model.dirty = 3;
    event.propagate(false);
    if let Some(wake) = wake_signal {
        wake.ring();
    }
}

fn wake(world: &World) {
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

struct Scene {
    root: Entity,
    model: Entity,
    camera: Entity,
    handle: Handle<Gltf>,
    image: Handle<Image>,
    layer: usize,
    ready: bool,
    fitted: bool,
}

#[derive(Resource, Default)]
struct Scenes(HashMap<Entity, Scene>);

pub(super) fn install(app: &mut App) {
    app.init_resource::<Scenes>().add_systems(
        PostUpdate,
        update
            .before(bevy::transform::TransformSystems::Propagate)
            .before(bevy::camera::CameraUpdateSystems),
    );
}

fn create(world: &mut World, owner: Entity, asset: &ImportedAsset, layer: usize) -> Option<Scene> {
    let server = world.get_resource::<AssetServer>()?;
    let handle = server
        .load_builder()
        .with_settings(|settings: &mut bevy::gltf::GltfLoaderSettings| {
            settings.load_cameras = false;
            settings.load_lights = false;
            settings.load_animations = false;
        })
        .load(format!("topology://{}/{}", asset.id, asset.file));
    let mut image = Image::new_uninit(
        Extent3d {
            width: 640,
            height: 420,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::COPY_SRC
        | TextureUsages::RENDER_ATTACHMENT;
    let image = world.resource_mut::<Assets<Image>>().add(image);
    let root = world
        .spawn((Transform::default(), Visibility::default()))
        .id();
    let camera = world
        .spawn((
            Camera3d::default(),
            Camera {
                order: -3,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            RenderLayers::none().with(layer),
            Msaa::Off,
            bevy::camera::Exposure::INDOOR,
            Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::FRAC_PI_4,
                near: 0.01,
                far: 1000.0,
                ..default()
            }),
            Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
            ChildOf(root),
            crate::native::topology::surface_budget::residency::Target {
                image: image.id(),
                release: false,
            },
        ))
        .id();
    for (position, intensity) in [
        (Vec3::new(-3.0, 4.0, 5.0), 120_000.0),
        (Vec3::new(3.0, 1.0, -3.0), 80_000.0),
    ] {
        world.spawn((
            PointLight {
                intensity,
                range: 30.0,
                ..default()
            },
            Transform::from_translation(position),
            RenderLayers::none().with(layer),
            ChildOf(root),
        ));
    }
    let viewport = world.get::<Model>(owner)?.viewport;
    world.get_mut::<ImageNode>(viewport)?.image = image.clone();
    world.get_mut::<Node>(viewport)?.display = Display::Flex;
    Some(Scene {
        root,
        model: Entity::PLACEHOLDER,
        camera,
        handle,
        image,
        layer,
        ready: false,
        fitted: false,
    })
}

fn fit(world: &mut World, scene: &mut Scene) {
    if world.get::<Children>(scene.model).is_none() {
        return;
    }
    let mut pending = vec![(scene.model, bevy::math::Affine3A::IDENTITY)];
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    while let Some((entity, parent)) = pending.pop() {
        let transform = parent
            * world
                .get::<Transform>(entity)
                .map_or(bevy::math::Affine3A::IDENTITY, Transform::compute_affine);
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().map(|child| (child, transform)));
        }
        if let Some(aabb) = world.get::<Aabb>(entity) {
            let center = Vec3::from(aabb.center);
            let extent = Vec3::from(aabb.half_extents);
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    for z in [-1.0, 1.0] {
                        let point =
                            transform.transform_point3(center + extent * Vec3::new(x, y, z));
                        min = min.min(point);
                        max = max.max(point);
                    }
                }
            }
        }
        world.entity_mut(entity).insert((
            RenderLayers::none().with(scene.layer),
            bevy::picking::Pickable::IGNORE,
        ));
    }
    if min.is_finite() && max.is_finite() {
        let center = min * 0.5 + max * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.00001);
        *world.get_mut::<Transform>(scene.model).unwrap() =
            Transform::from_scale(Vec3::splat(radius.recip())).with_translation(-center / radius);
        *world.get_mut::<Visibility>(scene.model).unwrap() = Visibility::Inherited;
        scene.fitted = true;
    }
}

fn update(world: &mut World) {
    let replies: Vec<_> = world
        .get_resource::<Worker>()
        .map(|worker| worker.replies.lock().unwrap().try_iter().collect())
        .unwrap_or_default();
    for (owner, result) in replies {
        let Some(mut model) = world.get_mut::<Model>(owner) else {
            continue;
        };
        model.busy = false;
        match result {
            Ok(Some(asset)) => {
                model.asset = Some(asset);
                message(world, owner, "Loading model…", false);
            }
            Ok(None) => message(
                world,
                owner,
                "3D model · download to explore it here and offline.",
                true,
            ),
            Err(error) => message(world, owner, &error, true),
        }
    }
    let mut scenes = world.remove_resource::<Scenes>().unwrap();
    scenes.0.retain(|owner, scene| {
        if world.get::<Model>(*owner).is_some() {
            return true;
        }
        world.despawn(scene.root);
        if let Some(mut images) = world.get_resource_mut::<Assets<Image>>() {
            images.remove(scene.image.id());
        }
        false
    });
    let pending: Vec<_> = world
        .query::<(Entity, &Model)>()
        .iter(world)
        .filter(|(owner, model)| model.asset.is_some() && !scenes.0.contains_key(owner))
        .map(|(owner, model)| (owner, model.asset.clone().unwrap()))
        .collect();
    for (owner, asset) in pending {
        let layer = crate::native::render_layers::MODELS
            .indices()
            .find(|layer| scenes.0.values().all(|scene| scene.layer != *layer));
        if let Some(layer) = layer
            && let Some(scene) = create(world, owner, &asset, layer)
        {
            scenes.0.insert(owner, scene);
        }
    }
    for (owner, scene) in &mut scenes.0 {
        if !scene.ready {
            if let Some(gltf) = world
                .get_resource::<Assets<Gltf>>()
                .and_then(|assets| assets.get(&scene.handle))
            {
                if let Some(asset) = gltf
                    .default_scene
                    .clone()
                    .or_else(|| gltf.scenes.first().cloned())
                {
                    scene.model = world
                        .spawn((
                            WorldAssetRoot(asset),
                            Transform::default(),
                            Visibility::Hidden,
                            ChildOf(scene.root),
                        ))
                        .id();
                    scene.ready = true;
                }
            } else if world.get_resource::<AssetServer>().is_some_and(|server| {
                matches!(
                    server.load_state(scene.handle.id()),
                    bevy::asset::LoadState::Failed(_)
                ) || matches!(
                    server.recursive_dependency_load_state(scene.handle.id()),
                    bevy::asset::RecursiveDependencyLoadState::Failed(_)
                )
            }) {
                message(
                    world,
                    *owner,
                    "This model could not be loaded. Check that it is a self-contained glTF 2.0 model.",
                    true,
                );
            }
        }
        if scene.ready && !scene.fitted {
            fit(world, scene);
            if scene.fitted {
                world.get_mut::<Model>(*owner).unwrap().dirty = 32;
            }
        }
        let model = world.get::<Model>(*owner).unwrap();
        let viewport = model.viewport;
        let size = world
            .get::<ComputedNode>(viewport)
            .map(|node| node.size() * node.inverse_scale_factor())
            .unwrap_or(Vec2::ZERO);
        let visible = size.min_element() > 1.0
            && !crate::native::laboratory::blocked(world, *owner)
            && world
                .get::<InheritedVisibility>(viewport)
                .is_none_or(|value| value.get());
        let width = size.x.round().clamp(32.0, 1536.0) as u32;
        let height = size.y.round().clamp(32.0, 1024.0) as u32;
        let changed = world
            .get_resource::<Assets<Image>>()
            .and_then(|assets| assets.get(&scene.image))
            .is_some_and(|image| image.width() != width || image.height() != height);
        if visible && changed {
            world
                .resource_mut::<Assets<Image>>()
                .get_mut(&scene.image)
                .unwrap()
                .resize(Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                });
            world.get_mut::<Model>(*owner).unwrap().dirty = 4;
        }
        let model = world.get::<Model>(*owner).unwrap();
        let rotation = Quat::from_euler(EulerRot::YXZ, model.yaw, model.pitch, 0.0);
        let target = rotation * model.pan.extend(0.0);
        let distance = model.distance;
        let dirty = model.dirty;
        *world.get_mut::<Transform>(scene.camera).unwrap() =
            Transform::from_translation(target + rotation * Vec3::Z * distance)
                .looking_at(target, rotation * Vec3::Y);
        let active = visible && (!scene.fitted || dirty > 0);
        world.get_mut::<Camera>(scene.camera).unwrap().is_active = visible;
        if active {
            let mut model = world.get_mut::<Model>(*owner).unwrap();
            model.dirty = model.dirty.saturating_sub(1);
            wake(world);
        }
        if scene.fitted {
            let placeholder = world.get::<Model>(*owner).unwrap().placeholder;
            if let Some(mut node) = world.get_mut::<Node>(placeholder) {
                node.display = Display::None;
            }
        }
    }
    world.insert_resource(scenes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_uses_the_models_local_hierarchy_before_transform_propagation() {
        let mut world = World::new();
        let root = world.spawn(Transform::default()).id();
        let model = world
            .spawn((Transform::default(), Visibility::Hidden, ChildOf(root)))
            .id();
        let group = world
            .spawn((Transform::from_xyz(1000.0, 0.0, 0.0), ChildOf(model)))
            .id();
        let bone = world
            .spawn((
                Transform::from_xyz(0.0, 20.0, 0.0),
                Aabb {
                    center: Vec3::ZERO.into(),
                    half_extents: Vec3::ONE.into(),
                },
                ChildOf(group),
            ))
            .id();
        let mut scene = Scene {
            root,
            model,
            camera: Entity::PLACEHOLDER,
            handle: Handle::default(),
            image: Handle::default(),
            layer: crate::native::render_layers::MODELS.get(0).unwrap(),
            ready: true,
            fitted: false,
        };
        fit(&mut world, &mut scene);
        assert!(scene.fitted);
        let transform = world.get::<Transform>(model).unwrap();
        assert!(
            transform
                .transform_point(Vec3::new(1000.0, 20.0, 0.0))
                .length()
                < 0.001
        );
        assert!((transform.scale.x - 1.0 / 3.0_f32.sqrt()).abs() < 0.001);
        assert_eq!(
            *world.get::<Visibility>(model).unwrap(),
            Visibility::Inherited
        );
        assert!(
            !world
                .get::<RenderLayers>(bone)
                .unwrap()
                .intersects(&RenderLayers::layer(0))
        );
    }

    #[test]
    fn model_pixels_render_inline_rotate_and_release_their_scene() {
        use bevy::asset::{
            AssetApp,
            io::{AssetSource, AssetSourceBuilder},
        };
        use bevy::render::gpu_readback::{Readback, ReadbackComplete};
        use std::time::{Duration, Instant};
        #[derive(Resource, Default)]
        struct Pixels(Vec<u8>);
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("fixture.glb");
        if let Some(path) = std::env::var_os("LINCE_TEST_MODEL") {
            std::fs::copy(path, &source).unwrap();
        } else {
            std::fs::write(&source, include_bytes!("../../../fixtures/showoff.glb")).unwrap();
        }
        let assets = directory.path().join("assets");
        let asset = package(&source, &assets, "model/gltf-binary").unwrap();
        let mut app = App::new();
        let reader = assets.to_string_lossy().into_owned();
        app.register_asset_source(
            "topology",
            AssetSourceBuilder::new(move || AssetSource::get_default_reader(reader.clone())()),
        );
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::log::LogPlugin>(),
        )
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<crate::native::tokens::ThemeSettings>()
        .init_resource::<Pixels>();
        install(&mut app);
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.plugins_state() != bevy::app::PluginsState::Ready {
            assert!(Instant::now() < deadline, "GPU initialization timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        app.finish();
        app.cleanup();
        let target =
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    640,
                    480,
                    TextureFormat::Rgba8UnormSrgb,
                    None,
                ));
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Camera::default(),
                RenderTarget::Image(target.into()),
                Msaa::Off,
            ))
            .id();
        let parent = app
            .world_mut()
            .spawn((
                Node {
                    width: px(640),
                    height: px(480),
                    ..default()
                },
                UiTargetCamera(camera),
            ))
            .id();
        let frame = super::super::frame(app.world_mut(), parent, "Model");
        spawn(
            app.world_mut(),
            frame,
            lince_resources::Resource {
                uri: "https://example.invalid/model.glb".into(),
                media_type: "model/gltf-binary".into(),
                ..default()
            },
            false,
        );
        app.world_mut().get_mut::<Model>(frame.owner).unwrap().asset = Some(asset);
        app.world_mut().get_mut::<Model>(frame.owner).unwrap().dirty = u8::MAX;
        let mut readback = None;
        loop {
            app.update();
            if readback.is_none()
                && let Some(scene) = app.world().resource::<Scenes>().0.get(&frame.owner)
            {
                let image = scene.image.clone();
                readback = Some(
                    app.world_mut()
                        .spawn(Readback::texture(image))
                        .observe(|event: On<ReadbackComplete>, mut pixels: ResMut<Pixels>| {
                            pixels.0 = event.data.clone()
                        })
                        .id(),
                );
            }
            if app
                .world()
                .resource::<Pixels>()
                .0
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 20)
                .count()
                > 100
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "The embedded model rendered no pixels; state: {:?}",
                app.world()
                    .resource::<Scenes>()
                    .0
                    .get(&frame.owner)
                    .map(|scene| (
                        scene.ready,
                        scene.fitted,
                        app.world()
                            .get::<ComputedNode>(
                                app.world().get::<Model>(frame.owner).unwrap().viewport
                            )
                            .map(ComputedNode::size),
                        app.world()
                            .get::<Camera>(scene.camera)
                            .map(|camera| camera.is_active),
                        app.world()
                            .resource::<AssetServer>()
                            .load_state(scene.handle.id()),
                        app.world().resource::<Pixels>().0.len()
                    ))
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let before = app.world().resource::<Pixels>().0.clone();
        let root = app.world().resource::<Scenes>().0[&frame.owner].root;
        let image = app.world().resource::<Scenes>().0[&frame.owner].image.id();
        app.world_mut().get_mut::<Model>(frame.owner).unwrap().yaw = 1.1;
        app.world_mut().get_mut::<Model>(frame.owner).unwrap().dirty = 32;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.update();
            if app.world().resource::<Pixels>().0 != before {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Rotating the model did not change its rendered pixels"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            app.world().get::<Node>(frame.controls).unwrap().display,
            Display::None
        );
        app.world_mut().despawn(readback.unwrap());
        app.world_mut().despawn(frame.owner);
        app.update();
        assert!(app.world().get_entity(root).is_err());
        assert!(!app.world().resource::<Assets<Image>>().contains(image));
        assert!(app.world().resource::<Scenes>().0.is_empty());
    }

    #[test]
    fn model_gestures_rotate_and_zoom_without_scrolling_the_record_or_opening_controls() {
        use bevy::picking::{
            backend::HitData,
            pointer::{Location, PointerId},
        };
        let mut app = crate::native::sand_panel::tests::app();
        app.add_plugins(crate::native::layout::LayoutPlugin);
        let world = app.world_mut();
        let parent = world
            .spawn((
                Node::default(),
                ComputedNode {
                    size: Vec2::new(600.0, 500.0),
                    content_size: Vec2::new(600.0, 1500.0),
                    ..default()
                },
            ))
            .id();
        crate::native::scroll_sand::attach(world, parent);
        let frame = super::super::frame(world, parent, "Model");
        spawn(
            world,
            frame,
            lince_resources::Resource {
                uri: "https://example.invalid/model.glb".into(),
                media_type: "model/gltf-binary".into(),
                ..default()
            },
            false,
        );
        let viewport = world.get::<Model>(frame.owner).unwrap().viewport;
        let location = Location {
            target: bevy::camera::NormalizedRenderTarget::Image(Handle::<Image>::default().into()),
            position: Vec2::ZERO,
        };
        world.trigger(Pointer::new(
            PointerId::Mouse,
            location.clone(),
            Drag {
                button: PointerButton::Primary,
                distance: Vec2::new(50.0, 20.0),
                delta: Vec2::new(50.0, 20.0),
            },
            viewport,
        ));
        let model = world.get::<Model>(frame.owner).unwrap();
        assert!(model.yaw.abs() > 0.1);
        assert!(model.pitch > 0.1);
        world.trigger(Pointer::new(
            PointerId::Mouse,
            location,
            Scroll {
                unit: bevy::input::mouse::MouseScrollUnit::Line,
                x: 0.0,
                y: 1.0,
                phase: bevy::input::touch::TouchPhase::Moved,
                hit: HitData::new(viewport, 0.0, None, None),
            },
            viewport,
        ));
        assert!(world.get::<Model>(frame.owner).unwrap().distance < 3.0);
        assert_eq!(world.get::<ScrollPosition>(parent).unwrap().0, Vec2::ZERO);
        assert_eq!(
            world.get::<Node>(frame.controls).unwrap().display,
            Display::None
        );
    }

    #[test]
    fn retained_models_reuse_a_package_and_reopen_without_the_origin_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("model.glb");
        std::fs::write(&path, include_bytes!("../../../fixtures/showoff.glb")).unwrap();
        let library = lince_resources::Library::new(directory.path().join("library"));
        let resource = lince_resources::Resource {
            uri: lince_resources::address(path.to_str().unwrap())
                .unwrap()
                .to_string(),
            media_type: "model/gltf-binary".into(),
            hash: Some(lince_resources::fingerprint(include_bytes!(
                "../../../fixtures/showoff.glb"
            ))),
            ..default()
        };
        let destination = directory.path().join("assets");
        let first = prepare(&library, &resource, &destination, false, true)
            .unwrap()
            .unwrap();
        assert!(first.valid());
        std::fs::remove_file(&path).unwrap();
        let second = prepare(&library, &resource, &destination, false, true)
            .unwrap()
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            std::fs::read(destination.join(first.id).join(first.file)).unwrap(),
            include_bytes!("../../../fixtures/showoff.glb")
        );
    }

    #[test]
    fn opening_a_remote_reference_never_fetches_a_model_or_reads_an_uncached_local_uri() {
        let directory = tempfile::tempdir().unwrap();
        let library = lince_resources::Library::new(directory.path().join("library"));
        for uri in ["https://127.0.0.1:1/model.glb", "file:///private/model.glb"] {
            assert!(
                prepare(
                    &library,
                    &lince_resources::Resource {
                        uri: uri.into(),
                        media_type: "model/gltf-binary".into(),
                        ..default()
                    },
                    &directory.path().join("assets"),
                    false,
                    false
                )
                .unwrap()
                .is_none()
            );
        }
    }

    #[test]
    fn model_packages_cannot_resolve_neighboring_or_remote_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("model.gltf");
        for uri in [
            "../private.bin",
            "https://example.invalid/texture.png",
            "texture.png",
            "file:///private/key",
        ] {
            let value = serde_json::json!({"asset":{"version":"2.0"},"scenes":[{"nodes":[]}],"meshes":[{"primitives":[]}],"buffers":[{"uri":uri,"byteLength":12}]});
            std::fs::write(&path, value.to_string()).unwrap();
            assert!(
                package(&path, &directory.path().join("assets"), "model/gltf+json").is_err(),
                "{uri}"
            );
        }
    }
}
