use super::*;

#[derive(Default)]
pub(super) struct Inputs {
    names: Vec<(Entity, String)>,
    searches: Vec<(Entity, Entity, String)>,
}

pub(super) fn inputs(
    world: &mut World,
    names: &mut QueryState<(&'static Name, &'static EditableText)>,
    searches: &mut QueryState<(Entity, &'static Search, &'static EditableText)>,
    mut pending: Local<Inputs>,
) {
    let _capture = nucleus::performance::Span::new("recorder-input-capture");
    pending.names.clear();
    pending.names.extend(
        names
            .iter(world)
            .filter(|(name, text)| {
                world
                    .get::<RecorderCastle>(name.0)
                    .is_some_and(|castle| text.value() != castle.name.as_str())
            })
            .map(|(name, text)| (name.0, text.value().to_string())),
    );
    for (owner, name) in pending.names.drain(..) {
        if let Some(mut castle) = world.get_mut::<RecorderCastle>(owner)
            && castle.name != name
        {
            castle.name = name;
        }
    }
    pending.searches.clear();
    pending.searches.extend(
        searches
            .iter(world)
            .filter(|(_, search, text)| text.value() != search.last.as_str())
            .map(|(entity, search, text)| (entity, search.owner, text.value().to_string())),
    );
    for (entity, owner, value) in pending.searches.drain(..) {
        value.clone_into(&mut world.get_mut::<Search>(entity).unwrap().last);
        render_matches(world, owner, &value);
    }
    if names.iter(world).next().is_none() {
        pending.names = Vec::new();
    }
    if searches.iter(world).next().is_none() {
        pending.searches = Vec::new();
    }
}

pub(super) fn refresh(
    world: &mut World,
    views: &mut QueryState<(Entity, &'static View)>,
    mut pending: Local<Vec<(Entity, u64, Option<String>)>>,
) {
    let _refresh = nucleus::performance::Span::new("recorder-library-refresh");
    pending.clear();
    pending.extend(views.iter(world).filter_map(|(owner, view)| {
        let audio = crate::native::sound::audio_for(world, owner)?;
        (view.revision != audio.revision).then(|| (owner, audio.revision, audio.error.clone()))
    }));
    for (owner, revision, error) in pending.drain(..) {
        world.get_mut::<View>(owner).unwrap().revision = revision;
        list(world, owner);
        if let Some(error) = &error {
            status(world, owner, error);
        }
    }
    if views.iter(world).next().is_none() {
        *pending = Vec::new();
    }
}
