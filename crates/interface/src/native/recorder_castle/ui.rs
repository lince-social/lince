// non-lean-yet
// Scope: mod choices, mod maintenance, mod presentation, struct Name.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 126, 131, 149,
//   296, 369.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (398, 412,
//   418).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (138, 148, 235, 288, 293, 347, 370).

use super::*;
use crate::native::{
    actions::Action, castle_feed::button, edit_mode::label, sound::dsp::Distortion,
};
use bevy::text::EditableText;

mod choices;
mod maintenance;
mod presentation;
pub(crate) use choices::{Target as RecordingTarget, recordings as recording_choices};

#[derive(Component)]
pub(super) struct Name(pub Entity);
#[derive(Component)]
struct Search {
    owner: Entity,
    last: String,
}

#[derive(Clone, Copy)]
enum EffectField {
    Drive,
    Tone,
    Crush,
    Mix,
    Level,
}

impl EffectField {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Drive, "Drive"),
        (Self::Tone, "Tone"),
        (Self::Crush, "Bit crush"),
        (Self::Mix, "Wet / dry"),
        (Self::Level, "Output level"),
    ];

    fn get(self, effect: &Effects) -> f32 {
        match self {
            Self::Drive => effect.drive,
            Self::Tone => effect.tone,
            Self::Crush => effect.crush,
            Self::Mix => effect.mix,
            Self::Level => effect.level,
        }
    }

    fn set(self, effect: &mut Effects, value: f32) {
        let target = match self {
            Self::Drive => &mut effect.drive,
            Self::Tone => &mut effect.tone,
            Self::Crush => &mut effect.crush,
            Self::Mix => &mut effect.mix,
            Self::Level => &mut effect.level,
        };
        *target = value;
    }
}

#[derive(Clone)]
pub(crate) enum Control {
    Create,
    Record,
    Finish,
    Play,
    Stop,
    Apply,
    Refresh,
    Select(String),
    Enabled,
    Mode(Distortion),
    Reset,
}

impl Action for Control {
    fn tutorial_supports(&self) -> &'static [lince_interface::practice::Operation] {
        if matches!(
            self,
            Self::Select(_) | Self::Enabled | Self::Mode(_) | Self::Reset | Self::Refresh
        ) {
            &[lince_interface::practice::Operation::PreviewRecording]
        } else {
            &[]
        }
    }
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        match self {
            Self::Play => &[Operation::PreviewRecording],
            Self::Stop => &[Operation::StopRecordingPlayback],
            _ => &[],
        }
    }
    fn apply(&self, world: &mut World, owner: Entity) {
        let _action = nucleus::performance::Span::new("recorder-control-action");
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::native::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::native::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(world, owner, workspace, position, RecorderCastle::default());
            return;
        }
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        inputs(world);
        let Some(castle) = world.get::<RecorderCastle>(owner) else {
            return;
        };
        let command = match self {
            Self::Record => {
                if world.get::<View>(owner).is_some_and(|view| view.recording) {
                    return;
                }
                Command::Record(
                    owner,
                    crate::native::sound::library::recording_path(&castle.name),
                )
            }
            Self::Finish => Command::Finish(owner),
            Self::Play => Command::Play {
                owner,
                path: castle.selected.clone(),
                effects: Some(castle.effect()),
                volume: 1.0,
            },
            Self::Stop => Command::Stop(owner),
            Self::Apply => Command::Apply(owner, castle.selected.clone(), castle.effect()),
            Self::Refresh => Command::Refresh,
            Self::Select(path) => {
                if !crate::native::sound::library::valid_path(path) {
                    return;
                }
                if castle.selected != *path {
                    path.clone_into(&mut world.get_mut::<RecorderCastle>(owner).unwrap().selected);
                }
                effects(world, owner);
                list(world, owner);
                return;
            }
            Self::Enabled | Self::Mode(_) | Self::Reset => {
                if castle.selected.is_empty() {
                    return;
                }
                let mut castle = world.get_mut::<RecorderCastle>(owner).unwrap();
                let selected = castle.selected.clone();
                let effect = castle.effects.entry(selected).or_default();
                match self {
                    Self::Enabled => effect.enabled = !effect.enabled,
                    Self::Mode(mode) => {
                        effect.distortion = *mode;
                        effect.enabled = true;
                    }
                    Self::Reset => *effect = Effects::default(),
                    _ => {}
                }
                effects(world, owner);
                return;
            }
            Self::Create => return,
        };
        let playback = matches!(command, Command::Play { .. });
        if playback {
            world.entity_mut(owner).remove::<(
                crate::native::sound::PlaybackResult,
                crate::native::sound::StoppedPlayback,
            )>();
            world
                .entity_mut(owner)
                .insert(crate::native::sound::PlaybackPending);
        }
        if matches!(command, Command::Stop(_)) {
            world
                .entity_mut(owner)
                .insert(crate::native::sound::StopPending);
        }
        match crate::native::sound::send_to(world, owner, command) {
            Ok(()) => status(world, owner, "Working…"),
            Err(error) => {
                status(world, owner, &error);
                if playback {
                    world
                        .entity_mut(owner)
                        .insert(crate::native::sound::PlaybackResult(Err(error)));
                }
            }
        }
    }
}

pub(crate) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(crate) fn stack(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(crate) fn input(world: &mut World, parent: Entity, title: &str, value: &str) -> Entity {
    let entity = world
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(entity).insert(ChildOf(parent));
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.max_characters = Some(180);
    text.visible_lines = Some(1.0);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(title);
    }
    entity
}

pub(crate) fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

pub(super) fn transport(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let (parent, recording) = (view.transport, view.recording);
    if crate::native::retained::Presentation::matches(world, parent, |old: &(Entity, bool)| {
        *old == (owner, recording)
    }) {
        return;
    }
    clear(world, parent);
    button(
        world,
        parent,
        owner,
        if recording {
            "■ Stop & save"
        } else {
            "● Record"
        },
        if recording {
            Control::Finish
        } else {
            Control::Record
        },
    );
    button(world, parent, owner, "▶ Preview FX", Control::Play);
    button(world, parent, owner, "■ Stop playback", Control::Stop);
    crate::native::retained::Presentation::remember(world, parent, (owner, recording));
}

pub(super) fn effects(world: &mut World, owner: Entity) {
    let _effects = nucleus::performance::Span::new("recorder-effect-presentation");
    let parent = world.get::<View>(owner).unwrap().effects;
    if presentation::effects_match(world, owner, parent) {
        nucleus::performance::add("recorder-effect-controls-retained", 1);
        return;
    }
    let castle = world.get::<RecorderCastle>(owner).unwrap();
    let state = presentation::EffectState {
        owner,
        selected: castle.selected.clone(),
        effect: (!castle.selected.is_empty()).then(|| castle.effect()),
    };
    clear(world, parent);
    nucleus::performance::add("recorder-effect-controls-built", 1);
    label(world, parent, "TRACK FX · selected recording", 18.0);
    let Some(effect) = state.effect else {
        label(
            world,
            parent,
            "Record or select a take below to shape its sound.",
            13.0,
        );
        presentation::remember_effects(world, parent, state);
        return;
    };
    label(world, parent, &state.selected, 13.0);
    let modes = row(world, parent);
    button(
        world,
        modes,
        owner,
        if effect.enabled { "FX ON" } else { "FX BYPASS" },
        Control::Enabled,
    );
    for (mode, title) in [
        (Distortion::Overdrive, "Overdrive"),
        (Distortion::Fuzz, "Fuzz"),
        (Distortion::BitCrush, "Bit crush"),
    ] {
        let title = if effect.distortion == mode {
            format!("● {title}")
        } else {
            title.into()
        };
        button(world, modes, owner, &title, Control::Mode(mode));
    }
    for (field, title) in EffectField::ALL {
        let slider = crate::native::slider::spawn(
            world,
            parent,
            title,
            crate::native::slider::SliderSand {
                start: 0.0,
                end: 1.0,
                step: 0.01,
                decimals: 2,
            },
            field.get(&effect),
            "",
        )
        .unwrap();
        world.entity_mut(slider).observe(
            move |event: On<crate::native::slider::SliderChanged>, mut commands: Commands| {
                let value = event.value;
                commands.queue(move |world: &mut World| {
                    if !value.is_finite() || crate::native::laboratory::suspended(world, owner) {
                        return;
                    }
                    let Some(castle) = world.get::<RecorderCastle>(owner) else {
                        return;
                    };
                    if castle.selected.is_empty() {
                        return;
                    }
                    let value = value.clamp(0.0, 1.0);
                    if castle
                        .effects
                        .get(&castle.selected)
                        .is_some_and(|effect| field.get(effect).to_bits() == value.to_bits())
                    {
                        return;
                    }
                    let selected = castle.selected.clone();
                    let mut castle = world.get_mut::<RecorderCastle>(owner).unwrap();
                    let effect = castle.effects.entry(selected).or_default();
                    field.set(effect, value);
                });
            },
        );
    }
    let actions = row(world, parent);
    button(
        world,
        actions,
        owner,
        "Save FX for sound areas",
        Control::Apply,
    );
    button(world, actions, owner, "Reset FX", Control::Reset);
    label(
        world,
        parent,
        "Preview hears these controls. Save FX updates the WAV; the original take is kept.",
        12.0,
    );
    presentation::remember_effects(world, parent, state);
}

pub(super) fn list(world: &mut World, owner: Entity) {
    let query = world
        .run_system_cached_with(presentation::current_query, owner)
        .expect("read current recording search");
    render_matches(world, owner, &query);
}

#[derive(Component)]
struct Matches(Entity);

fn render_matches(world: &mut World, owner: Entity, query: &str) {
    presentation::matches(world, owner, query);
}

pub(super) fn inputs(world: &mut World) {
    world
        .run_system_cached(maintenance::inputs)
        .expect("capture current Recorder inputs");
}

pub(super) fn refresh(world: &mut World) {
    world
        .run_system_cached(maintenance::refresh)
        .expect("refresh current recording libraries");
}

pub(crate) fn preview(world: &mut World, owner: Entity) {
    if world
        .get::<crate::native::sound::PlaybackResult>(owner)
        .is_none()
        && world
            .get::<crate::native::sound::PlaybackPending>(owner)
            .is_none()
    {
        Control::Play.apply(world, owner);
    }
}

pub(crate) fn stop(world: &mut World, owner: Entity) {
    if world
        .get::<crate::native::sound::StoppedPlayback>(owner)
        .is_none()
        && world
            .get::<crate::native::sound::StopPending>(owner)
            .is_none()
    {
        Control::Stop.apply(world, owner);
    }
}
