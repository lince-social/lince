// non-lean-yet
// Scope: enum Target, enum Key, struct Rows, enum Part.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 58.

use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Recorder(Entity),
    Area(Entity, bool),
}

impl Target {
    fn owner(self) -> Entity {
        match self {
            Self::Recorder(owner) | Self::Area(owner, _) => owner,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Empty,
    Path(String, usize),
}

#[derive(Component, Default)]
struct Rows(RetainedChildren<Key>);

enum Part<'a> {
    Empty(&'a str, f32),
    Path(&'a str, bool),
}

struct State {
    target: Target,
    text: String,
    selected: Option<bool>,
    size: f32,
}

pub(crate) fn recordings<A: Action>(
    world: &mut World,
    parent: Entity,
    target: Target,
    paths: &[String],
    selected: Option<&str>,
    empty: (&str, f32),
    choose: impl Fn(&str) -> A,
) {
    let _presentation = nucleus::performance::Span::new("recording-choice-presentation");
    let mut parts = Vec::with_capacity(paths.len().max(1));
    if paths.is_empty() {
        parts.push((Key::Empty, Part::Empty(empty.0, empty.1)));
    }
    let mut occurrences = HashMap::<&str, usize>::new();
    for path in paths {
        let occurrence = occurrences.entry(path.as_str()).or_default();
        parts.push((
            Key::Path(path.clone(), *occurrence),
            Part::Path(path, selected == Some(path.as_str())),
        ));
        *occurrence += 1;
    }
    let mut rows = world.entity_mut(parent).take::<Rows>().unwrap_or_default();
    rows.0.reconcile(
        world,
        parent,
        parts,
        |world, parent, part| {
            let row = stack(world, parent);
            draw(world, row, target, part, &choose);
            row
        },
        |world, row, part| draw(world, row, target, part, &choose),
    );
    world.entity_mut(parent).insert(rows);
}

fn draw<A: Action>(
    world: &mut World,
    row: Entity,
    target: Target,
    part: &Part<'_>,
    choose: &impl Fn(&str) -> A,
) {
    let (text, selected, size) = match *part {
        Part::Empty(text, size) => (text, None, size),
        Part::Path(path, selected) => (path, Some(selected), 14.0),
    };
    if Presentation::matches(world, row, |old: &State| {
        old.target == target && old.text == text && old.selected == selected && old.size == size
    }) {
        nucleus::performance::add("recording-choice-parts-retained", 1);
        return;
    }
    world.entity_mut(row).despawn_children();
    if let Some(selected) = selected {
        let title = if selected {
            std::borrow::Cow::Owned(format!("● {text}"))
        } else {
            std::borrow::Cow::Borrowed(text)
        };
        button(world, row, target.owner(), &title, choose(text));
    } else {
        label(world, row, text, size);
    }
    Presentation::remember(
        world,
        row,
        State {
            target,
            text: text.into(),
            selected,
            size,
        },
    );
    nucleus::performance::add("recording-choice-parts-built", 1);
}
