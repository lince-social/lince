use super::{Arrangement, LayoutBox, Overflow, Sizing, engine};
use crate::{
    actions::{Action, ActionButton},
    edit_mode::{EditMode, label},
    sand_text::SandText,
};
use bevy::{a11y::AccessibilityNode, picking::events::Scroll, prelude::*, text::EditableText};

#[derive(Component)]
pub struct LayoutPanel {
    root: Entity,
    target: Entity,
    fields: Vec<Entity>,
    error: Entity,
    choose_parent: bool,
}

#[derive(Clone, Copy)]
pub enum LayoutAction {
    Open,
    Close,
    Apply,
    Sizing(usize, Sizing),
    Overflow(usize, Overflow),
    Arrange(Arrangement),
    Wrap,
    ChooseParent,
    Parent(Entity),
    Detach,
}

pub fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..super::MAX_DEPTH {
        if world.get::<EditMode>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
    None
}

fn allowed(world: &World, entity: Entity) -> Option<Entity> {
    let root = root(world, entity)?;
    let mode = world.get::<EditMode>(root)?;
    let owner = if world.get::<SandText>(entity).is_some() {
        world.get::<ChildOf>(entity)?.parent()
    } else {
        entity
    };
    let member = world.get::<crate::workspace::WorkspaceMember>(owner)?;
    (mode.enabled && world.get::<crate::workspace::Workspaces>(root)?.active == member.0)
        .then_some(root)
}

impl Action for LayoutAction {
    fn apply(&self, world: &mut World, target: Entity) {
        let Some(root) = allowed(world, target) else {
            return;
        };
        let current = world
            .query::<(Entity, &LayoutPanel)>()
            .iter(world)
            .find(|(_, panel)| panel.root == root)
            .map(|(entity, panel)| (entity, panel.target, panel.choose_parent));
        if matches!(self, Self::Open) {
            if let Some((entity, _, _)) = current {
                world.despawn(entity);
            }
            render(world, root, target, false);
            return;
        }
        let Some((panel, selected, choose_parent)) = current else {
            return;
        };
        if selected != target {
            return;
        }
        if matches!(self, Self::Close) {
            world.despawn(panel);
            crate::edit_mode::render_panel(world, root);
            return;
        }
        let result = apply(world, panel, target, *self);
        if let Err(error) = result {
            let notice = world.get::<LayoutPanel>(panel).unwrap().error;
            world.get_mut::<Text>(notice).unwrap().0 = error.into();
            return;
        }
        world.despawn(panel);
        render(
            world,
            root,
            target,
            if matches!(self, Self::ChooseParent) {
                !choose_parent
            } else {
                false
            },
        );
    }
}

fn apply(
    world: &mut World,
    panel: Entity,
    target: Entity,
    action: LayoutAction,
) -> Result<(), &'static str> {
    let mut rules = engine::rules(world, target).ok_or("This item is no longer available.")?;
    let fields = &world.get::<LayoutPanel>(panel).unwrap().fields;
    let mut numbers = Vec::new();
    for field in fields {
        let text = world
            .get::<EditableText>(*field)
            .ok_or("Open the controls again.")?;
        if text.is_composing() || text.pending_paste.is_some() {
            return Err("Finish typing or pasting first.");
        }
        let number = text
            .value()
            .to_string()
            .trim()
            .parse::<f32>()
            .map_err(|_| "Use numbers for sizes and spacing.")?;
        if !number.is_finite() {
            return Err("Use finite numbers for sizes and spacing.");
        }
        numbers.push(number);
    }
    if numbers.len() != 12 {
        return Err("Open the controls again.");
    }
    for axis in 0..2 {
        rules.axes[axis].size = numbers[axis * 3];
        rules.axes[axis].min = numbers[axis * 3 + 1];
        rules.axes[axis].max = numbers[axis * 3 + 2];
    }
    rules.padding = numbers[6];
    rules.gap = numbers[7];
    if numbers[8].fract() != 0.0 || !(1.0..=128.0).contains(&numbers[8]) {
        return Err("Use 1–128 columns.");
    }
    rules.columns = numbers[8] as u16;
    if numbers[9..11]
        .iter()
        .any(|n| !(0.0..=super::LIMIT).contains(n))
    {
        return Err("Position must be 0–100000.");
    }
    if numbers[11].fract() != 0.0 || !(-100_000.0..=100_000.0).contains(&numbers[11]) {
        return Err("Order must be a whole number between -100000 and 100000.");
    }
    match action {
        LayoutAction::Sizing(axis, sizing) => rules.axes[axis].sizing = sizing,
        LayoutAction::Overflow(axis, overflow) => rules.axes[axis].overflow = overflow,
        LayoutAction::Arrange(arrangement) => rules.arrangement = arrangement,
        LayoutAction::Wrap => {
            if !rules.wrap && rules.axes[0].sizing == Sizing::Fit {
                return Err("Choose fixed width or fill parent before wrapping text.");
            }
            rules.wrap = !rules.wrap;
        }
        _ => {}
    }
    if !rules.valid() {
        return Err(
            "Sizes must be between the minimum and maximum, within 1–100000. Spacing must be 0–10000.",
        );
    }
    if let LayoutAction::Parent(parent) = action {
        engine::attach(world, target, parent)?;
    }
    engine::configure(world, target, rules)?;
    if matches!(action, LayoutAction::Detach) {
        engine::detach(world, target);
    }
    if let Some(mut text) = world.get_mut::<SandText>(target) {
        text.offset = [numbers[9], numbers[10]];
        text.size = [rules.axes[0].size, rules.axes[1].size];
        text.order = numbers[11] as i32;
    } else if let Some(mut layout) = world.get_mut::<LayoutBox>(target) {
        if !matches!(action, LayoutAction::Parent(_)) {
            layout.offset = [numbers[9], numbers[10]];
            layout.order = numbers[11] as i32;
        }
    }
    super::records::remember(world, target);
    Ok(())
}

pub fn button(
    world: &mut World,
    parent: Entity,
    target: Entity,
    action: LayoutAction,
    title: &str,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            ActionButton::new(target, crate::actions![action]),
            AccessibilityNode::default(),
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    label(world, entity, title, 14.0);
    entity
}

fn name(world: &World, entity: Entity) -> String {
    if let Some(area) = world.get::<crate::area::InfluenceArea>(entity) {
        return area.name.clone();
    }
    if world.get::<SandText>(entity).is_some() {
        let title: String = crate::sand_text::value(world, entity)
            .chars()
            .take(30)
            .collect();
        return if title.is_empty() {
            "Text".into()
        } else {
            title.replace('\n', " ")
        };
    }
    let sand = world.get::<crate::sand_store::StoredSand>(entity);
    if let Some(content) = sand.and_then(|sand| sand.content) {
        let title: String = crate::sand_text::value(world, content)
            .chars()
            .take(30)
            .collect();
        if !title.is_empty() {
            return format!(
                "{} · {}",
                sand.unwrap().kind.name(),
                title.replace('\n', " ")
            );
        }
    }
    if let Some(title) = world
        .get::<crate::area::RecordProperties>(entity)
        .and_then(|record| record.0.get("head"))
        .and_then(serde_json::Value::as_str)
    {
        return title.chars().take(40).collect();
    }
    let number = world
        .get::<ChildOf>(entity)
        .and_then(|parent| world.get::<Children>(parent.parent()))
        .and_then(|children| {
            children
                .iter()
                .filter(|child| world.get::<crate::canvas::CanvasItem>(*child).is_some())
                .position(|child| child == entity)
        })
        .map_or(1, |index| index + 1);
    format!("{} {number}", sand.map_or("Sand", |sand| sand.kind.name()))
}

fn field(world: &mut World, parent: Entity, title: &str, value: f32) -> Entity {
    label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor(
        &value.to_string(),
        world.resource::<crate::theme::Typography>(),
        0,
    );
    let entity = world
        .spawn((bundle, AccessibilityNode::default(), ChildOf(parent)))
        .id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(16);
    world.get_mut::<Node>(entity).unwrap().flex_shrink = 0.0;
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    entity
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn render(world: &mut World, root: Entity, target: Entity, choose_parent: bool) {
    let Some(rules) = engine::rules(world, target) else {
        return;
    };
    let panel = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(56),
                width: px(360),
                max_width: percent(95),
                max_height: percent(85),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                overflow: bevy::prelude::Overflow::scroll_y(),
                ..default()
            },
            GlobalZIndex(30),
            crate::inspection::InspectionExcluded,
            crate::token_style::background(crate::tokens::Token::Surface),
            ChildOf(root),
        ))
        .observe(
            |mut event: On<Pointer<Scroll>>,
             mut panels: Query<&mut ScrollPosition, With<LayoutPanel>>| {
                if let Ok(mut position) = panels.get_mut(event.entity) {
                    position.0.y = (position.0.y
                        - event.y
                            * if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                                24.0
                            } else {
                                1.0
                            })
                    .max(0.0);
                    event.propagate(false);
                }
            },
        )
        .id();
    label(
        world,
        panel,
        &format!("Layout · {}", name(world, target)),
        20.0,
    );
    if world.get::<crate::area::InfluenceArea>(target).is_some() {
        label(
            world,
            panel,
            "Square and circle areas keep equal width and height.",
            14.0,
        );
    }
    button(world, panel, target, LayoutAction::Close, "Close layout");
    let mut fields = Vec::new();
    for (axis, title) in ["Width", "Height"].into_iter().enumerate() {
        label(
            world,
            panel,
            &format!("{title}: {:?}", rules.axes[axis].sizing),
            18.0,
        );
        let choices = row(world, panel);
        for (mode, title) in [
            (Sizing::Fixed, "Fixed"),
            (Sizing::Fit, "Fit contents"),
            (Sizing::Fill, "Fill parent"),
        ] {
            button(
                world,
                choices,
                target,
                LayoutAction::Sizing(axis, mode),
                title,
            );
        }
        for (title, value) in [
            ("Size", rules.axes[axis].size),
            ("Minimum", rules.axes[axis].min),
            ("Maximum", rules.axes[axis].max),
        ] {
            fields.push(field(world, panel, title, value));
        }
        label(
            world,
            panel,
            &format!("Overflow: {:?}", rules.axes[axis].overflow),
            14.0,
        );
        let choices = row(world, panel);
        button(
            world,
            choices,
            target,
            LayoutAction::Overflow(axis, Overflow::Clip),
            "Clip",
        );
        button(
            world,
            choices,
            target,
            LayoutAction::Overflow(axis, Overflow::Scroll),
            "Scroll",
        );
    }
    if world.get::<SandText>(target).is_some() {
        button(
            world,
            panel,
            target,
            LayoutAction::Wrap,
            if rules.wrap {
                "Wrapping: on"
            } else {
                "Wrapping: off"
            },
        );
    }
    label(
        world,
        panel,
        &format!("Arrangement: {:?}", rules.arrangement),
        18.0,
    );
    let choices = row(world, panel);
    for (arrangement, title) in [
        (Arrangement::Free, "Free"),
        (Arrangement::Row, "Row"),
        (Arrangement::Column, "Column"),
        (Arrangement::Grid, "Grid"),
    ] {
        button(
            world,
            choices,
            target,
            LayoutAction::Arrange(arrangement),
            title,
        );
    }
    let layout = world.get::<LayoutBox>(target).copied();
    let offset = world
        .get::<SandText>(target)
        .map(|text| text.offset)
        .or(layout.map(|layout| layout.offset))
        .unwrap_or_default();
    for (title, value) in [
        ("Padding", rules.padding),
        ("Gap", rules.gap),
        ("Columns", f32::from(rules.columns)),
        ("Left in parent", offset[0]),
        ("Top in parent", offset[1]),
        (
            "Order in parent",
            world.get::<SandText>(target).map_or_else(
                || layout.map_or(0, |layout| layout.order),
                |text| text.order,
            ) as f32,
        ),
    ] {
        fields.push(field(world, panel, title, value));
    }
    button(
        world,
        panel,
        target,
        LayoutAction::Apply,
        "Apply sizes and spacing",
    );
    let error = label(world, panel, "", 14.0);
    let physical_parent = world.get::<ChildOf>(target).map(ChildOf::parent);
    if world.get::<SandText>(target).is_some() {
        if let Some(parent) = physical_parent {
            button(
                world,
                panel,
                parent,
                LayoutAction::Open,
                "Enclosing Sand layout",
            );
        }
    } else {
        if let Some(parent_id) = layout.and_then(|layout| layout.parent) {
            let parent = world
                .query::<(Entity, &LayoutBox)>()
                .iter(world)
                .find(|(entity, layout)| {
                    layout.id == parent_id
                        && world.get::<ChildOf>(*entity).map(ChildOf::parent) == physical_parent
                })
                .map(|(entity, _)| entity);
            if let Some(parent) = parent {
                button(
                    world,
                    panel,
                    parent,
                    LayoutAction::Open,
                    &format!("Parent: {}", name(world, parent)),
                );
            }
            button(
                world,
                panel,
                target,
                LayoutAction::Detach,
                "Detach from parent",
            );
        }
        button(
            world,
            panel,
            target,
            LayoutAction::ChooseParent,
            "Choose enclosing Sand or area",
        );
        if choose_parent {
            let member = world
                .get::<crate::workspace::WorkspaceMember>(target)
                .copied();
            let candidates: Vec<_> = world
                .query::<(
                    Entity,
                    &crate::canvas::CanvasItem,
                    &ChildOf,
                    &crate::workspace::WorkspaceMember,
                )>()
                .iter(world)
                .filter(|(entity, _, parent, workspace)| {
                    *entity != target
                        && Some(parent.parent()) == physical_parent
                        && Some(**workspace) == member
                })
                .map(|(entity, _, _, _)| entity)
                .collect();
            for candidate in candidates {
                button(
                    world,
                    panel,
                    target,
                    LayoutAction::Parent(candidate),
                    &name(world, candidate),
                );
            }
        }
        let children: Vec<_> = world
            .get::<Children>(target)
            .map(|children| {
                children
                    .iter()
                    .filter(|entity| world.get::<SandText>(*entity).is_some())
                    .collect()
            })
            .unwrap_or_default();
        for child in children {
            button(
                world,
                panel,
                child,
                LayoutAction::Open,
                &format!("Text layout: {}", name(world, child)),
            );
        }
    }
    label(
        world,
        panel,
        "Fit follows the visible child size. Fixed size with scrolling stops growth. Shift + wheel scrolls sideways.",
        14.0,
    );
    world.entity_mut(panel).insert(LayoutPanel {
        root,
        target,
        fields,
        error,
        choose_parent,
    });
}

pub(super) fn cleanup(world: &mut World) {
    let stale: Vec<_> = world
        .query::<(Entity, &LayoutPanel)>()
        .iter(world)
        .filter(|(_, panel)| allowed(world, panel.target) != Some(panel.root))
        .map(|(entity, _)| entity)
        .collect();
    for entity in stale {
        world.despawn(entity);
    }
}
