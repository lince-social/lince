use super::*;
use crate::{actions::ActionButton, edit_mode::label, tokens::Token};
use bevy::{
    input::keyboard::KeyboardInput,
    input_focus::{FocusCause, FocusedInput, InputFocus, tab_navigation::TabIndex},
    picking::events::Scroll,
};

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    text: &str,
    tip: &str,
    command: Command,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            Node {
                padding: UiRect::axes(px(8), px(6)),
                min_width: px(0),
                min_height: px(32),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
            crate::icons::Tooltip(tip.into()),
            ActionButton::new(owner, crate::actions![command]),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(tip);
    }
    let text = label(world, entity, text, 14.0);
    world.get_mut::<Node>(text).unwrap().max_width = percent(100);
    entity
}

fn scroll(mut event: On<Pointer<Scroll>>, mut scrolls: Query<&mut ScrollPosition>) {
    if !event.y.is_finite() {
        return;
    }
    if let Ok(mut position) = scrolls.get_mut(event.entity) {
        let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
            28.0
        } else {
            1.0
        };
        position.0.y = (position.0.y - event.y * multiplier).max(0.0);
        event.propagate(false);
    }
}

fn keyboard(
    mut event: On<FocusedInput<KeyboardInput>>,
    mut scrolls: Query<(&mut ScrollPosition, &ComputedNode)>,
) {
    if !event.input.state.is_pressed() {
        return;
    }
    let Ok((mut position, node)) = scrolls.get_mut(event.focused_entity) else {
        return;
    };
    let page = node.size().y * node.inverse_scale_factor();
    let max = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
    let next = match event.input.key_code {
        KeyCode::ArrowDown => position.0.y + 28.0,
        KeyCode::ArrowUp => position.0.y - 28.0,
        KeyCode::PageDown => position.0.y + page,
        KeyCode::PageUp => position.0.y - page,
        KeyCode::Home => 0.0,
        KeyCode::End => max,
        _ => return,
    };
    position.0.y = next.clamp(0.0, max);
    event.propagate(false);
}

fn scrolling(world: &mut World, parent: Entity, name: &str) -> Entity {
    let mut accessibility = accesskit::Node::new(accesskit::Role::ScrollView);
    accessibility.set_label(name);
    world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                min_width: px(0),
                min_height: px(0),
                height: percent(100),
                row_gap: px(10),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ChildOf(parent),
            ScrollPosition::default(),
            TabIndex(0),
            bevy::a11y::AccessibilityNode::from(accessibility),
        ))
        .observe(scroll)
        .observe(keyboard)
        .id()
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let nav_scroll = world
        .get::<View>(owner)
        .and_then(|view| view.nav)
        .and_then(|nav| world.get::<ScrollPosition>(nav))
        .cloned()
        .unwrap_or_default();
    let pages = world.resource::<Book>().0.clone();
    let entries = world.resource::<Book>().1.clone();
    let selected = world
        .get::<Instinct>(owner)
        .and_then(|state| state.page.as_deref());
    let selected = entries
        .iter()
        .find(|entry| Some(entry.id.as_str()) == selected)
        .or_else(|| entries.first())
        .cloned();
    let index = pages
        .iter()
        .position(|page| selected.as_ref().is_some_and(|entry| entry.page == page.id))
        .unwrap_or(0);
    let mut focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let mut refocus = false;
    while let Some(entity) = focus {
        if entity == owner {
            refocus = true;
            break;
        }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    if let Some(body) = world.get_mut::<View>(owner).unwrap().body.take() {
        world.despawn(body);
    }
    let body = world
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    world.get_mut::<View>(owner).unwrap().body = Some(body);
    label(world, body, "Instinct", 18.0);
    if let Some(error) = world.resource::<Book>().2.clone() {
        label(world, body, "Instinct could not be loaded.", 16.0);
        label(world, body, &error, 14.0);
        return;
    }
    let Some(page) = pages.get(index) else {
        label(world, body, "No Instinct Records are embedded.", 16.0);
        return;
    };
    let selected = selected.unwrap();
    world.get_mut::<Instinct>(owner).unwrap().page = Some(selected.id.clone());
    if selected.id != page.id {
        world
            .entity_mut(owner)
            .insert(ScrollToRecord(selected.uid.clone()));
    }
    let main = world
        .spawn((
            Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_basis: px(0),
                min_height: px(0),
                column_gap: px(16),
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    let nav = scrolling(world, main, "Instinct Records");
    world.entity_mut(nav).insert(nav_scroll);
    world.get_mut::<View>(owner).unwrap().nav = Some(nav);
    world.get_mut::<Node>(nav).unwrap().width = percent(28);
    let mut selected_button = None;
    for entry in entries.iter() {
        let tab = button(
            world,
            nav,
            owner,
            &entry.title,
            &entry.title,
            Command::Page(entry.id.clone()),
        );
        let mut node = world.get_mut::<Node>(tab).unwrap();
        node.width = percent(100);
        node.padding.left = px(8.0 + entry.depth.min(8) as f32 * 10.0);
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(tab)
            .unwrap()
            .set_selected(entry.id == selected.id);
        if entry.id == selected.id {
            world.entity_mut(tab).insert(SelectedEntry);
            world.entity_mut(owner).insert(RevealSelection);
            world.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(4),
                    bottom: px(4),
                    width: px(3),
                    ..default()
                },
                crate::token_style::background(Token::Accent),
                Pickable::IGNORE,
                ChildOf(tab),
            ));
            selected_button = Some(tab);
        }
    }
    let article = scrolling(world, main, &page.title);
    world.get_mut::<Node>(article).unwrap().flex_grow = 1.0;
    world.get_mut::<Node>(article).unwrap().flex_basis = px(0);
    crate::description::heading(world, article, &page.title, 26.0);
    world.get_mut::<View>(owner).unwrap().article = Some(article);
    for (index, section) in page.sections.iter().enumerate() {
        let container = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    flex_shrink: 0.0,
                    ..default()
                },
                RecordSection(section.uid.clone()),
                ChildOf(article),
            ))
            .id();
        if index > 0 {
            crate::description::heading(world, container, &section.title, 22.0);
        }
        if section.tutorial {
            crate::description::button(
                world,
                container,
                owner,
                "Tutorial: Areas of Influence",
                crate::tutorial::Start,
            );
        }
        crate::description::spawn(
            world,
            container,
            &section.body,
            crate::description::Context {
                owner,
                source: crate::protein_area::Source::Local,
            },
        );
    }
    let footer = world
        .spawn((
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    let previous = button(
        world,
        footer,
        owner,
        "‹",
        "Previous chapter",
        Command::Page(pages[index.saturating_sub(1)].id.clone()),
    );
    if index == 0 {
        world
            .entity_mut(previous)
            .insert(bevy::ui::InteractionDisabled);
    }
    label(
        world,
        footer,
        &format!("{} / {}", index + 1, pages.len()),
        14.0,
    );
    let next = button(
        world,
        footer,
        owner,
        "›",
        "Next chapter",
        Command::Page(pages[(index + 1).min(pages.len() - 1)].id.clone()),
    );
    if index + 1 == pages.len() {
        world.entity_mut(next).insert(bevy::ui::InteractionDisabled);
    }
    if refocus
        && let Some(mut focus) = world.get_resource_mut::<InputFocus>()
        && let Some(selected) = selected_button
    {
        focus.set(selected, FocusCause::Navigated);
    }
}

#[derive(Component)]
pub(super) struct ScrollToRecord(pub String);

#[derive(Component)]
struct RecordSection(String);

#[derive(Component)]
struct SelectedEntry;

#[derive(Component)]
struct RevealSelection;

pub(super) fn reveal_selection(world: &mut World) {
    let pending: Vec<_> = world
        .query_filtered::<(Entity, &View), With<RevealSelection>>()
        .iter(world)
        .filter_map(|(owner, view)| Some((owner, view.nav?)))
        .collect();
    for (owner, nav) in pending {
        let target = world
            .query_filtered::<(&UiGlobalTransform, &ComputedNode, &ChildOf), With<SelectedEntry>>()
            .iter(world)
            .find(|(_, _, parent)| parent.parent() == nav)
            .map(|(transform, node, _)| {
                (
                    transform.translation.y - node.size().y / 2.0,
                    transform.translation.y + node.size().y / 2.0,
                )
            });
        let Some((top, bottom)) = target else {
            continue;
        };
        let Some(node) = world.get::<ComputedNode>(nav) else {
            continue;
        };
        if node.size().y <= 0.0 {
            continue;
        }
        let Some(transform) = world.get::<UiGlobalTransform>(nav) else {
            continue;
        };
        let start = transform.translation.y - node.size().y / 2.0;
        let end = start + node.size().y;
        let offset = if top < start {
            top - start
        } else if bottom > end {
            bottom - end
        } else {
            0.0
        } * node.inverse_scale_factor();
        if let Some(mut scroll) = world.get_mut::<ScrollPosition>(nav) {
            scroll.0.y = (scroll.0.y + offset).max(0.0);
        }
        world.entity_mut(owner).remove::<RevealSelection>();
    }
}

pub(super) fn scroll_to_record(world: &mut World) {
    let pending: Vec<_> = world
        .query::<(Entity, &ScrollToRecord, &View)>()
        .iter(world)
        .filter_map(|(owner, request, view)| Some((owner, request.0.clone(), view.article?)))
        .collect();
    for (owner, uid, article) in pending {
        let target = world
            .query::<(&RecordSection, &UiGlobalTransform, &ComputedNode, &ChildOf)>()
            .iter(world)
            .find(|(section, _, _, parent)| section.0 == uid && parent.parent() == article)
            .map(|(_, transform, node, _)| transform.translation - node.size() / 2.0);
        let Some(target) = target else { continue };
        let Some(node) = world.get::<ComputedNode>(article) else {
            continue;
        };
        if node.size().y <= 0.0 {
            continue;
        }
        let Some(transform) = world.get::<UiGlobalTransform>(article) else {
            continue;
        };
        let offset = (target.y - transform.translation.y + node.size().y / 2.0)
            * node.inverse_scale_factor();
        if let Some(mut scroll) = world.get_mut::<ScrollPosition>(article) {
            scroll.0.y = (scroll.0.y + offset).max(0.0);
        }
        world.entity_mut(owner).remove::<ScrollToRecord>();
    }
}
