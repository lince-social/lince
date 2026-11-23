// non-lean-yet

use super::{Open, Search, preparation::MAX_RESULTS};
use crate::native::{
    actions::ActionButton,
    edit_mode::label,
    sand::button,
    theme::Typography,
    token_style::{BackgroundToken, background},
    tokens::Token,
};
use bevy::prelude::*;
use nucleus::performance::{Span, add};
use std::sync::Arc;

const ROW_HEIGHT: f32 = 32.0;
const ROWS_ABOVE_SELECTION: usize = 5;

#[derive(Component)]
struct PopupPresentation {
    rows: Vec<Entity>,
    caption: Option<Entity>,
    selected: usize,
    font: Handle<Font>,
}

impl PopupPresentation {
    fn current(&self, world: &World, popup: Entity) -> bool {
        if world
            .get_resource::<Typography>()
            .is_none_or(|font| self.font != font.0)
        {
            return false;
        }
        let mut children = world
            .get::<Children>(popup)
            .into_iter()
            .flat_map(|children| children.iter());
        self.rows
            .iter()
            .copied()
            .chain(self.caption)
            .all(|expected| children.by_ref().any(|child| child == expected))
    }
}

fn scroll_to_selection(world: &mut World, popup: Entity, selected: usize) {
    let first = selected.saturating_sub(ROWS_ABOVE_SELECTION);
    // Long titles wrap, so measured rows are used once layout has run.
    let offset = world
        .get::<PopupPresentation>(popup)
        .map(|view| {
            view.rows
                .iter()
                .take(first)
                .map(|row| {
                    world
                        .get::<ComputedNode>(*row)
                        .map(|node| node.size().y * node.inverse_scale_factor())
                        .filter(|height| *height > 0.0)
                        .unwrap_or(ROW_HEIGHT)
                })
                .sum::<f32>()
        })
        .unwrap_or(first as f32 * ROW_HEIGHT);
    if let Some(mut position) = world.get_mut::<ScrollPosition>(popup)
        && position.0.y != offset
    {
        position.0.y = offset;
    }
}

pub(super) fn select(world: &mut World, form: Entity, popup: Entity) {
    let _selection = Span::new("record-creation-search-selection");
    let Some(selected) = world.get::<Search>(form).map(|search| search.selected) else {
        return;
    };
    let Some(presented) = world
        .get::<PopupPresentation>(popup)
        .filter(|view| view.current(world, popup))
    else {
        draw(world, form, popup);
        return;
    };
    if presented.selected != selected {
        let previous = presented.rows.get(presented.selected).copied();
        let next = presented.rows.get(selected).copied();
        if let Some(previous) = previous
            && let Ok(mut row) = world.get_entity_mut(previous)
        {
            row.remove::<BackgroundToken>()
                .insert(BackgroundColor::default());
        }
        if let Some(next) = next
            && let Ok(mut row) = world.get_entity_mut(next)
        {
            row.insert(background(Token::Accent));
        }
        if let Some(mut view) = world.get_mut::<PopupPresentation>(popup) {
            view.selected = selected;
        }
    }
    scroll_to_selection(world, popup, selected);
}

pub(super) fn draw(world: &mut World, form: Entity, popup: Entity) {
    let _draw = Span::new("record-creation-search-draw");
    let Some(search) = world.get::<Search>(form) else {
        return;
    };
    let Some(font) = world
        .get_resource::<Typography>()
        .map(|font| font.0.clone())
    else {
        return;
    };
    let Ok(view) = world.get_entity(popup) else {
        return;
    };
    let options = Arc::clone(&search.options);
    let selected = search.selected;
    let total = search.total;
    let message = search.message.clone();
    let empty = search.empty;
    if let Some(children) = view.get::<Children>() {
        for child in children.iter().collect::<Vec<_>>() {
            world.despawn(child);
        }
    }
    let mut rows = Vec::new();
    let caption = if !message.is_empty() {
        Some(label(world, popup, &message, 13.0))
    } else if options.is_empty() {
        Some(label(world, popup, empty, 13.0))
    } else {
        rows.reserve_exact(options.len());
        for (index, (uid, title)) in options.iter().enumerate() {
            let row = world
                .spawn((
                    button(0),
                    ActionButton::new(form, crate::native::actions![Open(uid.clone())]),
                    Node {
                        width: percent(100),
                        min_width: px(0),
                        min_height: px(ROW_HEIGHT),
                        flex_shrink: 0.0,
                        padding: UiRect::axes(px(8), px(6)),
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip_x(),
                        ..default()
                    },
                    ChildOf(popup),
                ))
                .id();
            rows.push(row);
            if index == selected {
                world.entity_mut(row).insert(background(Token::Accent));
            }
            let caption = label(world, row, title, 14.0);
            crate::native::record_creation::wrap(world, caption);
        }
        (total > MAX_RESULTS).then(|| {
            label(
                world,
                popup,
                &format!("{MAX_RESULTS} of {total} matches shown. Keep typing to narrow the list."),
                13.0,
            )
        })
    };
    if let Some(caption) = caption {
        crate::native::record_creation::wrap(world, caption);
    }
    add("record-creation-search-rows-created", rows.len() as u64);
    world.entity_mut(popup).insert(PopupPresentation {
        rows,
        caption,
        selected,
        font,
    });
    scroll_to_selection(world, popup, selected);
}
