// non-lean-yet
// Scope: enum Key, enum Part, struct Panel, fn clear.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 38.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (36, 39, 42, 104).

use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Header,
    Editor,
    Outcome(String, usize),
}

enum Part<'a> {
    Header,
    Editor,
    Outcome(&'a Arc<Value>),
}

#[derive(Component, Default)]
struct Panel {
    children: RetainedChildren<Key>,
    templates: Option<choices::Templates>,
}

pub(super) fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .into_iter()
        .flat_map(|children| children.iter())
        .collect();
    for child in children {
        world.despawn(child);
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let _render = nucleus::performance::Span::new("karma-schedule-presentation");
    let view = world.get::<ScheduleView>(owner).unwrap();
    let panel = view.panel;
    let source = view.rows.clone();
    let editing = world.get::<KarmaCastle>(owner).unwrap().schedule.is_some();
    let mut state = world.entity_mut(panel).take::<Panel>().unwrap_or_default();
    let templates = if editing {
        let view = world.get::<View>(owner).unwrap();
        Some(
            state
                .templates
                .get_or_insert_with(Default::default)
                .prepare(&view.transfers, view.acting_person.as_deref()),
        )
    } else {
        state.templates = None;
        None
    };
    let mut parts = vec![(Key::Header, Part::Header)];
    if editing {
        parts.push((Key::Editor, Part::Editor));
    }
    let mut occurrences = HashMap::<&str, usize>::new();
    for value in source.iter() {
        let Some(uid) = value["uid"].as_str() else {
            continue;
        };
        let occurrence = occurrences.entry(uid).or_default();
        parts.push((Key::Outcome(uid.into(), *occurrence), Part::Outcome(value)));
        *occurrence += 1;
    }
    state.children.reconcile(
        world,
        panel,
        parts,
        |world, parent, part| {
            let entity = if matches!(part, Part::Header) {
                ui::row(world, parent)
            } else {
                ui::stack(world, parent)
            };
            draw(world, owner, entity, part, templates.as_ref());
            nucleus::performance::add("karma-schedule-parts-created", 1);
            entity
        },
        |world, entity, part| draw(world, owner, entity, part, templates.as_ref()),
    );
    world.entity_mut(panel).insert(state);
}

fn draw(
    world: &mut World,
    owner: Entity,
    entity: Entity,
    part: &Part<'_>,
    templates: Option<&Arc<Vec<choices::Template>>>,
) {
    match part {
        Part::Header => {
            if Presentation::matches(world, entity, |_: &()| true) {
                return;
            }
            clear(world, entity);
            crate::native::edit_mode::label(world, entity, "Scheduled changes", 18.0);
            button(world, entity, owner, "One change", Command::New(false));
            button(world, entity, owner, "Start and end", Command::New(true));
            button(world, entity, owner, "Refresh outcomes", Command::Refresh);
            Presentation::remember(world, entity, ());
        }
        Part::Editor => form::render(world, owner, entity, templates.unwrap()),
        Part::Outcome(value) => {
            if Presentation::matches(world, entity, |old: &Arc<Value>| {
                Arc::ptr_eq(old, value) || old.as_ref() == value.as_ref()
            }) {
                nucleus::performance::add("karma-schedule-outcomes-retained", 1);
                return;
            }
            outcomes::render(world, owner, entity, value);
            Presentation::remember(world, entity, Arc::clone(value));
            nucleus::performance::add("karma-schedule-outcomes-updated", 1);
        }
    }
}
