use bevy::{
    asset::{AssetId, RenderAssetUsages},
    camera::{RenderTarget, visibility::RenderLayers},
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_resource::{
            Extent3d, PipelineCache, SpecializedRenderPipelines, TextureDimension, TextureFormat,
            TextureUsages,
        },
        view::ExtractedView,
    },
    ui_render::UiMaterialPipeline,
};
use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType},
    shader::{Shader, ValidateShader},
    ui::RelativeCursorPosition,
    ui_render::ui_material::UiMaterialKey,
};
use nucleus::shader_effect::Effect;
use std::time::{Duration, Instant};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
};

pub(crate) mod extent;
mod scheduling;
mod snapshot;
pub(crate) mod spatial;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderShaderEffects;

pub(crate) const BALL: &str = include_str!("shader_ball.wgsl");

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub(crate) struct ShaderInputs {
    size: Vec2,
    time: f32,
    delta: f32,
    mouse: Vec4,
    quantity: f32,
    quantity_present: u32,
    task_state: u32,
    effect_strength: f32,
    motion_allowed: u32,
    progress: f32,
    progress_present: u32,
    padding: u32,
    numeric0: Vec4,
    numeric1: Vec4,
    present0: UVec4,
    present1: UVec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(ShaderKey)]
struct ShaderMaterial {
    #[uniform(0)]
    inputs: ShaderInputs,
    shader: Handle<Shader>,
    offscreen: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ShaderKey(Handle<Shader>, bool);

impl From<&ShaderMaterial> for ShaderKey {
    fn from(material: &ShaderMaterial) -> Self {
        Self(material.shader.clone(), material.offscreen)
    }
}

impl UiMaterial for ShaderMaterial {
    fn specialize(descriptor: &mut RenderPipelineDescriptor, key: UiMaterialKey<Self>) {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = key.bind_group_data.0;
            if key.bind_group_data.1 {
                for target in fragment.targets.iter_mut().flatten() {
                    target.blend = None;
                }
            }
        }
    }
}

#[derive(Component)]
pub(crate) struct Preview {
    source: String,
    pending: Option<Instant>,
    started: Instant,
    surface: Entity,
    label: Entity,
    material: Option<Handle<ShaderMaterial>>,
}

pub(super) fn install(app: &mut App) {
    if !app.is_plugin_added::<crate::native::topology::surface_budget::BudgetPlugin>() {
        app.add_plugins(crate::native::topology::surface_budget::BudgetPlugin);
    }
    app.add_systems(
        PostUpdate,
        extent::measure
            .after(bevy::ui::UiSystems::PostLayout)
            .after(bevy::transform::TransformSystems::Propagate),
    );
    spatial::install(app);
    if app.world().contains_resource::<AssetServer>() {
        app.add_plugins(UiMaterialPlugin::<ShaderMaterial>::default());
    }
    app.add_systems(
        Update,
        update
            .in_set(RenderShaderEffects)
            .after(super::sync_editors)
            .after(crate::native::shader_effects::UpdateShaderEffects),
    );
    let status = PipelineStatus::default();
    app.insert_resource(status.clone())
        .init_resource::<EffectAssets>();
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(status)
            .init_resource::<PipelineRequests>()
            .add_systems(ExtractSchedule, extract_requests)
            .add_systems(Render, prepare_candidates.in_set(RenderSystems::Queue));
    }
}

pub(super) fn spawn(world: &mut World, parent: Entity, source: &str, old: Option<Entity>) {
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_millis(200));
    }
    if let Some(old) = old {
        let mut preview = world.get_mut::<Preview>(old).unwrap();
        if preview.source != source {
            preview.source = source.into();
            preview.pending = Some(Instant::now());
        }
        world.entity_mut(old).insert(ChildOf(parent));
        return;
    }
    let owner = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let surface = world
        .spawn((
            Node {
                width: percent(100),
                height: px(280),
                flex_shrink: 0.0,
                ..default()
            },
            RelativeCursorPosition::default(),
            ChildOf(owner),
        ))
        .id();
    let label = crate::native::edit_mode::label(world, owner, "Compiling WGSL…", 12.0);
    world.entity_mut(owner).insert(Preview {
        source: source.into(),
        pending: Some(Instant::now()),
        started: Instant::now(),
        surface,
        label,
        material: None,
    });
}

pub(crate) fn compile(source: &str) -> Result<String, String> {
    nucleus::shader_effect::compile(source)
}

fn visible(world: &World, entity: Entity) -> bool {
    if world
        .get::<ComputedNode>(entity)
        .is_none_or(ComputedNode::is_empty)
        || world
            .get::<InheritedVisibility>(entity)
            .is_some_and(|visible| !visible.get())
    {
        return false;
    }
    if let (Some(node), Some(transform), Some(target)) = (
        world.get::<ComputedNode>(entity),
        world.get::<UiGlobalTransform>(entity),
        world.get::<ComputedUiRenderTargetInfo>(entity),
    ) {
        let bounds = Rect::from_center_size(transform.affine().translation, node.size());
        let mut clip = Rect::from_corners(Vec2::ZERO, target.physical_size().as_vec2());
        if let Some(calculated) = world.get::<CalculatedClip>(entity) {
            clip = clip.intersect(calculated.clip);
        }
        if bounds.intersect(clip).is_empty() {
            return false;
        }
    }
    let mut cursor = Some(entity);
    while let Some(entity) = cursor {
        if world
            .get::<crate::native::topology::presentation::Surface>(entity)
            .is_some_and(|surface| !surface.visible)
        {
            return false;
        }
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
            || world.get::<Visibility>(entity) == Some(&Visibility::Hidden)
        {
            return false;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    true
}

fn update(world: &mut World) {
    let previews: Vec<_> = world
        .query_filtered::<Entity, (With<Preview>, Without<EffectPreview>)>()
        .iter(world)
        .collect();
    let mut active = false;
    for entity in previews {
        let (surface, label) = {
            let preview = world.get::<Preview>(entity).unwrap();
            (preview.surface, preview.label)
        };
        let disabled = crate::native::shader_effects::disabled(world);
        world.get_mut::<Node>(surface).unwrap().display = if disabled {
            Display::None
        } else {
            Display::Flex
        };
        if !visible(world, surface) {
            continue;
        }
        let preview = world.get::<Preview>(entity).unwrap();
        active |= preview.pending.is_some()
            || (crate::native::shader_effects::motion_allowed(world) && preview.material.is_some());
        if preview
            .pending
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(200))
        {
            let result = compile(&preview.source);
            let current = preview.material.clone();
            world.get_mut::<Preview>(entity).unwrap().pending = None;
            let result = result.and_then(|text| {
                if !world.contains_resource::<Assets<Shader>>()
                    || !world.contains_resource::<Assets<ShaderMaterial>>()
                {
                    return Err("Shader rendering needs the GPU renderer.".into());
                }
                let mut shader =
                    Shader::from_wgsl(text, format!("description-{}.wgsl", entity.to_bits()));
                shader.validate_shader = ValidateShader::Enabled;
                if let Some(material) = current {
                    let handle = world
                        .resource::<Assets<ShaderMaterial>>()
                        .get(&material)
                        .unwrap()
                        .shader
                        .clone();
                    world
                        .resource_mut::<Assets<Shader>>()
                        .insert(handle.id(), shader)
                        .map_err(|error| error.to_string())?;
                } else {
                    let shader = world.resource_mut::<Assets<Shader>>().add(shader);
                    let material =
                        world
                            .resource_mut::<Assets<ShaderMaterial>>()
                            .add(ShaderMaterial {
                                inputs: default(),
                                shader,
                                offscreen: false,
                            });
                    world
                        .entity_mut(surface)
                        .insert(MaterialNode(material.clone()));
                    world.get_mut::<Preview>(entity).unwrap().material = Some(material);
                }
                Ok(())
            });
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = result
                    .err()
                    .map_or_else(String::new, |error| format!("WGSL: {error}"));
            }
        }
        let preview = world.get::<Preview>(entity).unwrap();
        let Some(material) = preview.material.clone() else {
            continue;
        };
        let size = world.get::<ComputedNode>(surface).unwrap().size();
        let cursor = world.get::<RelativeCursorPosition>(surface);
        let inside = cursor.is_some_and(RelativeCursorPosition::cursor_over);
        let pointer = cursor
            .and_then(|cursor| cursor.normalized)
            .map_or(Vec2::ZERO, |position| position + Vec2::splat(0.5))
            * size;
        let pressed = inside
            && world
                .get_resource::<ButtonInput<MouseButton>>()
                .is_some_and(|buttons| buttons.pressed(MouseButton::Left));
        let inputs = ShaderInputs {
            size,
            time: if crate::native::shader_effects::motion_allowed(world) {
                preview.started.elapsed().as_secs_f32()
            } else {
                0.0
            },
            delta: if crate::native::shader_effects::motion_allowed(world) {
                world.get_resource::<Time>().map_or(0.0, Time::delta_secs)
            } else {
                0.0
            },
            effect_strength: 1.0,
            motion_allowed: u32::from(crate::native::shader_effects::motion_allowed(world)),
            mouse: Vec4::new(
                pointer.x,
                pointer.y,
                u8::from(pressed) as f32,
                u8::from(inside) as f32,
            ),
            ..default()
        };
        if let Some(mut material) = world
            .resource_mut::<Assets<ShaderMaterial>>()
            .get_mut(&material)
        {
            material.inputs = inputs;
        }
    }
    if active && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

#[derive(Resource, Clone, Default)]
struct PipelineStatus(Arc<Mutex<HashMap<AssetId<Shader>, Result<(), String>>>>);

#[derive(Resource, Default)]
struct PipelineRequests(Vec<Handle<Shader>>);

#[derive(Resource, Default)]
struct EffectAssets {
    frame: u64,
    shaders: HashMap<String, Handle<Shader>>,
    available: Vec<Handle<Shader>>,
    owned: HashMap<
        Entity,
        (
            Entity,
            Entity,
            Handle<Image>,
            Option<Handle<ShaderMaterial>>,
        ),
    >,
}

fn extract_requests(
    mut requests: ResMut<PipelineRequests>,
    previews: Extract<Query<&EffectPreview>>,
) {
    requests.0 = previews
        .iter()
        .filter(|preview| {
            preview.observed.as_ref().is_some_and(|effect| {
                matches!(effect.mode, nucleus::shader_effect::spatial::Mode::Surface)
            })
        })
        .flat_map(|preview| preview.candidates.values().cloned())
        .collect();
}

fn prepare_candidates(
    requests: Res<PipelineRequests>,
    status: Res<PipelineStatus>,
    pipeline: Option<Res<UiMaterialPipeline<ShaderMaterial>>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<UiMaterialPipeline<ShaderMaterial>>>,
    cache: Res<PipelineCache>,
    views: Query<&ExtractedView, With<bevy::ui_render::UiViewTarget>>,
) {
    let Some(pipeline) = pipeline else { return };
    let formats: HashSet<_> = views
        .iter()
        .map(|view| view.target_format)
        .filter(|format| !format.is_depth_stencil_format())
        .collect();
    if formats.is_empty() {
        return;
    }
    for shader in &requests.0 {
        let mut ready = true;
        let mut error = None;
        for format in &formats {
            let id = pipelines.specialize(
                &cache,
                &pipeline,
                UiMaterialKey {
                    target_format: *format,
                    bind_group_data: ShaderKey(shader.clone(), true),
                },
            );
            match cache.get_render_pipeline_state(id) {
                bevy::render::render_resource::CachedPipelineState::Ok(_) => {}
                bevy::render::render_resource::CachedPipelineState::Err(cause) => {
                    error = Some(cause.to_string());
                }
                _ => ready = false,
            }
        }
        let mut results = status.0.lock().unwrap();
        if let Some(error) = error {
            results.insert(shader.id(), Err(error));
        } else if ready {
            results.insert(shader.id(), Ok(()));
        }
    }
}

#[derive(Component)]
pub(crate) struct EffectPreview {
    pub(crate) effect: Effect,
    working: Option<Effect>,
    pub(crate) quantity: Option<String>,
    pub(crate) numeric: nucleus::shader_effect::bindings::NumericValues,
    pub(crate) numeric_bindings: Vec<nucleus::shader_effect::bindings::NumericBinding>,
    pub(crate) strength: f32,
    pub(crate) motion: bool,
    pub(crate) frozen: bool,
    pub(crate) disabled: bool,
    pub(crate) error: String,
    observed: Option<Effect>,
    candidates: BTreeMap<String, Handle<Shader>>,
    programs: BTreeMap<String, Handle<Shader>>,
    render_surface: Entity,
    camera: Entity,
    pub(crate) ready: bool,
    pending: Option<Instant>,
    pub(crate) priority: bool,
    painted: u8,
    last_served: u64,
    last_quantity: Option<String>,
    last_numeric: nucleus::shader_effect::bindings::NumericValues,
    snapshot: snapshot::Snapshot,
    budget_blocked: bool,
}

impl EffectPreview {
    fn complete_frame(&mut self, program: String, inputs: ShaderInputs) {
        self.painted = self.painted.saturating_add(1);
        self.snapshot.painted(program, inputs);
        self.snapshot.synchronize(self.frozen, self.painted);
    }
}

pub(crate) fn effect(
    world: &mut World,
    parent: Entity,
    definition: Effect,
    quantity: Option<String>,
) -> Entity {
    effect_with_priority(world, parent, definition, quantity, false)
}

pub(crate) fn effect_with_priority(
    world: &mut World,
    parent: Entity,
    definition: Effect,
    quantity: Option<String>,
    priority: bool,
) -> Entity {
    spawn(world, parent, nucleus::shader_effect::CALM, None);
    let owner = *world.get::<Children>(parent).unwrap().last().unwrap();
    world.get_mut::<Preview>(owner).unwrap().pending = None;
    let surface = world.get::<Preview>(owner).unwrap().surface;
    world.init_resource::<EffectAssets>();
    let ordinary = world
        .resource::<EffectAssets>()
        .owned
        .keys()
        .filter(|entity| {
            world
                .get::<EffectPreview>(**entity)
                .is_none_or(|preview| !preview.priority)
        })
        .count();
    let budget_blocked = world.contains_resource::<Assets<Image>>()
        && (world.resource::<EffectAssets>().owned.len() >= 64 || (!priority && ordinary >= 56));
    let (camera, render_surface) = if world.contains_resource::<Assets<Image>>() && !budget_blocked
    {
        let mut image = Image::new_uninit(
            Extent3d {
                width: 1,
                height: 1,
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
        let camera = world
            .spawn((
                Camera2d,
                Camera {
                    order: -5,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                Msaa::Off,
                RenderTarget::Image(image.clone().into()),
                crate::native::capture::render::TargetSource(owner),
                crate::native::topology::surface_budget::residency::Target {
                    image: image.id(),
                    release: false,
                },
                RenderLayers::layer(30),
            ))
            .id();
        let render_surface = world
            .spawn((
                Node {
                    width: px(1),
                    height: px(1),
                    ..default()
                },
                UiTargetCamera(camera),
                crate::native::capture::render::TargetSource(owner),
                bevy::picking::Pickable::IGNORE,
            ))
            .id();
        world.entity_mut(surface).insert((
            ImageNode::new(image.clone()).with_mode(bevy::ui::widget::NodeImageMode::Stretch),
            bevy::picking::Pickable::IGNORE,
        ));
        world.init_resource::<EffectAssets>();
        world
            .resource_mut::<EffectAssets>()
            .owned
            .insert(owner, (camera, render_surface, image, None));
        (camera, render_surface)
    } else {
        (Entity::PLACEHOLDER, surface)
    };
    world.entity_mut(owner).insert(EffectPreview {
        effect: definition,
        working: None,
        quantity,
        numeric: default(),
        numeric_bindings: vec![],
        strength: 1.0,
        motion: true,
        frozen: false,
        disabled: false,
        error: String::new(),
        observed: None,
        candidates: BTreeMap::new(),
        programs: BTreeMap::new(),
        render_surface,
        camera,
        ready: false,
        pending: None,
        priority,
        painted: 0,
        last_served: 0,
        last_quantity: None,
        last_numeric: default(),
        snapshot: default(),
        budget_blocked,
    });
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_millis(30));
    }
    owner
}

pub(crate) fn working(world: &World, entity: Entity) -> Option<&Effect> {
    world.get::<EffectPreview>(entity)?.working.as_ref()
}

pub(crate) fn lighting_inputs(
    world: &World,
    entity: Entity,
) -> Option<nucleus::shader_effect::ResolvedInputs> {
    let state = world.get::<EffectPreview>(entity)?;
    if state.disabled {
        return None;
    }
    let quantity = if state.snapshot.frozen() || state.frozen && state.painted >= 3 {
        &state.last_quantity
    } else {
        &state.quantity
    };
    Some(state.working.as_ref()?.inputs.resolve(quantity.as_deref()))
}

pub(crate) fn output(world: &World, entity: Entity) -> Entity {
    world.get::<Preview>(entity).unwrap().surface
}

pub(crate) fn background_output(world: &mut World, entity: Entity) -> Entity {
    let preview = world.get::<Preview>(entity).unwrap();
    let (surface, label) = (preview.surface, preview.label);
    world.get_mut::<Node>(label).unwrap().display = Display::None;
    surface
}

pub(crate) fn effect_image(world: &World, entity: Entity) -> Option<Handle<Image>> {
    let state = world.get::<EffectPreview>(entity)?;
    if state.disabled || state.budget_blocked || state.working.is_none() {
        return None;
    }
    world
        .get_resource::<EffectAssets>()?
        .owned
        .get(&entity)
        .map(|owned| owned.2.clone())
}

pub(crate) fn update_effects(world: &mut World) {
    world.init_resource::<EffectAssets>();
    let frame = {
        let mut assets = world.resource_mut::<EffectAssets>();
        assets.frame = assets.frame.saturating_add(1);
        assets.frame
    };
    let mut entities: Vec<_> = world
        .query_filtered::<Entity, With<EffectPreview>>()
        .iter(world)
        .collect();
    scheduling::order(world, &mut entities, frame);
    let alive: HashSet<_> = entities.iter().copied().collect();
    spatial::prune(world, &alive);
    let dead: Vec<_> = world
        .resource::<EffectAssets>()
        .owned
        .keys()
        .filter(|entity| !alive.contains(entity))
        .copied()
        .collect();
    for entity in dead {
        if let Some((camera, surface, image, material)) =
            world.resource_mut::<EffectAssets>().owned.remove(&entity)
        {
            world.despawn(camera);
            world.despawn(surface);
            world.resource_mut::<Assets<Image>>().remove(image.id());
            if let Some(material) = material {
                world
                    .resource_mut::<Assets<ShaderMaterial>>()
                    .remove(material.id());
            }
        }
    }
    let mut animated = 0;
    let mut rendered = 0;
    for entity in entities {
        let (surface, label, started) = {
            let preview = world.get::<Preview>(entity).unwrap();
            (preview.surface, preview.label, preview.started)
        };
        {
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            let freeze = preview.frozen && preview.working.is_some();
            let painted = preview.painted;
            preview.snapshot.synchronize(freeze, painted);
        }
        let (definition, changed, disabled, camera, render_surface) = {
            let preview = world.get::<EffectPreview>(entity).unwrap();
            let changed = preview.observed.as_ref() != Some(&preview.effect);
            (
                (changed || preview.pending.is_some() || !preview.candidates.is_empty())
                    .then(|| preview.effect.clone()),
                changed,
                preview.disabled,
                preview.camera,
                preview.render_surface,
            )
        };
        if world.get::<EffectPreview>(entity).unwrap().budget_blocked {
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = "Effect view budget reached. Close unused effect views.".into();
            }
            continue;
        }
        let frozen = {
            let preview = world.get::<EffectPreview>(entity).unwrap();
            preview.snapshot.frozen()
        };
        let repainting = frozen && world.get::<EffectPreview>(entity).unwrap().painted < 3;
        if !frozen {
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            if preview.last_quantity != preview.quantity || preview.last_numeric != preview.numeric
            {
                preview.last_quantity = preview.quantity.clone();
                preview.last_numeric = preview.numeric.clone();
                preview.painted = 0;
            }
        }
        let disabled = disabled || crate::native::shader_effects::disabled(world);
        let display = if disabled {
            Display::None
        } else {
            Display::Flex
        };
        let mut display_changed = false;
        if let Some(mut node) = world.get_mut::<Node>(surface) {
            display_changed = node.display != display;
            if display_changed {
                node.display = display;
            }
        }
        if display_changed
            && !disabled
            && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
        {
            wake.after(Duration::from_millis(30));
        }
        let showing = !disabled
            && visible(world, surface)
            && world
                .get::<extent::Raster>(entity)
                .is_none_or(|raster| raster.visible);
        let eligible = scheduling::eligible(world.get::<EffectPreview>(entity).unwrap(), changed);
        let render = showing && eligible && (!frozen || repainting) && rendered < scheduling::SLOTS;
        if render {
            rendered += 1;
            world.get_mut::<EffectPreview>(entity).unwrap().last_served = frame;
        }
        if let Some(mut target) =
            world.get_mut::<crate::native::topology::surface_budget::residency::Target>(camera)
        {
            target.release = !showing || frozen && !repainting || !eligible;
        }
        if let Some(mut camera) = world.get_mut::<Camera>(camera) {
            camera.is_active = render;
        }
        if disabled {
            let message = if world
                .get::<EffectPreview>(entity)
                .unwrap()
                .quantity
                .is_none()
            {
                "Quantity unavailable; effect inactive"
            } else {
                "Effect disabled"
            };
            if let Some(mut text) = world.get_mut::<Text>(label)
                && text.0 != message
            {
                text.0 = message.into();
            }
        }
        if frozen && showing {
            let message = extent::label(world, entity, "Snapshot frozen".into());
            if world.get::<EffectPreview>(entity).unwrap().error.is_empty()
                && let Some(mut text) = world.get_mut::<Text>(label)
            {
                if text.0 != message {
                    text.0 = message;
                }
            }
        }
        if !showing || frozen && !repainting {
            continue;
        }
        if changed && !frozen {
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            preview.observed = definition.clone();
            preview.pending = Some(Instant::now());
            preview.ready = false;
            preview.painted = 0;
            preview.candidates.clear();
        }
        if !frozen
            && world
                .get::<EffectPreview>(entity)
                .unwrap()
                .pending
                .is_some_and(|instant| instant.elapsed() >= Duration::from_millis(200))
        {
            let definition = definition.as_ref().unwrap();
            let result = definition.validate();
            let mut candidates = BTreeMap::new();
            let result = result.and_then(|()| {
                match &definition.mode {
                    nucleus::shader_effect::spatial::Mode::Mesh(_)
                    | nucleus::shader_effect::spatial::Mode::Volume(_) => {
                        candidates = spatial::candidate(world, entity, definition)?;
                        return Ok(());
                    }
                    nucleus::shader_effect::spatial::Mode::Surface => {}
                }
                if !world.contains_resource::<Assets<Shader>>()
                    || !world.contains_resource::<Assets<ShaderMaterial>>()
                {
                    return Err("Shader rendering needs the GPU renderer.".into());
                }
                for (name, source) in &definition.programs {
                    let existing = world
                        .resource::<EffectAssets>()
                        .shaders
                        .get(source)
                        .cloned();
                    let shader = if let Some(existing) = existing {
                        existing
                    } else {
                        let cache_limit = if world.get::<EffectPreview>(entity).unwrap().priority {
                            72
                        } else {
                            64
                        };
                        if world.resource::<EffectAssets>().shaders.len() >= cache_limit {
                            return Err(
                                "Shader cache is full. Close unused effect views before editing."
                                    .into(),
                            );
                        }
                        let mut shader = Shader::from_wgsl(compile(source)?, "record-effect.wgsl");
                        shader.validate_shader = ValidateShader::Enabled;
                        let available = world.resource_mut::<EffectAssets>().available.pop();
                        let handle = if let Some(handle) = available {
                            world
                                .resource_mut::<Assets<Shader>>()
                                .insert(handle.id(), shader)
                                .map_err(|error| error.to_string())?;
                            if let Some(status) = world.get_resource::<PipelineStatus>() {
                                status.0.lock().unwrap().remove(&handle.id());
                            }
                            handle
                        } else {
                            world.resource_mut::<Assets<Shader>>().add(shader)
                        };
                        world
                            .resource_mut::<EffectAssets>()
                            .shaders
                            .insert(source.clone(), handle.clone());
                        handle
                    };
                    candidates.insert(name.clone(), shader);
                }
                Ok(())
            });
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            preview.observed = Some(definition.clone());
            preview.pending = None;
            preview.ready = false;
            preview.error = result.err().unwrap_or_default();
            preview.candidates = if preview.error.is_empty() {
                candidates
            } else {
                BTreeMap::new()
            };
        }
        let candidates = world
            .get::<EffectPreview>(entity)
            .unwrap()
            .candidates
            .clone();
        if !frozen
            && !candidates.is_empty()
            && world.get::<EffectPreview>(entity).unwrap().error.is_empty()
        {
            let statuses = world
                .get_resource::<PipelineStatus>()
                .map(|status| status.0.lock().unwrap().clone())
                .unwrap_or_default();
            let renderer =
                world.contains_resource::<PipelineStatus>() && camera != Entity::PLACEHOLDER;
            let mut error = candidates.values().find_map(|shader| {
                statuses
                    .get(&shader.id())
                    .and_then(|result| result.as_ref().err())
                    .cloned()
            });
            let mut ready = !renderer
                || candidates
                    .values()
                    .all(|shader| statuses.get(&shader.id()).is_some_and(Result::is_ok));
            let mesh = definition.as_ref().is_some_and(|effect| {
                !matches!(effect.mode, nucleus::shader_effect::spatial::Mode::Surface)
            });
            if mesh {
                match spatial::ready(world, entity) {
                    Ok(value) => {
                        ready = value;
                        error = None;
                    }
                    Err(cause) => {
                        ready = false;
                        error = Some(cause);
                    }
                }
            }
            ready = ready && render;
            if ready && error.is_none() {
                if mesh {
                    spatial::commit(world, entity);
                } else {
                    spatial::surface(world, entity);
                }
            }
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            if let Some(error) = error {
                preview.error = format!("GPU pipeline: {error}");
                preview.candidates.clear();
            } else if ready {
                preview.working = definition.clone();
                preview.programs = candidates;
                preview.candidates.clear();
                preview.ready = true;
                preview.painted = 0;
            }
        }
        let preview = world.get::<EffectPreview>(entity).unwrap();
        let message = if !preview.error.is_empty() {
            format!("WGSL: {}", preview.error)
        } else if frozen {
            "Snapshot frozen".into()
        } else if !preview.ready {
            "Preparing WGSL…".into()
        } else if preview.quantity.is_none() {
            "Quantity unavailable; showing fallback".into()
        } else if let Some(working) = &preview.working
            && (working.inputs.task.is_some() || working.inputs.progress.is_some())
        {
            working.inputs.resolve(preview.quantity.as_deref()).label()
        } else {
            String::new()
        };
        let message = extent::label(world, entity, message);
        if let Some(mut text) = world.get_mut::<Text>(label) {
            if text.0 != message {
                text.0 = message;
            }
        }
        if !render {
            continue;
        }
        let preview = world.get::<EffectPreview>(entity).unwrap();
        let Some(working) = &preview.working else {
            continue;
        };
        let numeric = if preview.numeric_bindings == working.bindings {
            preview.numeric.clone()
        } else {
            default()
        };
        let selected = preview
            .snapshot
            .program()
            .unwrap_or_else(|| working.selected_with_inputs(preview.quantity.as_deref(), &numeric));
        let Some(shader) = preview.programs.get(selected).cloned() else {
            continue;
        };
        let quantity = nucleus::shader_effect::gpu_quantity(preview.quantity.as_deref());
        let mapped = working.inputs.resolve(preview.quantity.as_deref());
        let motion = !frozen
            && preview.motion
            && crate::native::shader_effects::motion_allowed(world)
            && render
            && animated < scheduling::SLOTS;
        if motion {
            animated += 1;
        }
        let strength = preview.strength;
        let node = world.get::<ComputedNode>(surface).unwrap();
        let size = world.get::<extent::Raster>(entity).map_or_else(
            || node.size() * node.inverse_scale_factor(),
            |raster| raster.pixels.as_vec2(),
        );
        let cursor = world.get::<RelativeCursorPosition>(surface);
        let inside = cursor.is_some_and(RelativeCursorPosition::cursor_over);
        let pointer = cursor
            .and_then(|cursor| cursor.normalized)
            .map_or(Vec2::ZERO, |position| position + Vec2::splat(0.5))
            * size;
        let pressed = inside
            && world
                .get_resource::<ButtonInput<MouseButton>>()
                .is_some_and(|buttons| buttons.pressed(MouseButton::Left));
        let (values, present) = numeric.gpu();
        let inputs = preview
            .snapshot
            .inputs(size)
            .unwrap_or_else(|| ShaderInputs {
                numeric0: Vec4::from_array(values[..4].try_into().unwrap()),
                numeric1: Vec4::from_array(values[4..].try_into().unwrap()),
                present0: UVec4::from_array(present[..4].try_into().unwrap()),
                present1: UVec4::from_array(present[4..].try_into().unwrap()),
                size,
                time: if motion {
                    started.elapsed().as_secs_f32()
                } else {
                    0.0
                },
                delta: if motion {
                    world.get_resource::<Time>().map_or(0.0, Time::delta_secs)
                } else {
                    0.0
                },
                quantity: quantity.unwrap_or(0.0),
                quantity_present: u32::from(quantity.is_some()),
                task_state: mapped.task as u32,
                progress: mapped.progress.unwrap_or(0.0),
                progress_present: u32::from(mapped.progress.is_some()),
                effect_strength: strength.clamp(0.0, 1.0),
                motion_allowed: u32::from(motion),
                mouse: if motion {
                    Vec4::new(
                        pointer.x,
                        pointer.y,
                        u8::from(pressed) as f32,
                        u8::from(inside) as f32,
                    )
                } else {
                    Vec4::ZERO
                },
                ..default()
            });
        let program = selected.to_owned();
        if !matches!(working.mode, nucleus::shader_effect::spatial::Mode::Surface) {
            spatial::draw(world, entity, &program, inputs);
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            preview.complete_frame(program, inputs);
            continue;
        }
        let material = world.get::<Preview>(entity).unwrap().material.clone();
        if let Some(material) = material {
            let changed = world
                .resource::<Assets<ShaderMaterial>>()
                .get(&material)
                .is_some_and(|value| value.shader != shader || value.inputs != inputs);
            if changed
                && let Some(mut value) = world
                    .resource_mut::<Assets<ShaderMaterial>>()
                    .get_mut(&material)
            {
                value.shader = shader;
                value.inputs = inputs;
            }
        } else {
            let material = world
                .resource_mut::<Assets<ShaderMaterial>>()
                .add(ShaderMaterial {
                    shader,
                    inputs,
                    offscreen: camera != Entity::PLACEHOLDER,
                });
            world
                .entity_mut(render_surface)
                .insert(MaterialNode(material.clone()));
            world.get_mut::<Preview>(entity).unwrap().material = Some(material.clone());
            if let Some(owned) = world.resource_mut::<EffectAssets>().owned.get_mut(&entity) {
                owned.3 = Some(material);
            }
        }
        if render {
            let mut preview = world.get_mut::<EffectPreview>(entity).unwrap();
            preview.complete_frame(program, inputs);
        }
    }
    let mut used: HashSet<_> = world
        .query::<&EffectPreview>()
        .iter(world)
        .flat_map(|preview| {
            preview
                .programs
                .values()
                .chain(preview.candidates.values())
                .map(|shader| shader.id())
        })
        .collect();
    used.extend(spatial::shaders(world));
    let removed: Vec<_> = world
        .resource::<EffectAssets>()
        .shaders
        .iter()
        .filter(|(_, shader)| !used.contains(&shader.id()))
        .map(|(source, shader)| (source.clone(), shader.id()))
        .collect();
    for (source, id) in removed {
        if let Some(handle) = world.resource_mut::<EffectAssets>().shaders.remove(&source) {
            if let Some(handle) = spatial::recycle(world, &source, handle) {
                world.resource_mut::<EffectAssets>().available.push(handle);
            }
        }
        if let Some(status) = world.get_resource::<PipelineStatus>() {
            status.0.lock().unwrap().remove(&id);
        }
    }
    if animated > 0
        && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
    {
        wake.ring();
    }
    let preparing = world.query::<&EffectPreview>().iter(world).any(|preview| {
        !preview.disabled
            && (!preview.frozen || preview.painted < 3)
            && (preview.pending.is_some()
                || !preview.candidates.is_empty()
                || preview.working.is_some() && preview.painted < 3)
    });
    if preparing && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_millis(30));
    }
}

#[cfg(test)]
pub(crate) fn prepare_test_previews(world: &mut World) {
    world.init_resource::<Assets<Shader>>();
    world.init_resource::<Assets<ShaderMaterial>>();
    let surfaces: Vec<_> = world
        .query::<&Preview>()
        .iter(world)
        .map(|preview| preview.surface)
        .collect();
    for surface in surfaces {
        world
            .entity_mut(surface)
            .remove::<ComputedUiRenderTargetInfo>()
            .insert(InheritedVisibility::VISIBLE);
        if let Some(mut node) = world.get_mut::<ComputedNode>(surface) {
            node.size = Vec2::splat(256.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Pixels(Vec<[u8; 4]>);

    #[derive(Resource, Default)]
    struct EdgePixels(Vec<[u8; 4]>);

    #[derive(Resource, Default)]
    struct GeneratedVertices(Vec<u8>);

    #[derive(Resource, Default)]
    struct WorldPixels(Vec<[u8; 4]>);

    fn collect_vertices(
        event: On<bevy::render::gpu_readback::ReadbackComplete>,
        mut vertices: ResMut<GeneratedVertices>,
    ) {
        vertices.0 = event.data.clone();
    }

    fn collect_pixels(
        event: On<bevy::render::gpu_readback::ReadbackComplete>,
        mut pixels: ResMut<Pixels>,
        mut edges: ResMut<EdgePixels>,
        readbacks: Query<&bevy::render::gpu_readback::Readback>,
        images: Res<Assets<Image>>,
    ) {
        let Ok(bevy::render::gpu_readback::Readback::Texture(handle)) = readbacks.get(event.entity)
        else {
            return;
        };
        let Some(image) = images.get(handle) else {
            return;
        };
        let size = image.size().as_usizevec2();
        let stride = (size.x * 4).div_ceil(256) * 256;
        if event.data.len() != stride * size.y {
            return;
        }
        let offset = size.y / 2 * stride + size.x / 2 * 4;
        if event.data.len() >= offset + 4 {
            pixels
                .0
                .push(event.data[offset..offset + 4].try_into().unwrap());
        }
        let edge = size.y / 2 * stride + (size.x * 9 / 16).min(size.x - 1) * 4;
        if event.data.len() >= edge + 4 {
            edges.0.push(event.data[edge..edge + 4].try_into().unwrap());
        }
    }

    #[test]
    fn headless_effect_pixels_switch_at_zero_and_surface_opacity_is_host_controlled() {
        use bevy::render::gpu_readback::Readback;
        let mut app = App::new();
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
        .init_resource::<crate::native::shader_effects::ShaderSettings>()
        .init_resource::<Pixels>();
        app.init_resource::<EdgePixels>();
        app.init_resource::<GeneratedVertices>();
        app.init_resource::<WorldPixels>();
        install(&mut app);
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.plugins_state() != bevy::app::PluginsState::Ready {
            assert!(Instant::now() < deadline, "GPU initialization timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        app.finish();
        app.cleanup();
        let scene = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                256,
                256,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Camera {
                    order: -1,
                    ..default()
                },
                RenderTarget::Image(scene.into()),
                Msaa::Off,
            ))
            .id();
        let parent = app
            .world_mut()
            .spawn((
                Node {
                    width: px(256),
                    height: px(256),
                    ..default()
                },
                UiTargetCamera(camera),
            ))
            .id();
        let preview = effect(
            app.world_mut(),
            parent,
            Effect::default(),
            Some("-1".into()),
        );
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .motion = false;
        let image = app.world().resource::<EffectAssets>().owned[&preview]
            .2
            .clone();
        let readback = app
            .world_mut()
            .spawn(Readback::texture(image.clone()))
            .observe(collect_pixels)
            .id();
        let wait = |app: &mut App, expected: fn([u8; 4]) -> bool| {
            app.world_mut().resource_mut::<Pixels>().0.clear();
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut stable = 0;
            loop {
                app.update();
                if let Some(pixel) = app.world_mut().resource_mut::<Pixels>().0.pop() {
                    stable = if expected(pixel) { stable + 1 } else { 0 };
                    if stable >= 4 {
                        break;
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "Shader pixels did not arrive: {:?}; WGSL error: {}; raster: {:?}; node: {:?}",
                    app.world().resource::<Pixels>().0.last(),
                    app.world().get::<EffectPreview>(preview).unwrap().error,
                    app.world().get::<extent::Raster>(preview),
                    app.world()
                        .get::<ComputedNode>(output(app.world(), preview))
                        .map(ComputedNode::size)
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            app.world_mut().resource_mut::<Pixels>().0.clear();
        };
        wait(&mut app, |pixel| {
            pixel[2] > 200 && pixel[2].saturating_sub(pixel[0]) > 10
        });
        for _ in 0..8 {
            app.update();
        }
        {
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            assert!(data.painted >= 3);
            data.frozen = true;
            data.quantity = Some("0".into());
            data.painted = 0;
        }
        app.world_mut().resource_mut::<Pixels>().0.clear();
        for _ in 0..10 {
            app.update();
            std::thread::sleep(Duration::from_millis(10));
        }
        let camera = app.world().get::<EffectPreview>(preview).unwrap().camera;
        assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
        let pixels = &app.world().resource::<Pixels>().0;
        assert!(!pixels.is_empty());
        assert!(
            pixels
                .iter()
                .all(|pixel| pixel[2] > pixel[0].saturating_add(10))
        );
        app.world_mut().resource_mut::<Pixels>().0.clear();
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .frozen = false;
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .quantity = Some("0".into());
        wait(&mut app, |pixel| {
            pixel[0] > 70 && pixel[0].abs_diff(pixel[2]) < 8
        });
        let surface = Effect { version: 1, mode: Default::default(), programs: [("solid".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }".into())].into(), rules: vec![], fallback: "solid".into(), inputs: Default::default(), bindings: vec![] };
        {
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect = surface;
            data.strength = 0.6;
        }
        wait(&mut app, |pixel| {
            pixel[0] > 240 && pixel[1] == 0 && pixel[3].abs_diff(153) < 3
        });
        {
            use nucleus::shader_effect::{InputMappings, QuantityRange};
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect = Effect {
                version: 1,
                mode: Default::default(),
                programs: [("mapped".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(f32(inputs.task_state) / 3.0, inputs.progress, f32(inputs.progress_present), 1.0); }".into())].into(),
                rules: vec![], fallback: "mapped".into(),
                inputs: InputMappings {
                    task: Some(QuantityRange { start: "0".into(), end: "10".into() }),
                    progress: Some(QuantityRange { start: "0".into(), end: "10".into() }),
                },
                bindings: vec![],
            };
            data.quantity = Some("5".into());
            data.strength = 1.0;
        }
        wait(&mut app, |pixel| {
            pixel[0].abs_diff(213) < 8
                && pixel[1].abs_diff(188) < 8
                && pixel[2] > 245
                && pixel[3] > 245
        });
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .quantity = None;
        wait(&mut app, |pixel| {
            pixel[0] < 5 && pixel[1] < 5 && pixel[2] < 5 && pixel[3] > 245
        });
        {
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect.programs = [("task".into(), nucleus::shader_effect::REWARD.into())].into();
            data.effect.fallback = "task".into();
            data.quantity = Some("0".into());
        }
        wait(&mut app, |pixel| {
            pixel[0] > 100 && pixel[0].abs_diff(pixel[2]) < 8
        });
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .quantity = Some("10".into());
        wait(&mut app, |pixel| {
            pixel[2] > pixel[0].saturating_add(30) && pixel[1] > 170
        });
        {
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect
                .programs
                .insert("task".into(), nucleus::shader_effect::REMINDER.into());
            data.quantity = Some("0".into());
        }
        wait(&mut app, |pixel| {
            pixel[0] > 200 && pixel[0].saturating_sub(pixel[2]) > 80
        });
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .quantity = Some("10".into());
        wait(&mut app, |pixel| {
            pixel[0] > 100 && pixel[0].abs_diff(pixel[2]) < 8
        });
        {
            use nucleus::shader_effect::bindings::{NumericBinding, NumericSource, NumericValues};
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                data.effect.programs =
                    [("task".into(), nucleus::shader_effect::CLEANING.into())].into();
                data.quantity = Some("0".into());
            }
            wait(&mut app, |pixel| pixel[0] < 60 && pixel[3] > 245);
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("5".into());
            wait(&mut app, |pixel| {
                pixel[0] < 60 && pixel[3].abs_diff(128) < 5
            });
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("10".into());
            wait(&mut app, |pixel| pixel[3] < 3);
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = None;
            wait(&mut app, |pixel| pixel[3] < 3);
            let bindings = vec![NumericBinding {
                slot: 0,
                label: "Value".into(),
                source: NumericSource::default(),
            }];
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect = Effect {
                programs: [("numbers".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(inputs.numeric0.x, f32(inputs.present0.x), 0.0, 1.0); }".into())].into(),
                rules: vec![], fallback: "numbers".into(), bindings: bindings.clone(), ..default()
            };
            data.numeric = NumericValues::resolve(&bindings, |_| Some("0.25".into())).unwrap();
            data.numeric_bindings = bindings;
        }
        wait(&mut app, |pixel| {
            pixel[0].abs_diff(137) < 8 && pixel[1] > 245 && pixel[2] < 8 && pixel[3] > 245
        });
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .numeric = default();
        wait(&mut app, |pixel| {
            pixel[0] < 5 && pixel[1] < 5 && pixel[2] < 5 && pixel[3] > 245
        });
        {
            use nucleus::shader_effect::spatial::{Deformation, MeshSettings, Mode};
            let mesh_count = app.world().resource::<Assets<Mesh>>().len();
            let mut mesh = Effect::default();
            mesh.mode = Mode::Mesh(MeshSettings { deformation: Some(Deformation {
                source: "fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex { let shrink = select(1.0, 0.2, inputs.quantity > 0.0); let scale = select(shrink, 1000.0, inputs.quantity == 2.0); return MeshVertex(vertex.position * scale, vertex.normal, vertex.uv); }".into(), max_displacement: 0.5,
            }), ..default() });
            mesh.programs = [("mesh".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(f32(point.local_position.z > 0.1), f32(length(point.normal) > 0.9 && (inputs.quantity != 2.0 || length(point.local_position) > 0.9)), f32(distance(point.local_position, point.world_position) < 0.01 && length(point.local_position) <= 1.001), 1.0); }".into())].into();
            mesh.rules.clear();
            mesh.fallback = "mesh".into();
            let target = app.world().get::<UiTargetCamera>(parent).unwrap().0;
            let sibling_parent = app
                .world_mut()
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(256),
                        height: px(256),
                        ..default()
                    },
                    UiTargetCamera(target),
                ))
                .id();
            let sibling = effect(
                app.world_mut(),
                sibling_parent,
                mesh.clone(),
                Some("0".into()),
            );
            app.world_mut()
                .get_mut::<EffectPreview>(sibling)
                .unwrap()
                .motion = false;
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect = mesh.clone();
            data.quantity = Some("0".into());
            data.strength = 1.0;
            app.world_mut().resource_mut::<Pixels>().0.clear();
            app.world_mut().resource_mut::<EdgePixels>().0.clear();
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            assert!(app.world().get::<Camera3d>(camera).is_some());
            app.world_mut()
                .entity_mut(camera)
                .insert(bevy::core_pipeline::prepass::DepthPrepass);
            let deadline = Instant::now() + Duration::from_secs(30);
            while working(app.world(), sibling) != Some(&mesh) {
                assert!(
                    Instant::now() < deadline,
                    "Sibling mesh did not activate: {}",
                    app.world().get::<EffectPreview>(sibling).unwrap().error
                );
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            app.world_mut().despawn(sibling_parent);
            let anchor = app.world_mut().spawn_empty().id();
            assert!(spatial::place(
                app.world_mut(),
                preview,
                anchor,
                anchor,
                Transform::from_xyz(2.0, 3.0, 4.0),
                true
            ));
            assert_eq!(
                app.world_mut()
                    .query::<&crate::native::shader_effects::RecordMesh>()
                    .iter(app.world())
                    .count(),
                1
            );
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("1".into());
            app.world_mut().resource_mut::<EdgePixels>().0.clear();
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            assert!(
                app.world()
                    .resource::<EdgePixels>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] < 5)
            );
            let mut invalid = mesh.clone();
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("2".into());
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("1".into());
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            if let Mode::Mesh(settings) = &mut invalid.mode {
                settings
                    .deformation
                    .as_mut()
                    .unwrap()
                    .source
                    .push_str(" struct LinceVertexInput { value: f32, }");
            }
            assert!(invalid.validate().is_ok());
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .effect = invalid;
            let deadline = Instant::now() + Duration::from_secs(30);
            while app
                .world()
                .get::<EffectPreview>(preview)
                .unwrap()
                .error
                .is_empty()
            {
                assert!(
                    Instant::now() < deadline,
                    "Invalid mesh GPU pipeline did not report an error"
                );
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(working(app.world(), preview), Some(&mesh));
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .effect = mesh;
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("0".into());
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let data = app.world().get::<EffectPreview>(preview).unwrap();
                if data.ready
                    && data.pending.is_none()
                    && data.candidates.is_empty()
                    && data.painted >= 3
                    && data.last_quantity.as_deref() == Some("0")
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Recovered mesh did not finish preparing before Freeze"
                );
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                data.frozen = true;
                data.quantity = Some("1".into());
                data.painted = 0;
            }
            app.world_mut().despawn(readback);
            app.world_mut().resource_mut::<Pixels>().0.clear();
            app.world_mut()
                .spawn(Readback::texture(image))
                .observe(collect_pixels);
            for _ in 0..10 {
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
            assert!(!app.world().resource::<Pixels>().0.is_empty());
            assert!(
                app.world()
                    .resource::<Pixels>()
                    .0
                    .iter()
                    .all(|pixel| pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240),
                "Frozen mesh pixels: {:?}",
                app.world().resource::<Pixels>().0
            );
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .frozen = false;
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("0".into());
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            spatial::rotate(app.world_mut(), preview);
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.effect = Effect { programs: [("green".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }".into())].into(), rules: vec![], fallback: "green".into(), ..default() };
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            assert!(app.world().get::<Camera2d>(camera).is_some());
            assert!(app.world().get::<Camera3d>(camera).is_none());
            assert!(
                app.world()
                    .get::<bevy::core_pipeline::prepass::DepthPrepass>(camera)
                    .is_none()
            );
            assert_eq!(
                app.world()
                    .resource::<Assets<spatial::MeshMaterial>>()
                    .len(),
                0
            );
            assert_eq!(app.world().resource::<Assets<Mesh>>().len(), mesh_count);
            assert_eq!(
                app.world_mut()
                    .query::<&crate::native::shader_effects::RecordMesh>()
                    .iter(app.world())
                    .count(),
                0
            );
        }
        {
            use bevy::render::storage::ShaderBuffer;
            use nucleus::shader_effect::spatial::{GeneratedGeometry, MeshSettings, Mode};
            let buffers = app.world().resource::<Assets<ShaderBuffer>>().len();
            let generated = Effect {
                mode: Mode::Mesh(MeshSettings {
                    geometry: Some(GeneratedGeometry {
                        source: "fn generate_vertex(index: u32, vertex_count: u32, inputs: ShaderInputs) -> MeshVertex { let x = select(-4.0, 4.0, index == 1u || index == 2u || index == 4u); let y = select(-4.0, 4.0, index == 2u || index == 4u || index == 5u); return MeshVertex(vec3<f32>(x, y, inputs.quantity * 0.2), vec3<f32>(0.0, 0.0, 10.0), vec2<f32>(x, y) + 0.5); }".into(),
                        vertices: 6,
                    }),
                    ..default()
                }),
                programs: [("generated".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(f32(point.local_position.z > 0.1), 1.0, 1.0, 1.0); }".into())].into(),
                rules: vec![],
                fallback: "generated".into(),
                ..default()
            };
            assert!(generated.validate().is_ok());
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                data.effect = generated.clone();
                data.quantity = Some("0".into());
            }
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            app.world_mut()
                .entity_mut(camera)
                .insert(bevy::core_pipeline::prepass::DepthPrepass);
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("1".into());
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            let id = app
                .world()
                .resource::<Assets<ShaderBuffer>>()
                .iter()
                .find(|(_, buffer)| buffer.buffer_description.size == 6 * 48)
                .unwrap()
                .0;
            let buffer = app
                .world_mut()
                .resource_mut::<Assets<ShaderBuffer>>()
                .get_strong_handle(id)
                .unwrap();
            let readback = app
                .world_mut()
                .spawn(Readback::buffer(buffer.clone()))
                .observe(collect_vertices)
                .id();
            let deadline = Instant::now() + Duration::from_secs(30);
            while app.world().resource::<GeneratedVertices>().0.is_empty() {
                assert!(
                    Instant::now() < deadline,
                    "Generated vertex buffer did not arrive"
                );
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            let vertices = &app.world().resource::<GeneratedVertices>().0;
            assert_eq!(vertices.len(), 6 * 48);
            for vertex in vertices.chunks_exact(48) {
                let value = |offset: usize| {
                    f32::from_le_bytes(vertex[offset..offset + 4].try_into().unwrap())
                };
                assert_eq!(value(0).abs(), 0.5);
                assert_eq!(value(4).abs(), 0.5);
                assert!((value(8) - 0.2).abs() < 0.0001);
                assert_eq!([value(16), value(20), value(24)], [0.0, 0.0, 1.0]);
                assert!((0.0..=1.0).contains(&value(32)));
                assert!((0.0..=1.0).contains(&value(36)));
            }
            app.world_mut().despawn(readback);
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                assert!(data.painted >= 3);
                data.frozen = true;
                data.quantity = Some("0".into());
                data.painted = 0;
            }
            app.world_mut()
                .resource_mut::<GeneratedVertices>()
                .0
                .clear();
            let readback = app
                .world_mut()
                .spawn(Readback::buffer(buffer.clone()))
                .observe(collect_vertices)
                .id();
            for _ in 0..10 {
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
            let frozen = &app.world().resource::<GeneratedVertices>().0;
            assert_eq!(frozen.len(), 6 * 48);
            assert!(frozen.chunks_exact(48).all(|vertex| {
                (f32::from_le_bytes(vertex[8..12].try_into().unwrap()) - 0.2).abs() < 0.0001
            }));
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .frozen = false;
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] > 240
            });
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                data.disabled = true;
                data.quantity = Some("1".into());
            }
            app.world_mut().despawn(readback);
            app.world_mut()
                .resource_mut::<GeneratedVertices>()
                .0
                .clear();
            let readback = app
                .world_mut()
                .spawn(Readback::buffer(buffer))
                .observe(collect_vertices)
                .id();
            for _ in 0..10 {
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
            assert_eq!(app.world().resource::<GeneratedVertices>().0.len(), 6 * 48);
            assert!(
                app.world()
                    .resource::<GeneratedVertices>()
                    .0
                    .chunks_exact(48)
                    .all(
                        |vertex| f32::from_le_bytes(vertex[8..12].try_into().unwrap()).abs()
                            < 0.0001
                    )
            );
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .disabled = false;
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            app.world_mut().despawn(readback);
            let mut invalid = generated.clone();
            if let Mode::Mesh(settings) = &mut invalid.mode {
                settings
                    .geometry
                    .as_mut()
                    .unwrap()
                    .source
                    .push_str(" const lince_counts: u32 = 0u;");
            }
            assert!(invalid.validate().is_ok());
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .effect = invalid;
            let deadline = Instant::now() + Duration::from_secs(30);
            while app
                .world()
                .get::<EffectPreview>(preview)
                .unwrap()
                .error
                .is_empty()
            {
                assert!(
                    Instant::now() < deadline,
                    "Invalid compute pipeline did not report an error"
                );
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(working(app.world(), preview), Some(&generated));
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240
            });
            app.world_mut().get_mut::<EffectPreview>(preview).unwrap().effect = Effect { programs: [("green".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }".into())].into(), rules: vec![], fallback: "green".into(), ..default() };
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            assert_eq!(
                app.world().resource::<Assets<ShaderBuffer>>().len(),
                buffers
            );
        }
        {
            use bevy::render::storage::ShaderBuffer;
            use nucleus::shader_effect::spatial::{Mode, Quality, VolumeSettings};
            let mesh_count = app.world().resource::<Assets<Mesh>>().len();
            let buffer_count = app.world().resource::<Assets<ShaderBuffer>>().len();
            let mut volume = Effect {
                mode: Mode::Volume(VolumeSettings { quality: Quality::Low, density: 1.0 }),
                programs: [("field".into(), "fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(1.0, 0.0, 0.0, f32(inputs.quantity > 0.0)); }".into())].into(),
                fallback: "field".into(), rules: vec![], ..default()
            };
            assert!(volume.validate().is_ok());
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                data.effect = volume.clone();
                data.quantity = Some("1".into());
            }
            wait(&mut app, |pixel| {
                pixel[0] > 240 && pixel[1] < 5 && pixel[2] < 5 && pixel[3] > 100 && pixel[3] < 240
            });
            assert!(
                app.world()
                    .get::<bevy::core_pipeline::prepass::DepthPrepass>(camera)
                    .is_some()
            );
            let mut image =
                Image::new_target_texture(256, 256, TextureFormat::Rgba8UnormSrgb, None);
            image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
            let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
            let scene_camera = app
                .world_mut()
                .spawn((
                    Camera3d::default(),
                    crate::native::topology::presentation::WorldCamera,
                    bevy::core_pipeline::tonemapping::Tonemapping::None,
                    Camera {
                        clear_color: ClearColorConfig::Custom(Color::NONE),
                        ..default()
                    },
                    RenderTarget::Image(image.clone().into()),
                    Msaa::Off,
                    Transform::from_xyz(0.0, 0.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
                ))
                .id();
            let anchor = app.world_mut().spawn_empty().id();
            assert!(spatial::place(
                app.world_mut(),
                preview,
                anchor,
                anchor,
                Transform::default(),
                true
            ));
            assert!(
                app.world()
                    .get::<bevy::core_pipeline::prepass::DepthPrepass>(scene_camera)
                    .is_some()
            );
            let plane_mesh = app
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .add(Cuboid::new(8.0, 8.0, 0.01));
            let plane_material = app
                .world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color: Color::linear_rgb(0.0, 1.0, 0.0),
                    unlit: true,
                    ..default()
                });
            let plane = app
                .world_mut()
                .spawn((
                    Mesh3d(plane_mesh.clone()),
                    MeshMaterial3d(plane_material.clone()),
                    Transform::from_xyz(0.0, 0.0, -1.0),
                ))
                .id();
            let readback = app
                .world_mut()
                .spawn(Readback::texture(image.clone()))
                .observe(
                    |event: On<bevy::render::gpu_readback::ReadbackComplete>,
                     mut pixels: ResMut<WorldPixels>| {
                        let offset = (128 * 256 + 128) * 4;
                        if event.data.len() >= offset + 4 {
                            pixels
                                .0
                                .push(event.data[offset..offset + 4].try_into().unwrap());
                        }
                    },
                )
                .id();
            let mut probe = 0;
            let mut wait_world = |app: &mut App, expected: fn([u8; 4]) -> bool| {
                probe += 1;
                app.world_mut().resource_mut::<WorldPixels>().0.clear();
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut stable = 0;
                loop {
                    app.update();
                    let samples =
                        std::mem::take(&mut app.world_mut().resource_mut::<WorldPixels>().0);
                    if let Some(pixel) = samples.last().copied() {
                        stable = if expected(pixel) { stable + 1 } else { 0 };
                        if stable >= 3 {
                            break pixel;
                        }
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Volume world probe {probe} did not settle: {:?}; {}",
                        samples.last(),
                        app.world().get::<EffectPreview>(preview).unwrap().error
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            };
            let full = wait_world(&mut app, |pixel| {
                pixel[0] > 195
                    && pixel[0] < 220
                    && pixel[1] > 145
                    && pixel[1] < 180
                    && pixel[2] < 5
                    && pixel[3] > 245
            });
            app.world_mut()
                .get_mut::<Transform>(plane)
                .unwrap()
                .translation
                .z = 0.0;
            let half = wait_world(&mut app, |pixel| {
                pixel[0] > 150 && pixel[0] < 190 && pixel[1] > 185 && pixel[1] < 225 && pixel[2] < 5
            });
            assert!(half[0] < full[0] && half[1] > full[1]);
            app.world_mut()
                .get_mut::<Transform>(plane)
                .unwrap()
                .translation
                .z = 1.0;
            wait_world(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            app.world_mut()
                .get_mut::<Transform>(plane)
                .unwrap()
                .translation
                .z = -1.0;
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            for quality in [Quality::Medium, Quality::High] {
                if let Mode::Volume(settings) = &mut volume.mode {
                    settings.quality = quality;
                }
                app.world_mut()
                    .get_mut::<EffectPreview>(preview)
                    .unwrap()
                    .effect = volume.clone();
                let deadline = Instant::now() + Duration::from_secs(30);
                while working(app.world(), preview) != Some(&volume) {
                    assert!(
                        Instant::now() < deadline,
                        "Volume quality did not activate: {}",
                        app.world().get::<EffectPreview>(preview).unwrap().error
                    );
                    app.update();
                    std::thread::sleep(Duration::from_millis(10));
                }
                let pixel = wait_world(&mut app, |pixel| {
                    pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
                });
                assert!(pixel[0].abs_diff(full[0]) < 5 && pixel[1].abs_diff(full[1]) < 5);
            }
            *app.world_mut().get_mut::<Transform>(scene_camera).unwrap() =
                Transform::from_xyz(2.0, 0.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y);
            wait_world(&mut app, |pixel| {
                pixel[0] > 216 && pixel[1] < 150 && pixel[1] > 110
            });
            *app.world_mut().get_mut::<Transform>(scene_camera).unwrap() =
                Transform::from_xyz(0.0, 0.0, 0.55);
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            *app.world_mut().get_mut::<Transform>(scene_camera).unwrap() = Transform::default();
            wait_world(&mut app, |pixel| {
                pixel[0] > 150 && pixel[0] < 190 && pixel[1] > 185 && pixel[1] < 225
            });
            app.world_mut()
                .get_mut::<Transform>(plane)
                .unwrap()
                .translation
                .z = -0.25;
            wait_world(&mut app, |pixel| {
                pixel[0] > 95 && pixel[0] < 150 && pixel[1] > 220 && pixel[1] < 248
            });
            app.world_mut()
                .entity_mut(scene_camera)
                .insert(Projection::Orthographic(OrthographicProjection {
                    scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                        viewport_height: 2.0,
                    },
                    ..OrthographicProjection::default_3d()
                }));
            wait_world(&mut app, |pixel| {
                pixel[0] > 120 && pixel[0] < 140 && pixel[1] > 223 && pixel[1] < 235
            });
            app.world_mut()
                .get_mut::<Transform>(plane)
                .unwrap()
                .translation
                .z = -1.0;
            *app.world_mut().get_mut::<Transform>(scene_camera).unwrap() =
                Transform::from_xyz(0.0, 0.0, 2.0);
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            {
                let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
                data.frozen = true;
                data.quantity = Some("0".into());
            }
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .frozen = false;
            wait(&mut app, |pixel| pixel[3] < 5);
            wait_world(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .quantity = Some("1".into());
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .disabled = true;
            wait_world(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .disabled = false;
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            let mut invalid = volume.clone();
            invalid
                .programs
                .get_mut("field")
                .unwrap()
                .push_str(" struct LinceVaryings { value: f32, }");
            assert!(invalid.validate().is_ok());
            app.world_mut()
                .get_mut::<EffectPreview>(preview)
                .unwrap()
                .effect = invalid;
            let deadline = Instant::now() + Duration::from_secs(30);
            while app
                .world()
                .get::<EffectPreview>(preview)
                .unwrap()
                .error
                .is_empty()
            {
                assert!(
                    Instant::now() < deadline,
                    "Invalid volume pipeline did not report an error"
                );
                app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(working(app.world(), preview), Some(&volume));
            wait_world(&mut app, |pixel| {
                pixel[0] > 195 && pixel[0] < 220 && pixel[1] > 145 && pixel[1] < 180
            });
            app.world_mut().get_mut::<EffectPreview>(preview).unwrap().effect = Effect { programs: [("green".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }".into())].into(), rules: vec![], fallback: "green".into(), ..default() };
            wait(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            wait_world(&mut app, |pixel| {
                pixel[0] < 5 && pixel[1] > 240 && pixel[2] < 5
            });
            assert!(
                app.world()
                    .get::<bevy::core_pipeline::prepass::DepthPrepass>(scene_camera)
                    .is_none()
            );
            assert_eq!(
                app.world()
                    .resource::<Assets<spatial::MeshMaterial>>()
                    .len(),
                0
            );
            assert_eq!(
                app.world().resource::<Assets<ShaderBuffer>>().len(),
                buffer_count
            );
            for entity in [readback, scene_camera, plane, anchor] {
                app.world_mut().despawn(entity);
            }
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .remove(image.id());
            app.world_mut()
                .resource_mut::<Assets<Mesh>>()
                .remove(plane_mesh.id());
            app.world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .remove(plane_material.id());
            assert_eq!(app.world().resource::<Assets<Mesh>>().len(), mesh_count);
        }
    }

    #[test]
    fn headless_appearance_preflight_and_lit_generated_meshes_share_shadow_views() {
        use std::sync::atomic::{AtomicBool, Ordering};
        #[derive(Resource, Clone, Default)]
        struct ShadowSeen(Arc<AtomicBool>);
        let mut app = App::new();
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
        .init_resource::<Pixels>()
        .init_resource::<EdgePixels>()
        .insert_resource(bevy::light::PointLightShadowMap { size: 256 });
        install(&mut app);
        let seen = ShadowSeen::default();
        app.insert_resource(seen.clone());
        app.sub_app_mut(RenderApp)
            .insert_resource(seen)
            .add_systems(
                Render,
                (|views: Query<&ExtractedView>, seen: Res<ShadowSeen>| {
                    if views
                        .iter()
                        .any(|view| view.target_format.is_depth_stencil_format())
                    {
                        seen.0.store(true, Ordering::Relaxed);
                    }
                })
                .in_set(RenderSystems::Queue),
            );
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.plugins_state() != bevy::app::PluginsState::Ready {
            assert!(Instant::now() < deadline, "GPU initialization timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        app.finish();
        app.cleanup();
        let ui_image =
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    256,
                    256,
                    TextureFormat::Rgba8UnormSrgb,
                    None,
                ));
        let mut scene_target =
            Image::new_target_texture(256, 256, TextureFormat::Rgba8UnormSrgb, None);
        scene_target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let scene_image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(scene_target);
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                RenderTarget::Image(ui_image.clone().into()),
                Msaa::Off,
            ))
            .id();
        let scene_camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                RenderTarget::Image(scene_image.clone().into()),
                Transform::from_xyz(0.0, 1.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                Msaa::Off,
            ))
            .id();
        let cube = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::default());
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let cube = app
            .world_mut()
            .spawn((Mesh3d(cube), MeshMaterial3d(material), Transform::default()))
            .id();
        let light = app
            .world_mut()
            .spawn((
                PointLight {
                    shadow_maps_enabled: true,
                    intensity: 8000.0,
                    range: 10.0,
                    ..default()
                },
                Transform::from_xyz(0.0, 2.0, 1.0),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !app
            .world()
            .resource::<ShadowSeen>()
            .0
            .load(Ordering::Relaxed)
        {
            app.update();
            assert!(
                Instant::now() < deadline,
                "Expected point-shadow depth views before compiling the UI shader"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let parent = app
            .world_mut()
            .spawn((
                Node {
                    width: px(256),
                    height: px(256),
                    ..default()
                },
                UiTargetCamera(camera),
            ))
            .id();
        let definition = Effect { programs: [("solid".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.2, 0.6, 0.9, 1.0); }".into())].into(), rules: vec![], fallback: "solid".into(), ..default() };
        let preview = effect(app.world_mut(), parent, definition, Some("1".into()));
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .motion = false;
        let image = app.world().resource::<EffectAssets>().owned[&preview]
            .2
            .clone();
        app.world_mut()
            .spawn(bevy::render::gpu_readback::Readback::texture(image))
            .observe(collect_pixels);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.update();
            if app
                .world()
                .resource::<ShadowSeen>()
                .0
                .load(Ordering::Relaxed)
                && app
                    .world()
                    .resource::<Pixels>()
                    .0
                    .last()
                    .is_some_and(|pixel| pixel[2] > 180 && pixel[1] > 120)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Expected a blue UI shader while point-shadow depth views exist; pixels: {:?}; working: {}; error: {}",
                app.world().resource::<Pixels>().0.last(),
                working(app.world(), preview).is_some(),
                app.world().get::<EffectPreview>(preview).unwrap().error
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        use nucleus::shader_effect::spatial::{Deformation, GeneratedGeometry, MeshSettings, Mode};
        app.world_mut().despawn(cube);
        *app.world_mut().get_mut::<Transform>(scene_camera).unwrap() =
            Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::new(0.0, -0.3, 0.0), Vec3::Y);
        *app.world_mut().get_mut::<Transform>(light).unwrap() = Transform::from_xyz(-1.5, 3.0, 3.0);
        app.world_mut()
            .get_mut::<PointLight>(light)
            .unwrap()
            .intensity = 50000.0;
        let floor_mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(8.0, 0.02, 8.0));
        let floor_material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                perceptual_roughness: 1.0,
                ..default()
            });
        app.world_mut().spawn((
            Mesh3d(floor_mesh),
            MeshMaterial3d(floor_material),
            Transform::from_xyz(0.0, -1.01, 0.0),
            bevy::light::NotShadowCaster,
        ));
        app.world_mut()
            .entity_mut(parent)
            .insert(crate::native::topology::appearance::Material(
                nucleus::canvas::appearance::Material {
                    lit: true,
                    roughness: 0.5,
                    ..default()
                },
            ));
        app.world_mut().get_mut::<EffectPreview>(preview).unwrap().effect = Effect {
            mode: Mode::Mesh(MeshSettings {
                geometry: Some(GeneratedGeometry { source: nucleus::shader_effect::spatial::GENERATED_SPHERE.into(), vertices: 3840 }),
                deformation: Some(Deformation { source: nucleus::shader_effect::spatial::DEFORM_SPHERE.into(), max_displacement: 0.2 }), ..default()
            }),
            programs: [("solid".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.2, 0.6, 0.9, 1.0); }".into())].into(), rules: vec![], fallback: "solid".into(), ..default()
        };
        let anchor = app.world_mut().spawn_empty().id();
        for _ in 0..3 {
            app.update();
        }
        let camera = app.world().get::<Camera>(scene_camera).unwrap();
        let camera_transform = app.world().get::<GlobalTransform>(scene_camera).unwrap();
        let shadow_probe = camera
            .world_to_viewport(camera_transform, Vec3::new(1.6, -1.0, -0.7))
            .unwrap();
        #[derive(Resource, Default)]
        struct MeshPixels(Vec<([u8; 4], [u8; 4])>);
        app.init_resource::<MeshPixels>();
        app.world_mut()
            .spawn(bevy::render::gpu_readback::Readback::texture(scene_image))
            .observe(
                move |event: On<bevy::render::gpu_readback::ReadbackComplete>,
                      mut pixels: ResMut<MeshPixels>| {
                    let center = (128 * 256 + 128) * 4;
                    let shadow = (shadow_probe.y as usize * 256 + shadow_probe.x as usize) * 4;
                    if event.data.len() >= shadow + 4 && event.data.len() >= center + 4 {
                        pixels.0.push((
                            event.data[center..center + 4].try_into().unwrap(),
                            event.data[shadow..shadow + 4].try_into().unwrap(),
                        ));
                    }
                },
            );
        let wait_mesh = |app: &mut App,
                         stage: &str,
                         predicate: &dyn Fn(([u8; 4], [u8; 4])) -> bool| {
            app.world_mut().resource_mut::<MeshPixels>().0.clear();
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut stable = 0;
            let mut last = None;
            loop {
                spatial::place(
                    app.world_mut(),
                    preview,
                    anchor,
                    parent,
                    Transform::from_scale(Vec3::splat(2.0)),
                    true,
                );
                app.update();
                if let Some(pixel) = app.world_mut().resource_mut::<MeshPixels>().0.pop() {
                    last = Some(pixel);
                    stable = if predicate(pixel) { stable + 1 } else { 0 };
                    if stable >= 4 {
                        return pixel;
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "Lit generated mesh {stage} pixels did not settle; pixels: {last:?}; WGSL: {}",
                    app.world().get::<EffectPreview>(preview).unwrap().error
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let lit = wait_mesh(&mut app, "initial", &|pixel| {
            pixel.0[2] > 40 && pixel.0[2] < 180 && pixel.0[2] > pixel.0[0] + 10
        });
        app.world_mut()
            .get_mut::<PointLight>(light)
            .unwrap()
            .intensity = 0.0;
        wait_mesh(&mut app, "light off", &|pixel| pixel.0[2] + 3 < lit.0[2]);
        app.world_mut()
            .get_mut::<PointLight>(light)
            .unwrap()
            .intensity = 50000.0;
        let small = wait_mesh(&mut app, "light restored", &|pixel| {
            pixel.0[2] >= lit.0[2].saturating_sub(2)
        });
        {
            let mut data = app.world_mut().get_mut::<EffectPreview>(preview).unwrap();
            data.frozen = true;
            data.quantity = Some("10".into());
        }
        let frozen = wait_mesh(&mut app, "frozen", &|pixel| {
            pixel.1[0].abs_diff(small.1[0]) <= 2
        });
        app.world_mut()
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .frozen = false;
        let deformed = wait_mesh(&mut app, "deformed shadow", &|pixel| {
            pixel.1[0] + 3 < frozen.1[0]
        });
        app.world_mut()
            .get_mut::<crate::native::topology::appearance::Material>(parent)
            .unwrap()
            .0
            .cast_shadows = false;
        let no_shadow = wait_mesh(&mut app, "casting disabled", &|pixel| {
            pixel.1[0] > deformed.1[0] + 3
        });
        let fade = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, 0.5),
                ..default()
            });
        app.world_mut()
            .entity_mut(parent)
            .insert(crate::native::topology::presentation::Surface {
                camera: Entity::PLACEHOLDER,
                image: Handle::default(),
                visual: anchor,
                body: anchor,
                face: anchor,
                size: Vec2::splat(100.0),
                pixels: UVec2::splat(100),
                density: 1.0,
                material: fade.clone(),
                uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE),
                visible: true,
            });
        {
            let mut settings = app
                .world_mut()
                .get_mut::<crate::native::topology::appearance::Material>(parent)
                .unwrap();
            settings.0.lit = false;
            settings.0.cast_shadows = true;
        }
        wait_mesh(&mut app, "partial opacity with shadow", &|pixel| {
            pixel.0[2] > 120 && pixel.0[2] < 230 && pixel.1[0] + 3 < no_shadow.1[0]
        });
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&fade)
            .unwrap()
            .base_color
            .set_alpha(0.0);
        wait_mesh(&mut app, "transparent without ghost shadow", &|pixel| {
            pixel.0[2] < 120 && pixel.1[0].abs_diff(no_shadow.1[0]) <= 2
        });
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&fade)
            .unwrap()
            .base_color
            .set_alpha(1.0);
        app.world_mut()
            .entity_mut(parent)
            .remove::<crate::native::topology::appearance::Material>();
        wait_mesh(&mut app, "unstyled", &|pixel| pixel.0[2] > 220);
    }

    fn effect_world(images: bool) -> World {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<Assets<Shader>>();
        world.init_resource::<Assets<ShaderMaterial>>();
        world.init_resource::<crate::native::theme::Typography>();
        if images {
            world.init_resource::<Assets<Image>>();
        }
        world
    }

    fn show(world: &mut World, entity: Entity) {
        let surface = output(world, entity);
        world
            .entity_mut(surface)
            .remove::<ComputedUiRenderTargetInfo>()
            .insert(InheritedVisibility::VISIBLE);
        world.get_mut::<ComputedNode>(surface).unwrap().size = Vec2::splat(120.0);
    }

    fn prepare(world: &mut World, entity: Entity) {
        show(world, entity);
        update_effects(world);
        world.get_mut::<EffectPreview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        update_effects(world);
    }

    #[test]
    fn appearance_lighting_inputs_follow_working_mappings_freeze_and_disable() {
        use nucleus::shader_effect::{InputMappings, QuantityRange, TaskState};
        let mut world = effect_world(false);
        let parent = world.spawn(Node::default()).id();
        let mut definition = Effect::default();
        definition.inputs = InputMappings {
            task: Some(QuantityRange {
                start: "-2".into(),
                end: "0".into(),
            }),
            progress: Some(QuantityRange {
                start: "-2".into(),
                end: "0".into(),
            }),
        };
        let preview = effect(&mut world, parent, definition, Some("-1".into()));
        assert!(lighting_inputs(&world, preview).is_none());
        prepare(&mut world, preview);
        assert_eq!(
            lighting_inputs(&world, preview).unwrap().progress,
            Some(0.5)
        );
        {
            let mut state = world.get_mut::<EffectPreview>(preview).unwrap();
            state.frozen = true;
            state.painted = 3;
            state.quantity = Some("0".into());
            state.effect.inputs = InputMappings::default();
        }
        assert_eq!(
            lighting_inputs(&world, preview).unwrap().task,
            TaskState::Active
        );
        world.get_mut::<EffectPreview>(preview).unwrap().frozen = false;
        assert_eq!(
            lighting_inputs(&world, preview).unwrap().task,
            TaskState::Completed
        );
        world.get_mut::<EffectPreview>(preview).unwrap().disabled = true;
        assert!(lighting_inputs(&world, preview).is_none());
    }

    #[test]
    fn appearance_surface_overlay_uses_working_image_and_releases_on_disable() {
        use crate::native::topology::{appearance, presentation};
        let mut world = effect_world(true);
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        let owner = world.spawn(Node::default()).id();
        let definition = Effect::default();
        let preview = effect(&mut world, owner, definition.clone(), Some("-1".into()));
        prepare(&mut world, preview);
        let image = effect_image(&world, preview).unwrap();
        let output = output(&world, preview);
        let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::default());
        let rectangle = world
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::default());
        let original = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let visual = world
            .spawn((Transform::default(), Visibility::Inherited))
            .id();
        let body = world
            .spawn((
                Mesh3d(cube),
                MeshMaterial3d(original.clone()),
                Transform::default(),
                ChildOf(visual),
            ))
            .id();
        let face = world
            .spawn((
                Mesh3d(rectangle),
                MeshMaterial3d(original.clone()),
                Transform::default(),
                ChildOf(visual),
            ))
            .id();
        world.entity_mut(owner).insert((
            crate::native::shader_effects::Surface::default(),
            crate::native::shader_effects::Background { preview },
            appearance::Material(nucleus::canvas::appearance::Material {
                lit: true,
                emissive: 2.0,
                ..default()
            }),
            presentation::Surface {
                camera: Entity::PLACEHOLDER,
                image: Handle::default(),
                visual,
                body,
                face,
                size: Vec2::splat(100.0),
                pixels: UVec2::splat(100),
                density: 1.0,
                material: original,
                uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE),
                visible: true,
            },
        ));
        appearance::update(&mut world);
        assert!(world.get::<appearance::LitEffectOutput>(output).is_some());
        let (overlay, material) = world
            .query::<(Entity, &MeshMaterial3d<StandardMaterial>)>()
            .iter(&world)
            .find(|(_, handle)| {
                world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&handle.0)
                    .is_some_and(|material| material.base_color_texture.as_ref() == Some(&image))
            })
            .map(|(entity, handle)| (entity, handle.0.clone()))
            .unwrap();
        assert!(
            !world
                .resource::<Assets<StandardMaterial>>()
                .get(&material)
                .unwrap()
                .unlit
        );
        assert!(world.get::<bevy::light::NotShadowCaster>(overlay).is_some());
        world
            .get_mut::<appearance::Material>(owner)
            .unwrap()
            .0
            .color[3] = 0.0;
        appearance::update(&mut world);
        assert_eq!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&material)
                .unwrap()
                .base_color
                .alpha(),
            1.0
        );
        let backing = world
            .get::<MeshMaterial3d<StandardMaterial>>(body)
            .unwrap()
            .0
            .clone();
        assert_eq!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&backing)
                .unwrap()
                .base_color
                .alpha(),
            0.0
        );
        world
            .get_mut::<EffectPreview>(preview)
            .unwrap()
            .effect
            .programs
            .insert("negative".into(), "invalid WGSL".into());
        prepare(&mut world, preview);
        assert_eq!(working(&world, preview), Some(&definition));
        appearance::update(&mut world);
        assert_eq!(
            world
                .get::<MeshMaterial3d<StandardMaterial>>(overlay)
                .unwrap()
                .0,
            material
        );
        world
            .get_mut::<crate::native::shader_effects::Surface>(owner)
            .unwrap()
            .disabled = true;
        appearance::update(&mut world);
        assert!(world.get::<appearance::LitEffectOutput>(output).is_none());
        assert!(world.get_entity(overlay).is_err());
        assert!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&material)
                .is_none()
        );
    }

    #[test]
    fn task_uniforms_follow_quantity_and_keep_working_mappings_after_invalid_edits() {
        use nucleus::shader_effect::{InputMappings, QuantityRange, TaskState};
        let mut world = effect_world(false);
        let parent = world.spawn(Node::default()).id();
        let mut definition = Effect::default();
        definition.inputs = InputMappings {
            task: Some(QuantityRange {
                start: "-2".into(),
                end: "0".into(),
            }),
            progress: Some(QuantityRange {
                start: "-2".into(),
                end: "0".into(),
            }),
        };
        let preview = effect(&mut world, parent, definition.clone(), Some("-1".into()));
        prepare(&mut world, preview);
        let material = world
            .get::<Preview>(preview)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let inputs = &world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .inputs;
        assert_eq!(inputs.task_state, TaskState::Active as u32);
        assert_eq!(inputs.progress, 0.5);
        assert_eq!(inputs.progress_present, 1);
        let mut invalid = definition.clone();
        invalid.inputs.task.as_mut().unwrap().end = "-2".into();
        world.get_mut::<EffectPreview>(preview).unwrap().effect = invalid;
        prepare(&mut world, preview);
        assert_eq!(working(&world, preview), Some(&definition));
        world.get_mut::<EffectPreview>(preview).unwrap().quantity = Some("0".into());
        update_effects(&mut world);
        let inputs = &world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .inputs;
        assert_eq!(inputs.task_state, TaskState::Completed as u32);
        assert_eq!(inputs.progress, 1.0);
        world.get_mut::<EffectPreview>(preview).unwrap().quantity = None;
        update_effects(&mut world);
        let inputs = &world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .inputs;
        assert_eq!(inputs.task_state, TaskState::Unknown as u32);
        assert_eq!(inputs.progress_present, 0);
        let label = world.get::<Preview>(preview).unwrap().label;
        world.get_mut::<EffectPreview>(preview).unwrap().disabled = true;
        update_effects(&mut world);
        assert_eq!(
            world.get::<Text>(label).unwrap().0,
            "Quantity unavailable; effect inactive"
        );
        world.get_mut::<EffectPreview>(preview).unwrap().quantity = Some("0".into());
        update_effects(&mut world);
        assert_eq!(world.get::<Text>(label).unwrap().0, "Effect disabled");
    }

    #[test]
    fn numeric_uniforms_preserve_presence_and_reject_stale_binding_values() {
        use nucleus::shader_effect::{
            Operator, Rule,
            bindings::{NumericBinding, NumericSource, NumericValues},
        };
        let mut world = effect_world(false);
        let parent = world.spawn(Node::default()).id();
        let mut definition = Effect::default();
        definition.bindings = vec![NumericBinding {
            slot: 0,
            label: "Protein".into(),
            source: NumericSource::default(),
        }];
        definition.rules = vec![Rule {
            operator: Operator::Greater,
            value: "16777216".into(),
            program: "negative".into(),
            binding: Some(0),
        }];
        let numeric =
            NumericValues::resolve(&definition.bindings, |_| Some("16777217".into())).unwrap();
        let preview = effect(&mut world, parent, definition.clone(), None);
        {
            let mut data = world.get_mut::<EffectPreview>(preview).unwrap();
            data.numeric = numeric;
            data.numeric_bindings = definition.bindings.clone();
        }
        prepare(&mut world, preview);
        let material = world
            .get::<Preview>(preview)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let rendered = world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap();
        assert_eq!(rendered.inputs.numeric0.x, 16777216.0);
        assert_eq!(rendered.inputs.present0.x, 1);
        assert_eq!(rendered.inputs.present0.y, 0);
        assert_eq!(
            rendered.shader,
            world.get::<EffectPreview>(preview).unwrap().programs["negative"]
        );
        definition.bindings[0].source.record = Some(nucleus::new_uid("r"));
        world.get_mut::<EffectPreview>(preview).unwrap().effect = definition;
        prepare(&mut world, preview);
        let rendered = world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap();
        assert_eq!(rendered.inputs.present0, UVec4::ZERO);
        assert_eq!(rendered.inputs.numeric0, Vec4::ZERO);
    }

    #[test]
    fn changed_numeric_inputs_render_before_idle_editor_animations() {
        use nucleus::shader_effect::bindings::{NumericBinding, NumericSource, NumericValues};
        let mut world = effect_world(true);
        let parent = world.spawn(Node::default()).id();
        let mut definition = Effect::default();
        definition.bindings = vec![NumericBinding {
            slot: 0,
            label: "Value".into(),
            source: NumericSource::default(),
        }];
        let ordinary = effect(&mut world, parent, definition.clone(), Some("0".into()));
        let mut previews = vec![ordinary];
        for _ in 0..8 {
            previews.push(effect_with_priority(
                &mut world,
                parent,
                Effect::default(),
                Some("0".into()),
                true,
            ));
        }
        for preview in &previews {
            show(&mut world, *preview);
        }
        update_effects(&mut world);
        for preview in &previews {
            world.get_mut::<EffectPreview>(*preview).unwrap().pending =
                Some(Instant::now() - Duration::from_secs(1));
        }
        for _ in 0..16 {
            update_effects(&mut world);
        }
        let camera = world.get::<EffectPreview>(ordinary).unwrap().camera;
        let served = world.get::<EffectPreview>(ordinary).unwrap().last_served;
        for _ in 0..16 {
            update_effects(&mut world);
        }
        assert!(world.get::<EffectPreview>(ordinary).unwrap().last_served > served);
        {
            let mut data = world.get_mut::<EffectPreview>(ordinary).unwrap();
            data.numeric =
                NumericValues::resolve(&definition.bindings, |_| Some("17".into())).unwrap();
            data.numeric_bindings = definition.bindings;
        }
        update_effects(&mut world);
        assert!(world.get::<Camera>(camera).unwrap().is_active);
        assert!(
            world
                .query::<&Camera>()
                .iter(&world)
                .filter(|camera| camera.is_active)
                .count()
                <= 8
        );
    }

    #[test]
    fn focused_sources_keep_their_camera_under_editor_contention() {
        let mut world = effect_world(true);
        let parent = world.spawn(Node::default()).id();
        let focused = effect(&mut world, parent, Effect::default(), Some("0".into()));
        prepare(&mut world, focused);
        world.entity_mut(focused).insert(extent::Raster {
            pixels: UVec2::splat(120),
            requested: UVec2::splat(120),
            visible: true,
            device_limited: false,
            budget_limited: false,
            priority: 5,
        });
        for _ in 0..8 {
            let editor = effect_with_priority(
                &mut world,
                parent,
                Effect::default(),
                Some("0".into()),
                true,
            );
            prepare(&mut world, editor);
        }
        for _ in 0..32 {
            update_effects(&mut world);
            let camera = world.get::<EffectPreview>(focused).unwrap().camera;
            assert!(world.get::<Camera>(camera).unwrap().is_active);
            assert!(
                world
                    .query::<&Camera>()
                    .iter(&world)
                    .filter(|camera| camera.is_active)
                    .count()
                    <= scheduling::SLOTS
            );
        }
    }

    #[test]
    fn failed_candidates_without_working_programs_do_not_consume_camera_slots() {
        let mut world = effect_world(true);
        let parent = world.spawn(Node::default()).id();
        let mut invalid = Effect::default();
        *invalid.programs.values_mut().next().unwrap() = "fn shade(".into();
        let mut failed = Vec::new();
        for _ in 0..9 {
            let source = effect(&mut world, parent, invalid.clone(), Some("0".into()));
            prepare(&mut world, source);
            assert!(!world.get::<EffectPreview>(source).unwrap().error.is_empty());
            failed.push(source);
        }
        let valid = effect(&mut world, parent, Effect::default(), Some("0".into()));
        prepare(&mut world, valid);
        update_effects(&mut world);
        assert!(world.get::<EffectPreview>(valid).unwrap().ready);
        assert!(
            world
                .get::<Camera>(world.get::<EffectPreview>(valid).unwrap().camera)
                .unwrap()
                .is_active
        );
        for source in failed {
            assert!(
                !world
                    .get::<Camera>(world.get::<EffectPreview>(source).unwrap().camera)
                    .unwrap()
                    .is_active
            );
        }
    }

    #[test]
    fn frozen_snapshots_stop_cameras_and_inputs_but_can_be_disabled_and_resumed() {
        let mut world = effect_world(true);
        let parent = world.spawn(Node::default()).id();
        let preview = effect(&mut world, parent, Effect::default(), Some("-1".into()));
        world.get_mut::<EffectPreview>(preview).unwrap().frozen = true;
        prepare(&mut world, preview);
        for _ in 0..2 {
            update_effects(&mut world);
        }
        assert!(
            world
                .get::<EffectPreview>(preview)
                .unwrap()
                .snapshot
                .frozen()
        );
        update_effects(&mut world);
        let camera = world.get::<EffectPreview>(preview).unwrap().camera;
        assert!(world.get::<EffectPreview>(preview).unwrap().painted >= 3);
        assert!(!world.get::<Camera>(camera).unwrap().is_active);
        let material = world
            .get::<Preview>(preview)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let frozen_inputs = world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .inputs;
        world.get_mut::<EffectPreview>(preview).unwrap().quantity = Some("0".into());
        update_effects(&mut world);
        assert_eq!(
            world
                .resource::<Assets<ShaderMaterial>>()
                .get(&material)
                .unwrap()
                .inputs,
            frozen_inputs
        );
        assert!(!world.get::<Camera>(camera).unwrap().is_active);
        let frozen_shader = world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .shader
            .clone();
        let size = Vec2::new(480.0, 240.0);
        let surface = output(&world, preview);
        world.get_mut::<ComputedNode>(surface).unwrap().size = size;
        world.get_mut::<EffectPreview>(preview).unwrap().painted = 0;
        let mut resized_inputs = frozen_inputs;
        resized_inputs.mouse.x *= size.x / resized_inputs.size.x;
        resized_inputs.mouse.y *= size.y / resized_inputs.size.y;
        resized_inputs.size = size;
        for _ in 0..3 {
            update_effects(&mut world);
            assert!(world.get::<Camera>(camera).unwrap().is_active);
            let material = world
                .resource::<Assets<ShaderMaterial>>()
                .get(&material)
                .unwrap();
            assert_eq!(material.inputs, resized_inputs);
            assert_eq!(material.shader, frozen_shader);
            assert_eq!(
                world
                    .get::<EffectPreview>(preview)
                    .unwrap()
                    .last_quantity
                    .as_deref(),
                Some("-1")
            );
        }
        update_effects(&mut world);
        assert!(!world.get::<Camera>(camera).unwrap().is_active);
        world.get_mut::<EffectPreview>(preview).unwrap().disabled = true;
        update_effects(&mut world);
        assert_eq!(
            world.get::<Node>(output(&world, preview)).unwrap().display,
            Display::None
        );
        {
            let mut data = world.get_mut::<EffectPreview>(preview).unwrap();
            data.disabled = false;
            data.frozen = false;
        }
        update_effects(&mut world);
        assert!(world.get::<Camera>(camera).unwrap().is_active);
        assert_eq!(
            world
                .resource::<Assets<ShaderMaterial>>()
                .get(&material)
                .unwrap()
                .inputs
                .quantity,
            0.0
        );
    }

    #[test]
    fn bundles_share_programs_but_keep_quantities_and_failed_candidates_independent() {
        let mut world = effect_world(false);
        let parent = world.spawn(Node::default()).id();
        let first = effect(&mut world, parent, Effect::default(), Some("-1".into()));
        let second = effect(&mut world, parent, Effect::default(), Some("0".into()));
        prepare(&mut world, first);
        prepare(&mut world, second);
        assert_eq!(world.resource::<Assets<Shader>>().len(), 2);
        let first_material = world
            .get::<Preview>(first)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let second_material = world
            .get::<Preview>(second)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let materials = world.resource::<Assets<ShaderMaterial>>();
        assert_eq!(
            materials.get(&first_material).unwrap().inputs.quantity,
            -1.0
        );
        assert_eq!(
            materials.get(&second_material).unwrap().inputs.quantity,
            0.0
        );
        assert_ne!(
            materials.get(&first_material).unwrap().shader,
            materials.get(&second_material).unwrap().shader
        );
        world.get_mut::<EffectPreview>(first).unwrap().quantity = Some("0".into());
        update_effects(&mut world);
        assert_eq!(world.resource::<Assets<Shader>>().len(), 2);
        world
            .get_mut::<EffectPreview>(first)
            .unwrap()
            .effect
            .programs
            .insert("negative".into(), "fn shade(".into());
        prepare(&mut world, first);
        assert!(!world.get::<EffectPreview>(first).unwrap().error.is_empty());
        assert_eq!(working(&world, first), Some(&Effect::default()));
        assert_eq!(working(&world, second), Some(&Effect::default()));
        let mut candidate = Effect::default();
        candidate
            .programs
            .insert("negative".into(), nucleus::shader_effect::RIPPLE.into());
        world.get_mut::<EffectPreview>(first).unwrap().effect = candidate;
        let camera = world.spawn(Camera::default()).id();
        world.get_mut::<EffectPreview>(first).unwrap().camera = camera;
        world.init_resource::<PipelineStatus>();
        prepare(&mut world, first);
        let shader = world.get::<EffectPreview>(first).unwrap().candidates["negative"].id();
        world
            .resource::<PipelineStatus>()
            .0
            .lock()
            .unwrap()
            .insert(shader, Err("simulated device failure".into()));
        update_effects(&mut world);
        assert!(
            world
                .get::<EffectPreview>(first)
                .unwrap()
                .error
                .contains("device failure")
        );
        assert_eq!(working(&world, first), Some(&Effect::default()));
    }

    #[test]
    fn render_targets_and_active_cameras_are_bounded_and_released() {
        let mut world = effect_world(true);
        let parent = world.spawn(Node::default()).id();
        for _ in 0..65 {
            let entity = effect(&mut world, parent, Effect::default(), Some("0".into()));
            show(&mut world, entity);
        }
        for _ in 0..8 {
            let entity = effect_with_priority(
                &mut world,
                parent,
                Effect::default(),
                Some("0".into()),
                true,
            );
            show(&mut world, entity);
        }
        update_effects(&mut world);
        assert_eq!(world.resource::<Assets<Image>>().len(), 64);
        assert_eq!(
            world
                .query::<&Camera>()
                .iter(&world)
                .filter(|camera| camera.is_active)
                .count(),
            8
        );
        assert!(
            world
                .resource::<Assets<Image>>()
                .iter()
                .all(|(_, image)| image.width() <= 512 && image.height() <= 512)
        );
        world.despawn(parent);
        update_effects(&mut world);
        assert_eq!(world.query::<&Camera>().iter(&world).count(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn repeated_edits_reuse_shader_slots() {
        let mut world = effect_world(false);
        let parent = world.spawn(Node::default()).id();
        let entity = effect(&mut world, parent, Effect::default(), Some("-1".into()));
        prepare(&mut world, entity);
        for index in 0..24 {
            let mut definition = Effect::default();
            definition.programs.insert("negative".into(), format!("fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> {{ return vec4<f32>({}.0 / 24.0, 0.0, 0.0, 1.0); }}", index));
            world.get_mut::<EffectPreview>(entity).unwrap().effect = definition;
            prepare(&mut world, entity);
            assert!(world.get::<EffectPreview>(entity).unwrap().ready);
            assert!(world.resource::<Assets<Shader>>().len() <= 3);
            assert_eq!(world.resource::<Assets<ShaderMaterial>>().len(), 1);
        }
    }

    #[test]
    fn validates_ball_and_rejects_invalid_or_extra_resources() {
        assert!(compile(BALL).is_ok());
        assert!(compile("fn shade(").is_err());
        assert!(
            compile(&format!(
                "@group(2) @binding(0) var image: texture_2d<f32>;\n{BALL}"
            ))
            .is_err()
        );
        assert!(
            compile(&format!(
                "@compute @workgroup_size(1) fn other() {{}}\n{BALL}"
            ))
            .is_err()
        );
        assert!(compile(&"x".repeat(65_537)).is_err());
        assert!(compile(&format!("override missing: f32;\n{BALL}")).is_err());
        assert!(
            compile("fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> f32 { return 1.0; }")
                .is_err()
        );
    }

    #[test]
    fn edits_reuse_gpu_assets_and_keep_last_valid_shader_on_errors() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<Assets<Shader>>();
        world.init_resource::<Assets<ShaderMaterial>>();
        world.init_resource::<crate::native::theme::Typography>();
        let parent = world.spawn(Node::default()).id();
        spawn(&mut world, parent, BALL, None);
        let entity = world.get::<Children>(parent).unwrap()[0];
        let surface = world.get::<Preview>(entity).unwrap().surface;
        world
            .entity_mut(surface)
            .remove::<ComputedUiRenderTargetInfo>()
            .insert(InheritedVisibility::VISIBLE);
        world.get_mut::<ComputedNode>(surface).unwrap().size = Vec2::new(400.0, 280.0);
        world
            .get_mut::<RelativeCursorPosition>(surface)
            .unwrap()
            .normalized = Some(Vec2::ZERO);
        world.get_mut::<Preview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        update(&mut world);
        world
            .entity_mut(surface)
            .remove::<ComputedUiRenderTargetInfo>();
        let material = world
            .get::<Preview>(entity)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let shader = world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .shader
            .clone();
        assert_eq!(
            world
                .resource::<Assets<ShaderMaterial>>()
                .get(&material)
                .unwrap()
                .inputs
                .mouse
                .x,
            200.0
        );
        assert_eq!(ShaderInputs::min_size().get(), 128);
        assert!(matches!(
            world
                .resource::<Assets<Shader>>()
                .get(&shader)
                .unwrap()
                .validate_shader,
            ValidateShader::Enabled
        ));
        spawn(&mut world, parent, "fn shade(", Some(entity));
        let label = world.get::<Preview>(entity).unwrap().label;
        world.get_mut::<Preview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        update(&mut world);
        assert!(world.get::<Text>(label).unwrap().0.contains("WGSL:"));
        let bevy::shader::Source::Wgsl(source) = &world
            .resource::<Assets<Shader>>()
            .get(&shader)
            .unwrap()
            .source
        else {
            panic!("WGSL expected")
        };
        assert!(source.contains(BALL));
        let replacement = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(uv, 0.0, 1.0); }";
        spawn(&mut world, parent, replacement, Some(entity));
        update(&mut world);
        assert!(world.get::<Preview>(entity).unwrap().pending.is_some());
        world.get_mut::<Preview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        world.get_mut::<Node>(parent).unwrap().display = Display::None;
        update(&mut world);
        assert!(world.get::<Preview>(entity).unwrap().pending.is_some());
        world.get_mut::<Node>(parent).unwrap().display = Display::Flex;
        update(&mut world);
        assert!(world.get::<Text>(label).unwrap().0.is_empty());
        assert_eq!(
            world.get::<Preview>(entity).unwrap().material.as_ref(),
            Some(&material)
        );
        assert_eq!(world.resource::<Assets<Shader>>().len(), 1);
        assert_eq!(world.resource::<Assets<ShaderMaterial>>().len(), 1);
    }
}
