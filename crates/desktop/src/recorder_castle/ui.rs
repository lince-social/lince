use super::*;
use crate::{actions::Action, castle_feed::button, edit_mode::label, sound::dsp::Distortion};
use bevy::text::EditableText;

#[derive(Component)]
pub(super) struct Name(pub Entity);
#[derive(Component)]
struct Search {
    owner: Entity,
    last: String,
}

#[derive(Clone)]
pub(super) enum Control {
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
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(world, owner, workspace, position, RecorderCastle::default());
            return;
        }
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        inputs(world);
        let Some(castle) = world.get::<RecorderCastle>(owner).cloned() else {
            return;
        };
        let command = match self {
            Self::Record => {
                if world.get::<View>(owner).is_some_and(|view| view.recording) {
                    return;
                }
                Command::Record(owner, crate::sound::library::recording_path(&castle.name))
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
                if !crate::sound::library::valid_path(path) {
                    return;
                }
                world.get_mut::<RecorderCastle>(owner).unwrap().selected = path.clone();
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
        match crate::sound::send(world, command) {
            Ok(()) => status(world, owner, "Working…"),
            Err(error) => status(world, owner, &error),
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
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
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
}

pub(super) fn effects(world: &mut World, owner: Entity) {
    let parent = world.get::<View>(owner).unwrap().effects;
    clear(world, parent);
    let castle = world.get::<RecorderCastle>(owner).unwrap().clone();
    label(world, parent, "TRACK FX · selected recording", 18.0);
    if castle.selected.is_empty() {
        label(
            world,
            parent,
            "Record or select a take below to shape its sound.",
            13.0,
        );
        return;
    }
    label(world, parent, &castle.selected, 13.0);
    let effect = castle.effect();
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
    for (index, title, value) in [
        (0, "Drive", effect.drive),
        (1, "Tone", effect.tone),
        (2, "Bit crush", effect.crush),
        (3, "Wet / dry", effect.mix),
        (4, "Output level", effect.level),
    ] {
        let slider = crate::slider::spawn(
            world,
            parent,
            title,
            crate::slider::SliderSand {
                start: 0.0,
                end: 1.0,
                step: 0.01,
                decimals: 2,
            },
            value,
            "",
        )
        .unwrap();
        world.entity_mut(slider).observe(
            move |event: On<crate::slider::SliderChanged>, mut commands: Commands| {
                let value = event.value;
                commands.queue(move |world: &mut World| {
                    if !value.is_finite() || crate::laboratory::suspended(world, owner) {
                        return;
                    }
                    let Some(mut castle) = world.get_mut::<RecorderCastle>(owner) else {
                        return;
                    };
                    let selected = castle.selected.clone();
                    if selected.is_empty() {
                        return;
                    }
                    let effect = castle.effects.entry(selected).or_default();
                    let field = match index {
                        0 => &mut effect.drive,
                        1 => &mut effect.tone,
                        2 => &mut effect.crush,
                        3 => &mut effect.mix,
                        _ => &mut effect.level,
                    };
                    *field = value.clamp(0.0, 1.0);
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
}

pub(super) fn list(world: &mut World, owner: Entity) {
    let parent = world.get::<View>(owner).unwrap().list;
    let query = world
        .query::<(&Search, &EditableText)>()
        .iter(world)
        .find(|(search, _)| search.owner == owner)
        .map(|(_, text)| text.value().to_string())
        .unwrap_or_default();
    clear(world, parent);
    label(world, parent, "Recordings", 18.0);
    button(world, parent, owner, "Refresh recordings", Control::Refresh);
    let search = input(world, parent, "Find a recording", &query);
    world.entity_mut(search).insert(Search {
        owner,
        last: query.clone(),
    });
    render_matches(world, owner, &query);
}

#[derive(Component)]
struct Matches(Entity);

fn render_matches(world: &mut World, owner: Entity, query: &str) {
    let old: Vec<_> = world
        .query::<(Entity, &Matches)>()
        .iter(world)
        .filter(|(_, m)| m.0 == owner)
        .map(|(e, _)| e)
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let parent = world.get::<View>(owner).unwrap().list;
    let matches = stack(world, parent);
    world.entity_mut(matches).insert(Matches(owner));
    let selected = world.get::<RecorderCastle>(owner).unwrap().selected.clone();
    let paths: Vec<_> = world
        .get_resource::<crate::sound::Audio>()
        .map(|audio| {
            crate::sound::library::suggestions(&audio.paths, query)
                .into_iter()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if paths.is_empty() {
        label(world, matches, "No matching recordings yet.", 13.0);
    }
    for path in paths {
        let title = if path == selected {
            format!("● {path}")
        } else {
            path.clone()
        };
        button(world, matches, owner, &title, Control::Select(path));
    }
}

pub(super) fn inputs(world: &mut World) {
    let names: Vec<_> = world
        .query::<(&Name, &EditableText)>()
        .iter(world)
        .map(|(name, text)| (name.0, text.value().to_string()))
        .collect();
    for (owner, name) in names {
        if let Some(mut castle) = world.get_mut::<RecorderCastle>(owner)
            && castle.name != name
        {
            castle.name = name;
        }
    }
    let searches: Vec<_> = world
        .query::<(Entity, &Search, &EditableText)>()
        .iter(world)
        .filter(|(_, search, text)| search.last != text.value().to_string())
        .map(|(e, search, text)| (e, search.owner, text.value().to_string()))
        .collect();
    for (entity, owner, value) in searches {
        world.get_mut::<Search>(entity).unwrap().last = value.clone();
        render_matches(world, owner, &value);
    }
}

pub(super) fn refresh(world: &mut World) {
    let (revision, error) = world
        .get_resource::<crate::sound::Audio>()
        .map(|audio| (audio.revision, audio.error.clone()))
        .unwrap_or_default();
    let owners: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.revision != revision)
        .map(|(e, _)| e)
        .collect();
    for owner in owners {
        world.get_mut::<View>(owner).unwrap().revision = revision;
        list(world, owner);
        if let Some(error) = &error {
            status(world, owner, error);
        }
    }
}
