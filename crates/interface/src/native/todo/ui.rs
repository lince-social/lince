use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Empty,
    Record(String, usize),
    Page,
}

#[derive(Component, Default)]
struct Rows(RetainedChildren<Key>);

enum Part<'a> {
    Label(String, f32),
    Record(&'a str, &'a Value, bool),
}

struct Record {
    owner: Entity,
    uid: String,
    title: String,
    body: Option<String>,
    show_ids: bool,
    quantity: Option<Value>,
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let view = world.get::<TodoSand>(owner).unwrap();
    let (list, show_ids, active, ready) = (view.list, view.show_ids, view.active, view.ready);
    let start = active / PAGE * PAGE;
    let rows = view.rows.clone();
    let total = rows.len();
    let mut parts = Vec::with_capacity(PAGE + 1);
    if start >= total {
        parts.push((
            Key::Empty,
            Part::Label(
                if ready {
                    "Nothing needs doing."
                } else {
                    "Waiting for tasks…"
                }
                .into(),
                16.0,
            ),
        ));
    }
    let mut occurrences = HashMap::<&str, usize>::new();
    for (index, row) in rows.iter().skip(start).take(PAGE).enumerate() {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        let occurrence = occurrences.entry(uid).or_default();
        parts.push((
            Key::Record(uid.into(), *occurrence),
            Part::Record(uid, row, start + index == active),
        ));
        *occurrence += 1;
    }
    if total > PAGE {
        parts.push((
            Key::Page,
            Part::Label(
                format!("{}–{} of {total}", start + 1, (start + PAGE).min(total)),
                12.0,
            ),
        ));
    }
    let mut children = world.entity_mut(list).take::<Rows>().unwrap_or_default();
    children.0.reconcile(
        world,
        list,
        parts,
        |world, parent, part| match part {
            Part::Label(text, size) => {
                let entity = crate::native::edit_mode::label(world, parent, text, *size);
                Presentation::remember(world, entity, text.clone());
                entity
            }
            Part::Record(uid, row, selected) => {
                let entity = panel::column(world, parent);
                world.get_mut::<Node>(entity).unwrap().padding = UiRect::all(px(6));
                record(world, entity, owner, uid, row, *selected, show_ids);
                entity
            }
        },
        |world, entity, part| match part {
            Part::Label(text, _) => {
                if !Presentation::matches(world, entity, |old: &String| old == text) {
                    panel::status(world, entity, text);
                    let font = world
                        .resource::<crate::native::theme::Typography>()
                        .0
                        .clone();
                    world.get_mut::<TextFont>(entity).unwrap().font = font.into();
                    Presentation::remember(world, entity, text.clone());
                }
            }
            Part::Record(uid, row, selected) => {
                record(world, entity, owner, uid, row, *selected, show_ids);
            }
        },
    );
    world.entity_mut(list).insert(children);
    world.get_mut::<TodoSand>(owner).unwrap().dirty = false;
}

fn record(
    world: &mut World,
    entry: Entity,
    owner: Entity,
    uid: &str,
    row: &Value,
    selected: bool,
    show_ids: bool,
) {
    let title = row["head"]
        .as_str()
        .filter(|text| !text.is_empty())
        .or_else(|| row["slug"].as_str())
        .unwrap_or(uid);
    let body = row["body"]
        .as_str()
        .filter(|body| !body.is_empty())
        .map(|body| {
            let end = body
                .char_indices()
                .nth(1000)
                .map_or(body.len(), |(end, _)| end);
            &body[..end]
        });
    let value = (!show_ids)
        .then(|| &row["quantity"])
        .filter(|value| value.is_string() || value.is_number());
    if Presentation::matches(world, entry, |old: &Record| {
        old.owner == owner
            && old.uid == uid
            && old.title == title
            && old.body.as_deref() == body
            && old.show_ids == show_ids
            && old.quantity.as_ref() == value
    }) {
        nucleus::performance::add("todo-rows-retained", 1);
    } else {
        panel::clear(world, entry);
        let top = panel::row(world, entry);
        panel::button(world, top, owner, "Done", Command::Complete(uid.into()));
        panel::button(world, top, owner, title, Command::Select(uid.into()));
        if let Some(body) = body {
            crate::native::edit_mode::label(world, entry, body, 13.0);
        }
        let meta = if show_ids {
            uid.into()
        } else {
            format!(
                "quantity {}",
                quantity(row).unwrap_or_else(|_| "unavailable".into())
            )
        };
        crate::native::edit_mode::label(world, entry, &meta, 12.0);
        Presentation::remember(
            world,
            entry,
            Record {
                owner,
                uid: uid.into(),
                title: title.into(),
                body: body.map(str::to_owned),
                show_ids,
                quantity: value.cloned(),
            },
        );
        nucleus::performance::add("todo-rows-rebuilt", 1);
    }
    let highlighted = world
        .get::<crate::native::token_style::BackgroundToken>(entry)
        .is_some();
    if selected != highlighted {
        if selected {
            world
                .entity_mut(entry)
                .insert(crate::native::token_style::background(
                    crate::native::tokens::Token::Surface,
                ));
        } else {
            world
                .entity_mut(entry)
                .remove::<crate::native::token_style::BackgroundToken>()
                .insert(BackgroundColor::default());
        }
        nucleus::performance::add("todo-selection-highlights-changed", 1);
    }
}
