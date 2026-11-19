use super::*;

#[derive(Default)]
pub(super) struct Pending {
    statuses: Vec<(Entity, String)>,
    fields: Vec<(Entity, Entity, bool, Entity, String)>,
}

pub(super) fn capture(
    world: &mut World,
    statuses: &mut QueryState<(Entity, &'static StatusLabel, &'static Text)>,
    fields: &mut QueryState<(Entity, &'static Input, &'static EditableText)>,
    mut pending: Local<Pending>,
) {
    let _capture = nucleus::performance::Span::new("sound-control-capture");
    pending.statuses.clear();
    pending
        .statuses
        .extend(statuses.iter(world).filter_map(|(entity, label, text)| {
            let message = world
                .get::<SoundStatus>(label.0)
                .map(|status| status.0.as_str())
                .unwrap_or_default();
            (text.0 != message).then(|| (entity, message.to_owned()))
        }));
    for (entity, message) in pending.statuses.drain(..) {
        if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != message
        {
            text.0 = message;
            nucleus::performance::add("sound-status-label-updates", 1);
        }
    }
    pending.fields.clear();
    pending.fields.extend(
        fields
            .iter(world)
            .filter(|(_, input, text)| {
                let revision = crate::native::sound::audio_for(world, input.owner)
                    .map_or(0, |audio| audio.revision);
                input.revision != revision || text.value() != input.last.as_str()
            })
            .map(|(entity, input, text)| {
                (
                    entity,
                    input.owner,
                    input.enter,
                    input.suggestions,
                    text.value().to_string(),
                )
            }),
    );
    for (entity, owner, enter, suggestions, value) in pending.fields.drain(..) {
        let revision =
            crate::native::sound::audio_for(world, owner).map_or(0, |audio| audio.revision);
        if allowed(world, owner).is_none() {
            continue;
        }
        if (value.is_empty() || crate::native::sound::library::valid_path(&value))
            && world
                .get::<InfluenceArea>(owner)
                .and_then(|area| area.sound.as_ref())
                .is_some_and(|sound| (if enter { &sound.enter } else { &sound.leave }) != &value)
            && let Some(sound) = &mut world.get_mut::<InfluenceArea>(owner).unwrap().sound
        {
            value.clone_into(if enter {
                &mut sound.enter
            } else {
                &mut sound.leave
            });
            nucleus::performance::add("sound-input-path-writes", 1);
        }
        let mut input = world.get_mut::<Input>(entity).unwrap();
        value.clone_into(&mut input.last);
        input.revision = revision;
        let paths: Vec<_> = crate::native::sound::audio_for(world, owner)
            .map(|audio| {
                crate::native::sound::library::suggestions(&audio.paths, &value)
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        crate::native::recorder_castle::ui::recording_choices(
            world,
            suggestions,
            crate::native::recorder_castle::ui::RecordingTarget::Area(owner, enter),
            &paths,
            None,
            (
                "Record a take in Recorder Castle, or refresh the recordings list.",
                12.0,
            ),
            |path| Change::Choose(enter, path.into()),
        );
    }
    if statuses.iter(world).next().is_none() {
        pending.statuses = Vec::new();
    }
    if fields.iter(world).next().is_none() {
        pending.fields = Vec::new();
    }
}
