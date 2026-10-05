use super::*;

const SAMPLE: &str = "recordings/instinct-demo.wav";

#[derive(Default)]
pub(super) struct State {
    receiver: Option<Mutex<mpsc::Receiver<Result<(), String>>>>,
    ready: bool,
    failed: bool,
}

pub(super) fn handles(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::PreviewRecording
            | Operation::StopRecordingPlayback
            | Operation::EnableAreaSound
            | Operation::AssignAreaSound
            | Operation::PreviewAreaSound
    )
}

pub(super) fn prepare(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if !matches!(practice.runner.lesson.subject, "recorder" | "area-sound")
        || practice.setup.is_some()
        || practice.records.is_empty()
    {
        return;
    }
    let result = practice
        .media
        .receiver
        .as_ref()
        .and_then(|receiver| receiver.lock().ok()?.try_recv().ok());
    if let Some(result) = result {
        world.get_mut::<Practice>(root).unwrap().media.receiver = None;
        match result {
            Ok(()) => {
                let source = world.get::<Practice>(root).unwrap().source.clone();
                let path = world
                    .resource::<crate::practice_cells::PracticeCells>()
                    .directories[&source]
                    .clone();
                let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
                world
                    .resource_mut::<crate::practice_cells::PracticeCells>()
                    .audio
                    .insert(source, crate::sound::Audio::open(path, wake));
                world.get_mut::<Practice>(root).unwrap().media.ready = true;
            }
            Err(error) => {
                let mut practice = world.get_mut::<Practice>(root).unwrap();
                practice.media.failed = true;
                practice.runner.phase = Phase::Unavailable(error);
                input::release(world, root);
                render(world, root);
            }
        }
    }
    let practice = world.get::<Practice>(root).unwrap();
    if !practice.media.ready {
        if practice.media.receiver.is_none() && !practice.media.failed {
            let Some(path) = world
                .resource::<crate::practice_cells::PracticeCells>()
                .directories
                .get(&practice.source)
                .cloned()
            else {
                return;
            };
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                return;
            };
            let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
            let (sender, receiver) = mpsc::channel();
            let task = handle.spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    tokio::task::spawn_blocking(move || {
                        let library = crate::sound::library::Library::open(&path)?;
                        let clip = crate::sound::library::Clip {
                            rate: 16_000,
                            samples: (0..1600)
                                .map(|index| {
                                    (index as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin()
                                        * 0.05
                                })
                                .collect(),
                        };
                        library.save_recording(SAMPLE, &clip)
                    }),
                )
                .await
                .map_err(|_| "Preparing the sound sample timed out.".to_string())
                .and_then(|result| result.map_err(|error| error.to_string()))
                .and_then(|result| result);
                let _ = sender.send(result);
                if let Some(wake) = wake {
                    wake.ring();
                }
            });
            let mut practice = world.get_mut::<Practice>(root).unwrap();
            practice.media.receiver = Some(Mutex::new(receiver));
            practice.tasks.push(task);
        }
        return;
    }
    if find(world, root, Role::Feature).is_none() {
        let practice = world.get::<Practice>(root).unwrap();
        let owner = if practice.runner.lesson.subject == "recorder" {
            crate::recorder_castle::spawn(
                world,
                root,
                practice.workspace,
                DVec2::new(800.0, 0.0),
                crate::recorder_castle::RecorderCastle {
                    selected: SAMPLE.into(),
                    ..default()
                },
            )
        } else {
            let Ok(owner) = area(world, root, Role::Feature) else {
                return;
            };
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::Areas.apply(world, root);
            crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(owner))
                .apply(world, root);
            let revision = world
                .get::<crate::edit_mode::EditMode>(root)
                .map(|mode| mode.revision);
            world.get_mut::<Practice>(root).unwrap().owned_edit_revision = revision;
            owner
        };
        own(world, root, owner, Role::Feature);
    }
    if world.get::<Practice>(root).unwrap().pending.is_some()
        && let Some(operation) = world
            .get::<Practice>(root)
            .unwrap()
            .runner
            .current()
            .and_then(|step| step.operation)
    {
        let _ = execute(world, root, operation);
    }
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    let Some(owner) = find(world, root, Role::Feature) else {
        return Ok(());
    };
    match operation {
        Operation::PreviewRecording => crate::recorder_castle::ui::preview(world, owner),
        Operation::StopRecordingPlayback => crate::recorder_castle::ui::stop(world, owner),
        Operation::EnableAreaSound => crate::sound_area::enable(world, owner),
        Operation::AssignAreaSound => crate::sound_area::assign(world, owner, SAMPLE),
        Operation::PreviewAreaSound => crate::sound_area::preview(world, owner),
        _ => return Err("This sound lesson is unavailable.".into()),
    }
    Ok(())
}

pub(super) fn unavailable_playback(world: &mut World, root: Entity, operation: Operation) -> bool {
    matches!(
        operation,
        Operation::PreviewRecording | Operation::PreviewAreaSound
    ) && find(world, root, Role::Feature).is_some_and(|owner| {
        world
            .get::<crate::sound::PlaybackResult>(owner)
            .is_some_and(|result| result.0.is_err())
    })
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    let Some(owner) = find(world, root, Role::Feature) else {
        return false;
    };
    match operation {
        Operation::PreviewRecording | Operation::PreviewAreaSound => world
            .get::<crate::sound::PlaybackResult>(owner)
            .is_some_and(|result| result.0.is_ok()),
        Operation::StopRecordingPlayback => {
            world.get::<crate::sound::StoppedPlayback>(owner).is_some()
        }
        Operation::EnableAreaSound => world
            .get::<InfluenceArea>(owner)
            .is_some_and(|area| area.sound.is_some()),
        Operation::AssignAreaSound => world
            .get::<InfluenceArea>(owner)
            .and_then(|area| area.sound.as_ref())
            .is_some_and(|sound| sound.enter == SAMPLE),
        _ => false,
    }
}
