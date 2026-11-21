use super::{Panel, management};
use crate::native::{
    actions::Action, edit_mode::label, protein_area::RecordBinding, thread_castle, token_style,
    tokens::Token,
};
use bevy::prelude::*;
use cell::FioteStatus;
use serde_json::Value;

#[derive(Component)]
pub(super) struct Dashboard {
    roster: Entity,
    conversation: Entity,
    body: Entity,
    rows: Vec<RosterRow>,
    toggle: Entity,
}

#[derive(PartialEq, Eq)]
struct RosterRow {
    record: String,
    title: String,
    detail: String,
    selected: bool,
}

#[derive(Component)]
struct DashboardPanel(Entity);

pub(super) fn controls(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(4),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                min_width: px(0),
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    action: impl Action,
) -> Entity {
    let entity = crate::native::description::button(world, parent, owner, title, action);
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.min_width = px(0);
        node.max_width = percent(100);
        node.flex_shrink = 1.0;
    }
    let text = world
        .get::<Children>(entity)
        .and_then(|children| children.first())
        .copied();
    if let Some(text) = text {
        if let Some(mut node) = world.get_mut::<Node>(text) {
            node.min_width = px(0);
            node.flex_shrink = 1.0;
        }
        if let Some(mut layout) = world.get_mut::<bevy::text::TextLayout>(text) {
            layout.linebreak = bevy::text::LineBreak::WordOrCharacter;
        }
    }
    entity
}

pub(super) fn populate(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    settings: Entity,
    toggle: Entity,
    binding: &RecordBinding,
    data: &Value,
) {
    let body = world
        .spawn((
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                column_gap: px(12),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let roster = world
        .spawn((
            Node {
                width: percent(27),
                max_width: px(240),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                padding: UiRect::right(px(8)),
                border: UiRect::right(px(1)),
                ..default()
            },
            token_style::border(Token::Accent),
            ChildOf(body),
        ))
        .id();
    crate::native::scroll_sand::attach(world, roster);
    let conversation = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_basis: px(0),
                flex_grow: 1.0,
                min_width: px(0),
                min_height: px(0),
                row_gap: px(6),
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    thread_castle::populate_dashboard(world, conversation, binding.clone(), data);
    world.entity_mut(owner).insert(Dashboard {
        roster,
        conversation,
        body,
        rows: Vec::new(),
        toggle,
    });
    world.entity_mut(parent).insert(DashboardPanel(owner));
    if let Some(mut node) = world.get_mut::<Node>(settings) {
        node.display = Display::None;
        node.flex_grow = 1.0;
        node.min_height = px(0);
    }
    crate::native::scroll_sand::attach(world, settings);
}

pub(crate) fn refresh(world: &mut World, row: Entity, data: &Value) -> bool {
    let Some(panel) = world.get::<DashboardPanel>(row) else {
        return false;
    };
    let Some(dashboard) = world.get::<Dashboard>(panel.0) else {
        return false;
    };
    thread_castle::refresh(world, dashboard.conversation, data)
}

pub(super) fn show(world: &mut World, owner: Entity, normal: bool) {
    let Some(dashboard) = world.get::<Dashboard>(owner) else {
        return;
    };
    let body = dashboard.body;
    let toggle = dashboard.toggle;
    let settings = world.get::<Panel>(owner).map(|panel| panel.content);
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.display = if normal { Display::Flex } else { Display::None };
    }
    if let Some(mut node) = settings.and_then(|settings| world.get_mut::<Node>(settings)) {
        node.display = if normal { Display::None } else { Display::Flex };
    }
    let caption = if normal {
        "Manage Fiote"
    } else {
        "Back to conversations"
    };
    let text = world
        .get::<Children>(toggle)
        .and_then(|children| children.first())
        .copied();
    if let Some(mut text) = text.and_then(|text| world.get_mut::<Text>(text)) {
        text.0 = caption.into();
    }
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle) {
        accessibility.set_label(caption);
        accessibility.set_expanded(!normal);
    }
}

pub(super) fn open_current(world: &mut World, owner: Entity) {
    if let Some(conversation) = world
        .get::<Dashboard>(owner)
        .map(|dashboard| dashboard.conversation)
    {
        thread_castle::dashboard_current(world, conversation);
    }
}

pub(super) fn update(world: &mut World, owner: Entity, saved: &FioteStatus) {
    let Some(dashboard) = world.get::<Dashboard>(owner) else {
        return;
    };
    let roster = dashboard.roster;
    let conversation = dashboard.conversation;
    thread_castle::dashboard_activity(world, conversation, &saved.running);
    let Some(dashboard) = world.get::<Dashboard>(owner) else {
        return;
    };
    let previous: std::collections::HashMap<_, _> = dashboard
        .rows
        .iter()
        .map(|row| (row.record.as_str(), row.detail.as_str()))
        .collect();
    let rows: Vec<_> = saved
        .fiotes
        .iter()
        .map(|choice| {
            let selected = choice.record == saved.record;
            let detail = if let Some(state) = &choice.dashboard {
                let activity = if state.working {
                    "Working"
                } else if state.ready {
                    "Idle"
                } else {
                    "Needs setup"
                };
                state.model.as_ref().map_or_else(
                    || activity.to_owned(),
                    |model| format!("{activity} · {model}"),
                )
            } else if selected {
                let activity = if !saved.running.is_empty() {
                    "Working"
                } else if !saved.settings.enabled || saved.requires_credential && !saved.has_key {
                    "Needs setup"
                } else {
                    "Idle"
                };
                if saved.settings.model.is_empty() {
                    activity.into()
                } else {
                    format!("{activity} · {}", saved.settings.model)
                }
            } else {
                previous
                    .get(choice.record.as_str())
                    .copied()
                    .unwrap_or_default()
                    .to_owned()
            };
            RosterRow {
                record: choice.record.clone(),
                title: choice.title.clone(),
                detail,
                selected,
            }
        })
        .collect();
    if rows == dashboard.rows {
        return;
    }
    let children: Vec<_> = world
        .get::<Children>(roster)
        .into_iter()
        .flatten()
        .filter(|child| {
            world
                .get::<bevy::a11y::AccessibilityNode>(**child)
                .is_some()
        })
        .copied()
        .collect();
    for child in children {
        world.despawn(child);
    }
    for row in &rows {
        let select = button(
            world,
            roster,
            owner,
            &row.title,
            management::Select(row.record.clone()),
        );
        if let Some(mut node) = world.get_mut::<Node>(select) {
            node.flex_direction = FlexDirection::Column;
            node.align_items = AlignItems::Start;
            node.flex_shrink = 0.0;
            node.min_width = px(0);
        }
        if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(select) {
            accessibility.set_selected(row.selected);
            if !row.detail.is_empty() {
                accessibility.set_description(row.detail.as_str());
            }
        }
        if row.selected {
            world
                .entity_mut(select)
                .insert(token_style::background(Token::Accent));
        }
        if !row.detail.is_empty() {
            let detail = label(world, select, &row.detail, 12.0);
            if let Some(mut node) = world.get_mut::<Node>(detail) {
                node.max_width = percent(100);
            }
            world.entity_mut(detail).insert(bevy::text::TextLayout {
                linebreak: bevy::text::LineBreak::WordOrCharacter,
                ..default()
            });
        }
    }
    if let Some(mut dashboard) = world.get_mut::<Dashboard>(owner) {
        dashboard.rows = rows;
    }
}
