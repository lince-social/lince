use super::*;
use crate::{actions::Action, castle_feed::button, edit_mode::label};
use bevy::text::EditableText;

#[derive(Clone)]
enum Change {
    Toggle,
    Choose(bool, String),
    Preview(bool),
    Refresh,
}

fn allowed(world: &World, owner: Entity) -> Option<Entity> {
    let root = world.get::<ChildOf>(owner)?.parent();
    (crate::area_panel::owns(world, root, owner)
        && world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled)
        && world
            .get::<crate::area_panel::AreaEditor>(root)
            .is_some_and(|editor| editor.selected == Some(owner)))
    .then_some(root)
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = allowed(world, owner) else {
            return;
        };
        match self {
            Self::Toggle => {
                let mut area = world.get_mut::<InfluenceArea>(owner).unwrap();
                area.sound = if area.sound.is_some() {
                    None
                } else {
                    Some(SoundArea::default())
                };
                crate::edit_mode::render_panel(world, root);
            }
            Self::Choose(enter, path) => {
                if !path.is_empty() && !crate::sound::library::valid_path(path) {
                    return;
                }
                if let Some(sound) = &mut world.get_mut::<InfluenceArea>(owner).unwrap().sound {
                    if *enter {
                        sound.enter = path.clone();
                    } else {
                        sound.leave = path.clone();
                    }
                }
                crate::edit_mode::render_panel(world, root);
            }
            Self::Preview(enter) => {
                if let Some(sound) = &world.get::<InfluenceArea>(owner).unwrap().sound {
                    let path = if *enter {
                        sound.enter.clone()
                    } else {
                        sound.leave.clone()
                    };
                    let result = crate::sound::send(
                        world,
                        crate::sound::Command::Play {
                            owner,
                            path,
                            effects: None,
                            volume: sound.volume,
                        },
                    );
                    if let Err(error) = result {
                        world.entity_mut(owner).insert(SoundStatus(error));
                    }
                }
            }
            Self::Refresh => {
                let _ = crate::sound::send(world, crate::sound::Command::Refresh);
            }
        }
    }
}

#[derive(Component)]
struct StatusLabel(Entity);

#[derive(Component)]
struct Input {
    owner: Entity,
    enter: bool,
    suggestions: Entity,
    last: String,
    revision: u64,
}

pub(crate) fn controls(world: &mut World, _: Entity, panel: Entity, owner: Entity) {
    let sound = world
        .get::<InfluenceArea>(owner)
        .and_then(|area| area.sound.clone());
    label(world, panel, "Sound on crossing", 18.0);
    button(
        world,
        panel,
        owner,
        if sound.is_some() {
            "Disable sound area"
        } else {
            "Enable sound area"
        },
        Change::Toggle,
    );
    let Some(sound) = sound else { return };
    label(
        world,
        panel,
        "Choose a recording path. Empty means silent.",
        12.0,
    );
    for (enter, path, title) in [
        (true, sound.enter, "On enter"),
        (false, sound.leave, "On leave"),
    ] {
        label(world, panel, title, 14.0);
        let field = crate::recorder_castle::ui::input(world, panel, title, &path);
        let suggestions = crate::recorder_castle::ui::stack(world, panel);
        world.entity_mut(field).insert(Input {
            owner,
            enter,
            suggestions,
            last: String::new(),
            revision: u64::MAX,
        });
        let row = crate::recorder_castle::ui::row(world, panel);
        button(world, row, owner, "Preview", Change::Preview(enter));
        button(
            world,
            row,
            owner,
            "Clear",
            Change::Choose(enter, String::new()),
        );
    }
    let slider = crate::slider::spawn(
        world,
        panel,
        "Sound volume",
        crate::slider::SliderSand {
            start: 0.0,
            end: 1.0,
            step: 0.01,
            decimals: 2,
        },
        sound.volume,
        "",
    )
    .unwrap();
    world.entity_mut(slider).observe(
        move |event: On<crate::slider::SliderChanged>, mut commands: Commands| {
            let value = event.value;
            commands.queue(move |world: &mut World| {
                if allowed(world, owner).is_some()
                    && value.is_finite()
                    && let Some(sound) = &mut world.get_mut::<InfluenceArea>(owner).unwrap().sound
                {
                    sound.volume = value.clamp(0.0, 1.0);
                }
            });
        },
    );
    button(world, panel, owner, "Refresh recordings", Change::Refresh);
    let message = world
        .get::<SoundStatus>(owner)
        .map(|status| status.0.clone())
        .unwrap_or_default();
    let status = label(world, panel, &message, 12.0);
    world.entity_mut(status).insert(StatusLabel(owner));
}

pub(super) fn inputs(world: &mut World) {
    let statuses: Vec<_> = world
        .query::<(Entity, &StatusLabel)>()
        .iter(world)
        .map(|(entity, label)| {
            let message = world
                .get::<SoundStatus>(label.0)
                .map(|status| status.0.clone())
                .unwrap_or_default();
            (entity, message)
        })
        .collect();
    for (entity, message) in statuses {
        if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != message
        {
            text.0 = message;
        }
    }
    let revision = world
        .get_resource::<crate::sound::Audio>()
        .map_or(0, |audio| audio.revision);
    let fields: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter(|(_, input, text)| {
            input.last != text.value().to_string() || input.revision != revision
        })
        .map(|(e, input, text)| {
            (
                e,
                input.owner,
                input.enter,
                input.suggestions,
                text.value().to_string(),
            )
        })
        .collect();
    for (entity, owner, enter, suggestions, value) in fields {
        if allowed(world, owner).is_none() {
            continue;
        }
        if (value.is_empty() || crate::sound::library::valid_path(&value))
            && let Some(sound) = &mut world.get_mut::<InfluenceArea>(owner).unwrap().sound
        {
            if enter {
                sound.enter = value.clone();
            } else {
                sound.leave = value.clone();
            }
        }
        let mut input = world.get_mut::<Input>(entity).unwrap();
        input.last = value.clone();
        input.revision = revision;
        crate::recorder_castle::ui::clear(world, suggestions);
        let paths: Vec<_> = world
            .get_resource::<crate::sound::Audio>()
            .map(|audio| {
                crate::sound::library::suggestions(&audio.paths, &value)
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if paths.is_empty() {
            label(
                world,
                suggestions,
                "Record a take in Recorder Castle, or refresh the recordings list.",
                12.0,
            );
        }
        for path in paths {
            button(
                world,
                suggestions,
                owner,
                &path,
                Change::Choose(enter, path.clone()),
            );
        }
    }
}
