use super::*;
use crate::actions::Action;
use lince_interface::sound::{Cue, Mode, Output};
use std::collections::HashSet;

#[derive(Component)]
struct Panel(Entity);

#[derive(Component)]
struct Snapshot(u64, lince_interface::sound::Settings);

#[derive(Resource, Default)]
struct Runtime {
    owners: HashSet<u64>,
}

pub(super) fn controls(world: &mut World, owner: Entity, parent: Entity) {
    let row = crate::sand_panel::row(world, parent);
    for (label, mode) in [
        ("Sound off", Mode::Off),
        ("Blip", Mode::Blip),
        ("Speak title", Mode::Title),
    ] {
        chrome::button(world, row, owner, label, Change::Mode(mode));
    }
    let label =
        crate::edit_mode::label(world, parent, "Sound off · volume 60% · system voice", 12.0);
    world.entity_mut(owner).insert(Panel(label));
    let row = crate::sand_panel::row(world, parent);
    for (label, change) in [
        ("Volume −", Change::Volume(-10)),
        ("Volume +", Change::Volume(10)),
        ("Next voice", Change::Voice),
        ("Test sound", Change::Test),
    ] {
        chrome::button(world, row, owner, label, change);
    }
}

#[derive(Clone)]
enum Change {
    Mode(Mode),
    Volume(i16),
    Voice,
    Test,
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        match self {
            Self::Mode(mode) => {
                if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
                    settings.0.sound.mode = *mode;
                }
                if *mode != Mode::Off {
                    world.init_resource::<crate::sound_cues::Native>();
                }
                if let Some(native) = world.get_resource::<crate::sound_cues::Native>() {
                    let _ = native.cancel(owner.to_bits());
                    if *mode == Mode::Title {
                        let _ = native.catalog();
                    }
                }
            }
            Self::Volume(delta) => {
                if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
                    settings.0.sound.volume =
                        (i16::from(settings.0.sound.volume) + delta).clamp(0, 100) as u8;
                }
            }
            Self::Voice => {
                world.init_resource::<crate::sound_cues::Native>();
                let voices = world.resource::<crate::sound_cues::Native>().voices.clone();
                if voices.is_empty() {
                    let _ = world.resource::<crate::sound_cues::Native>().catalog();
                } else if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
                    let next = settings
                        .0
                        .sound
                        .voice
                        .as_ref()
                        .and_then(|id| voices.iter().position(|voice| voice.id == *id))
                        .map_or(Some(0), |index| {
                            (index + 1 < voices.len()).then_some(index + 1)
                        });
                    settings.0.sound.voice = next.map(|index| voices[index].id.clone());
                }
            }
            Self::Test => {
                world.init_resource::<crate::sound_cues::Native>();
                let mut settings = world.get::<TimeSettings>(owner).unwrap().0.sound.clone();
                if settings.mode == Mode::Off {
                    settings.mode = Mode::Blip;
                }
                if let Err(error) = world.resource::<crate::sound_cues::Native>().preview(Cue {
                    scope: owner.to_bits(),
                    key: "preview".into(),
                    at_ms: 0,
                    title: "Time Castle".into(),
                    projected: false,
                    settings,
                }) {
                    world.resource_mut::<crate::sound_cues::Native>().error = Some(error);
                }
            }
        }
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

pub(super) fn update(world: &mut World) {
    if crate::laboratory::active(world) {
        return;
    }
    if let Some(mut native) = world.get_resource_mut::<crate::sound_cues::Native>() {
        native.poll();
    }
    let owners: Vec<_> = world
        .query::<(Entity, &TimeSettings, &View)>()
        .iter(world)
        .filter(|(_, settings, _)| settings.0.sound.mode != Mode::Off && settings.0.valid())
        .map(|(entity, _, _)| entity)
        .collect();
    if !owners.is_empty() {
        world.init_resource::<Runtime>();
        world.init_resource::<crate::sound_cues::Native>();
    }
    let now = chrono::Utc::now().timestamp_millis();
    if world.contains_resource::<Runtime>() {
        let mut runtime = world.remove_resource::<Runtime>().unwrap();
        let active: HashSet<_> = owners.iter().map(|entity| entity.to_bits()).collect();
        if let Some(native) = world.get_resource::<crate::sound_cues::Native>() {
            for scope in runtime.owners.difference(&active) {
                let _ = native.cancel(*scope);
            }
        }
        for owner in owners {
            let view = world.get::<View>(owner).unwrap();
            let settings = world.get::<TimeSettings>(owner).unwrap().0.sound.clone();
            if world
                .get::<Snapshot>(owner)
                .is_some_and(|snapshot| snapshot.0 == view.revision && snapshot.1 == settings)
                && runtime.owners.contains(&owner.to_bits())
            {
                continue;
            }
            let revision = view.revision;
            let cues = cues(view, owner.to_bits(), &settings);
            if let Some(native) = world.get_resource::<crate::sound_cues::Native>()
                && let Err(error) = native.schedule(owner.to_bits(), now, cues)
            {
                world.resource_mut::<crate::sound_cues::Native>().error = Some(error);
            }
            world.entity_mut(owner).insert(Snapshot(revision, settings));
        }
        runtime.owners = active;
        world.insert_resource(runtime);
    }
    let panels: Vec<_> = world
        .query::<(Entity, &Panel, &TimeSettings)>()
        .iter(world)
        .map(|(owner, panel, settings)| (owner, panel.0, settings.0.sound.clone()))
        .collect();
    for (_, label, settings) in panels {
        let native = world.get_resource::<crate::sound_cues::Native>();
        let voice = settings
            .voice
            .as_ref()
            .and_then(|id| {
                native.and_then(|native| native.voices.iter().find(|voice| voice.id == *id))
            })
            .map_or("system voice", |voice| voice.name.as_str());
        let mode = match settings.mode {
            Mode::Off => "Sound off",
            Mode::Blip => "Blip",
            Mode::Title => "Spoken titles",
        };
        let message = format!(
            "{mode} · volume {}% · {voice}{}",
            settings.volume,
            native
                .and_then(|native| native.error.as_ref())
                .map_or(String::new(), |error| format!("\n{error}"))
        );
        if let Some(mut text) = world.get_mut::<Text>(label)
            && text.0 != message
        {
            text.0 = message;
        }
    }
}

pub(super) fn cues(
    view: &View,
    scope: u64,
    settings: &lince_interface::sound::Settings,
) -> Vec<Cue> {
    view.entries
        .iter()
        .filter_map(|entry| entry.sound_cue(scope, settings))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deadline(world: &mut World, expected: Option<i64>) {
        let started = std::time::Instant::now();
        loop {
            let mut native = world.resource_mut::<crate::sound_cues::Native>();
            native.poll();
            if native.next_ms == expected {
                break;
            }
            assert!(started.elapsed().as_secs_f32() < 1.0);
            std::thread::yield_now();
        }
    }

    #[test]
    fn hidden_clocks_keep_pending_alerts_and_disabling_cancels_them() {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let mut settings = Settings::default();
        settings.sound.mode = Mode::Title;
        let owner = world
            .spawn((
                Node {
                    display: Display::None,
                    ..default()
                },
                TimeSettings(settings),
            ))
            .id();
        populate(&mut world, owner);
        let at = chrono::Utc::now().timestamp_millis() + 60_000;
        world.get_mut::<View>(owner).unwrap().entries.push(Entry {
            id: "hidden".into(),
            record_uid: "record".into(),
            head: "Hidden task".into(),
            quantity: "-1".into(),
            category: model::Category::Timed,
            time: Some(nucleus::schedule::TimeRange {
                from_ms: at,
                until_ms: None,
            }),
            origin: serde_json::json!({"kind":"manual"}),
            preview: false,
            start_date: None,
            due_date: None,
        });
        update(&mut world);
        assert!(
            world
                .resource::<Runtime>()
                .owners
                .contains(&owner.to_bits())
        );
        deadline(&mut world, Some(at));
        assert_eq!(world.resource::<crate::sound_cues::Native>().started, 0);
        {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.entries[0].time.as_mut().unwrap().from_ms += 60_000;
            view.revision += 1;
        }
        update(&mut world);
        deadline(&mut world, Some(at + 60_000));
        world.get_mut::<TimeSettings>(owner).unwrap().0.sound.mode = Mode::Off;
        update(&mut world);
        assert!(world.resource::<Runtime>().owners.is_empty());
        deadline(&mut world, None);
    }

    #[test]
    fn default_sound_does_not_initialize_a_native_device() {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world.spawn(Node::default()).id();
        populate(&mut world, owner);
        update(&mut world);
        assert!(!world.contains_resource::<crate::sound_cues::Native>());
        assert!(!world.contains_resource::<Runtime>());
    }
}
