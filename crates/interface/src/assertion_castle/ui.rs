use super::*;

pub(super) const PAGE_SIZE: usize = 100;

pub(super) fn stack(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(6),
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_shrink: 0.0,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    for child in children {
        world.despawn(child);
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let (controls, list, page, selecting, busy) = (
        view.controls,
        view.list,
        view.page,
        view.selecting,
        view.batch.is_some(),
    );
    let rows = view.rows.clone();
    let choices = view.choices.clone();
    let selected = world
        .get::<AssertionCastle>(owner)
        .unwrap()
        .selection
        .clone();
    let area = world.get::<Frame>(owner).unwrap().area;
    let source = config(world, owner)
        .map(|config| config.source)
        .unwrap_or_default();
    clear(world, controls);
    clear(world, list);
    let buttons = row(world, controls);
    let label = selected.as_ref().map_or_else(
        || "Choose assertion".into(),
        |selection| format!("#{} ▾", selection.name),
    );
    if busy {
        crate::edit_mode::label(world, buttons, &label, 14.0);
    } else {
        castle_feed::button(world, buttons, owner, &label, Command::Choose);
        if selected.is_some()
            && !rows.is_empty()
            && world.get::<View>(owner).unwrap().error.is_none()
        {
            castle_feed::button(
                world,
                buttons,
                owner,
                &format!("Renumber {} Records", rows.len()),
                Command::Renumber,
            );
        }
    }
    crate::edit_mode::label(
        world,
        controls,
        "Drag Records to arrange them · Renumber saves 1, 2, 3… in the displayed order",
        12.0,
    );
    if selecting {
        let options = stack(world, controls);
        world.get_mut::<Node>(options).unwrap().max_height = px(180);
        world.get_mut::<Node>(options).unwrap().flex_shrink = 1.0;
        crate::scroll_sand::attach(world, options);
        if choices.is_empty() {
            crate::edit_mode::label(
                world,
                options,
                "No assertions without a target or unit in these Records. Add one in a Record Castle, then select it here.",
                14.0,
            );
        }
        for choice in choices {
            castle_feed::button(
                world,
                options,
                owner,
                &format!("#{}", choice.name),
                Command::Select(choice),
            );
        }
    }
    if rows.len() > PAGE_SIZE {
        let navigation = row(world, controls);
        if !busy {
            castle_feed::button(world, navigation, owner, "Previous", Command::Page(false));
            castle_feed::button(world, navigation, owner, "Next", Command::Page(true));
        }
        crate::edit_mode::label(
            world,
            navigation,
            &format!(
                "Page {} / {} · {} Records",
                page + 1,
                rows.len().div_ceil(PAGE_SIZE),
                rows.len()
            ),
            12.0,
        );
    }
    if rows.is_empty() {
        crate::edit_mode::label(
            world,
            list,
            "No Records. Use Protein to choose the records for this list.",
            14.0,
        );
    }
    for (index, item) in rows
        .iter()
        .enumerate()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
    {
        let entry = row(world, list);
        world.entity_mut(entry).insert((
            input::ListRow {
                owner,
                uid: item.uid.clone(),
            },
            crate::token_style::border(crate::tokens::Token::Accent),
        ));
        world.get_mut::<Node>(entry).unwrap().min_height = px(36);
        crate::edit_mode::label(world, entry, "↕", 14.0);
        crate::edit_mode::label(world, entry, &format!("{}.", index + 1), 14.0);
        castle_feed::button(
            world,
            entry,
            owner,
            &item.head,
            crate::full_record::Open(RecordBinding {
                area,
                uid: item.uid.clone(),
                source: source.clone(),
            }),
        );
        if let Some(selection) = &selected {
            let value = item
                .quantity
                .map_or_else(|| "—".into(), |quantity| quantity.to_string());
            crate::edit_mode::label(world, entry, &format!("#{}: {value}", selection.name), 14.0);
        }
    }
}

pub(super) fn status(world: &mut World, owner: Entity) {
    let frame = world.get::<Frame>(owner).unwrap();
    let status = frame.status;
    let view = world.get::<View>(owner).unwrap();
    let text = if let Some(batch) = &view.batch {
        format!("Renumbering {} / {}", batch.next, batch.rows.len())
    } else if let Some(error) = &view.error {
        error.clone()
    } else if !view.message.is_empty() {
        view.message.clone()
    } else {
        format!(
            "{} · {} Records",
            crate::protein_area::calendar_status(world, frame.area),
            view.rows.len()
        )
    };
    world.get_mut::<Text>(status).unwrap().0 = text;
}
