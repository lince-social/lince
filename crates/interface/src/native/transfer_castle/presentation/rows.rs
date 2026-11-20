use super::*;
use crate::native::retained::Children as RetainedChildren;

#[derive(Clone, PartialEq)]
pub(super) struct Card {
    pub(super) uid: String,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) parties: String,
    pub(super) counts: String,
    pub(super) awaiting: bool,
    pub(super) depth: usize,
}

#[derive(Clone, PartialEq)]
enum Row {
    Card(Card),
    Empty(&'static str),
    Pages { page: usize, total: usize },
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Card(String, usize),
    Empty,
    Pages,
}

#[derive(Component, Default)]
struct Rows(RetainedChildren<Key>);

#[derive(PartialEq)]
struct Presented {
    owner: Entity,
    row: Row,
}

#[derive(Component)]
struct Selected(bool);

fn content(world: &mut World, entity: Entity, owner: Entity, value: &Row) {
    clear(world, entity);
    match value {
        Row::Card(card) => {
            world.entity_mut(entity).insert((
                crate::native::actions::ActionButton::new(
                    owner,
                    crate::native::actions![Command::Select(card.uid.clone())],
                ),
                crate::native::icons::Tooltip(card.title.clone()),
            ));
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
                node.set_label(card.title.as_str());
            }
            let margin = px((card.depth * 12) as f32);
            if let Some(mut node) = world.get_mut::<Node>(entity)
                && node.margin.left != margin
            {
                node.margin.left = margin;
            }
            crate::native::edit_mode::label(world, entity, &card.title, 14.0);
            for caption in [&card.status, &card.parties, &card.counts] {
                crate::native::edit_mode::label(world, entity, caption, 12.0);
            }
            if card.awaiting {
                crate::native::edit_mode::label(world, entity, "Awaiting your response", 12.0);
            }
        }
        Row::Empty(caption) => {
            if let Some(mut text) = world.get_mut::<Text>(entity)
                && text.0 != *caption
            {
                text.0 = (*caption).into();
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
        Row::Pages { page, total } => {
            if *page > 0 {
                button(world, entity, owner, "Previous", Command::Page(page - 1));
            }
            crate::native::edit_mode::label(
                world,
                entity,
                &format!("{} / {}", page + 1, total.div_ceil(30)),
                12.0,
            );
            if (page + 1) * 30 < *total {
                button(world, entity, owner, "Next", Command::Page(page + 1));
            }
        }
    }
    Presentation::remember(
        world,
        entity,
        Presented {
            owner,
            row: value.clone(),
        },
    );
    nucleus::performance::add("transfer-rows-rebuilt", 1);
}

fn selection(world: &mut World, entity: Entity, value: &Row, selected: &str) {
    let Row::Card(card) = value else { return };
    let selected = card.uid == selected;
    if world
        .get::<Selected>(entity)
        .is_some_and(|old| old.0 == selected)
    {
        return;
    }
    if selected {
        world
            .entity_mut(entity)
            .insert(crate::native::token_style::border(
                crate::native::tokens::Token::Accent,
            ));
    } else {
        world
            .entity_mut(entity)
            .remove::<crate::native::token_style::BorderToken>();
        world.entity_mut(entity).insert(BorderColor::default());
    }
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.border = UiRect::all(px(if selected { 1 } else { 0 }));
    }
    world.entity_mut(entity).insert(Selected(selected));
    nucleus::performance::add("transfer-selection-writes", 1);
}

pub(super) fn render(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    cards: Vec<Card>,
    selected: &str,
    page: usize,
    total: usize,
    empty: &'static str,
) {
    let mut rows = Vec::with_capacity(cards.len() + 1);
    let mut duplicates = HashMap::new();
    if cards.is_empty() {
        rows.push((Key::Empty, Row::Empty(empty)));
    }
    for card in cards {
        let occurrence = duplicates.entry(card.uid.clone()).or_insert(0);
        let key = Key::Card(card.uid.clone(), *occurrence);
        *occurrence += 1;
        rows.push((key, Row::Card(card)));
    }
    if total > 30 {
        rows.push((Key::Pages, Row::Pages { page, total }));
    }
    let mut retained = world.entity_mut(parent).take::<Rows>().unwrap_or_default();
    retained.0.reconcile(
        world,
        parent,
        rows,
        |world, parent, value| {
            let entity = match value {
                Row::Card(_) => world
                    .spawn((
                        crate::native::sand::button(0),
                        crate::native::sand::Square,
                        Node {
                            width: percent(100),
                            min_height: px(30),
                            flex_shrink: 0.0,
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Start,
                            row_gap: px(5),
                            padding: UiRect::all(px(12)),
                            ..default()
                        },
                        ChildOf(parent),
                    ))
                    .id(),
                Row::Empty(caption) => {
                    crate::native::edit_mode::label(world, parent, caption, 14.0)
                }
                Row::Pages { .. } => ui::row(world, parent),
            };
            content(world, entity, owner, value);
            selection(world, entity, value, selected);
            entity
        },
        |world, entity, value| {
            if Presentation::matches(world, entity, |old: &Presented| {
                old.owner == owner && old.row == *value
            }) {
                nucleus::performance::add("transfer-rows-retained", 1);
            } else {
                content(world, entity, owner, value);
            }
            selection(world, entity, value, selected);
        },
    );
    world.entity_mut(parent).insert(retained);
}
