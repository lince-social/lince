use crate::native::actions::Action;
use bevy::{
    asset::{LoadState, RenderAssetUsages, io::embedded::EmbeddedAssetRegistry},
    camera::{RenderTarget, visibility::RenderLayers},
    core_pipeline::tonemapping::Tonemapping,
    gltf::Gltf,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    window::{PrimaryWindow, WindowOccluded},
    world_serialization::WorldInstanceReady,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

mod behavior;
mod palette;
use behavior::{Activity, Behavior};
use palette::{Binding, Palette, Role};

const SVG: &[u8] =
    include_bytes!("../../../../../institute/assets/characters/fiote/fiote-seated.svg");
const GLB: &[u8] = include_bytes!("../../../../../institute/assets/characters/fiote/fiote.glb");
const GLB_PATH: &str = "embedded://fiote/fiote.glb";
const LAYER: usize = 32;
const FRAME: Duration = Duration::from_millis(66);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    Static,
    Animated,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub appearance: Appearance,
    pub paused: bool,
    pub reduced_motion: bool,
}

impl Preferences {
    fn animated(self) -> bool {
        self.appearance == Appearance::Animated && !self.paused && !self.reduced_motion
    }
}

#[derive(Resource, Default)]
struct Character {
    preferences: Preferences,
    path: Option<PathBuf>,
    error: Option<String>,
    failure: bool,
    portraits: HashMap<Palette, (u32, Handle<Image>)>,
    loading: Option<Handle<Gltf>>,
    scenes: Vec<Scene>,
    occluded: bool,
    last: Option<Instant>,
    alarm: Option<Instant>,
}

struct Scene {
    palette: Palette,
    layer: usize,
    running: bool,
    rendered: Option<Instant>,
    materials: Vec<Binding>,
    root: Entity,
    model: Entity,
    camera: Entity,
    image: Handle<Image>,
    graph: Handle<AnimationGraph>,
    clips: HashMap<&'static str, (AnimationNodeIndex, f32)>,
    players: Vec<Entity>,
    behavior: Behavior,
}

#[derive(Component)]
pub struct Avatar;

#[derive(Serialize)]
pub struct Metrics {
    pub avatars: usize,
    pub visible_avatars: usize,
    pub animated_scenes: usize,
    pub cached_portraits: usize,
    pub render_target_color_bytes: u64,
    pub motion_materials: usize,
    pub refresh_period_ms: u128,
}

pub fn metrics(world: &mut World) -> Metrics {
    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<Avatar>>()
        .iter(world)
        .collect();
    let avatars = entities.len();
    let visible_avatars = entities
        .iter()
        .filter(|entity| visible(world, **entity))
        .count();
    let character = world.resource::<Character>();
    let images = world.get_resource::<Assets<Image>>();
    Metrics {
        avatars,
        visible_avatars,
        refresh_period_ms: character
            .scenes
            .iter()
            .map(|scene| scene.behavior.activity.frame_period().as_millis())
            .min()
            .unwrap_or(0),
        animated_scenes: character.scenes.len(),
        cached_portraits: character.portraits.len(),
        render_target_color_bytes: character
            .scenes
            .iter()
            .filter_map(|scene| images.and_then(|images| images.get(&scene.image)))
            .map(|image| u64::from(image.width()) * u64::from(image.height()) * 4)
            .sum(),
        motion_materials: character
            .scenes
            .iter()
            .map(|scene| {
                scene
                    .materials
                    .iter()
                    .filter(|binding| binding.joint.is_some())
                    .count()
            })
            .sum(),
    }
}

#[derive(Component, Clone, Copy)]
struct RenderedPalette(Palette);

#[derive(Component)]
struct SettingsPanel {
    status: Entity,
}

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.world()
            .resource::<EmbeddedAssetRegistry>()
            .insert_asset(
                PathBuf::from("institute/assets/characters/fiote/fiote.glb"),
                std::path::Path::new("fiote/fiote.glb"),
                GLB,
            );
        app.init_resource::<Character>()
            .add_systems(PreUpdate, occlusion)
            .add_systems(Update, update)
            .add_systems(
                PostUpdate,
                palette::motion_effects.after(bevy::transform::TransformSystems::Propagate),
            );
    }
}

fn occlusion(
    mut events: MessageReader<WindowOccluded>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut character: ResMut<Character>,
) {
    for event in events.read() {
        if primary.contains(event.window) {
            character.occluded = event.occluded;
        }
    }
}

fn initialize(world: &mut World) {
    world.init_resource::<Character>();
    world.init_resource::<crate::native::tokens::ThemeSettings>();
    if world.resource::<Character>().path.is_some() {
        return;
    }
    let Some(directory) = world
        .get_resource::<crate::native::workspace::WorkspaceFile>()
        .map(|file| file.directory().to_path_buf())
    else {
        return;
    };
    let path = directory.join("fiote-appearance.json");
    let loaded = read_preferences(&path);
    let mut character = world.resource_mut::<Character>();
    character.path = Some(path);
    match loaded {
        Ok(preferences) => character.preferences = preferences,
        Err(error) => {
            character.error = Some(format!("Fiote appearance could not be loaded: {error}"))
        }
    }
}

fn read_preferences(path: &std::path::Path) -> Result<Preferences, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 4096 {
        return Err("Appearance settings exceed their size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn save_preferences(path: &std::path::Path, preferences: Preferences) -> Result<(), String> {
    use std::io::Write;
    let directory = path
        .parent()
        .ok_or("Appearance settings have no directory")?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut temporary, &preferences)
        .map_err(|error| error.to_string())?;
    temporary
        .write_all(b"\n")
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temporary.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn preferences(world: &World) -> Preferences {
    world
        .get_resource::<Character>()
        .map_or(Preferences::default(), |character| character.preferences)
}

pub fn set_preferences(world: &mut World, preferences: Preferences) {
    initialize(world);
    let path = world.resource::<Character>().path.clone();
    let result = path
        .map(|path| save_preferences(&path, preferences))
        .transpose();
    let mut character = world.resource_mut::<Character>();
    character.preferences = preferences;
    character.failure = false;
    character.error = result
        .err()
        .map(|error| format!("Appearance changed for this session; saving failed: {error}"));
}

fn density(world: &mut World) -> u32 {
    let scale = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map_or(1.0, Window::scale_factor);
    let ui_scale = world.get_resource::<UiScale>().map_or(1.0, |scale| scale.0);
    let nodes: Vec<(Entity, f32)> = world
        .query_filtered::<(Entity, &ComputedNode), With<Avatar>>()
        .iter(world)
        .map(|(entity, node)| (entity, node.size().max_element()))
        .collect();
    let measured = nodes
        .into_iter()
        .filter(|(entity, _)| visible(world, *entity))
        .map(|(_, size)| size)
        .fold(0.0_f32, f32::max);
    ((56.0 * scale * ui_scale).max(measured).ceil() as u32)
        .clamp(64, 512)
        .next_power_of_two()
}

fn rasterize(edge: u32, palette: Palette) -> Result<Image, String> {
    let tree = resvg::usvg::Tree::from_str(&palette.svg(), &resvg::usvg::Options::default())
        .map_err(|error| error.to_string())?;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(edge, edge).ok_or("Fiote portrait size is invalid")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(edge as f32 / 256.0, edge as f32 / 256.0),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    for rgba in pixels.chunks_exact_mut(4) {
        if rgba[3] > 0 {
            for index in 0..3 {
                rgba[index] = ((u32::from(rgba[index]) * 255) / u32::from(rgba[3])).min(255) as u8;
            }
        }
    }
    Ok(Image::new(
        Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

pub(super) fn portrait(world: &mut World, source: Entity) -> Handle<Image> {
    initialize(world);
    let palette = Palette::resolve(world, source);
    portrait_for(world, palette)
}

fn portrait_for(world: &mut World, palette: Palette) -> Handle<Image> {
    let edge = density(world);
    if let Some((size, image)) = world.resource::<Character>().portraits.get(&palette) {
        if *size == edge {
            return image.clone();
        }
    }
    let image = rasterize(edge, palette).unwrap_or_else(|_| {
        let image = image::load_from_memory(include_bytes!(
            "../../../../../institute/assets/characters/fiote/fiote-seated.png"
        ))
        .expect("packaged Fiote portrait")
        .to_rgba8();
        Image::new(
            Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            image.into_raw(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    });
    world.init_resource::<Assets<Image>>();
    let handle = world.resource_mut::<Assets<Image>>().add(image);
    world
        .resource_mut::<Character>()
        .portraits
        .insert(palette, (edge, handle.clone()));
    handle
}

pub fn settings_button(world: &mut World, parent: Entity, content: Entity) {
    initialize(world);
    let panel = world
        .spawn((
            ChildOf(content),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
        ))
        .id();
    crate::native::description::button(world, parent, panel, "Fiote appearance", ToggleSettings);
    crate::native::edit_mode::label(world, panel, "Fiote appearance", 18.0);
    crate::native::edit_mode::label(
        world,
        panel,
        "Choose a seated picture or an animated lynx. Select a lynx to customize its colors.",
        14.0,
    );
    crate::native::edit_mode::label(
        world,
        panel,
        "Fur: Sand background · Light details: Sand text · Moving highlights: Accent",
        14.0,
    );
    let row = crate::native::sand_panel::row(world, panel);
    crate::native::description::button(world, row, panel, "Static", Choose(Appearance::Static));
    crate::native::description::button(world, row, panel, "Animated", Choose(Appearance::Animated));
    crate::native::description::button(world, row, panel, "Customize Fiotes", Customize(true));
    crate::native::description::button(world, row, panel, "Pause / resume", TogglePause);
    crate::native::description::button(
        world,
        row,
        panel,
        "Reduced motion on / off",
        ToggleReducedMotion,
    );
    let status = crate::native::edit_mode::label(world, panel, "Static Fiote", 14.0);
    crate::native::accessibility::status(world, status);
    world.entity_mut(panel).insert(SettingsPanel { status });
}

#[derive(Clone)]
struct ToggleSettings;
impl Action for ToggleSettings {
    fn apply(&self, world: &mut World, target: Entity) {
        if let Some(mut node) = world.get_mut::<Node>(target) {
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}

#[derive(Clone)]
pub(crate) struct Customize(pub bool);

impl Action for Customize {
    fn apply(&self, world: &mut World, owner: Entity) {
        let mut cursor = Some(owner);
        while let Some(entity) = cursor {
            if world
                .get::<crate::native::edit_mode::EditMode>(entity)
                .is_some()
            {
                let scope = if self.0 {
                    crate::native::customization::Scope::Kind(
                        crate::native::tokens::SandStyleKind::Fiote,
                    )
                } else {
                    crate::native::customization::Scope::Sand(owner)
                };
                world.entity_mut(entity).insert(scope);
                crate::native::edit_mode::show_customization(world, entity);
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

#[derive(Clone)]
struct Choose(Appearance);
impl Action for Choose {
    fn apply(&self, world: &mut World, _: Entity) {
        let mut settings = preferences(world);
        settings.appearance = self.0;
        settings.paused = false;
        set_preferences(world, settings);
    }
}

#[derive(Clone)]
struct TogglePause;
impl Action for TogglePause {
    fn apply(&self, world: &mut World, _: Entity) {
        let mut settings = preferences(world);
        settings.paused = !settings.paused;
        set_preferences(world, settings);
    }
}

#[derive(Clone)]
struct ToggleReducedMotion;
impl Action for ToggleReducedMotion {
    fn apply(&self, world: &mut World, _: Entity) {
        let mut settings = preferences(world);
        settings.reduced_motion = !settings.reduced_motion;
        set_preferences(world, settings);
    }
}

fn visible(world: &World, mut entity: Entity) -> bool {
    loop {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
            || world
                .get::<Visibility>(entity)
                .is_some_and(|visibility| *visibility == Visibility::Hidden)
        {
            return false;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return true;
        };
        entity = parent.parent();
    }
}

fn descendants(world: &World, root: Entity) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        result.push(entity);
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    result
}

fn update(world: &mut World) {
    initialize(world);
    let avatars: Vec<Entity> = world
        .query_filtered::<Entity, With<Avatar>>()
        .iter(world)
        .collect();
    let windows_visible = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .is_none_or(|window| window.visible && window.focused);
    let showing = !world.resource::<Character>().occluded
        && windows_visible
        && avatars.iter().any(|entity| visible(world, *entity));
    let animated = world.resource::<Character>().preferences.animated()
        && !world.resource::<Character>().failure;
    let groups: std::collections::BTreeMap<Palette, Vec<Entity>> =
        avatars
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut groups, entity| {
                groups
                    .entry(Palette::resolve(world, *entity))
                    .or_default()
                    .push(*entity);
                groups
            });
    if !animated || avatars.is_empty() {
        destroy_scene(world);
    } else if showing {
        let wanted: Vec<Palette> = groups
            .iter()
            .filter(|(_, entities)| entities.iter().any(|entity| visible(world, *entity)))
            .map(|(palette, _)| *palette)
            .collect();
        let mut previous = std::mem::take(&mut world.resource_mut::<Character>().scenes);
        let mut scenes = Vec::new();
        for (index, palette) in wanted.iter().enumerate() {
            let layer = LAYER + index;
            let position = previous
                .iter()
                .position(|scene| scene.palette == *palette)
                .or_else(|| {
                    groups[palette]
                        .iter()
                        .filter_map(|entity| world.get::<RenderedPalette>(*entity))
                        .find_map(|old| {
                            (!wanted.contains(&old.0))
                                .then(|| previous.iter().position(|scene| scene.palette == old.0))
                                .flatten()
                        })
                });
            if let Some(position) = position {
                let mut scene = previous.remove(position);
                if scene.palette != *palette {
                    scene.palette = *palette;
                    scene.rendered = None;
                    for binding in &scene.materials {
                        if let Some(mut material) = world
                            .resource_mut::<Assets<StandardMaterial>>()
                            .get_mut(&binding.material)
                        {
                            binding.role.apply(&mut material, *palette, binding.motion);
                        }
                    }
                    world.resource_mut::<Character>().alarm = None;
                }
                if scene.layer != layer {
                    let children = descendants(world, scene.root);
                    for child in children {
                        world.entity_mut(child).insert(RenderLayers::layer(layer));
                    }
                    scene.layer = layer;
                }
                scenes.push(scene);
            } else if let Some(scene) = load_scene(world, *palette, layer) {
                scenes.push(scene);
            }
        }
        for scene in previous {
            world.entity_mut(scene.root).despawn();
        }
        world.resource_mut::<Character>().scenes = scenes;
        if world.resource::<Character>().failure {
            destroy_scene(world);
        }
    }
    let images: Vec<Handle<Image>> = world
        .resource::<Character>()
        .scenes
        .iter()
        .map(|scene| scene.image.clone())
        .collect();
    for image in images {
        let edge = density(world).clamp(128, 256);
        let resized = world
            .resource_mut::<Assets<Image>>()
            .get_mut(&image)
            .is_some_and(|mut image| {
                if image.width() == edge {
                    return false;
                }
                image.resize(Extent3d {
                    width: edge,
                    height: edge,
                    depth_or_array_layers: 1,
                });
                true
            });
        if resized {
            let mut character = world.resource_mut::<Character>();
            character.alarm = None;
            for scene in &mut character.scenes {
                scene.rendered = None;
            }
        }
    }
    for (palette, entities) in &groups {
        let static_image = portrait_for(world, *palette);
        let handle = world
            .resource::<Character>()
            .scenes
            .iter()
            .find(|scene| scene.palette == *palette && !scene.players.is_empty())
            .map_or(static_image, |scene| scene.image.clone());
        for entity in entities {
            if let Some(mut image) = world.get_mut::<ImageNode>(*entity) {
                if image.image != handle {
                    image.image = handle.clone();
                }
            }
            world.entity_mut(*entity).insert(RenderedPalette(*palette));
        }
    }
    world
        .resource_mut::<Character>()
        .portraits
        .retain(|palette, _| groups.contains_key(palette));
    animate(world, showing && animated);
    refresh_status(world);
}

fn destroy_scene(world: &mut World) {
    let scenes = std::mem::take(&mut world.resource_mut::<Character>().scenes);
    for scene in scenes {
        world.entity_mut(scene.root).despawn();
    }
    let mut character = world.resource_mut::<Character>();
    character.loading = None;
    character.last = None;
}

fn failed(world: &mut World, error: String) {
    destroy_scene(world);
    let mut character = world.resource_mut::<Character>();
    character.failure = true;
    character.error = Some(format!(
        "Showing Static Fiote: {error}. Choose Animated to retry."
    ));
}

fn load_scene(world: &mut World, palette: Palette, layer: usize) -> Option<Scene> {
    if world.resource::<Character>().loading.is_none() {
        let handle = world.resource::<AssetServer>().load(GLB_PATH);
        world.resource_mut::<Character>().loading = Some(handle);
        return None;
    }
    let handle = world
        .resource::<Character>()
        .loading
        .as_ref()
        .unwrap()
        .clone();
    if let Some(LoadState::Failed(error)) =
        world.resource::<AssetServer>().get_load_state(handle.id())
    {
        failed(world, format!("The lynx model could not load: {error}"));
        return None;
    }
    let Some(gltf) = world.resource::<Assets<Gltf>>().get(&handle) else {
        return None;
    };
    let Some(model_asset) = gltf.scenes.first().cloned() else {
        failed(world, "The lynx model contains no scene".into());
        return None;
    };
    let mut clips = Vec::new();
    let mut durations = Vec::new();
    for activity in Activity::ALL {
        let Some(clip) = gltf.named_animations.get(activity.clip()) else {
            failed(
                world,
                format!("The lynx model is missing {}", activity.clip()),
            );
            return None;
        };
        let Some(asset) = world.resource::<Assets<AnimationClip>>().get(clip) else {
            return None;
        };
        clips.push(clip.clone());
        durations.push(asset.duration());
    }
    let (graph, indices) = AnimationGraph::from_clips(clips);
    let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
    let clips = Activity::ALL
        .into_iter()
        .zip(indices)
        .zip(durations)
        .map(|((activity, index), duration)| (activity.clip(), (index, duration)))
        .collect();
    let edge = density(world).clamp(128, 256);
    let mut image = Image::new_uninit(
        Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::COPY_SRC
        | TextureUsages::RENDER_ATTACHMENT;
    let image = world.resource_mut::<Assets<Image>>().add(image);
    let root = world
        .spawn((Transform::default(), Visibility::default()))
        .id();
    let model = world
        .spawn((
            WorldAssetRoot(model_asset),
            Transform::default(),
            Visibility::default(),
            ChildOf(root),
        ))
        .observe(scene_ready)
        .id();
    let camera = world
        .spawn((
            Camera3d::default(),
            Camera {
                order: -2,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                    viewport_height: 2.45,
                },
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 1.35, 5.0).looking_at(Vec3::new(0.0, 1.05, 0.0), Vec3::Y),
            RenderTarget::Image(image.clone().into()),
            RenderLayers::layer(layer),
            Tonemapping::None,
            bevy::camera::Exposure::INDOOR,
            Msaa::Off,
            ChildOf(root),
        ))
        .id();
    for (position, intensity) in [
        (Vec3::new(-3.0, 4.0, 4.0), 60_000.0),
        (Vec3::new(3.0, 2.0, 3.0), 30_000.0),
    ] {
        world.spawn((
            PointLight {
                intensity,
                range: 20.0,
                ..default()
            },
            Transform::from_translation(position),
            RenderLayers::layer(layer),
            ChildOf(root),
        ));
    }
    Some(Scene {
        palette,
        layer,
        running: false,
        rendered: None,
        materials: Vec::new(),
        root,
        model,
        camera,
        image,
        graph,
        clips,
        players: Vec::new(),
        behavior: Behavior::default(),
    })
}

fn scene_ready(
    event: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut character: ResMut<Character>,
    meshes: Query<(
        &bevy::gltf::GltfMaterialName,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    names: Query<&Name>,
    extras: Query<&bevy::gltf::GltfExtras>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(scene) = character
        .scenes
        .iter_mut()
        .find(|scene| scene.model == event.entity)
    else {
        return;
    };
    let descendants: Vec<Entity> = children.iter_descendants(event.entity).collect();
    for entity in &descendants {
        if let Ok(extras) = extras.get(*entity) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&extras.value) {
                let stride = value["fiote_walk_stride"].as_f64();
                let stance = value["fiote_walk_stance"].as_f64();
                if let (Some(stride), Some(stance)) = (stride, stance) {
                    if (0.01..=1.0).contains(&stride) && (0.5..=0.9).contains(&stance) {
                        scene.behavior.walk_speed =
                            stride as f32 / (stance as f32 * scene.clips["walk"].1.max(0.1));
                    }
                }
            }
        }
    }
    let mut copies = HashMap::new();
    let index = scene.clips[scene.behavior.activity.clip()].0;
    for entity in &descendants {
        let entity = *entity;
        commands
            .entity(entity)
            .insert((RenderLayers::layer(scene.layer), Pickable::IGNORE));
        if let Ok((name, source)) = meshes.get(entity) {
            let handle = copies
                .entry(source.0.id())
                .or_insert_with(|| {
                    let mut material = materials
                        .get(&source.0)
                        .expect("loaded Fiote material")
                        .clone();
                    let role = Role::from_name(&name.0);
                    material.perceptual_roughness = 0.90;
                    material.reflectance = 0.15;
                    material.unlit = name.0 == "Fur outline";
                    role.apply(&mut material, scene.palette, 0.0);
                    let handle = materials.add(material);
                    let joint = name
                        .0
                        .strip_prefix("Motion:")
                        .and_then(|joint| {
                            descendants.iter().find(|entity| {
                                names.get(**entity).is_ok_and(|name| name.as_str() == joint)
                            })
                        })
                        .copied();
                    scene.materials.push(Binding {
                        material: handle.clone(),
                        role,
                        joint,
                        previous: None,
                        motion: 0.0,
                    });
                    handle
                })
                .clone();
            commands.entity(entity).insert(MeshMaterial3d(handle));
        }
        if let Ok(mut player) = players.get_mut(entity) {
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(&mut player, index, Duration::ZERO)
                .repeat();
            commands
                .entity(entity)
                .insert((AnimationGraphHandle(scene.graph.clone()), transitions));
            scene.players.push(entity);
        }
    }
    if scene.players.is_empty() {
        character.failure = true;
        character.error =
            Some("Showing Static Fiote: the lynx scene has no animation player.".into());
    }
}

fn animate(world: &mut World, active: bool) {
    let now = Instant::now();
    let period = world
        .resource::<Character>()
        .scenes
        .iter()
        .map(|scene| scene.behavior.activity.frame_period())
        .min()
        .unwrap_or(FRAME);
    let (delta, due) = {
        let mut character = world.resource_mut::<Character>();
        let delta = if active {
            character
                .last
                .replace(now)
                .map_or(0.0, |last| now.duration_since(last).as_secs_f32())
        } else {
            character.last = None;
            0.0
        };
        let due = active && character.alarm.is_none_or(|alarm| now >= alarm);
        if due {
            character.alarm = Some(now + period);
        }
        (delta, due)
    };
    let mut scenes = std::mem::take(&mut world.resource_mut::<Character>().scenes);
    if scenes.is_empty() {
        if active && due {
            if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                wake.after(period);
            }
        }
        return;
    }
    for scene in &mut scenes {
        scene.running = active;
        let render_due = active
            && scene.rendered.is_none_or(|last| {
                now.duration_since(last) >= scene.behavior.activity.frame_period()
            });
        if render_due {
            scene.rendered = Some(now);
        }
        if let Some(mut camera) = world.get_mut::<Camera>(scene.camera) {
            camera.is_active = render_due;
        }
        let duration = scene.clips[scene.behavior.activity.clip()].1;
        let changed = active && !scene.players.is_empty() && scene.behavior.tick(delta, duration);
        for entity in &scene.players {
            if let Some(mut player) = world.get_mut::<AnimationPlayer>(*entity) {
                if active && delta > 0.0 {
                    player.resume_all();
                } else {
                    player.pause_all();
                }
            }
            if changed {
                let (index, _) = scene.clips[scene.behavior.activity.clip()];
                let mut transitions = world.get_mut::<AnimationTransitions>(*entity).unwrap();
                let mut transitions = std::mem::take(&mut *transitions);
                if let Some(mut player) = world.get_mut::<AnimationPlayer>(*entity) {
                    let animation =
                        transitions.play(&mut player, index, Duration::from_millis(180));
                    if scene.behavior.activity.loops() {
                        animation.repeat();
                    }
                }
                *world.get_mut::<AnimationTransitions>(*entity).unwrap() = transitions;
            }
        }
        if let Some(mut transform) = world.get_mut::<Transform>(scene.model) {
            transform.translation =
                Vec3::new(scene.behavior.position.x, 0.0, scene.behavior.position.y);
            transform.rotation = Quat::from_rotation_y(scene.behavior.yaw);
        }
    }
    let next_period = scenes
        .iter()
        .map(|scene| scene.behavior.activity.frame_period())
        .min()
        .unwrap_or(FRAME);
    world.resource_mut::<Character>().scenes = scenes;
    if active && (due || next_period < period) {
        world.resource_mut::<Character>().alarm = Some(now + next_period);
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.after(next_period);
        }
    }
}

fn refresh_status(world: &mut World) {
    let character = world.resource::<Character>();
    let settings = character.preferences;
    let state = if settings.reduced_motion {
        "Reduced motion: on"
    } else if settings.paused {
        "Animation paused"
    } else if settings.appearance == Appearance::Static {
        "Static Fiote"
    } else if character
        .scenes
        .iter()
        .any(|scene| !scene.players.is_empty())
    {
        "Animated Fiote"
    } else {
        "Animated Fiote loads when its bubble is visible"
    };
    let text = format!(
        "{state} · Selected: {} · Reduced motion: {}{}",
        if settings.appearance == Appearance::Static {
            "Static"
        } else {
            "Animated"
        },
        if settings.reduced_motion { "on" } else { "off" },
        character
            .error
            .as_ref()
            .map_or(String::new(), |error| format!(" · {error}"))
    );
    let labels: Vec<Entity> = world
        .query::<&SettingsPanel>()
        .iter(world)
        .map(|panel| panel.status)
        .collect();
    for label in labels {
        if let Some(mut value) = world.get_mut::<Text>(label) {
            if value.0 != text {
                value.0 = text.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests;
