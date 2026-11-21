// non-lean-yet
// Scope: mod maintenance, enum Change, fn allowed, struct StatusLabel.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 80, 82, 102, 134,
//   229.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (238).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (48, 62, 67, 68, 78, 199, 210, 217, 220).

use super::*;
use crate::native::{actions::Action, castle_feed::button, edit_mode::label};
use bevy::text::EditableText;

mod maintenance;

#[derive(Clone)]
enum Change {
    Toggle,
    Choose(bool, String),
    Preview(bool),
    Refresh,
}

fn allowed(world: &World, owner: Entity) -> Option<Entity> {
    let root = world.get::<ChildOf>(owner)?.parent();
    (crate::native::area_panel::owns(world, root, owner)
        && world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled)
        && world
            .get::<crate::native::area_panel::AreaEditor>(root)
            .is_some_and(|editor| editor.selected == Some(owner)))
    .then_some(root)
}

impl Action for Change {
    fn tutorial_supports(&self) -> &'static [lince_interface::practice::Operation] {
        match self {
            Self::Choose(true, _) => &[lince_interface::practice::Operation::AssignAreaSound],
            _ => &[],
        }
    }
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        match self {
            Self::Toggle => &[Operation::EnableAreaSound],
            Self::Preview(true) => &[Operation::PreviewAreaSound],
            _ => &[],
        }
    }
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
                crate::native::edit_mode::render_panel(world, root);
            }
            Self::Choose(enter, path) => {
                if !path.is_empty() && !crate::native::sound::library::valid_path(path) {
                    return;
                }
                if world
                    .get::<InfluenceArea>(owner)
                    .unwrap()
                    .sound
                    .as_ref()
                    .is_some_and(|sound| (if *enter { &sound.enter } else { &sound.leave }) != path)
                {
                    let mut area = world.get_mut::<InfluenceArea>(owner).unwrap();
                    let sound = area.sound.as_mut().unwrap();
                    path.clone_into(if *enter {
                        &mut sound.enter
                    } else {
                        &mut sound.leave
                    });
                }
                crate::native::edit_mode::render_panel(world, root);
            }
            Self::Preview(enter) => {
                if let Some(sound) = &world.get::<InfluenceArea>(owner).unwrap().sound {
                    let path = if *enter {
                        sound.enter.clone()
                    } else {
                        sound.leave.clone()
                    };
                    let volume = sound.volume;
                    world
                        .entity_mut(owner)
                        .remove::<crate::native::sound::PlaybackResult>();
                    world
                        .entity_mut(owner)
                        .insert(crate::native::sound::PlaybackPending);
                    let result = crate::native::sound::send(
                        world,
                        crate::native::sound::Command::Play {
                            owner,
                            path,
                            effects: None,
                            volume,
                        },
                    );
                    if let Err(error) = result {
                        world.entity_mut(owner).insert((
                            SoundStatus(error.clone()),
                            crate::native::sound::PlaybackResult(Err(error)),
                        ));
                    }
                }
            }
            Self::Refresh => {
                let _ = crate::native::sound::send_to(
                    world,
                    owner,
                    crate::native::sound::Command::Refresh,
                );
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
        let field = crate::native::recorder_castle::ui::input(world, panel, title, &path);
        if enter {
            world
                .entity_mut(field)
                .insert(crate::native::actions::TutorialControl {
                    owner,
                    operation: lince_interface::practice::Operation::AssignAreaSound,
                });
        }
        let suggestions = crate::native::recorder_castle::ui::stack(world, panel);
        world.entity_mut(field).insert(Input {
            owner,
            enter,
            suggestions,
            last: String::new(),
            revision: u64::MAX,
        });
        let row = crate::native::recorder_castle::ui::row(world, panel);
        button(world, row, owner, "Preview", Change::Preview(enter));
        button(
            world,
            row,
            owner,
            "Clear",
            Change::Choose(enter, String::new()),
        );
    }
    let slider = crate::native::slider::spawn(
        world,
        panel,
        "Sound volume",
        crate::native::slider::SliderSand {
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
        move |event: On<crate::native::slider::SliderChanged>, mut commands: Commands| {
            let value = event.value;
            commands.queue(move |world: &mut World| {
                if allowed(world, owner).is_none() || !value.is_finite() {
                    return;
                }
                let value = value.clamp(0.0, 1.0);
                if world
                    .get::<InfluenceArea>(owner)
                    .unwrap()
                    .sound
                    .as_ref()
                    .is_some_and(|sound| sound.volume.to_bits() != value.to_bits())
                {
                    world
                        .get_mut::<InfluenceArea>(owner)
                        .unwrap()
                        .sound
                        .as_mut()
                        .unwrap()
                        .volume = value;
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
    world
        .run_system_cached(maintenance::capture)
        .expect("capture current Area sound controls");
}

pub(crate) fn assign(world: &mut World, owner: Entity, path: &str) {
    if world
        .get::<InfluenceArea>(owner)
        .is_some_and(|area| area.sound.is_none())
    {
        Change::Toggle.apply(world, owner);
    }
    Change::Choose(true, path.into()).apply(world, owner);
}

pub(crate) fn enable(world: &mut World, owner: Entity) {
    if world
        .get::<InfluenceArea>(owner)
        .is_some_and(|area| area.sound.is_none())
    {
        Change::Toggle.apply(world, owner);
    }
}

pub(crate) fn preview(world: &mut World, owner: Entity) {
    if world
        .get::<crate::native::sound::PlaybackResult>(owner)
        .is_none()
        && world
            .get::<crate::native::sound::PlaybackPending>(owner)
            .is_none()
    {
        Change::Preview(true).apply(world, owner);
    }
}
