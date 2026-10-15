use crate::native::{
    actions::Action,
    description::shader,
    protein_area::{Config, RecordBinding, Source},
};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
    text::EditableText,
};
use cell::{ClientMessage, ServerMessage};
use nucleus::shader_effect::{self as model, Effect, Extension, Operator, Rule};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

mod catalog;
#[cfg(test)]
mod cleaning_tests;
mod mesh;
mod numeric;
mod placement;
mod sphere;
pub use sphere::RecordSphere;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlaceSpatialEffects;

pub(crate) fn appearance_owner(world: &World, preview: Entity) -> Option<Entity> {
    let parent = world.get::<ChildOf>(preview)?.parent();
    Some(
        world
            .get::<Floating>(parent)
            .map_or(parent, |floating| floating.record),
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatingSettings {
    #[serde(default)]
    pub effect_id: Option<String>,
    pub size: f32,
    pub gap: f32,
    pub elevation: f32,
    pub billboard: bool,
    #[serde(default)]
    pub sphere: bool,
    pub motion: bool,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub distance: DistancePolicy,
    #[serde(default = "default_min_pixels")]
    pub min_pixels: f32,
}

fn default_min_pixels() -> f32 {
    32.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistancePolicy {
    #[default]
    Thumbnail,
    Hide,
    Live,
}

impl Default for FloatingSettings {
    fn default() -> Self {
        Self {
            effect_id: None,
            size: 120.0,
            gap: 16.0,
            elevation: 40.0,
            billboard: false,
            sphere: false,
            motion: true,
            frozen: false,
            distance: DistancePolicy::Thumbnail,
            min_pixels: default_min_pixels(),
        }
    }
}

impl FloatingSettings {
    pub fn valid(&self) -> bool {
        self.effect_id
            .as_ref()
            .is_none_or(|id| model::valid_effect_id(id))
            && self.size.is_finite()
            && (32.0..=256.0).contains(&self.size)
            && self.gap.is_finite()
            && (0.0..=64.0).contains(&self.gap)
            && self.elevation.is_finite()
            && (0.0..=256.0).contains(&self.elevation)
            && self.min_pixels.is_finite()
            && (8.0..=128.0).contains(&self.min_pixels)
    }
}

#[derive(Component)]
struct PlacementInput {
    area: Entity,
    key: u8,
}

#[derive(Clone)]
struct PlacementToggle(Entity, bool);

#[derive(Clone)]
struct SphereToggle(Entity);

impl Action for SphereToggle {
    fn apply(&self, world: &mut World, _: Entity) {
        placement::change(world, self.0, |settings| settings.sphere = !settings.sphere);
        placement::refresh(world, self.0);
    }
}
impl Action for PlacementToggle {
    fn apply(&self, world: &mut World, _: Entity) {
        placement::change(world, self.0, |settings| {
            if self.1 {
                settings.billboard = !settings.billboard;
            } else {
                settings.motion = !settings.motion;
            }
        });
        placement::refresh(world, self.0);
    }
}

pub(crate) fn placement_controls(
    world: &mut World,
    owner: Entity,
    panel: Entity,
    settings: &FloatingSettings,
) {
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        if settings.sphere {
            "3D shape: sphere"
        } else {
            "3D shape: panel"
        },
        SphereToggle(owner),
    );
    for (key, title, value) in [
        (0, "Effect size 32–256", settings.size),
        (1, "North gap 0–64", settings.gap),
        (2, "3D elevation 0–256", settings.elevation),
        (
            3,
            "Distant effect threshold 8–128 pixels",
            settings.min_pixels,
        ),
    ] {
        crate::native::edit_mode::label(world, panel, title, 13.0);
        let entity = world
            .spawn(crate::native::sand::text_editor(
                &value.to_string(),
                world.resource::<crate::native::theme::Typography>(),
                0,
            ))
            .id();
        world.entity_mut(entity).insert((
            Node {
                width: percent(100),
                height: px(32),
                ..default()
            },
            ChildOf(panel),
            PlacementInput { area: owner, key },
        ));
        crate::native::accessibility::input(world, entity, title, false);
    }
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        if settings.billboard {
            "3D orientation: camera facing"
        } else {
            "3D orientation: panel aligned"
        },
        PlacementToggle(owner, true),
    );
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        if settings.motion {
            "Record effect motion: On"
        } else {
            "Record effect motion: Off"
        },
        PlacementToggle(owner, false),
    );
    placement::extra_controls(world, owner, panel, settings);
}

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    #[serde(default)]
    pub effect_id: Option<String>,
    pub effect: Effect,
    pub preset: String,
    pub draft: Option<Effect>,
    pub record: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub quantity_only: bool,
    #[serde(default)]
    pub room_ambient: bool,
    pub quantity: String,
    pub strength: f32,
    pub glow: f32,
    pub motion: bool,
    #[serde(default)]
    pub frozen: bool,
    pub disabled: bool,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            effect_id: None,
            effect: preset(model::GRADIENT),
            preset: "gradient".into(),
            draft: None,
            record: None,
            source: Source::Local,
            quantity_only: false,
            room_ambient: false,
            quantity: "0".into(),
            strength: 0.6,
            glow: 12.0,
            motion: true,
            frozen: false,
            disabled: false,
        }
    }
}

impl Surface {
    pub fn cleaning() -> Self {
        Self {
            effect: preset(model::CLEANING),
            preset: "cleaning".into(),
            quantity_only: true,
            room_ambient: true,
            quantity: String::new(),
            strength: 0.65,
            glow: 0.0,
            motion: false,
            ..default()
        }
    }

    pub fn valid(&self) -> bool {
        self.effect_id
            .as_ref()
            .is_none_or(|id| model::valid_effect_id(id))
            && self.effect.validate().is_ok()
            && matches!(
                self.preset.as_str(),
                "gradient"
                    | "ripple"
                    | "calm"
                    | "energy"
                    | "reward"
                    | "reminder"
                    | "grain"
                    | "numeric"
                    | "cleaning"
            )
            && self.draft.as_ref().is_none_or(draft_valid)
            && self
                .record
                .as_ref()
                .is_none_or(|record| nucleus::valid_uid(record, "r"))
            && self.quantity.len() <= 512
            && self.strength.is_finite()
            && (0.0..=1.0).contains(&self.strength)
            && self.glow.is_finite()
            && (0.0..=24.0).contains(&self.glow)
            && match &self.source {
                Source::Local => true,
                Source::Organ(uid) => nucleus::valid_uid(uid, "r"),
            }
    }
}

fn preset(source: &str) -> Effect {
    Effect {
        version: 1,
        mode: Default::default(),
        programs: [("surface".into(), source.into())].into(),
        rules: vec![],
        fallback: "surface".into(),
        inputs: Default::default(),
        bindings: vec![],
    }
}

fn draft_valid(effect: &Effect) -> bool {
    model::bindings::bounded(&effect.bindings)
        && effect.mode.bounded()
        && effect.programs.len() <= 8
        && effect.rules.len() <= 16
        && effect
            .programs
            .iter()
            .all(|(name, source)| name.len() <= 64 && source.len() <= 65_536)
        && effect.sources().map(str::len).sum::<usize>() <= 131_072
        && effect
            .rules
            .iter()
            .all(|rule| rule.value.len() <= 128 && rule.program.len() <= 64)
        && effect.fallback.len() <= 64
        && [&effect.inputs.task, &effect.inputs.progress]
            .into_iter()
            .flatten()
            .all(|range| range.start.len() <= 128 && range.end.len() <= 128)
}

#[derive(Component, Clone)]
struct RecordData {
    binding: RecordBinding,
    extension: Option<Extension>,
    quantity: Option<String>,
}

#[derive(Component)]
struct Floating {
    record: Entity,
    preview: Entity,
    distant: bool,
}

#[derive(Component)]
pub(crate) struct EffectSlot(Entity);

#[derive(Component)]
pub struct RecordMesh(pub Entity);

#[derive(Component)]
struct RecordStateLabel(Entity);

#[derive(Component)]
pub(crate) struct Background {
    pub(crate) preview: Entity,
}

#[derive(Component)]
struct LightPreview(Entity);

#[derive(Clone)]
enum Target {
    Record(RecordBinding),
    Surface(Entity),
}

#[derive(Component)]
struct Editor {
    catalog: catalog::Draft,
    target: Target,
    effect: Effect,
    original: Effect,
    revision: u64,
    loaded: bool,
    selected: String,
    panel: Entity,
    preview: Entity,
    status: Entity,
    quantity: Option<String>,
    custom_quantity: bool,
    pending: Option<(String, Instant, Effect)>,
    message: String,
    rebuild: bool,
    bootstrap: Option<Effect>,
}

#[derive(Component, Clone)]
struct Input {
    editor: Entity,
    key: InputKey,
}

#[derive(Clone)]
enum InputKey {
    DeformationSource,
    GeometrySource,
    EffectLabel,
    NewEffectId,
    NewEffectLabel,
    ReferencedEffect,
    Source(String),
    Threshold(usize),
    Quantity,
    Record,
    Organ,
    Strength,
    Glow,
    Range { task: bool, end: bool },
    BindingLabel(u8),
    BindingRecord(u8),
    BindingOrgan(u8),
    BindingNamespace(u8),
    BindingPath(u8),
}

#[derive(Component)]
struct MappingStatus(Entity);

#[derive(Component)]
struct EditorClose;

struct Feed {
    binding: RecordBinding,
    id: String,
    sent: bool,
    row: Option<Value>,
    error: String,
    retry: Instant,
}

#[derive(Resource, Default)]
struct Feeds(HashMap<String, Feed>, Vec<(RecordBinding, String)>);

#[derive(Resource, Default, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct ShaderSettings {
    pub disabled: bool,
    pub reduced_motion: bool,
}

pub(crate) fn motion_allowed(world: &World) -> bool {
    world
        .get_resource::<ShaderSettings>()
        .is_none_or(|settings| !settings.reduced_motion)
}
pub(crate) fn disabled(world: &World) -> bool {
    world
        .get_resource::<ShaderSettings>()
        .is_some_and(|settings| settings.disabled)
}

pub(crate) fn lighting_inputs(world: &World, entity: Entity) -> Option<model::ResolvedInputs> {
    if disabled(world) {
        return None;
    }
    let preview = world
        .get::<Background>(entity)
        .map(|value| value.preview)
        .or_else(|| world.get::<LightPreview>(entity).map(|value| value.0))
        .or_else(|| world.get::<Floating>(entity).map(|value| value.preview))?;
    shader::lighting_inputs(world, preview)
}

pub(crate) fn surface_image(world: &World, entity: Entity) -> Option<(Entity, Handle<Image>)> {
    if disabled(world) || world.get::<Surface>(entity)?.disabled {
        return None;
    }
    let preview = world.get::<Background>(entity)?.preview;
    let image = shader::effect_image(world, preview)?;
    Some((shader::output(world, preview), image))
}

#[derive(Clone, Debug)]
pub struct SurfaceReading {
    pub quantity: Option<String>,
    pub inputs: Option<model::ResolvedInputs>,
    pub enabled: bool,
    pub ready: bool,
    pub image_available: bool,
    pub error: String,
}

pub fn surface_reading(world: &World, entity: Entity) -> Option<SurfaceReading> {
    let preview = world.get::<Background>(entity)?.preview;
    let state = world.get::<shader::EffectPreview>(preview)?;
    Some(SurfaceReading {
        quantity: state.quantity.clone(),
        inputs: shader::lighting_inputs(world, preview),
        enabled: !state.disabled && !disabled(world),
        ready: state.ready && shader::working(world, preview).is_some(),
        image_available: surface_image(world, entity).is_some(),
        error: state.error.clone(),
    })
}

pub(crate) fn organs(world: &World) -> std::collections::HashSet<String> {
    let mut organs = numeric::organs(world);
    if let Some(feeds) = world.get_resource::<Feeds>() {
        organs.extend(
            feeds
                .0
                .values()
                .map(|feed| &feed.binding)
                .chain(feeds.1.iter().map(|(binding, _)| binding))
                .filter_map(|binding| match &binding.source {
                    Source::Organ(uid) => Some(uid.clone()),
                    Source::Local => None,
                }),
        );
    }
    organs
}

pub struct ShaderEffectsPlugin;
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct UpdateShaderEffects;
impl Plugin for ShaderEffectsPlugin {
    fn build(&self, app: &mut App) {
        sphere::install(app);
        app.init_resource::<Feeds>()
            .init_resource::<numeric::NumericFeeds>()
            .init_resource::<ShaderSettings>()
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                update
                    .after(crate::native::protein_area::UpdateProteinAreas)
                    .in_set(UpdateShaderEffects),
            );
    }
}

fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    while world
        .get::<crate::native::workspace::Workspaces>(entity)
        .is_none()
    {
        entity = world.get::<ChildOf>(entity)?.parent();
    }
    Some(entity)
}

fn feed_key(binding: &RecordBinding) -> String {
    serde_json::to_string(&(&binding.source, &binding.uid)).unwrap()
}

fn sender(
    world: &World,
    binding: &RecordBinding,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    crate::native::protein_area::editor_sender(world, binding)
}

#[derive(Clone)]
pub(crate) struct ToggleRecords(pub Entity);
impl Action for ToggleRecords {
    fn apply(&self, world: &mut World, _: Entity) {
        if let Some(mut config) = crate::native::protein_area::configuration(world, self.0) {
            config.shader_effects = !config.shader_effects;
            crate::native::protein_area::set_configuration(world, self.0, Some(config));
        }
    }
}

#[derive(Clone)]
pub(crate) struct OpenRecord(pub RecordBinding);
impl Action for OpenRecord {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = root(world, owner) else {
            return;
        };
        let data = world
            .query::<&RecordData>()
            .iter(world)
            .find(|data| data.binding.uid == self.0.uid && data.binding.source == self.0.source)
            .cloned();
        let extension = data.as_ref().and_then(|data| data.extension.clone());
        let selected = crate::native::protein_area::configuration(world, self.0.area)
            .map(|config| placement::effective(world, &self.0, &config.shader_placement).effect_id)
            .flatten();
        let effect = extension
            .as_ref()
            .and_then(|extension| extension.selected(selected.as_deref()).cloned())
            .unwrap_or_default();
        let revision = extension.as_ref().map_or(0, |extension| extension.revision);
        let editor = open(
            world,
            root,
            Target::Record(self.0.clone()),
            effect,
            revision,
            data.and_then(|data| data.quantity),
        );
        if let Some(extension) = extension {
            if let Some(id) = selected {
                world.get_mut::<Editor>(editor).unwrap().catalog.choose(id);
            }
            catalog::load(world, editor, extension, false);
            render_editor(world, editor);
        }
    }
}

#[derive(Clone)]
pub(crate) struct OpenSurface(pub Entity);
impl Action for OpenSurface {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = root(world, owner) else {
            return;
        };
        if world.get_entity(self.0).is_err() {
            return;
        }
        let surface = world.get::<Surface>(self.0).cloned().unwrap_or_default();
        world.entity_mut(self.0).insert(surface.clone());
        let original = surface.effect.clone();
        let editor = open(
            world,
            root,
            Target::Surface(self.0),
            surface.draft.unwrap_or(surface.effect),
            0,
            Some(surface.quantity),
        );
        world.get_mut::<Editor>(editor).unwrap().original = original.clone();
        let preview = world.get::<Editor>(editor).unwrap().preview;
        world
            .get_mut::<shader::EffectPreview>(preview)
            .unwrap()
            .effect = original;
    }
}

fn open(
    world: &mut World,
    root: Entity,
    target: Target,
    effect: Effect,
    revision: u64,
    quantity: Option<String>,
) -> Entity {
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    let position = world
        .get::<crate::native::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(crate::native::description::CREDITS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(700.0, 900.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(6),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
        ))
        .id();
    crate::native::scroll_sand::attach(world, owner);
    crate::native::edit_mode::label(
        world,
        owner,
        match target {
            Target::Record(_) => "Record WGSL extension",
            Target::Surface(_) => "Sand surface effect",
        },
        20.0,
    );
    let controls = crate::native::sand_panel::column(world, owner);
    for (title, command) in [
        ("Save / Apply", Command::Save),
        ("Reload working", Command::Reload),
        ("Reset", Command::Reset),
        ("Disable / Enable", Command::Disable),
        ("Animation / Still", Command::Motion),
        ("Freeze snapshot / Resume", Command::Snapshot),
        ("Close", Command::Close),
    ] {
        let closes = matches!(&command, Command::Close);
        let button = crate::native::sand_panel::button(world, controls, owner, title, command);
        if closes {
            world.entity_mut(button).insert(EditorClose);
        }
    }
    if matches!(target, Target::Record(_)) {
        crate::native::sand_panel::button(
            world,
            controls,
            owner,
            "Follow Record quantity",
            Command::FollowQuantity,
        );
    }
    let status = crate::native::edit_mode::label(world, owner, "", 13.0);
    let preview = shader::effect_with_priority(
        world,
        owner,
        if effect.validate().is_ok() {
            effect.clone()
        } else {
            Effect::default()
        },
        quantity.clone(),
        true,
    );
    let context = match &target {
        Target::Record(binding) => numeric::Context {
            record: Some(binding.uid.clone()),
            source: binding.source.clone(),
        },
        Target::Surface(entity) => world
            .get::<Surface>(*entity)
            .map(|surface| numeric::Context {
                record: surface.record.clone(),
                source: surface.source.clone(),
            })
            .unwrap_or(numeric::Context {
                record: None,
                source: Source::Local,
            }),
    };
    world.entity_mut(preview).insert(context);
    world
        .get_mut::<shader::EffectPreview>(preview)
        .unwrap()
        .priority = true;
    crate::native::accessibility::status(world, status);
    let panel = crate::native::sand_panel::column(world, owner);
    let selected = effect.programs.keys().next().cloned().unwrap_or_default();
    let bootstrap = Some(
        world
            .get::<shader::EffectPreview>(preview)
            .unwrap()
            .effect
            .clone(),
    );
    world.entity_mut(owner).insert(Editor {
        catalog: catalog::Draft::new(revision, effect.clone()),
        target,
        original: effect.clone(),
        effect,
        revision,
        loaded: false,
        selected,
        panel,
        preview,
        status,
        quantity,
        custom_quantity: false,
        pending: None,
        message: String::new(),
        rebuild: true,
        bootstrap,
    });
    render_editor(world, owner);
    owner
}

fn input(
    world: &mut World,
    parent: Entity,
    editor: Entity,
    key: InputKey,
    title: &str,
    value: &str,
    multiline: bool,
) {
    crate::native::edit_mode::label(world, parent, title, 13.0);
    let entity = world
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
            0,
        ))
        .id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.max_characters = Some(if multiline {
        16_384
    } else if matches!(key, InputKey::BindingPath(_)) {
        1024
    } else if matches!(key, InputKey::Threshold(_)) {
        32
    } else {
        128
    });
    text.allow_newlines = multiline;
    text.visible_lines = Some(if multiline { 10.0 } else { 1.0 });
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            height: px(if multiline { 220 } else { 32 }),
            flex_shrink: 0.0,
            ..default()
        },
        Input { editor, key },
        ChildOf(parent),
    ));
    crate::native::accessibility::input(world, entity, title, multiline);
}

fn render_editor(world: &mut World, owner: Entity) {
    let editor = world.get::<Editor>(owner).unwrap();
    let (panel, selected, effect, target) = (
        editor.panel,
        editor.selected.clone(),
        editor.effect.clone(),
        editor.target.clone(),
    );
    crate::native::sand_panel::clear(world, panel);
    catalog::controls(world, owner, panel);
    placement::editor_controls(world, owner, panel);
    numeric::controls(world, owner, panel, &effect);
    mesh::controls(world, owner, panel, &effect);
    if let Target::Surface(entity) = target
        && let Some(surface) = world.get::<Surface>(entity).cloned()
    {
        if surface.room_ambient {
            crate::native::edit_mode::label(
                world,
                panel,
                "This floor effect follows its room's ambient switch. Choose a Record/source and explicit task or progress range. Missing data stays clean; a static zero is a real quantity.",
                12.0,
            );
        }
        input(
            world,
            panel,
            owner,
            InputKey::Record,
            "Record UID (empty uses local WGSL)",
            surface.record.as_deref().unwrap_or(""),
            false,
        );
        input(
            world,
            panel,
            owner,
            InputKey::ReferencedEffect,
            "Referenced effect ID (empty uses Record default)",
            surface.effect_id.as_deref().unwrap_or(""),
            false,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Choose next referenced effect",
            Command::ReferenceEffect,
        );
        input(
            world,
            panel,
            owner,
            InputKey::Quantity,
            "Preview / static quantity",
            &surface.quantity,
            false,
        );
        input(
            world,
            panel,
            owner,
            InputKey::Strength,
            "Intensity 0–1",
            &surface.strength.to_string(),
            false,
        );
        input(
            world,
            panel,
            owner,
            InputKey::Glow,
            "Outward glow 0–24 pixels",
            &surface.glow.to_string(),
            false,
        );
        input(
            world,
            panel,
            owner,
            InputKey::Organ,
            "Source Organ UID (empty for local)",
            match &surface.source {
                Source::Local => "",
                Source::Organ(uid) => uid,
            },
            false,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            if surface.quantity_only {
                "Record reference: quantity only"
            } else {
                "Record reference: WGSL and quantity"
            },
            Command::ReferenceMode,
        );
        if surface.record.is_some() {
            crate::native::edit_mode::label(
                world,
                panel,
                if surface.quantity_only {
                    "The Sand keeps its local WGSL and receives the referenced Record's live quantity. This editor's static preview quantity stays separate."
                } else {
                    "This preview edits local WGSL. The Sand uses the referenced Record's extension; edit its programs from the Record Castle."
                },
                12.0,
            );
        }
    } else {
        let quantity = world
            .get::<Editor>(owner)
            .unwrap()
            .quantity
            .clone()
            .unwrap_or_default();
        input(
            world,
            panel,
            owner,
            InputKey::Quantity,
            "Preview quantity (does not edit the Record)",
            &quantity,
            false,
        );
    }
    for (task, range) in [
        (true, &effect.inputs.task),
        (false, &effect.inputs.progress),
    ] {
        let name = if task { "Task state" } else { "Progress" };
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!(
                "{name}: {}",
                if range.is_some() {
                    "quantity range (disable)"
                } else {
                    "unmapped (configure quantity range)"
                }
            ),
            Command::Mapping(task),
        );
        if let Some(range) = range {
            input(
                world,
                panel,
                owner,
                InputKey::Range { task, end: false },
                &format!("{name} start quantity"),
                &range.start,
                false,
            );
            input(
                world,
                panel,
                owner,
                InputKey::Range { task, end: true },
                &format!("{name} end quantity"),
                &range.end,
                false,
            );
        }
    }
    let resolved = effect
        .inputs
        .resolve(world.get::<Editor>(owner).unwrap().quantity.as_deref())
        .label();
    let status = crate::native::edit_mode::label(world, panel, &resolved, 13.0);
    world.entity_mut(status).insert(MappingStatus(owner));
    crate::native::edit_mode::label(
        world,
        panel,
        "Mappings are opt-in. Start is pending/0%; end is completed/100%. Descending ranges work too. task_state: 0 unknown, 1 pending, 2 active, 3 completed. progress is 0–1 when progress_present is 1. Presets do not assign task meaning.",
        12.0,
    );
    if !matches!(effect.mode, model::spatial::Mode::Volume(_)) {
        crate::native::edit_mode::label(
            world,
            panel,
            "Reward, Reminder and Cleaning replace quantity program rules, retaining your mappings. Cleaning needs an explicit task or progress range; missing quantities stay clean. Other Record presets edit the selected program.",
            12.0,
        );
        for (name, source) in [
            ("Calm ball", model::CALM),
            ("Energy ball", model::ENERGY),
            ("Gradient", model::GRADIENT),
            ("Ripple", model::RIPPLE),
            ("Grain", model::GRAIN),
            ("Numeric input glow", model::NUMERIC),
            ("Reward (mapped task state)", model::REWARD),
            ("Reminder (mapped task state)", model::REMINDER),
            ("Cleaning goo (mapped task/progress)", model::CLEANING),
        ] {
            crate::native::sand_panel::button(
                world,
                panel,
                owner,
                name,
                Command::Preset(source.into()),
            );
        }
    }
    for name in effect.programs.keys() {
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!("Edit program: {name}"),
            Command::Select(name.clone()),
        );
    }
    crate::native::sand_panel::button(world, panel, owner, "Add program", Command::AddProgram);
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        "Remove selected program",
        Command::RemoveProgram,
    );
    if let Some(source) = effect.programs.get(&selected) {
        input(
            world,
            panel,
            owner,
            InputKey::Source(selected.clone()),
            match &effect.mode {
                model::spatial::Mode::Surface => "WGSL: fn shade(uv, inputs) → vec4<f32>",
                model::spatial::Mode::Mesh(_) => {
                    "WGSL: fn shade_mesh(point, inputs) or shade(uv, inputs) → vec4<f32>"
                }
                model::spatial::Mode::Volume(_) => {
                    "WGSL: fn volume_sample(position, inputs) → vec4<f32>"
                }
            },
            source,
            true,
        );
    }
    crate::native::edit_mode::label(
        world,
        panel,
        "Inputs: size, time, delta, mouse, quantity, quantity_present, task_state, effect_strength, motion_allowed, progress, progress_present. Quantity rules use this Record's quantity; first match wins.",
        12.0,
    );
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        &format!("Fallback: {} (choose selected program)", effect.fallback),
        Command::Fallback,
    );
    for (index, rule) in effect.rules.iter().enumerate() {
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!("Rule {}: {:?} (change operator)", index + 1, rule.operator),
            Command::Operator(index),
        );
        if !effect.bindings.is_empty() {
            crate::native::sand_panel::button(
                world,
                panel,
                owner,
                &rule.binding.map_or_else(
                    || "Input: Record quantity".into(),
                    |slot| format!("Input: numeric slot {slot}"),
                ),
                Command::RuleInput(index),
            );
        }
        input(
            world,
            panel,
            owner,
            InputKey::Threshold(index),
            "Threshold",
            &rule.value,
            false,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!("Program: {} (use selected program)", rule.program),
            Command::RuleProgram(index),
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Move rule earlier",
            Command::MoveRule(index),
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Remove rule",
            Command::RemoveRule(index),
        );
    }
    crate::native::sand_panel::button(world, panel, owner, "Add quantity rule", Command::AddRule);
    world.get_mut::<Editor>(owner).unwrap().rebuild = false;
}

fn set_record_quantity(world: &mut World, owner: Entity, quantity: Option<String>) {
    let preview = world.get::<Editor>(owner).unwrap().preview;
    world.get_mut::<Editor>(owner).unwrap().quantity = quantity.clone();
    let inputs: Vec<_> = world
        .query::<(Entity, &Input)>()
        .iter(world)
        .filter(|(_, input)| input.editor == owner && matches!(input.key, InputKey::Quantity))
        .map(|(entity, _)| entity)
        .collect();
    for input in inputs {
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        if text.value() != quantity.as_deref().unwrap_or("") {
            text.editor.set_text(quantity.as_deref().unwrap_or(""));
        }
    }
    world
        .get_mut::<shader::EffectPreview>(preview)
        .unwrap()
        .quantity = quantity;
}

fn capture(world: &mut World, owner: Entity) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.editor == owner)
        .map(|(input, text)| (input.key.clone(), text.value().to_string()))
        .collect();
    for (key, value) in values {
        let target = world.get::<Editor>(owner).unwrap().target.clone();
        match key {
            InputKey::GeometrySource => {
                if let model::spatial::Mode::Mesh(settings) =
                    &mut world.get_mut::<Editor>(owner).unwrap().effect.mode
                    && let Some(stage) = &mut settings.geometry
                {
                    stage.source = value;
                }
            }
            InputKey::DeformationSource => {
                if let model::spatial::Mode::Mesh(settings) =
                    &mut world.get_mut::<Editor>(owner).unwrap().effect.mode
                    && let Some(stage) = &mut settings.deformation
                {
                    stage.source = value;
                }
            }
            key @ (InputKey::EffectLabel | InputKey::NewEffectId | InputKey::NewEffectLabel) => {
                catalog::edit(world, owner, &key, value)
            }
            InputKey::ReferencedEffect => {
                if let Target::Surface(entity) = target
                    && (value.trim().is_empty() || model::valid_effect_id(value.trim()))
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    surface.effect_id = (!value.trim().is_empty()).then(|| value.trim().to_owned());
                }
            }
            key @ (InputKey::BindingLabel(_)
            | InputKey::BindingRecord(_)
            | InputKey::BindingOrgan(_)
            | InputKey::BindingNamespace(_)
            | InputKey::BindingPath(_)) => {
                numeric::edit(world, owner, &key, value);
            }
            InputKey::Source(name) => {
                world
                    .get_mut::<Editor>(owner)
                    .unwrap()
                    .effect
                    .programs
                    .insert(name, value);
            }
            InputKey::Threshold(index) => {
                if let Some(rule) = world
                    .get_mut::<Editor>(owner)
                    .unwrap()
                    .effect
                    .rules
                    .get_mut(index)
                {
                    rule.value = value;
                }
            }
            InputKey::Quantity => {
                let editor = world.get::<Editor>(owner).unwrap();
                let customized = matches!(target, Target::Record(_))
                    && value != editor.quantity.as_deref().unwrap_or("");
                if customized {
                    world.get_mut::<Editor>(owner).unwrap().custom_quantity = true;
                }
                world.get_mut::<Editor>(owner).unwrap().quantity =
                    model::quantity(&Value::String(value.clone()));
                if let Target::Surface(entity) = target
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    surface.quantity = value;
                }
            }
            InputKey::Record => {
                if let Target::Surface(entity) = target
                    && (value.trim().is_empty() || nucleus::valid_uid(value.trim(), "r"))
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    surface.record = (!value.trim().is_empty()).then(|| value.trim().to_string());
                }
            }
            InputKey::Organ => {
                if let Target::Surface(entity) = target
                    && (value.trim().is_empty() || nucleus::valid_uid(value.trim(), "r"))
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    surface.source = if value.trim().is_empty() {
                        Source::Local
                    } else {
                        Source::Organ(value.trim().into())
                    };
                }
            }
            InputKey::Strength => {
                if let Target::Surface(entity) = target
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                    && let Ok(value) = value.parse::<f32>()
                    && value.is_finite()
                {
                    surface.strength = value.clamp(0.0, 1.0);
                }
            }
            InputKey::Glow => {
                if let Target::Surface(entity) = target
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                    && let Ok(value) = value.parse::<f32>()
                    && value.is_finite()
                {
                    surface.glow = value.clamp(0.0, 24.0);
                }
            }
            InputKey::Range { task, end } => {
                let mut editor = world.get_mut::<Editor>(owner).unwrap();
                let range = if task {
                    &mut editor.effect.inputs.task
                } else {
                    &mut editor.effect.inputs.progress
                };
                if let Some(range) = range {
                    if end {
                        range.end = value
                    } else {
                        range.start = value
                    }
                }
            }
        }
    }
    let active = world.get::<Editor>(owner).unwrap().effect.clone();
    if matches!(
        world.get::<Editor>(owner).unwrap().target,
        Target::Record(_)
    ) {
        let preview = world.get::<Editor>(owner).unwrap().preview;
        let working = shader::working(world, preview).cloned();
        let mut editor = world.get_mut::<Editor>(owner).unwrap();
        let waiting = editor
            .bootstrap
            .as_ref()
            .is_some_and(|bootstrap| working.as_ref() != Some(bootstrap));
        editor.catalog.capture(&active);
        if !waiting {
            editor.catalog.capture_working(&active, working.as_ref());
        }
    }
    let editor = world.get::<Editor>(owner).unwrap();
    let (effect, quantity, preview, target) = (
        editor.effect.clone(),
        editor.quantity.clone(),
        editor.preview,
        editor.target.clone(),
    );
    let bootstrap = world.get::<Editor>(owner).unwrap().bootstrap.clone();
    let bootstrapping = bootstrap
        .as_ref()
        .is_some_and(|effect| shader::working(world, preview) != Some(effect));
    if !bootstrapping {
        world.get_mut::<Editor>(owner).unwrap().bootstrap = None;
    }
    if let Some(mut shader) = world.get_mut::<shader::EffectPreview>(preview) {
        if !bootstrapping {
            shader.effect = effect.clone();
        } else if let Some(bootstrap) = bootstrap {
            shader.effect = bootstrap;
        }
        shader.quantity = quantity;
    }
    if let Target::Surface(entity) = &target
        && let Some(surface) = world.get::<Surface>(*entity).cloned()
        && let Some(mut shader) = world.get_mut::<shader::EffectPreview>(preview)
    {
        shader.strength = surface.strength.min(0.65);
        shader.motion = surface.motion;
        shader.frozen = surface.frozen;
        shader.disabled = surface.disabled;
    }
    if let Target::Surface(entity) = &target
        && let Some(surface) = world.get::<Surface>(*entity).cloned()
    {
        world.entity_mut(preview).insert(numeric::Context {
            record: surface.record,
            source: surface.source,
        });
    }
    let reference = match &target {
        Target::Surface(entity) => world
            .get::<Surface>(*entity)
            .map(|surface| catalog::reference_status(world, surface))
            .unwrap_or_default(),
        Target::Record(_) => String::new(),
    };
    if let Target::Surface(entity) = target
        && let Some(mut surface) = world.get_mut::<Surface>(entity)
    {
        surface.draft = Some(effect);
    }
    let live = if let Target::Surface(entity) = target {
        surface_reading(world, entity)
    } else {
        None
    };
    let editor = world.get::<Editor>(owner).unwrap();
    let mut label = editor
        .effect
        .inputs
        .resolve(editor.quantity.as_deref())
        .label();
    if let Some(live) = live {
        label.push_str(&format!(
            "\nLive Sand: quantity {}; {}{}",
            live.quantity.as_deref().unwrap_or("unavailable"),
            live.inputs
                .map_or_else(|| "inactive".into(), model::ResolvedInputs::label),
            if live.enabled {
                ""
            } else {
                " · disabled or source unavailable"
            }
        ));
    }
    let numeric = numeric::status(world, preview);
    if !numeric.is_empty() {
        label.push_str(&format!(" · {numeric}"));
    }
    if !reference.is_empty() {
        label.push_str(&format!(" · {reference}"));
    }
    for (status, mut text) in world.query::<(&MappingStatus, &mut Text)>().iter_mut(world) {
        if status.0 == owner && text.0 != label {
            text.0.clone_from(&label);
        }
    }
}

#[derive(Clone)]
enum Command {
    Save,
    Reload,
    FollowQuantity,
    ReferenceEffect,
    ReferenceMode,
    Reset,
    Disable,
    Motion,
    Snapshot,
    Close,
    Select(String),
    Preset(String),
    AddProgram,
    RemoveProgram,
    Fallback,
    Operator(usize),
    RuleProgram(usize),
    RuleInput(usize),
    AddBinding,
    RemoveBinding(u8),
    BindingField(u8),
    MoveRule(usize),
    RemoveRule(usize),
    AddRule,
    Mapping(bool),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<Editor>(owner).is_none() {
            return;
        }
        capture(world, owner);
        let (target, preview, definition, pending) = {
            let editor = world.get::<Editor>(owner).unwrap();
            (
                editor.target.clone(),
                editor.preview,
                editor.effect.clone(),
                editor.pending.is_some(),
            )
        };
        if pending && !matches!(self, Self::Close) {
            world.get_mut::<Editor>(owner).unwrap().message =
                "Waiting for save confirmation".into();
            return;
        }
        match self {
            Self::Close => {
                world.despawn(owner);
                return;
            }
            Self::Save => {
                if matches!(target, Target::Record(_))
                    && world
                        .get::<Editor>(owner)
                        .unwrap()
                        .catalog
                        .active()
                        .is_none()
                {
                    world.get_mut::<Editor>(owner).unwrap().message =
                        "Selected effect is unavailable. Choose or create an effect before saving."
                            .into();
                    return;
                }
                let result = definition.validate().and_then(|()| {
                    if shader::working(world, preview) != Some(&definition) {
                        if world
                            .get::<shader::EffectPreview>(preview)
                            .is_some_and(|preview| preview.frozen)
                        {
                            return Err("Resume the snapshot before applying these edits.".into());
                        }
                        return Err(
                            "Wait for every program's GPU pipeline to be ready before saving."
                                .into(),
                        );
                    }
                    match &target {
                        Target::Surface(entity) => {
                            let mut surface = world
                                .get_mut::<Surface>(*entity)
                                .ok_or("The Sand was removed")?;
                            surface.effect = definition.clone();
                            surface.draft = None;
                            Ok(())
                        }
                        Target::Record(binding) => {
                            let editor = world.get::<Editor>(owner).unwrap();
                            if !editor.loaded {
                                return Err(
                                    "Wait for the Record extension to load before saving.".into()
                                );
                            }
                            let revision = editor
                                .revision
                                .checked_add(1)
                                .ok_or("Revision limit reached")?;
                            let sender =
                                sender(world, binding).ok_or("Connect to the Record's Organ")?;
                            let id = nucleus::new_uid("shader");
                            let payload = world
                                .get::<Editor>(owner)
                                .unwrap()
                                .catalog
                                .payload(definition.clone(), revision)?;
                            sender
                                .try_send(ClientMessage::Act {
                                    id: id.clone(),
                                    action: engine::actions::Action::SetExtension {
                                        target: binding.uid.clone(),
                                        namespace: model::NAMESPACE.into(),
                                        fds: serde_json::to_value(&payload).unwrap(),
                                    },
                                })
                                .map_err(|error| error.to_string())?;
                            world.get_mut::<Editor>(owner).unwrap().pending =
                                Some((id, Instant::now(), definition.clone()));
                            world.get_mut::<Editor>(owner).unwrap().catalog.pending = Some(payload);
                            Ok(())
                        }
                    }
                });
                let mut editor = world.get_mut::<Editor>(owner).unwrap();
                editor.message = result.err().unwrap_or_else(|| {
                    if matches!(target, Target::Record(_)) {
                        "Saving…".into()
                    } else {
                        editor.original = definition;
                        "Surface applied".into()
                    }
                });
            }
            Self::ReferenceEffect => {
                if let Target::Surface(entity) = target
                    && let Some(surface) = world.get::<Surface>(entity).cloned()
                    && let Some(uid) = &surface.record
                {
                    let binding = RecordBinding {
                        area: Entity::PLACEHOLDER,
                        uid: uid.clone(),
                        source: surface.source.clone(),
                    };
                    let extension = world
                        .resource::<Feeds>()
                        .0
                        .get(&feed_key(&binding))
                        .and_then(|feed| feed.row.as_ref())
                        .and_then(|row| {
                            serde_json::from_value::<Extension>(row["shader_effect"].clone()).ok()
                        });
                    if let Some(extension) = extension {
                        let ids: Vec<_> = std::iter::once(None)
                            .chain(extension.effects.keys().cloned().map(Some))
                            .collect();
                        let index = ids
                            .iter()
                            .position(|id| *id == surface.effect_id)
                            .unwrap_or(0);
                        world.get_mut::<Surface>(entity).unwrap().effect_id =
                            ids[(index + 1) % ids.len()].clone();
                        world.get_mut::<Editor>(owner).unwrap().rebuild = true;
                    }
                }
            }
            Self::ReferenceMode => {
                if let Target::Surface(entity) = target
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    surface.quantity_only = !surface.quantity_only;
                    world.get_mut::<Editor>(owner).unwrap().rebuild = true;
                }
            }
            Self::FollowQuantity => {
                if let Target::Record(binding) = &target {
                    let quantity = world
                        .resource::<Feeds>()
                        .0
                        .get(&feed_key(binding))
                        .and_then(|feed| feed.row.as_ref())
                        .and_then(|row| model::quantity(&row["quantity"]));
                    world.get_mut::<Editor>(owner).unwrap().custom_quantity = false;
                    set_record_quantity(world, owner, quantity);
                    let mut editor = world.get_mut::<Editor>(owner).unwrap();
                    editor.rebuild = true;
                    editor.message = "Following the Record quantity".into();
                }
            }
            Self::Reload => {
                if let Target::Record(binding) = &target {
                    let data = world
                        .resource::<Feeds>()
                        .0
                        .get(&feed_key(binding))
                        .and_then(|feed| feed.row.clone());
                    if let Some(data) = data {
                        let extension =
                            serde_json::from_value::<Extension>(data["shader_effect"].clone()).ok();
                        catalog::load(
                            world,
                            owner,
                            extension.unwrap_or_else(|| Extension::single(0, Effect::default())),
                            true,
                        );
                        world.get_mut::<Editor>(owner).unwrap().loaded = true;
                    } else {
                        world.get_mut::<Editor>(owner).unwrap().message =
                            "Wait for the Record extension to load before reloading.".into();
                    }
                } else {
                    let original = world.get::<Editor>(owner).unwrap().original.clone();
                    world.get_mut::<Editor>(owner).unwrap().effect = original;
                }
                world.get_mut::<Editor>(owner).unwrap().rebuild = true;
            }
            Self::Reset => {
                if let Some(mut preview) = world.get_mut::<shader::EffectPreview>(preview) {
                    preview.frozen = false;
                }
                let source = if let Target::Surface(entity) = target
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    surface.strength = 0.6;
                    surface.glow = 12.0;
                    surface.motion = true;
                    surface.frozen = false;
                    match surface.preset.as_str() {
                        "ripple" => model::RIPPLE,
                        "calm" => model::CALM,
                        "energy" => model::ENERGY,
                        "reward" => model::REWARD,
                        "reminder" => model::REMINDER,
                        "grain" => model::GRAIN,
                        "numeric" => model::NUMERIC,
                        "cleaning" => {
                            surface.strength = 0.65;
                            surface.glow = 0.0;
                            surface.motion = false;
                            model::CLEANING
                        }
                        _ => model::GRADIENT,
                    }
                } else {
                    model::GRADIENT
                };
                let mut editor = world.get_mut::<Editor>(owner).unwrap();
                let inputs = if editor.effect.inputs.validate().is_ok() {
                    editor.effect.inputs.clone()
                } else {
                    editor.original.inputs.clone()
                };
                let bindings = if model::bindings::validate(&editor.effect.bindings).is_ok() {
                    editor.effect.bindings.clone()
                } else {
                    editor.original.bindings.clone()
                };
                editor.effect = if matches!(target, Target::Record(_)) {
                    Effect::default()
                } else {
                    preset(source)
                };
                if source == model::CLEANING {
                    editor.effect.inputs = inputs;
                    editor.effect.bindings = bindings;
                }
                editor.rebuild = true;
            }
            Self::Disable | Self::Motion | Self::Snapshot => {
                if let Some(mut preview) = world.get_mut::<shader::EffectPreview>(preview) {
                    if matches!(self, Self::Disable) {
                        preview.disabled = !preview.disabled;
                    } else if matches!(self, Self::Snapshot) {
                        preview.frozen = !preview.frozen;
                    } else {
                        preview.motion = !preview.motion;
                    }
                }
                if let Target::Surface(entity) = target
                    && let Some(mut surface) = world.get_mut::<Surface>(entity)
                {
                    if matches!(self, Self::Disable) {
                        surface.disabled = !surface.disabled;
                    } else if matches!(self, Self::Snapshot) {
                        surface.frozen = !surface.frozen;
                    } else {
                        surface.motion = !surface.motion;
                    }
                }
            }
            _ => {
                let mut editor = world.get_mut::<Editor>(owner).unwrap();
                match self {
                    Self::AddBinding => {
                        if let Some(slot) = (0..8).find(|slot| {
                            !editor
                                .effect
                                .bindings
                                .iter()
                                .any(|binding| binding.slot == *slot)
                        }) {
                            editor
                                .effect
                                .bindings
                                .push(model::bindings::NumericBinding {
                                    slot,
                                    label: format!("Input {slot}"),
                                    source: default(),
                                });
                        } else {
                            editor.message = "Use at most eight numeric inputs".into();
                        }
                    }
                    Self::RemoveBinding(slot) => {
                        if editor
                            .effect
                            .rules
                            .iter()
                            .any(|rule| rule.binding == Some(*slot))
                        {
                            editor.message =
                                "Change rules that reference this numeric input before removing it"
                                    .into();
                        } else {
                            editor
                                .effect
                                .bindings
                                .retain(|binding| binding.slot != *slot);
                        }
                    }
                    Self::BindingField(slot) => {
                        if let Some(binding) = editor
                            .effect
                            .bindings
                            .iter_mut()
                            .find(|binding| binding.slot == *slot)
                        {
                            binding.source.field = match binding.source.field {
                                model::bindings::NumericField::Quantity => {
                                    model::bindings::NumericField::Extension {
                                        namespace: "nutrition".into(),
                                        path: vec!["protein".into()],
                                    }
                                }
                                model::bindings::NumericField::Extension { .. } => {
                                    model::bindings::NumericField::Quantity
                                }
                            };
                        }
                    }
                    Self::Select(name) => editor.selected = name.clone(),
                    Self::Preset(source) => {
                        if matches!(target, Target::Surface(_))
                            || source == model::REWARD
                            || source == model::REMINDER
                            || source == model::CLEANING
                        {
                            let inputs = editor.effect.inputs.clone();
                            let bindings = editor.effect.bindings.clone();
                            let mode = editor.effect.mode.clone();
                            editor.effect = preset(source);
                            editor.effect.inputs = inputs;
                            editor.effect.bindings = bindings;
                            editor.effect.mode = mode;
                            editor.selected = "surface".into();
                        } else {
                            let selected = editor.selected.clone();
                            editor.effect.programs.insert(selected, source.clone());
                        }
                    }
                    Self::Mapping(task) => {
                        let mapping = if *task {
                            &mut editor.effect.inputs.task
                        } else {
                            &mut editor.effect.inputs.progress
                        };
                        *mapping = if mapping.is_some() {
                            None
                        } else {
                            Some(model::QuantityRange::default())
                        };
                    }
                    Self::AddProgram => {
                        if editor.effect.programs.len() < 8 {
                            let name = (1..=9)
                                .map(|index| format!("program{index}"))
                                .find(|name| !editor.effect.programs.contains_key(name))
                                .unwrap();
                            let source = match editor.effect.mode {
                                model::spatial::Mode::Surface => model::CALM,
                                model::spatial::Mode::Mesh(_) => model::spatial::MESH_SHADE,
                                model::spatial::Mode::Volume(_) => model::spatial::VOLUME_CLOUD,
                            };
                            editor.effect.programs.insert(name.clone(), source.into());
                            editor.selected = name;
                        }
                    }
                    Self::RemoveProgram => {
                        let selected = editor.selected.clone();
                        if editor.effect.programs.len() > 1
                            && editor.effect.fallback != selected
                            && !editor
                                .effect
                                .rules
                                .iter()
                                .any(|rule| rule.program == selected)
                        {
                            editor.effect.programs.remove(&selected);
                            editor.selected = editor.effect.programs.keys().next().unwrap().clone();
                        } else {
                            editor.message =
                                "Remove references to this program before deleting it".into();
                        }
                    }
                    Self::Fallback => editor.effect.fallback = editor.selected.clone(),
                    Self::Operator(index) => {
                        if let Some(rule) = editor.effect.rules.get_mut(*index) {
                            rule.operator = match rule.operator {
                                Operator::Less => Operator::AtMost,
                                Operator::AtMost => Operator::Equal,
                                Operator::Equal => Operator::AtLeast,
                                Operator::AtLeast => Operator::Greater,
                                Operator::Greater => Operator::Less,
                            };
                        }
                    }
                    Self::RuleProgram(index) => {
                        let selected = editor.selected.clone();
                        if let Some(rule) = editor.effect.rules.get_mut(*index) {
                            rule.program = selected;
                        }
                    }
                    Self::RuleInput(index) => {
                        let slots: Vec<_> = std::iter::once(None)
                            .chain(
                                editor
                                    .effect
                                    .bindings
                                    .iter()
                                    .map(|binding| Some(binding.slot)),
                            )
                            .collect();
                        if let Some(rule) = editor.effect.rules.get_mut(*index) {
                            let index = slots
                                .iter()
                                .position(|slot| *slot == rule.binding)
                                .unwrap_or(0);
                            rule.binding = slots[(index + 1) % slots.len()];
                        }
                    }
                    Self::MoveRule(index) => {
                        if *index > 0 && *index < editor.effect.rules.len() {
                            editor.effect.rules.swap(*index, index - 1);
                        }
                    }
                    Self::RemoveRule(index) => {
                        if *index < editor.effect.rules.len() {
                            editor.effect.rules.remove(*index);
                        }
                    }
                    Self::AddRule => {
                        if editor.effect.rules.len() < 16 {
                            let program = editor.selected.clone();
                            editor.effect.rules.push(Rule {
                                binding: None,
                                operator: Operator::Less,
                                value: "0".into(),
                                program,
                            });
                        }
                    }
                    _ => {}
                }
                editor.rebuild = true;
                if let Self::Preset(source) = self
                    && let Target::Surface(entity) = target
                {
                    let name = if source == model::RIPPLE {
                        "ripple"
                    } else if source == model::CALM {
                        "calm"
                    } else if source == model::ENERGY {
                        "energy"
                    } else if source == model::REWARD {
                        "reward"
                    } else if source == model::REMINDER {
                        "reminder"
                    } else if source == model::GRAIN {
                        "grain"
                    } else if source == model::NUMERIC {
                        "numeric"
                    } else if source == model::CLEANING {
                        "cleaning"
                    } else {
                        "gradient"
                    };
                    let room_floor = world
                        .get::<crate::native::topology::rooms::Member>(entity)
                        .is_some_and(|member| member.0.role == nucleus::canvas::room::Role::Floor);
                    if let Some(mut surface) = world.get_mut::<Surface>(entity) {
                        surface.preset = name.into();
                        if source == model::CLEANING {
                            surface.quantity_only = true;
                            surface.room_ambient |= room_floor;
                            surface.glow = 0.0;
                            surface.motion = false;
                        }
                    }
                }
            }
        }
        if world.get::<Editor>(owner).unwrap().rebuild {
            let effect = &world.get::<Editor>(owner).unwrap().effect;
            if !effect
                .programs
                .contains_key(&world.get::<Editor>(owner).unwrap().selected)
            {
                let selected = effect.programs.keys().next().cloned().unwrap_or_default();
                world.get_mut::<Editor>(owner).unwrap().selected = selected;
            }
            render_editor(world, owner);
        }
    }
}

pub(crate) fn record(
    world: &mut World,
    row: Entity,
    config: &Config,
    data: &Value,
    binding: Option<RecordBinding>,
) {
    world.entity_mut(row).remove::<LightPreview>();
    let slots: Vec<_> = world
        .query::<(Entity, &EffectSlot)>()
        .iter(world)
        .filter(|(_, slot)| slot.0 == row)
        .map(|(entity, _)| entity)
        .collect();
    for slot in slots {
        world.despawn(slot);
    }
    let previous: Vec<_> = world
        .query::<(Entity, &Floating)>()
        .iter(world)
        .filter(|(_, floating)| floating.record == row)
        .map(|(entity, _)| entity)
        .collect();
    for entity in previous {
        world.despawn(entity);
    }
    let Some(binding) = binding else { return };
    let extension = catalog::decode(&data["shader_effect"]);
    let effect_id = placement::effective(world, &binding, &config.shader_placement).effect_id;
    let quantity = model::quantity(&data["quantity"]);
    let numeric_context = numeric::Context {
        record: Some(binding.uid.clone()),
        source: binding.source.clone(),
    };
    world.entity_mut(row).insert(RecordData {
        binding,
        extension: extension.clone(),
        quantity: quantity.clone(),
    });
    if !config.shader_effects {
        return;
    }
    let Some(extension) = extension else { return };
    let selected = extension.selected(effect_id.as_deref()).cloned();
    let slot = world
        .spawn((
            Node {
                width: percent(100),
                height: px(config.shader_placement.size
                    + config.shader_placement.gap
                    + if selected.as_ref().is_none_or(|effect| {
                        effect.inputs.task.is_some()
                            || effect.inputs.progress.is_some()
                            || !effect.bindings.is_empty()
                    }) {
                        18.0
                    } else {
                        0.0
                    }),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(row),
            EffectSlot(row),
        ))
        .id();
    world.entity_mut(row).insert_children(0, &[slot]);
    let status = crate::native::edit_mode::label(world, slot, "", 12.0);
    world.entity_mut(status).insert((
        RecordStateLabel(row),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(0),
            width: percent(100),
            align_self: AlignSelf::Center,
            ..default()
        },
    ));
    let Some(root) = world.get::<ChildOf>(row).map(ChildOf::parent) else {
        world.despawn(slot);
        return;
    };
    let workspace = world
        .get::<crate::native::workspace::WorkspaceMember>(row)
        .map_or(1, |member| member.0);
    let container = world
        .spawn((
            Node {
                width: px(config.shader_placement.size),
                height: px(config.shader_placement.size),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::canvas::CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(config.shader_placement.size),
            },
            bevy::picking::Pickable::IGNORE,
            crate::native::inspection::InspectionExcluded,
            crate::native::sand_store::SandCredits(crate::native::description::CREDITS),
        ))
        .id();
    let missing = selected.is_none();
    let preview = shader::effect(world, container, selected.unwrap_or_default(), quantity);
    world.entity_mut(row).insert(LightPreview(preview));
    world
        .get_mut::<shader::EffectPreview>(preview)
        .unwrap()
        .disabled = missing;
    world.entity_mut(preview).insert(numeric_context);
    let output = shader::output(world, preview);
    world.get_mut::<Node>(output).unwrap().height = percent(100);
    world.get_mut::<Node>(preview).unwrap().height = percent(100);
    world.entity_mut(container).insert(Floating {
        record: row,
        preview,
        distant: false,
    });
}

pub(crate) fn refresh_record(world: &mut World, row: Entity, data: &Value) {
    let Some(binding) = world
        .get::<RecordData>(row)
        .map(|record| record.binding.clone())
        .or_else(|| world.get::<RecordBinding>(row).cloned())
    else {
        return;
    };
    let Some(config) = crate::native::protein_area::configuration(world, binding.area) else {
        return;
    };
    let extension = serde_json::from_value::<Extension>(data["shader_effect"].clone()).ok();
    let floating = world
        .query::<&Floating>()
        .iter(world)
        .any(|floating| floating.record == row);
    let enabled = config.shader_effects && extension.is_some();
    if floating != enabled {
        record(world, row, &config, data, Some(binding));
        return;
    }
    world.entity_mut(row).insert(RecordData {
        binding,
        extension,
        quantity: model::quantity(&data["quantity"]),
    });
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    numeric::receive(world, source, message);
    if !world.contains_resource::<Feeds>() {
        return;
    }
    if matches!(
        message,
        ServerMessage::LiveHello {
            login_required: true
        } | ServerMessage::SessionAuthenticated { .. }
    ) || matches!(message, ServerMessage::Error { id, code, .. } if id == crate::native::cell_bridge::CONNECTION || id == "connection" || code.as_deref() == Some("session_expired"))
    {
        for feed in world
            .resource_mut::<Feeds>()
            .0
            .values_mut()
            .filter(|feed| feed.binding.source == *source)
        {
            feed.row = None;
            feed.sent = false;
            feed.error = "Reconnect to load Record effects".into();
            feed.retry = Instant::now() + Duration::from_secs(3);
        }
        let editors: Vec<_> = world.query::<(Entity, &Editor)>().iter(world).filter(|(_, editor)| matches!(&editor.target, Target::Record(binding) if binding.source == *source)).map(|(entity, _)| entity).collect();
        for editor in editors {
            world.get_mut::<Editor>(editor).unwrap().loaded = false;
            if !world.get::<Editor>(editor).unwrap().custom_quantity {
                set_record_quantity(world, editor, None);
            }
        }
    }
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            let mut feeds = world.resource_mut::<Feeds>();
            if let Some(feed) = feeds
                .0
                .values_mut()
                .find(|feed| feed.id == *id && feed.binding.source == *source && feed.sent)
            {
                feed.row = rows
                    .first()
                    .filter(|row| row["uid"].as_str() == Some(&feed.binding.uid))
                    .cloned();
                feed.error = if feed.row.is_none() {
                    "Record unavailable or not readable".into()
                } else {
                    String::new()
                };
            }
        }
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => {
            let owners: Vec<_> = world
                .query::<(Entity, &Editor)>()
                .iter(world)
                .filter(|(_, editor)| {
                    matches!(&editor.target, Target::Record(binding) if binding.source == *source)
                        && editor
                            .pending
                            .as_ref()
                            .is_some_and(|pending| pending.0 == *id)
                })
                .map(|(entity, _)| entity)
                .collect();
            for owner in owners {
                let mut editor = world.get_mut::<Editor>(owner).unwrap();
                if let ServerMessage::Error { message, .. } = message {
                    editor.message = message.clone();
                    editor.pending = None;
                    editor.catalog.pending = None;
                } else {
                    let (_, _, effect) = editor.pending.take().unwrap();
                    editor.original = effect;
                    editor.revision += 1;
                    editor.catalog.acknowledge();
                    editor.message = "Record effect saved".into();
                }
            }
            if let ServerMessage::Error { id, message, .. } = message {
                for feed in world
                    .resource_mut::<Feeds>()
                    .0
                    .values_mut()
                    .filter(|feed| feed.id == *id && feed.binding.source == *source)
                {
                    feed.error = message.clone();
                    feed.row = None;
                    feed.sent = false;
                    feed.retry = Instant::now() + Duration::from_secs(3);
                }
            }
        }
        _ => {}
    }
}

fn release_obsolete_feeds(
    world: &mut World,
    feeds: &mut Feeds,
    bindings: &HashMap<String, RecordBinding>,
) {
    let obsolete: Vec<_> = feeds
        .0
        .keys()
        .filter(|key| !bindings.contains_key(*key))
        .cloned()
        .collect();
    for key in obsolete {
        if feeds.1.len() >= 32 {
            break;
        }
        if let Some(feed) = feeds.0.remove(&key) {
            feeds.1.push((feed.binding, feed.id));
        }
    }
    feeds.1.retain(|(binding, id)| {
        let message = ClientMessage::Unsubscribe { id: id.clone() };
        if let Some(sender) = sender(world, binding)
            && matches!(
                sender.try_send(message.clone()),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_))
            )
        {
            return true;
        }
        crate::native::protein_area::auxiliary_subscribed(world, &binding.source, &message);
        false
    });
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let selections: Vec<_> = world
        .query::<(&placement::EffectInput, &EditableText)>()
        .iter(world)
        .map(|(input, text)| (input.0, text.value().to_string()))
        .collect();
    for (owner, id) in selections {
        let id = id.trim();
        if id.is_empty() || model::valid_effect_id(id) {
            placement::change(world, owner, |settings| {
                settings.effect_id = (!id.is_empty()).then(|| id.to_owned())
            });
        }
    }
    let placements: Vec<_> = world
        .query::<(&PlacementInput, &EditableText)>()
        .iter(world)
        .map(|(input, text)| (input.area, input.key, text.value().to_string()))
        .collect();
    for (area, key, text) in placements {
        if let Ok(value) = text.parse::<f32>() {
            placement::change(world, area, |settings| match key {
                0 => settings.size = value,
                1 => settings.gap = value,
                2 => settings.elevation = value,
                _ => settings.min_pixels = value,
            });
        }
    }
    let messages = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for message in messages {
        receive(world, &Source::Local, &message);
    }
    let editors: Vec<_> = world
        .query_filtered::<Entity, With<Editor>>()
        .iter(world)
        .collect();
    for editor in &editors {
        capture(world, *editor);
    }
    let mut bindings: HashMap<String, RecordBinding> = HashMap::new();
    for (_, editor) in world.query::<(Entity, &Editor)>().iter(world) {
        if let Target::Record(binding) = &editor.target {
            bindings.insert(feed_key(binding), binding.clone());
        }
    }
    for surface in world.query::<&Surface>().iter(world) {
        if let Some(uid) = &surface.record
            && nucleus::valid_uid(uid, "r")
        {
            let binding = RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: uid.clone(),
                source: surface.source.clone(),
            };
            bindings.insert(feed_key(&binding), binding);
        }
    }
    let mut feeds = world.remove_resource::<Feeds>().unwrap_or_default();
    release_obsolete_feeds(world, &mut feeds, &bindings);
    let retiring = !feeds.1.is_empty();
    for (key, binding) in bindings {
        if feeds.0.len() >= 64 && !feeds.0.contains_key(&key) {
            continue;
        }
        let feed = feeds.0.entry(key).or_insert_with(|| Feed {
            binding,
            id: nucleus::new_uid("shader-feed"),
            sent: false,
            row: None,
            error: String::new(),
            retry: Instant::now(),
        });
        crate::native::protein_area::ensure_auxiliary(world, &feed.binding.source);
        if !retiring
            && !feed.sent
            && feed.retry <= Instant::now()
            && !crate::native::laboratory::active(world)
            && let Some(sender) = sender(world, &feed.binding)
        {
            let protein = protein::Protein {
                source: protein::Source::Record,
                filter: vec![protein::Predicate::UidEq(feed.binding.uid.clone())],
                fields: Some(
                    ["uid", "quantity", "shader_effect"]
                        .map(str::to_string)
                        .into(),
                ),
                include: protein::Include {
                    shader_effect: true,
                    ..default()
                },
                aggregate: None,
                order: vec![],
                limit: Some(1),
            };
            let message = ClientMessage::Subscribe {
                id: feed.id.clone(),
                protein,
            };
            feed.sent = sender.try_send(message.clone()).is_ok();
            if feed.sent {
                crate::native::protein_area::auxiliary_subscribed(
                    world,
                    &feed.binding.source,
                    &message,
                );
            }
            feed.retry = Instant::now() + Duration::from_secs(3);
        }
    }
    world.insert_resource(feeds);
    for owner in editors {
        let (target, loaded, dirty, pending) = {
            let editor = world.get::<Editor>(owner).unwrap();
            (
                editor.target.clone(),
                editor.loaded,
                editor.catalog.dirty(&editor.effect),
                editor.pending.is_some(),
            )
        };
        if let Target::Record(binding) = target {
            let data = world
                .resource::<Feeds>()
                .0
                .get(&feed_key(&binding))
                .map(|feed| (feed.row.clone(), feed.error.clone()));
            if let Some((Some(row), _)) = data {
                let extension = catalog::decode(&row["shader_effect"])
                    .unwrap_or_else(|| Extension::single(0, Effect::default()));
                let revision = extension.revision;
                let previous_revision = world.get::<Editor>(owner).unwrap().revision;
                if (!loaded || revision != previous_revision) && !dirty && !pending {
                    catalog::load(world, owner, extension.clone(), false);
                    let mut editor = world.get_mut::<Editor>(owner).unwrap();
                    editor.loaded = true;
                    if !editor.custom_quantity {
                        editor.quantity = model::quantity(&row["quantity"]);
                    }
                    editor.rebuild = true;
                } else if !loaded
                    && revision == previous_revision
                    && extension == world.get::<Editor>(owner).unwrap().catalog.base
                {
                    let mut editor = world.get_mut::<Editor>(owner).unwrap();
                    editor.loaded = true;
                    if !editor.custom_quantity {
                        editor.quantity = model::quantity(&row["quantity"]);
                    }
                    editor.message = "Draft restored".into();
                    editor.rebuild = true;
                } else if !loaded {
                    world.get_mut::<Editor>(owner).unwrap().message = "The Record loaded while the draft was being edited. Reload working before saving.".into();
                } else if revision != previous_revision && dirty {
                    world.get_mut::<Editor>(owner).unwrap().message =
                        "Record effect changed elsewhere. Reload working before saving.".into();
                }
                let editor = world.get::<Editor>(owner).unwrap();
                let quantity = model::quantity(&row["quantity"]);
                if editor.loaded && !editor.custom_quantity && editor.quantity != quantity {
                    set_record_quantity(world, owner, quantity);
                }
            } else if let Some((_, error)) = data {
                world.get_mut::<Editor>(owner).unwrap().message = error;
                world.get_mut::<Editor>(owner).unwrap().loaded = false;
                if !world.get::<Editor>(owner).unwrap().custom_quantity {
                    set_record_quantity(world, owner, None);
                }
            }
        }
        let (status, message, rebuild) = {
            let editor = world.get::<Editor>(owner).unwrap();
            (editor.status, editor.message.clone(), editor.rebuild)
        };
        if let Some(mut text) = world.get_mut::<Text>(status)
            && text.0 != message
        {
            text.0 = message;
        }
        if world
            .get::<Editor>(owner)
            .unwrap()
            .pending
            .as_ref()
            .is_some_and(|pending| pending.1.elapsed() > Duration::from_secs(20))
        {
            world.get_mut::<Editor>(owner).unwrap().message =
                "Save confirmation timed out. Reload working before retrying.".into();
            world.get_mut::<Editor>(owner).unwrap().pending = None;
            world.get_mut::<Editor>(owner).unwrap().catalog.pending = None;
        }
        if rebuild {
            render_editor(world, owner);
        }
    }
    update_presentations(world);
    numeric::update(world);
    if (!world.resource::<Feeds>().0.is_empty() || !world.resource::<Feeds>().1.is_empty())
        && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
    {
        wake.after(Duration::from_secs(3));
    }
}

fn update_presentations(world: &mut World) {
    let off = disabled(world);
    let floating: Vec<_> = world
        .query::<(Entity, &Floating)>()
        .iter(world)
        .map(|(entity, floating)| (entity, floating.record, floating.preview))
        .collect();
    for (entity, record, preview) in floating {
        let Some(item) = world
            .get::<crate::native::canvas::CanvasItem>(record)
            .copied()
        else {
            world.despawn(entity);
            continue;
        };
        let Some(data) = world.get::<RecordData>(record).cloned() else {
            world.despawn(entity);
            continue;
        };
        let config = crate::native::protein_area::configuration(world, data.binding.area);
        if !config.as_ref().is_some_and(|config| config.shader_effects) {
            world.despawn(entity);
            continue;
        }
        let settings =
            placement::effective(world, &data.binding, &config.unwrap().shader_placement);
        let spatial = crate::native::topology::spatial(world, record);
        let offset = spatial.rotation()
            * DVec3::new(
                0.0,
                f64::from(settings.elevation),
                -f64::from(item.size.y) * 0.5 + f64::from(settings.size) * 0.5,
            );
        let mut placement = spatial;
        placement.elevation += offset.y;
        placement.depth = Some(1.0);
        if settings.billboard
            && root(world, record)
                .and_then(|root| world.get::<crate::native::topology::view::View>(root))
                .is_some_and(|view| view.spatial)
            && let Some(rotation) = world
                .query_filtered::<&Transform, With<crate::native::topology::presentation::WorldCamera>>()
                .iter(world)
                .next()
                .map(|transform| transform.rotation.as_dquat())
        {
            placement.rotation = (rotation
                * bevy::math::DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2))
            .to_array();
        }
        world.entity_mut(entity).insert((
            crate::native::canvas::CanvasItem {
                position: item.position + DVec2::new(offset.x, offset.z),
                size: Vec2::splat(settings.size),
            },
            placement,
        ));
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let size = px(settings.size);
            if node.width != size {
                node.width = size;
            }
            if node.height != size {
                node.height = size;
            }
        }
        let distant = placement::distant(world, entity, record, &settings);
        world.get_mut::<Floating>(entity).unwrap().distant = distant;
        let frozen = settings.frozen || distant && settings.distance == DistancePolicy::Thumbnail;
        let selected = data
            .extension
            .as_ref()
            .and_then(|extension| extension.selected(settings.effect_id.as_deref()))
            .cloned();
        if let Some(mut shader) = world.get_mut::<shader::EffectPreview>(preview) {
            shader.quantity = data.quantity;
            if let Some(effect) = selected.clone() {
                shader.effect = effect;
            }
            shader.disabled =
                off || selected.is_none() || distant && settings.distance == DistancePolicy::Hide;
            shader.motion = settings.motion;
            shader.frozen = frozen;
        }
        let data = world.get::<RecordData>(record).unwrap();
        let effect = selected
            .as_ref()
            .and_then(|selected| shader::working(world, preview).or(Some(selected)));
        let mut label = effect
            .filter(|effect| effect.inputs.task.is_some() || effect.inputs.progress.is_some())
            .map(|effect| effect.inputs.resolve(data.quantity.as_deref()).label())
            .unwrap_or_default();
        if selected.is_none() {
            label = "Selected effect unavailable".into();
        }
        let numeric = if selected.is_some() {
            numeric::status(world, preview)
        } else {
            String::new()
        };
        if !numeric.is_empty() {
            if !label.is_empty() {
                label.push_str(" · ");
            }
            label.push_str(&numeric);
        }
        if frozen {
            if !label.is_empty() {
                label.push_str(" · ");
            }
            label.push_str("Effect snapshot frozen");
        }
        let slots: Vec<_> = world
            .query::<(Entity, &EffectSlot)>()
            .iter(world)
            .filter(|(_, slot)| slot.0 == record)
            .map(|(entity, _)| entity)
            .collect();
        let label_height = if label.is_empty() {
            0.0
        } else {
            world
                .query::<(&RecordStateLabel, &ComputedNode)>()
                .iter(world)
                .filter(|(owner, _)| owner.0 == record)
                .map(|(_, node)| node.size().y * node.inverse_scale_factor() + 4.0)
                .fold(18.0_f32, f32::max)
        };
        for slot in slots {
            if let Some(mut node) = world.get_mut::<Node>(slot) {
                let height = px(settings.size + settings.gap + label_height);
                if node.height != height {
                    node.height = height;
                }
            }
        }
        for (owner, mut text, mut node) in world
            .query::<(&RecordStateLabel, &mut Text, &mut Node)>()
            .iter_mut(world)
        {
            if owner.0 == record {
                if text.0 != label {
                    text.0.clone_from(&label);
                }
                let display = if label.is_empty() {
                    Display::None
                } else {
                    Display::Flex
                };
                if node.display != display {
                    node.display = display;
                }
            }
        }
    }
    let backgrounds: Vec<_> = world
        .query::<(Entity, &Background)>()
        .iter(world)
        .map(|(entity, background)| (entity, background.preview))
        .collect();
    for (entity, preview) in backgrounds {
        if world.get::<Surface>(entity).is_none() && world.get_entity(preview).is_ok() {
            world.despawn(preview);
            world.entity_mut(entity).remove::<Background>();
        }
    }
    let surfaces: Vec<_> = world
        .query::<(Entity, &Surface)>()
        .iter(world)
        .map(|(entity, surface)| (entity, surface.clone()))
        .collect();
    for (entity, surface) in surfaces {
        let bound = surface.record.as_ref().map(|uid| RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: uid.clone(),
            source: surface.source.clone(),
        });
        let data = bound
            .as_ref()
            .and_then(|binding| world.resource::<Feeds>().0.get(&feed_key(binding)))
            .and_then(|feed| feed.row.clone());
        let extension = data
            .as_ref()
            .and_then(|row| catalog::decode(&row["shader_effect"]));
        let selected = extension
            .as_ref()
            .and_then(|extension| extension.selected(surface.effect_id.as_deref()));
        let valid_reference = bound.is_none()
            || if surface.quantity_only {
                data.is_some()
            } else {
                selected.is_some()
            };
        let effect = if bound.is_some() && !surface.quantity_only {
            selected.cloned().unwrap_or_else(|| preset(model::GRADIENT))
        } else {
            surface.effect.clone()
        };
        let quantity = if bound.is_some() {
            data.as_ref()
                .and_then(|row| model::quantity(&row["quantity"]))
        } else {
            model::quantity(&Value::String(surface.quantity.clone()))
        };
        let preview = world
            .get::<Background>(entity)
            .map(|background| background.preview)
            .filter(|preview| world.get_entity(*preview).is_ok())
            .unwrap_or_else(|| {
                let preview = shader::effect(world, entity, effect.clone(), quantity.clone());
                let output = shader::output(world, preview);
                world.entity_mut(preview).insert((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    ZIndex(-1),
                    bevy::picking::Pickable::IGNORE,
                ));
                world.entity_mut(output).insert((
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    bevy::picking::Pickable::IGNORE,
                ));
                world.entity_mut(entity).insert(Background { preview });
                preview
            });
        let enabled = !surface.disabled
            && !off
            && valid_reference
            && (!surface.room_ambient
                || crate::native::topology::rooms::ambient_enabled(world, entity));
        world.entity_mut(preview).insert(numeric::Context {
            record: surface.record.clone(),
            source: surface.source.clone(),
        });
        let radius = world
            .get::<Node>(entity)
            .map(|node| node.border_radius)
            .unwrap_or_default();
        world.get_mut::<Node>(preview).unwrap().border_radius = radius;
        world
            .entity_mut(preview)
            .insert(BoxShadow(if enabled && surface.glow > 0.0 {
                vec![ShadowStyle {
                    color: Color::srgba(0.2, 0.45, 1.0, surface.strength.min(0.35)),
                    x_offset: px(0),
                    y_offset: px(0),
                    spread_radius: px(0),
                    blur_radius: px(surface.glow / 3.0),
                }]
            } else {
                vec![]
            }));
        let output = shader::output(world, preview);
        world.get_mut::<Node>(output).unwrap().border_radius = radius;
        protect_text(world, entity, enabled);
        if let Some(mut shader) = world.get_mut::<shader::EffectPreview>(preview) {
            shader.effect = effect;
            shader.quantity = quantity;
            shader.strength = surface.strength.min(0.65);
            shader.motion = surface.motion;
            shader.frozen = surface.frozen;
            shader.disabled = !enabled;
        }
    }
}

#[derive(Component)]
struct ProtectedText;

pub(crate) fn protect_text(world: &mut World, owner: Entity, enabled: bool) {
    let enabled = enabled
        || world
            .get::<crate::native::topology::appearance::Material>(owner)
            .is_some_and(|value| value.0.valid());
    let mut descendants = Vec::new();
    let mut pending = vec![owner];
    while let Some(entity) = pending.pop() {
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
        if world.get::<Text>(entity).is_some() || world.get::<EditableText>(entity).is_some() {
            descendants.push(entity);
        }
    }
    for entity in descendants {
        if enabled
            && world.get::<ProtectedText>(entity).is_none()
            && world
                .get::<bevy::text::TextBackgroundColor>(entity)
                .is_none_or(|background| background.0 == Color::NONE)
        {
            let color = crate::native::token_style::resolve(
                world,
                entity,
                crate::native::tokens::Token::Surface,
            )
            .0
            .color()
            .with_alpha(1.0);
            world
                .entity_mut(entity)
                .insert((bevy::text::TextBackgroundColor(color), ProtectedText));
        } else if enabled && world.get::<ProtectedText>(entity).is_some() {
            let color = crate::native::token_style::resolve(
                world,
                entity,
                crate::native::tokens::Token::Surface,
            )
            .0
            .color()
            .with_alpha(1.0);
            if let Some(mut background) = world.get_mut::<bevy::text::TextBackgroundColor>(entity)
                && background.0 != color
            {
                background.0 = color;
            }
        } else if !enabled && world.get::<ProtectedText>(entity).is_some() {
            world
                .entity_mut(entity)
                .remove::<(bevy::text::TextBackgroundColor, ProtectedText)>();
        }
    }
}

pub(crate) fn protect_material_text(world: &mut World, owner: Entity) {
    let active = !disabled(world)
        && world
            .get::<Surface>(owner)
            .is_some_and(|value| !value.disabled)
        && world
            .get::<Background>(owner)
            .and_then(|background| world.get::<shader::EffectPreview>(background.preview))
            .is_some_and(|preview| !preview.disabled);
    protect_text(world, owner, active);
}

#[derive(Clone)]
pub(crate) struct ToggleAll;
impl Action for ToggleAll {
    fn apply(&self, world: &mut World, _: Entity) {
        let disabled = disabled(world);
        world.resource_mut::<ShaderSettings>().disabled = !disabled;
    }
}

pub(crate) fn controls(world: &mut World, root: Entity, panel: Entity) {
    crate::native::sand_panel::button(
        world,
        panel,
        root,
        if disabled(world) {
            "Shader effects: Off"
        } else {
            "Shader effects: On"
        },
        ToggleAll,
    );
    crate::native::sand_panel::button(
        world,
        panel,
        root,
        if motion_allowed(world) {
            "Shader reduced motion: Off"
        } else {
            "Shader reduced motion: On"
        },
        ToggleMotion,
    );
}

#[derive(Clone)]
struct ToggleMotion;
impl Action for ToggleMotion {
    fn apply(&self, world: &mut World, _: Entity) {
        let motion = motion_allowed(world);
        world.resource_mut::<ShaderSettings>().reduced_motion = motion;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedEditor {
    catalog: catalog::Draft,
    pub workspace: u64,
    record: String,
    source: Source,
    effect: Effect,
    original: Effect,
    revision: u64,
    quantity: Option<String>,
    #[serde(default)]
    custom_quantity: bool,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::native::sand_placement::Placement,
}

impl SavedEditor {
    pub(crate) fn matches(&self, binding: &RecordBinding) -> bool {
        self.record == binding.uid && self.source == binding.source
    }
    pub(crate) fn valid(&self) -> bool {
        nucleus::valid_uid(&self.record, "r")
            && self.catalog.valid()
            && draft_valid(&self.effect)
            && self.original.validate().is_ok()
            && self
                .quantity
                .as_ref()
                .is_none_or(|quantity| quantity.len() <= 128)
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && match &self.source {
                Source::Local => true,
                Source::Organ(uid) => nucleus::valid_uid(uid, "r"),
            }
    }

    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        self.restore_owner(world, root, None);
    }

    fn restore_owner(
        self,
        world: &mut World,
        root: Entity,
        binding: Option<RecordBinding>,
    ) -> Entity {
        let owner = open(
            world,
            root,
            Target::Record(binding.unwrap_or(RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: self.record,
                source: self.source,
            })),
            self.effect,
            self.revision,
            self.quantity,
        );
        let preview = world.get::<Editor>(owner).unwrap().preview;
        let bootstrap = self.catalog.working_target();
        world.get_mut::<Editor>(owner).unwrap().catalog = self.catalog;
        world.get_mut::<Editor>(owner).unwrap().bootstrap = Some(bootstrap.clone());
        world
            .get_mut::<shader::EffectPreview>(preview)
            .unwrap()
            .effect = bootstrap;
        world.get_mut::<Editor>(owner).unwrap().original = self.original;
        world.get_mut::<Editor>(owner).unwrap().custom_quantity = self.custom_quantity;
        world.entity_mut(owner).insert((
            crate::native::workspace::WorkspaceMember(self.workspace),
            crate::native::canvas::CanvasItem {
                position: DVec2::from_array(self.position),
                size: Vec2::from_array(self.size),
            },
        ));
        self.placement.restore(world, owner);
        render_editor(world, owner);
        owner
    }
}

pub(crate) fn capture_editor(world: &mut World, owner: Entity) -> Option<SavedEditor> {
    world.get::<Editor>(owner)?;
    capture(world, owner);
    let editor = world.get::<Editor>(owner)?;
    let Target::Record(binding) = &editor.target else {
        return None;
    };
    let item = world
        .get::<crate::native::canvas::CanvasItem>(owner)
        .copied()
        .unwrap_or(crate::native::canvas::CanvasItem {
            position: DVec2::ZERO,
            size: Vec2::new(700.0, 900.0),
        });
    Some(SavedEditor {
        catalog: editor.catalog.clone(),
        workspace: world
            .get::<crate::native::workspace::WorkspaceMember>(owner)?
            .0,
        record: binding.uid.clone(),
        source: binding.source.clone(),
        effect: editor.effect.clone(),
        original: editor.original.clone(),
        revision: editor.revision,
        quantity: editor.quantity.clone(),
        custom_quantity: editor.custom_quantity,
        position: item.position.to_array(),
        size: item.size.to_array(),
        placement: crate::native::sand_placement::Placement::capture(world, owner),
    })
}

pub(crate) fn embed_record(
    world: &mut World,
    root: Entity,
    parent: Entity,
    binding: RecordBinding,
    saved: Option<SavedEditor>,
) -> Entity {
    let owner = if let Some(saved) = saved {
        saved.restore_owner(world, root, Some(binding))
    } else {
        open(
            world,
            root,
            Target::Record(binding),
            Effect::default(),
            0,
            None,
        )
    };
    world.entity_mut(owner).remove::<(
        crate::native::canvas::CanvasItem,
        crate::native::sand::InBox,
        crate::native::castle::Castle,
        crate::native::sand::Square,
    )>();
    world.entity_mut(owner).insert((
        ChildOf(parent),
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0),
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(12)),
            row_gap: px(6),
            overflow: Overflow::scroll_y(),
            ..default()
        },
    ));
    let close_buttons: Vec<_> = world
        .query_filtered::<(Entity, &crate::native::actions::ActionButton), With<EditorClose>>()
        .iter(world)
        .filter(|(_, button)| button.target == owner)
        .map(|(entity, _)| entity)
        .collect();
    for button in close_buttons {
        world.despawn(button);
    }
    owner
}

pub(crate) fn example_visible(world: &World, owner: Entity) -> bool {
    world.get::<Editor>(owner).is_some_and(|editor| {
        editor.loaded
            && editor.catalog.active().is_some()
            && editor.revision > 0
            && editor.pending.is_none()
            && shader::working(world, editor.preview) == Some(&editor.effect)
    })
}

pub(crate) fn create_example(world: &mut World, owner: Entity) -> bool {
    let Some(editor) = world.get::<Editor>(owner) else {
        return true;
    };
    if !editor.loaded {
        return false;
    }
    if editor.revision > 0 || editor.pending.is_some() {
        return true;
    }
    let definition = preset(model::ENERGY);
    if editor.effect != definition {
        capture(world, owner);
        let mut editor = world.get_mut::<Editor>(owner).unwrap();
        editor.effect = definition;
        editor.selected = "surface".into();
        editor.rebuild = true;
        render_editor(world, owner);
        return false;
    }
    if shader::working(world, editor.preview) != Some(&editor.effect) {
        return false;
    }
    Command::Save.apply(world, owner);
    true
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedEditor> {
    world
        .query::<(
            Entity,
            &Editor,
            &ChildOf,
            &crate::native::canvas::CanvasItem,
            &crate::native::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .filter_map(|(entity, editor, parent, item, workspace)| {
            if parent.parent() != root {
                return None;
            }
            let Target::Record(binding) = &editor.target else {
                return None;
            };
            Some(SavedEditor {
                catalog: editor.catalog.clone(),
                workspace: workspace.0,
                record: binding.uid.clone(),
                source: binding.source.clone(),
                effect: editor.effect.clone(),
                original: editor.original.clone(),
                revision: editor.revision,
                quantity: editor.quantity.clone(),
                custom_quantity: editor.custom_quantity,
                position: item.position.to_array(),
                size: item.size.to_array(),
                placement: crate::native::sand_placement::Placement::capture(world, entity),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn record_packets_wait_for_a_new_subscription_after_session_changes() {
        let mut world = World::new();
        world.init_resource::<Feeds>();
        let uid = nucleus::new_uid("r");
        let binding = RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: uid.clone(),
            source: Source::Local,
        };
        let key = feed_key(&binding);
        world.resource_mut::<Feeds>().0.insert(
            key.clone(),
            Feed {
                binding,
                id: "record-feed".into(),
                sent: true,
                row: None,
                error: String::new(),
                retry: Instant::now(),
            },
        );
        let snapshot = ServerMessage::Snapshot {
            id: "record-feed".into(),
            rows: vec![serde_json::json!({"uid":uid,"quantity":"12"})],
        };
        receive(&mut world, &Source::Local, &snapshot);
        assert_eq!(
            world.resource::<Feeds>().0[&key].row.as_ref().unwrap()["quantity"],
            "12"
        );
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::SessionAuthenticated {
                id: "auth".into(),
                session_id: "renewed".into(),
                person: nucleus::new_uid("r"),
                key_id: "key".into(),
            },
        );
        receive(&mut world, &Source::Local, &snapshot);
        assert!(world.resource::<Feeds>().0[&key].row.is_none());
        world.resource_mut::<Feeds>().0.get_mut(&key).unwrap().sent = true;
        receive(&mut world, &Source::Local, &snapshot);
        assert!(world.resource::<Feeds>().0[&key].row.is_some());
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::Error {
                id: "session".into(),
                message: "Expired".into(),
                code: Some("session_expired".into()),
            },
        );
        receive(&mut world, &Source::Local, &snapshot);
        assert!(world.resource::<Feeds>().0[&key].row.is_none());
    }

    #[tokio::test]
    async fn record_feed_cleanup_is_bounded_and_retries_full_transports() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let bridge = crate::native::cell_bridge::connect(
            cell::CellRuntime {
                commands: default(),
                engine: engine.clone(),
                store: engine.store.clone(),
                lanes: Arc::new(cell::LaneHub::new()),
                wire: default(),
                fiote: None,
                speech: None,
                information: None,
            },
            crate::native::wake::WakeSignal::new(|| {}),
        );
        while bridge
            .outgoing
            .try_send(ClientMessage::Unsubscribe { id: "fill".into() })
            .is_ok()
        {}
        let mut world = World::new();
        world.insert_non_send(bridge);
        let mut feeds = Feeds::default();
        for index in 0..64 {
            let binding = RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: nucleus::new_uid("r"),
                source: Source::Local,
            };
            feeds.0.insert(
                feed_key(&binding),
                Feed {
                    binding,
                    id: format!("feed-{index}"),
                    sent: true,
                    row: None,
                    error: String::new(),
                    retry: Instant::now(),
                },
            );
        }
        release_obsolete_feeds(&mut world, &mut feeds, &default());
        assert_eq!(feeds.0.len(), 32);
        assert_eq!(feeds.1.len(), 32);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
                release_obsolete_feeds(&mut world, &mut feeds, &default());
                assert!(feeds.1.len() <= 32);
                if feeds.0.is_empty() && feeds.1.is_empty() {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn unavailable_record_quantities_do_not_turn_into_zero_and_manual_zero_stays_pinned() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        world.init_resource::<Feeds>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let owner = open(
            &mut world,
            root,
            Target::Record(RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: nucleus::new_uid("r"),
                source: Source::Local,
            }),
            Effect::default(),
            1,
            Some("3".into()),
        );
        let preview = world.get::<Editor>(owner).unwrap().preview;
        let disconnect = ServerMessage::Error {
            id: crate::native::cell_bridge::CONNECTION.into(),
            message: "Disconnected".into(),
            code: None,
        };
        receive(&mut world, &Source::Local, &disconnect);
        capture(&mut world, owner);
        assert!(world.get::<Editor>(owner).unwrap().quantity.is_none());
        assert!(!world.get::<Editor>(owner).unwrap().custom_quantity);
        assert!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .quantity
                .is_none()
        );
        let quantity = world
            .query::<(Entity, &Input)>()
            .iter(&world)
            .find(|(_, input)| input.editor == owner && matches!(input.key, InputKey::Quantity))
            .unwrap()
            .0;
        assert!(world.get::<EditableText>(quantity).unwrap().value() == "");
        world
            .get_mut::<EditableText>(quantity)
            .unwrap()
            .editor
            .set_text("0");
        capture(&mut world, owner);
        receive(&mut world, &Source::Local, &disconnect);
        assert_eq!(
            world.get::<Editor>(owner).unwrap().quantity.as_deref(),
            Some("0")
        );
        assert!(world.get::<Editor>(owner).unwrap().custom_quantity);
    }

    #[test]
    fn cleaning_floor_preserves_explicit_mappings_and_follows_room_ambient_and_source_loss() {
        use crate::native::{canvas_host::Identity, topology::rooms, workspace::WorkspaceMember};
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        world.init_resource::<ShaderSettings>();
        world.init_resource::<Feeds>();
        let root = world.spawn(Node::default()).id();
        let room_id = nucleus::new_uid("placement");
        let room = world
            .spawn((
                rooms::Room(default()),
                Identity(room_id.clone()),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let uid = nucleus::new_uid("r");
        let mut surface = Surface::cleaning();
        assert!(surface.valid());
        assert!(surface.effect.inputs.task.is_none());
        assert!(surface.effect.inputs.progress.is_none());
        assert!(surface.quantity.is_empty());
        surface.record = Some(uid.clone());
        surface.effect.inputs.task = Some(model::QuantityRange {
            start: "0".into(),
            end: "10".into(),
        });
        surface.effect.inputs.progress = surface.effect.inputs.task.clone();
        let floor = world
            .spawn((
                Node::default(),
                surface,
                rooms::Member(nucleus::canvas::room::Member {
                    room: room_id,
                    parent: None,
                    local: nucleus::canvas::room::identity_pose(),
                    role: nucleus::canvas::room::Role::Floor,
                }),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let binding = RecordBinding {
            area: Entity::PLACEHOLDER,
            uid,
            source: Source::Local,
        };
        let key = feed_key(&binding);
        world.resource_mut::<Feeds>().0.insert(
            key.clone(),
            Feed {
                binding,
                id: "cleaning".into(),
                sent: true,
                row: Some(serde_json::json!({"quantity":"0"})),
                error: String::new(),
                retry: Instant::now(),
            },
        );
        update_presentations(&mut world);
        let preview = world.get::<Background>(floor).unwrap().preview;
        let data = world.get::<shader::EffectPreview>(preview).unwrap();
        assert!(!data.disabled);
        assert_eq!(
            data.effect.inputs.resolve(data.quantity.as_deref()).task,
            model::TaskState::Pending
        );
        let reading = surface_reading(&world, floor).unwrap();
        assert_eq!(reading.quantity.as_deref(), Some("0"));
        assert!(reading.enabled);
        world
            .get_mut::<rooms::Room>(room)
            .unwrap()
            .0
            .ambient_enabled = false;
        update_presentations(&mut world);
        assert!(!surface_reading(&world, floor).unwrap().enabled);
        assert!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        world
            .get_mut::<rooms::Room>(room)
            .unwrap()
            .0
            .ambient_enabled = true;
        world.resource_mut::<Feeds>().0.get_mut(&key).unwrap().row =
            Some(serde_json::json!({"quantity":"5"}));
        update_presentations(&mut world);
        let data = world.get::<shader::EffectPreview>(preview).unwrap();
        assert!(!data.disabled);
        assert_eq!(
            data.effect
                .inputs
                .resolve(data.quantity.as_deref())
                .progress,
            Some(0.5)
        );
        world.resource_mut::<Feeds>().0.get_mut(&key).unwrap().row = None;
        update_presentations(&mut world);
        let data = world.get::<shader::EffectPreview>(preview).unwrap();
        assert!(data.disabled);
        assert!(data.quantity.is_none());
        let saved: Surface = serde_json::from_value(
            serde_json::to_value(world.get::<Surface>(floor).unwrap()).unwrap(),
        )
        .unwrap();
        let reading = surface_reading(&world, floor).unwrap();
        assert!(!reading.enabled && reading.quantity.is_none() && !reading.image_available);
        assert!(saved.valid() && saved.room_ambient && saved.quantity_only);
        assert_eq!(saved.effect.inputs.progress.unwrap().end, "10");
        world.despawn(room);
        assert!(!rooms::ambient_enabled(&world, floor));
    }

    #[test]
    fn local_surface_programs_follow_record_quantities_and_hide_unavailable_sources() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        world.init_resource::<ShaderSettings>();
        world.init_resource::<Feeds>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let uid = nucleus::new_uid("r");
        let local = preset(model::RIPPLE);
        let surface = world
            .spawn((
                Node::default(),
                Surface {
                    effect: local.clone(),
                    preset: "ripple".into(),
                    record: Some(uid.clone()),
                    quantity_only: true,
                    ..default()
                },
            ))
            .id();
        let binding = RecordBinding {
            area: Entity::PLACEHOLDER,
            uid,
            source: Source::Local,
        };
        let key = feed_key(&binding);
        world.resource_mut::<Feeds>().0.insert(
            key.clone(),
            Feed {
                binding,
                id: "test".into(),
                sent: true,
                row: Some(serde_json::json!({"quantity":"2", "shader_effect":null})),
                error: String::new(),
                retry: Instant::now(),
            },
        );
        update_presentations(&mut world);
        let preview = world.get::<Background>(surface).unwrap().preview;
        let data = world.get::<shader::EffectPreview>(preview).unwrap();
        assert_eq!(data.effect, local);
        assert_eq!(data.quantity.as_deref(), Some("2"));
        assert!(!data.disabled);
        world
            .resource_mut::<Feeds>()
            .0
            .get_mut(&key)
            .unwrap()
            .row
            .as_mut()
            .unwrap()["quantity"] = "5".into();
        update_presentations(&mut world);
        assert_eq!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .quantity
                .as_deref(),
            Some("5")
        );
        let editor = open(
            &mut world,
            root,
            Target::Surface(surface),
            local.clone(),
            0,
            Some("0".into()),
        );
        Command::ReferenceMode.apply(&mut world, editor);
        update_presentations(&mut world);
        assert!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        let remote = Effect::default();
        world
            .resource_mut::<Feeds>()
            .0
            .get_mut(&key)
            .unwrap()
            .row
            .as_mut()
            .unwrap()["shader_effect"] =
            serde_json::to_value(Extension::single(1, remote.clone())).unwrap();
        update_presentations(&mut world);
        assert_eq!(
            world.get::<shader::EffectPreview>(preview).unwrap().effect,
            remote
        );
        let effect_input = world
            .query::<(Entity, &Input)>()
            .iter(&world)
            .find(|(_, input)| {
                input.editor == editor && matches!(input.key, InputKey::ReferencedEffect)
            })
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(effect_input)
            .unwrap()
            .editor
            .set_text("missing");
        capture(&mut world, editor);
        update_presentations(&mut world);
        assert!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        assert_eq!(
            catalog::reference_status(&world, world.get::<Surface>(surface).unwrap()),
            "Referenced effect is unavailable"
        );
        Command::ReferenceMode.apply(&mut world, editor);
        update_presentations(&mut world);
        assert_eq!(
            world.get::<shader::EffectPreview>(preview).unwrap().effect,
            local
        );
        let restored: Surface = serde_json::from_value(
            serde_json::to_value(world.get::<Surface>(surface).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(restored.valid());
        assert!(restored.quantity_only);
        assert_eq!(restored.effect_id.as_deref(), Some("missing"));
        assert!(
            !world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        world.resource_mut::<Feeds>().0.get_mut(&key).unwrap().row = None;
        update_presentations(&mut world);
        let data = world.get::<shader::EffectPreview>(preview).unwrap();
        assert!(data.disabled);
        assert!(data.quantity.is_none());
        assert_eq!(world.get::<Surface>(surface).unwrap().effect, local);
        assert_eq!(world.get::<Background>(surface).unwrap().preview, preview);
    }

    #[test]
    fn mapping_controls_edit_drafts_and_preserve_mappings_across_presets() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let surface = world
            .spawn(Surface {
                quantity: "0.5".into(),
                ..default()
            })
            .id();
        let owner = open(
            &mut world,
            root,
            Target::Surface(surface),
            preset(model::GRADIENT),
            0,
            Some("0.5".into()),
        );
        Command::Mapping(true).apply(&mut world, owner);
        Command::Mapping(false).apply(&mut world, owner);
        let endpoint = world
            .query::<(Entity, &Input)>()
            .iter(&world)
            .find(|(_, input)| {
                input.editor == owner
                    && matches!(
                        &input.key,
                        InputKey::Range {
                            task: true,
                            end: true
                        }
                    )
            })
            .map(|(entity, _)| entity)
            .unwrap();
        world
            .get_mut::<EditableText>(endpoint)
            .unwrap()
            .editor
            .set_text("2");
        capture(&mut world, owner);
        let mapping = world.get::<Editor>(owner).unwrap().effect.inputs.clone();
        assert_eq!(mapping.task.as_ref().unwrap().end, "2");
        assert_eq!(mapping.resolve(Some("0.5")).task, model::TaskState::Active);
        assert_eq!(mapping.resolve(Some("0.5")).progress, Some(0.5));
        assert!(
            world
                .query::<(&MappingStatus, &Text)>()
                .iter(&world)
                .any(|(_, text)| text.0.contains("active") && text.0.contains("50.0%"))
        );
        Command::Preset(model::REWARD.into()).apply(&mut world, owner);
        capture(&mut world, owner);
        assert_eq!(world.get::<Editor>(owner).unwrap().effect.inputs, mapping);
        assert_eq!(world.get::<Surface>(surface).unwrap().preset, "reward");
        assert_eq!(
            world
                .get::<Surface>(surface)
                .unwrap()
                .draft
                .as_ref()
                .unwrap()
                .inputs,
            mapping
        );
        let restored: Surface = serde_json::from_value(
            serde_json::to_value(world.get::<Surface>(surface).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.draft.unwrap().inputs, mapping);
        Command::Mapping(true).apply(&mut world, owner);
        assert_eq!(
            world
                .get::<Editor>(owner)
                .unwrap()
                .effect
                .inputs
                .resolve(Some("0.5"))
                .task,
            model::TaskState::Unknown
        );
        let mut definition = Effect::default();
        definition.inputs = mapping.clone();
        let record = open(
            &mut world,
            root,
            Target::Record(RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: nucleus::new_uid("r"),
                source: Source::Local,
            }),
            definition,
            0,
            Some("0.5".into()),
        );
        Command::Preset(model::REWARD.into()).apply(&mut world, record);
        let effect = &world.get::<Editor>(record).unwrap().effect;
        assert_eq!(effect.inputs, mapping);
        assert!(effect.rules.is_empty());
        assert_eq!(effect.programs.len(), 1);
        assert_eq!(effect.programs[&effect.fallback], model::REWARD);
    }

    #[test]
    fn surface_snapshot_preserves_binding_draft_and_recovery_settings() {
        let mut surface = Surface::default();
        surface.draft = Some(preset("fn shade("));
        surface.disabled = true;
        assert!(surface.valid());
        let restored: Surface =
            serde_json::from_value(serde_json::to_value(&surface).unwrap()).unwrap();
        assert_eq!(restored.effect, surface.effect);
        assert_eq!(restored.draft, surface.draft);
        assert!(restored.disabled);
        surface.effect = preset("fn shade(");
        assert!(!surface.valid());
    }

    #[test]
    fn reset_restores_grain_and_resumes_a_snapshot_without_changing_its_record_binding() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let uid = nucleus::new_uid("r");
        let surface = world
            .spawn(Surface {
                preset: "grain".into(),
                effect: preset(model::GRAIN),
                record: Some(uid.clone()),
                frozen: true,
                ..default()
            })
            .id();
        let restored: Surface = serde_json::from_value(
            serde_json::to_value(world.get::<Surface>(surface).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(restored.valid());
        assert!(restored.frozen);
        let editor = open(
            &mut world,
            root,
            Target::Surface(surface),
            restored.effect,
            0,
            Some("0".into()),
        );
        Command::Reset.apply(&mut world, editor);
        let surface = world.get::<Surface>(surface).unwrap();
        assert_eq!(surface.record.as_deref(), Some(uid.as_str()));
        assert!(!surface.frozen);
        let data = world.get::<Editor>(editor).unwrap();
        assert_eq!(data.effect.programs[&data.effect.fallback], model::GRAIN);
        assert!(
            !world
                .get::<shader::EffectPreview>(data.preview)
                .unwrap()
                .frozen
        );
    }

    #[test]
    fn castles_select_effects_independently_and_report_deleted_selections() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<ShaderSettings>();
        world.init_resource::<Feeds>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let uid = nucleus::new_uid("r");
        let mut extension = Extension::single(1, Effect::default());
        let calm = preset(model::CALM);
        extension.effects.insert(
            "calm".into(),
            model::NamedEffect {
                label: "Calm".into(),
                effect: calm.clone(),
            },
        );
        let data = serde_json::json!({"uid":uid,"quantity":"-1","shader_effect":extension});
        let mut rows = Vec::new();
        for id in [None, Some("calm".to_owned())] {
            let mut config = Config {
                shader_effects: true,
                ..Config::records()
            };
            config.shader_placement.effect_id = id;
            let config: Config =
                serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
            let mut area = crate::native::area::InfluenceArea::new(
                crate::native::area::AreaShape::Circle,
                DVec2::ZERO,
                DVec2::splat(1000.0),
            );
            area.protein = Some(config.clone());
            let area = world.spawn((area, ChildOf(root))).id();
            let binding = RecordBinding {
                area,
                uid: uid.clone(),
                source: Source::Local,
            };
            let row = world
                .spawn((
                    Node::default(),
                    ChildOf(root),
                    crate::native::canvas::CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::new(320.0, 200.0),
                    },
                ))
                .id();
            record(&mut world, row, &config, &data, Some(binding));
            rows.push(row);
        }
        update_presentations(&mut world);
        let previews: Vec<_> = rows
            .iter()
            .map(|row| {
                world
                    .query::<&Floating>()
                    .iter(&world)
                    .find(|floating| floating.record == *row)
                    .unwrap()
                    .preview
            })
            .collect();
        assert_eq!(
            world
                .get::<shader::EffectPreview>(previews[0])
                .unwrap()
                .effect,
            Effect::default()
        );
        assert_eq!(
            world
                .get::<shader::EffectPreview>(previews[1])
                .unwrap()
                .effect,
            calm
        );
        extension.effects.remove("calm");
        extension.revision += 1;
        let refreshed = serde_json::json!({"uid":uid,"quantity":"-1","shader_effect":extension});
        for row in &rows {
            refresh_record(&mut world, *row, &refreshed);
        }
        update_presentations(&mut world);
        assert!(
            !world
                .get::<shader::EffectPreview>(previews[0])
                .unwrap()
                .disabled
        );
        assert!(
            world
                .get::<shader::EffectPreview>(previews[1])
                .unwrap()
                .disabled
        );
        assert!(
            world
                .query::<(&RecordStateLabel, &Text)>()
                .iter(&world)
                .any(|(owner, text)| {
                    owner.0 == rows[1] && text.0 == "Selected effect unavailable"
                })
        );
    }

    #[test]
    fn floating_effects_follow_record_coordinates_and_do_not_duplicate_on_refresh() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<ShaderSettings>();
        world.init_resource::<Feeds>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let mut area = crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Circle,
            DVec2::ZERO,
            DVec2::splat(1000.0),
        );
        let config = Config {
            shader_effects: true,
            ..Config::records()
        };
        area.protein = Some(config.clone());
        let area = world.spawn((area, ChildOf(root))).id();
        let uid = nucleus::new_uid("r");
        let binding = RecordBinding {
            area,
            uid: uid.clone(),
            source: Source::Local,
        };
        let row = world
            .spawn((
                Node::default(),
                ChildOf(root),
                crate::native::workspace::WorkspaceMember(1),
                crate::native::canvas::CanvasItem {
                    position: DVec2::new(100.0, 200.0),
                    size: Vec2::new(320.0, 200.0),
                },
            ))
            .id();
        let mut definition = Effect::default();
        definition.inputs.task = Some(model::QuantityRange {
            start: "-2".into(),
            end: "0".into(),
        });
        definition.inputs.progress = definition.inputs.task.clone();
        let data = serde_json::json!({"uid":uid,"quantity":"-1","shader_effect":Extension::single(1, definition)});
        record(&mut world, row, &config, &data, Some(binding.clone()));
        update_presentations(&mut world);
        assert!(
            world
                .query::<(&RecordStateLabel, &Text)>()
                .iter(&world)
                .any(|(owner, text)| owner.0 == row
                    && text.0.contains("active")
                    && text.0.contains("50.0%"))
        );
        let floating = world
            .query_filtered::<Entity, With<Floating>>()
            .single(&world)
            .unwrap();
        assert_eq!(
            world
                .get::<crate::native::canvas::CanvasItem>(floating)
                .unwrap()
                .position,
            DVec2::new(100.0, 160.0)
        );
        assert_eq!(
            crate::native::topology::spatial(&world, floating).elevation,
            40.0
        );
        world
            .get_mut::<crate::native::canvas::CanvasItem>(row)
            .unwrap()
            .position += DVec2::new(300.0, 80.0);
        update_presentations(&mut world);
        assert_eq!(
            world
                .get::<crate::native::canvas::CanvasItem>(floating)
                .unwrap()
                .position,
            DVec2::new(400.0, 240.0)
        );
        let mut refreshed = data.clone();
        refreshed["quantity"] = serde_json::json!("0");
        refresh_record(&mut world, row, &refreshed);
        update_presentations(&mut world);
        assert!(
            world
                .query::<(&RecordStateLabel, &Text)>()
                .iter(&world)
                .any(|(owner, text)| owner.0 == row
                    && text.0.contains("completed")
                    && text.0.contains("100.0%"))
        );
        let preview = world.get::<Floating>(floating).unwrap().preview;
        assert_eq!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .quantity
                .as_deref(),
            Some("0")
        );
        record(&mut world, row, &config, &data, Some(binding));
        assert_eq!(world.query::<&Floating>().iter(&world).count(), 1);
        assert_eq!(world.query::<&EffectSlot>().iter(&world).count(), 1);
    }

    #[test]
    fn generated_record_surfaces_survive_area_capture_and_restore() {
        let mut world = World::new();
        let root = world.spawn(Node::default()).id();
        let area = world
            .spawn((
                crate::native::area::InfluenceArea::new(
                    crate::native::area::AreaShape::Circle,
                    DVec2::ZERO,
                    DVec2::splat(1000.0),
                ),
                ChildOf(root),
            ))
            .id();
        let uid = nucleus::new_uid("r");
        let binding = RecordBinding {
            area,
            uid: uid.clone(),
            source: Source::Local,
        };
        let mut surface = Surface::default();
        surface.disabled = true;
        surface.draft = Some(preset("fn shade("));
        world.spawn((
            Node::default(),
            ChildOf(root),
            binding.clone(),
            crate::native::record_presentation::RecordPresentation::default(),
            surface.clone(),
        ));
        let captured = crate::native::record_presentation::capture(
            &world,
            area,
            world
                .get::<crate::native::area::InfluenceArea>(area)
                .unwrap()
                .clone(),
        );
        assert_eq!(
            captured.records[&uid].shader_surface.as_ref(),
            Some(&surface)
        );
        world
            .get_mut::<crate::native::area::InfluenceArea>(area)
            .unwrap()
            .records = captured.records;
        let row = world.spawn((Node::default(), ChildOf(root), binding)).id();
        crate::native::record_presentation::restore(&mut world, row, false);
        assert_eq!(world.get::<Surface>(row), Some(&surface));
    }

    #[test]
    fn effect_projection_does_not_truncate_record_queries() {
        let mut config = Config {
            shader_effects: true,
            ..Config::records()
        };
        config.draft.query["limit"] = serde_json::json!(1000);
        let query = config.query().unwrap();
        assert_eq!(query.limit, Some(1000));
        assert!(query.include.shader_effect);
    }
}
