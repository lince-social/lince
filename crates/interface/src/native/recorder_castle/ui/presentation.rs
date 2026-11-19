use super::*;
use crate::native::retained::Presentation;

struct Header {
    owner: Entity,
    search: Entity,
    matches: Entity,
}

pub(super) fn current_query(
    In(owner): In<Entity>,
    fields: Query<(&Search, &EditableText)>,
) -> String {
    fields
        .iter()
        .find(|(search, _)| search.owner == owner)
        .map(|(_, text)| text.value().to_string())
        .unwrap_or_default()
}

fn header(world: &mut World, owner: Entity, query: &str) -> Entity {
    let parent = world.get::<View>(owner).unwrap().list;
    if Presentation::matches(world, parent, |old: &Header| {
        old.owner == owner
            && world
                .get::<Search>(old.search)
                .is_some_and(|search| search.owner == owner)
            && world.get::<EditableText>(old.search).is_some()
            && world
                .get::<ChildOf>(old.search)
                .is_some_and(|child| child.parent() == parent)
            && world
                .get::<Matches>(old.matches)
                .is_some_and(|matches| matches.0 == owner)
            && world
                .get::<ChildOf>(old.matches)
                .is_some_and(|child| child.parent() == parent)
    }) {
        nucleus::performance::add("recorder-search-editors-retained", 1);
        return Presentation::<Header>::state(world, parent)
            .unwrap()
            .matches;
    }
    clear(world, parent);
    label(world, parent, "Recordings", 18.0);
    button(world, parent, owner, "Refresh recordings", Control::Refresh);
    let search = input(world, parent, "Find a recording", query);
    world.entity_mut(search).insert(Search {
        owner,
        last: query.into(),
    });
    let matches = stack(world, parent);
    world.entity_mut(matches).insert(Matches(owner));
    Presentation::remember(
        world,
        parent,
        Header {
            owner,
            search,
            matches,
        },
    );
    nucleus::performance::add("recorder-search-editors-built", 1);
    matches
}

pub(super) fn matches(world: &mut World, owner: Entity, query: &str) {
    let _presentation = nucleus::performance::Span::new("recorder-list-presentation");
    let parent = header(world, owner, query);
    let selected = world.get::<RecorderCastle>(owner).unwrap().selected.clone();
    let paths: Vec<_> = crate::native::sound::audio_for(world, owner)
        .map(|audio| {
            crate::native::sound::library::suggestions(&audio.paths, query)
                .into_iter()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    recording_choices(
        world,
        parent,
        RecordingTarget::Recorder(owner),
        &paths,
        Some(&selected),
        ("No matching recordings yet.", 13.0),
        |path| Control::Select(path.into()),
    );
}

pub(super) struct EffectState {
    pub owner: Entity,
    pub selected: String,
    pub effect: Option<Effects>,
}

pub(super) fn effects_match(world: &World, owner: Entity, parent: Entity) -> bool {
    let castle = world.get::<RecorderCastle>(owner).unwrap();
    let effect = (!castle.selected.is_empty()).then(|| castle.effect());
    Presentation::matches(world, parent, |old: &EffectState| {
        old.owner == owner
            && old.selected == castle.selected
            && old.effect == effect
            && old.effect.zip(effect).is_none_or(|(previous, current)| {
                EffectField::ALL.iter().all(|(field, _)| {
                    field.get(&previous).to_bits() == field.get(&current).to_bits()
                })
            })
    })
}

pub(super) fn remember_effects(world: &mut World, parent: Entity, state: EffectState) {
    Presentation::remember(world, parent, state);
}
