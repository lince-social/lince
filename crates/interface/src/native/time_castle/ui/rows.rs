use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(in super::super) enum Key {
    Controls,
    Summary,
    Category(usize),
    Event(String, usize),
    Date(String, usize),
    Origin(String, usize),
    Empty,
    Heading,
    Paging,
}

#[derive(Clone, PartialEq)]
pub(in super::super) enum Command {
    Select(String),
    TryDate(String),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        match self {
            Self::Select(id) => Select(vec![id.clone()]).apply(world, owner),
            Self::TryDate(id) => TryDate(id.clone()).apply(world, owner),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(in super::super) enum Row {
    Label(String, f32),
    Button(String, Command, bool),
    Controls { all: bool, pages: bool },
    Heading(String),
}

#[derive(Component, Default)]
struct Rows(RetainedChildren<Key>);

#[derive(PartialEq)]
struct Presented {
    owner: Entity,
    row: Row,
}

fn text(world: &mut World, entity: Entity, caption: &str) {
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != caption
    {
        caption.clone_into(&mut text.0);
    }
    let font = bevy::text::FontSource::Handle(
        world.resource::<crate::native::theme::Typography>().0.clone(),
    );
    if let Some(mut text_font) = world.get_mut::<TextFont>(entity)
        && text_font.font != font
    {
        text_font.font = font;
    }
}

fn controls(world: &mut World, entity: Entity, owner: Entity, all: bool, pages: bool) {
    if all {
        chrome::button(
            world,
            entity,
            owner,
            "All scheduled work",
            Select(Vec::new()),
        );
    }
    if pages {
        chrome::button(world, entity, owner, "Previous", Page(-1));
        chrome::button(world, entity, owner, "Next", Page(1));
    }
}

fn heading(world: &mut World, entity: Entity, owner: Entity, caption: &str) {
    let label = crate::native::edit_mode::label(world, entity, caption, 12.0);
    world.get_mut::<Node>(label).unwrap().flex_grow = 1.0;
    chrome::button(world, entity, owner, "Close", chrome::DismissPeek);
}

pub(in super::super) fn present(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    rows: Vec<(Key, Row)>,
) {
    let _span = nucleus::performance::Span::new("time-detail-presentation");
    let mut retained = world.entity_mut(parent).take::<Rows>().unwrap_or_default();
    retained.0.reconcile(
        world,
        parent,
        rows,
        |world, parent, row| {
            let entity = match row {
                Row::Label(caption, size) => {
                    crate::native::edit_mode::label(world, parent, caption, *size)
                }
                Row::Button(caption, action, wide) => {
                    let entity = chrome::button(world, parent, owner, caption, action.clone());
                    if *wide {
                        world.get_mut::<Node>(entity).unwrap().width = percent(100);
                    }
                    entity
                }
                Row::Controls { all, pages } => {
                    let entity = crate::native::sand_panel::row(world, parent);
                    controls(world, entity, owner, *all, *pages);
                    entity
                }
                Row::Heading(caption) => {
                    let entity = crate::native::sand_panel::row(world, parent);
                    heading(world, entity, owner, caption);
                    entity
                }
            };
            Presentation::remember(
                world,
                entity,
                Presented { owner, row: row.clone() },
            );
            crate::host::performance::count(
                crate::host::performance::Count::TimeDetailRowsChanged,
            );
            entity
        },
        |world, entity, row| {
            if Presentation::matches(world, entity, |old: &Presented| {
                old.owner == owner && old.row == *row
            }) {
                crate::host::performance::count(
                    crate::host::performance::Count::TimeDetailRowsRetained,
                );
                return;
            }
            match row {
                Row::Label(caption, _) => text(world, entity, caption),
                Row::Button(caption, action, _) => {
                    let same_action = Presentation::<Presented>::state(world, entity)
                        .is_some_and(|old| {
                            old.owner == owner
                                && matches!(&old.row, Row::Button(_, previous, _) if previous == action)
                        });
                    if !same_action {
                        world.entity_mut(entity).insert(ActionButton::new(
                            owner,
                            crate::native::actions![action.clone()],
                        ));
                    }
                    set_caption(world, entity, caption);
                    let name = format!("button-{}", caption.to_lowercase().replace(' ', "-"));
                    if let Some(mut part) = world.get_mut::<crate::native::canvas_host::authoring::PartName>(entity)
                        && part.0 != name
                    {
                        part.0 = name;
                    }
                    let label = world.get::<Children>(entity).and_then(|children| {
                        children.iter().find(|child| world.get::<Text>(*child).is_some())
                    });
                    if let Some(label) = label {
                        text(world, label, caption);
                    } else {
                        crate::native::edit_mode::label(world, entity, caption, 14.0);
                    }
                }
                Row::Controls { all, pages } => {
                    world.entity_mut(entity).despawn_children();
                    controls(world, entity, owner, *all, *pages);
                }
                Row::Heading(caption) => {
                    let intact = Presentation::matches(world, entity, |old: &Presented| {
                        old.owner == owner
                    });
                    let label = world.get::<Children>(entity).and_then(|children| {
                        children.iter().find(|child| world.get::<Text>(*child).is_some())
                    });
                    if intact && let Some(label) = label {
                        text(world, label, caption);
                    } else {
                        world.entity_mut(entity).despawn_children();
                        heading(world, entity, owner, caption);
                    }
                }
            }
            Presentation::remember(
                world,
                entity,
                Presented { owner, row: row.clone() },
            );
            crate::host::performance::count(
                crate::host::performance::Count::TimeDetailRowsChanged,
            );
        },
    );
    world.entity_mut(parent).insert(retained);
}
