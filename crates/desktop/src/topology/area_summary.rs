use crate::area::{AreaShape, Direction, InfluenceArea};
use bevy::{math::DVec2, prelude::*};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behavior {
    Spawn,
    Attraction,
    Enter,
    Leave,
    ChangeFilter,
    Sorting,
    Immunity,
    Scale,
    SoundEnter,
    SoundLeave,
}

#[derive(Component)]
struct Summary {
    area: InfluenceArea,
    panel: Entity,
}

#[derive(Component)]
pub(crate) struct SummaryAtBottom;

#[derive(Component)]
struct FloatingSummary(Entity);

#[derive(Component)]
struct BehaviorRow {
    area: Entity,
    full_text: String,
    button: Entity,
}

#[derive(Resource, Default)]
struct HoverPreview {
    row: Option<Entity>,
    popup: Option<Entity>,
}

#[derive(Clone, Copy)]
struct Remove(Behavior);

impl crate::actions::Action for Remove {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            return;
        };
        if !editing(world, owner) || !crate::area_panel::owns(world, root, owner) {
            return;
        }
        let mut area = world.get::<InfluenceArea>(owner).unwrap().clone();
        match self.0 {
            Behavior::Spawn => area.protein = None,
            Behavior::Attraction => area.strength = 0.0,
            Behavior::Enter => area.changes.enter = Default::default(),
            Behavior::Leave => area.changes.leave = Default::default(),
            Behavior::ChangeFilter => area.change_filter = None,
            Behavior::Sorting => area.sorting = None,
            Behavior::Immunity => area.immunity = crate::area_effects::Immunity::None,
            Behavior::Scale => area.scale = 1.0,
            Behavior::SoundEnter => {
                if let Some(sound) = &mut area.sound {
                    sound.enter.clear();
                }
            }
            Behavior::SoundLeave => {
                if let Some(sound) = &mut area.sound {
                    sound.leave.clear();
                }
            }
        }
        if area
            .sound
            .as_ref()
            .is_some_and(|sound| sound.enter.is_empty() && sound.leave.is_empty())
        {
            area.sound = None;
        }
        if !area.validate() || world.get::<InfluenceArea>(owner) == Some(&area) {
            return;
        }
        crate::area_mutation::disarm(world, owner, "Area behavior removed.");
        world.entity_mut(owner).insert(area);
    }
}

fn compact(value: &str, limit: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() <= limit {
        value
    } else {
        format!(
            "{}...",
            value
                .chars()
                .take(limit.saturating_sub(3))
                .collect::<String>()
        )
    }
}

fn predicate(value: &Value) -> String {
    if let Some(values) = value.as_array() {
        return values
            .iter()
            .map(predicate)
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" and ");
    }
    let Some(object) = value.as_object() else {
        return value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
    };
    object
        .iter()
        .map(|(key, value)| match key.as_str() {
            "all" | "any" => value
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .map(predicate)
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                        .join(if key == "all" { " and " } else { " or " })
                })
                .unwrap_or_default(),
            "not" => format!("not ({})", predicate(value)),
            "concept_in" => format!("#{}", predicate(value)),
            "quantity_eq" => format!("quantity = {}", predicate(value)),
            "kind_eq" => predicate(value),
            _ => format!("{}: {}", key.replace('_', " "), predicate(value)),
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

fn query(config: &crate::protein_area::Config) -> String {
    let source = config.draft.query["source"].as_str().unwrap_or("records");
    let filter = predicate(&config.draft.query["where"]);
    if filter.is_empty() {
        format!("all {source}")
    } else {
        format!("{source}: {filter}")
    }
}

fn changes(value: &engine::area_transition::RecordChanges) -> String {
    let mut parts = Vec::new();
    if let Some(quantity) = &value.quantity {
        parts.push(format!("quantity {quantity}"));
    }
    parts.extend(value.assert.iter().map(|name| format!("+#{name}")));
    parts.extend(value.retract.iter().map(|name| format!("-#{name}")));
    parts.extend(value.assign.iter().map(|name| format!("assign {name}")));
    parts.extend(value.unassign.iter().map(|name| format!("unassign {name}")));
    parts.join(", ")
}

fn lines(area: &InfluenceArea) -> Vec<(Behavior, String)> {
    let mut rows = Vec::new();
    if let Some(protein) = &area.protein {
        rows.push((Behavior::Spawn, format!("Spawns · {}", query(protein))));
    }
    let filter = area.filter.as_ref().map(query).unwrap_or_else(|| {
        if area.rules.is_empty() {
            "general".into()
        } else {
            area.rules
                .iter()
                .map(|rule| format!("{} = {}", rule.property.name(), rule.value))
                .collect::<Vec<_>>()
                .join(if area.match_all { " and " } else { " or " })
        }
    });
    if area.attraction_enabled && area.strength > 0.0 {
        rows.push((
            Behavior::Attraction,
            format!(
                "{} · {}",
                if area.direction == Direction::Attract {
                    "Attracts"
                } else {
                    "Repels"
                },
                if area.filter.is_none() && area.rules.is_empty() {
                    "choose properties"
                } else {
                    &filter
                }
            ),
        ));
    }
    if area.changes_enabled {
        if !area.changes.enter.is_empty() {
            rows.push((
                Behavior::Enter,
                format!("On entry · {}", changes(&area.changes.enter)),
            ));
        }
        if !area.changes.leave.is_empty() {
            rows.push((
                Behavior::Leave,
                format!("On exit · {}", changes(&area.changes.leave)),
            ));
        }
        if !area.changes.is_empty()
            && let Some(filter) = &area.change_filter
        {
            rows.push((
                Behavior::ChangeFilter,
                format!("Changes for · {}", query(filter)),
            ));
        }
    }
    if area.attraction_enabled {
        if let Some(sorting) = &area.sorting {
            rows.push((
                Behavior::Sorting,
                format!(
                    "Sorts · {} · {}",
                    if sorting.horizontal {
                        "horizontal"
                    } else {
                        "vertical"
                    },
                    if sorting.reverse {
                        "reverse"
                    } else {
                        "forward"
                    }
                ),
            ));
        }
        if area.immunity != crate::area_effects::Immunity::None {
            rows.push((
                Behavior::Immunity,
                format!("Immunity · {:?}", area.immunity),
            ));
        }
        if area.scale != 1.0 {
            rows.push((Behavior::Scale, format!("Size × {} · {filter}", area.scale)));
        }
    }
    if let Some(sound) = &area.sound {
        if !sound.enter.is_empty() {
            rows.push((
                Behavior::SoundEnter,
                format!("Sound on enter · {}", sound.enter),
            ));
        }
        if !sound.leave.is_empty() {
            rows.push((
                Behavior::SoundLeave,
                format!("Sound on leave · {}", sound.leave),
            ));
        }
    }
    if rows.is_empty() {
        rows.push((Behavior::Spawn, "No behaviors".into()));
    }
    rows
}

fn inside(area: &InfluenceArea, center: DVec2, half: DVec2) -> bool {
    let corners = [
        center - half,
        center + DVec2::new(half.x, -half.y),
        center + half,
        center + DVec2::new(-half.x, half.y),
    ];
    if !corners.iter().all(|point| area.contains(*point)) {
        return false;
    }
    if !matches!(area.shape, AreaShape::Polygon(_)) {
        return true;
    }
    let min = center - half;
    let max = center + half;
    !area.outline().windows(2).any(|edge| {
        let delta = edge[1] - edge[0];
        let mut near: f64 = 0.0;
        let mut far: f64 = 1.0;
        for axis in 0..2 {
            if delta[axis].abs() < 1e-12 {
                if edge[0][axis] <= min[axis] || edge[0][axis] >= max[axis] {
                    return false;
                }
            } else {
                let a = (min[axis] - edge[0][axis]) / delta[axis];
                let b = (max[axis] - edge[0][axis]) / delta[axis];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
            }
        }
        near < far
    })
}

fn bounds(area: &InfluenceArea, rows: usize) -> (DVec2, Vec2, f32) {
    let center = DVec2::from_array(area.center);
    let size = DVec2::from_array(area.size);
    let desired = DVec2::new(304.0, rows as f64 * 32.0 - 4.0);
    let maximum = (size * 0.9 / desired).min_element().min(1.0);
    let mut best = (center, 0.0);
    for y in 0..9 {
        for x in 0..9 {
            let point = center + DVec2::new(f64::from(x - 4), f64::from(y - 4)) * size * 0.1;
            let mut low = 0.0;
            let mut high = maximum;
            for _ in 0..16 {
                let scale = (low + high) * 0.5;
                if inside(area, point, desired * scale * 0.5) {
                    low = scale;
                } else {
                    high = scale;
                }
            }
            if low > best.1 + 1e-5
                || ((low - best.1).abs() <= 1e-5
                    && point.distance_squared(center) < best.0.distance_squared(center))
            {
                best = (point, low);
            }
        }
    }
    (
        best.0 - center + size * 0.5,
        (desired * best.1).as_vec2(),
        best.1 as f32,
    )
}

pub(super) fn update(world: &mut World) {
    if !world.contains_resource::<crate::theme::Typography>() {
        return;
    }
    let areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter_map(|(entity, area)| {
            let mut area = area.clone();
            area.center = [0.0; 2];
            world
                .get::<Summary>(entity)
                .is_none_or(|summary| summary.area != area)
                .then_some((entity, area))
        })
        .collect();
    let obsolete: Vec<_> = world
        .query::<(Entity, &FloatingSummary)>()
        .iter(world)
        .filter(|(_, summary)| world.get::<InfluenceArea>(summary.0).is_none())
        .map(|(entity, _)| entity)
        .collect();
    for entity in obsolete {
        world.despawn(entity);
    }
    for (entity, area) in areas {
        if let Some(previous) = world.entity_mut(entity).take::<Summary>() {
            world.despawn(previous.panel);
        }
        if world.get::<Node>(entity).is_none() {
            world.entity_mut(entity).insert(Node {
                overflow: Overflow::clip(),
                ..default()
            });
        }
        let rows = lines(&area);
        let (mut center, size, scale) = bounds(&area, rows.len());
        let floating = world.get::<SummaryAtBottom>(entity).is_some();
        let parent = if floating {
            world.get::<ChildOf>(entity).unwrap().parent()
        } else {
            entity
        };
        if floating {
            center.y = area.size[1] + f64::from(size.y) * 0.5 + 8.0;
        }
        let panel = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(center.x as f32 - size.x * 0.5),
                    top: px(center.y as f32 - size.y * 0.5),
                    width: px(size.x),
                    height: px(size.y),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4.0 * scale),
                    overflow: Overflow::clip(),
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(parent),
            ))
            .id();
        if floating {
            world.entity_mut(panel).insert((
                FloatingSummary(entity),
                GlobalZIndex(5),
                crate::inspection::InspectionExcluded,
            ));
        }
        for (behavior, row) in rows {
            let rectangle = world
                .spawn((
                    Node {
                        width: percent(100),
                        height: px(28.0 * scale),
                        flex_shrink: 0.0,
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(px(8.0 * scale)),
                        border: UiRect::all(px(scale)),
                        overflow: Overflow::clip(),
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    },
                    crate::token_style::background(crate::tokens::Token::Surface),
                    crate::token_style::border(crate::tokens::Token::Accent),
                    Pickable::IGNORE,
                    ChildOf(panel),
                ))
                .id();
            let font = world
                .resource::<crate::theme::Typography>()
                .text(14.0 * scale);
            world.spawn((
                Text::new(compact(&row, 35)),
                font,
                TextLayout::no_wrap(),
                Node {
                    min_width: px(0),
                    flex_shrink: 1.0,
                    overflow: Overflow::clip(),
                    ..default()
                },
                crate::token_style::text(crate::tokens::Token::Ink),
                Pickable::IGNORE,
                ChildOf(rectangle),
            ));
            if row != "No behaviors" {
                let button = world
                    .spawn((
                        bevy::ui_widgets::Button,
                        bevy::a11y::AccessibilityNode::default(),
                        crate::actions::ActionButton::new(
                            entity,
                            crate::actions![Remove(behavior)],
                        ),
                        crate::inspection::InspectionExcluded,
                        Node {
                            display: Display::None,
                            width: px(24.0 * scale),
                            height: px(24.0 * scale),
                            flex_shrink: 0.0,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        ChildOf(rectangle),
                    ))
                    .id();
                if let Some(icon) = crate::icons::image(world, crate::icons::Icon::Delete) {
                    world.spawn((
                        icon,
                        crate::token_style::TextToken(crate::tokens::Token::Ink),
                        Node {
                            width: px(16.0 * scale),
                            height: px(16.0 * scale),
                            ..default()
                        },
                        Pickable::IGNORE,
                        ChildOf(button),
                    ));
                }
                world.entity_mut(rectangle).insert(BehaviorRow {
                    area: entity,
                    full_text: row,
                    button,
                });
                world
                    .get_mut::<bevy::a11y::AccessibilityNode>(button)
                    .unwrap()
                    .set_label("Remove behavior");
            }
        }
        world.entity_mut(entity).insert(Summary { area, panel });
    }
    let floating: Vec<_> = world
        .query::<(Entity, &FloatingSummary)>()
        .iter(world)
        .map(|(panel, summary)| (panel, summary.0))
        .collect();
    for (panel, owner) in floating {
        let root = world.get::<ChildOf>(owner).unwrap().parent();
        let active = world
            .get::<crate::workspace::WorkspaceMember>(owner)
            .is_none_or(|member| {
                world
                    .get::<crate::workspace::Workspaces>(root)
                    .is_none_or(|spaces| spaces.active == member.0)
            });
        let bounds = super::presentation::bounds(world, owner);
        let visible = active
            && (bounds.is_some() || !world.contains_resource::<super::presentation::SceneCamera>());
        let mut node = world.get::<Node>(panel).unwrap().clone();
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(bounds) = bounds {
            let width = match node.width {
                Val::Px(width) => width,
                _ => 300.0,
            };
            node.left = px(bounds.center().x - width * 0.5);
            node.top = px(bounds.max.y + 8.0);
        }
        world.get_mut::<Node>(panel).unwrap().set_if_neq(node);
    }
}

fn editing(world: &World, area: Entity) -> bool {
    world
        .get::<ChildOf>(area)
        .and_then(|parent| world.get::<crate::edit_mode::EditMode>(parent.parent()))
        .is_some_and(|mode| mode.enabled)
}

fn hovered_row(world: &World) -> Option<Entity> {
    let map = world.get_resource::<bevy::picking::hover::HoverMap>()?;
    let hits: Vec<_> = [
        super::input::CONTENT_POINTER,
        bevy::picking::pointer::PointerId::Mouse,
    ]
    .into_iter()
    .filter_map(|pointer| map.get(&pointer))
    .flat_map(|hits| hits.keys().copied())
    .collect();
    hits.iter().find_map(|hit| {
        let mut current = Some(*hit);
        while let Some(entity) = current {
            if let Some(row) = world.get::<BehaviorRow>(entity) {
                return editing(world, row.area).then_some(entity);
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        None
    })
}

pub(super) fn over_behavior(world: &World) -> bool {
    hovered_row(world).is_some()
}

fn preview_position(bounds: Rect, viewport: Vec2, scale: f32) -> (f32, f32, f32) {
    let right = bounds.max.x * scale + 6.0;
    let left_space = (bounds.min.x * scale - 14.0).max(0.0);
    let right_space = (viewport.x * scale - right - 8.0).max(0.0);
    let (left, width) = if right_space >= left_space {
        (right, right_space.min(480.0))
    } else {
        let width = left_space.min(480.0);
        (bounds.min.x * scale - width - 6.0, width)
    };
    (left, bounds.min.y * scale, width)
}

pub(super) fn hover(world: &mut World) {
    let hovered = hovered_row(world);
    let rows: Vec<_> = world
        .query::<(Entity, &BehaviorRow)>()
        .iter(world)
        .map(|(entity, row)| (entity, row.button, editing(world, row.area)))
        .collect();
    for (entity, button, editing) in rows {
        world.entity_mut(entity).insert(if editing {
            Pickable::default()
        } else {
            Pickable::IGNORE
        });
        if let Some(mut node) = world.get_mut::<Node>(button) {
            node.display = if Some(entity) == hovered {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    let previous = world
        .get_resource::<HoverPreview>()
        .and_then(|preview| preview.row);
    if previous == hovered
        && (hovered.is_none()
            || world
                .get_resource::<HoverPreview>()
                .and_then(|preview| preview.popup)
                .is_some_and(|popup| world.get_entity(popup).is_ok()))
    {
        return;
    }
    let old = world
        .get_resource::<HoverPreview>()
        .and_then(|preview| preview.popup);
    if let Some(popup) = old
        && world.get_entity(popup).is_ok()
    {
        world.despawn(popup);
    }
    let popup = hovered.and_then(|row| {
        let summary = world.get::<BehaviorRow>(row)?;
        let root = world.get::<ChildOf>(summary.area)?.parent();
        let text = summary.full_text.clone();
        let bounds = super::presentation::bounds(world, row).or_else(|| {
            let node = world.get::<ComputedNode>(row)?;
            let transform = world.get::<UiGlobalTransform>(row)?;
            Some(Rect::from_center_size(transform.translation, node.size()))
        })?;
        let viewport = world.get::<ComputedNode>(root)?.size();
        let scale = world.get::<ComputedNode>(root)?.inverse_scale_factor();
        let (left, top, width) = preview_position(bounds, viewport, scale);
        Some(
            world
                .spawn((
                    Text::new(text),
                    world.resource::<crate::theme::Typography>().text(14.0),
                    crate::token_style::text(crate::tokens::Token::Ink),
                    crate::token_style::background(crate::tokens::Token::Surface),
                    crate::token_style::border(crate::tokens::Token::Accent),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(left),
                        top: px(top),
                        width: px(width),
                        padding: UiRect::all(px(8)),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    GlobalZIndex(100),
                    Pickable::IGNORE,
                    crate::inspection::InspectionExcluded,
                    ChildOf(root),
                ))
                .id(),
        )
    });
    world.insert_resource(HoverPreview {
        row: hovered,
        popup,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;

    fn editing_app() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        app.world_mut()
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        (app, root)
    }

    #[test]
    fn summaries_reuse_content_when_moving_or_switching_modes() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        let world = app.world_mut();
        let entity = world
            .spawn((
                InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(200.0)),
                ChildOf(root),
            ))
            .id();
        update(world);
        let panel = world.get::<Summary>(entity).unwrap().panel;
        for editing in [true, false] {
            world
                .get_mut::<crate::edit_mode::EditMode>(root)
                .unwrap()
                .enabled = editing;
            world.get_mut::<InfluenceArea>(entity).unwrap().center = [100.0, 100.0];
            update(world);
            assert_eq!(world.get::<Summary>(entity).unwrap().panel, panel);
            assert_ne!(world.get::<Node>(panel).unwrap().display, Display::None);
            assert_eq!(world.get::<Pickable>(panel), Some(&Pickable::IGNORE));
        }
        world
            .get_mut::<InfluenceArea>(entity)
            .unwrap()
            .changes
            .enter
            .quantity = Some("-1".into());
        update(world);
        assert!(world.get_entity(panel).is_err());
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "On entry · quantity -1")
        );
        let long_name = "an assertion whose complete name must stay visible on hover";
        world
            .get_mut::<InfluenceArea>(entity)
            .unwrap()
            .changes
            .enter
            .assert
            .push(long_name.into());
        update(world);
        assert!(
            world
                .query::<&BehaviorRow>()
                .iter(world)
                .any(|row| row.full_text.contains(long_name))
        );
    }

    #[test]
    fn summaries_separate_filters_from_edits_and_truncate_unicode() {
        let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(200.0));
        area.protein = Some(crate::protein_area::Config::records());
        area.strength = 100.0;
        area.filter = Some(crate::protein_area::Config::records());
        area.changes.enter.quantity = Some("-2".into());
        area.changes.enter.assert.push("next".into());
        let rows = lines(&area);
        assert!(rows.iter().any(|(_, row)| row.starts_with("Spawns ·")));
        assert!(rows.iter().any(|(_, row)| row.starts_with("Attracts ·")));
        assert!(
            rows.iter()
                .any(|(_, row)| row == "On entry · quantity -2, +#next")
        );
        area.changes.enter.assign.push("alice".into());
        assert!(
            lines(&area)
                .iter()
                .any(|(_, row)| row.contains("assign alice"))
        );
        area.changes_enabled = false;
        assert!(
            !lines(&area)
                .iter()
                .any(|(_, row)| row.starts_with("On entry"))
        );
        assert_eq!(compact("éééééééééé", 7), "éééé...");
    }

    #[test]
    fn removing_one_behavior_keeps_the_other_behaviors() {
        let (mut app, root) = editing_app();
        let world = app.world_mut();
        let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(200.0));
        area.strength = 20.0;
        area.sorting = Some(crate::area_effects::Sorting::default());
        area.immunity = crate::area_effects::Immunity::Containment;
        area.protein = Some(crate::protein_area::Config::records());
        area.changes.enter.quantity = Some("+1".into());
        area.changes.leave.quantity = Some("-1".into());
        let entity = world
            .spawn((area, crate::workspace::WorkspaceMember(1), ChildOf(root)))
            .id();
        world
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = false;
        Remove(Behavior::Enter).apply(world, entity);
        assert!(
            !world
                .get::<InfluenceArea>(entity)
                .unwrap()
                .changes
                .enter
                .is_empty()
        );
        world
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        Remove(Behavior::Enter).apply(world, entity);
        let area = world.get::<InfluenceArea>(entity).unwrap();
        assert!(area.changes.enter.is_empty());
        assert!(!area.changes.leave.is_empty());
        assert!(area.sorting.is_some());
        assert_eq!(area.strength, 20.0);
        Remove(Behavior::Sorting).apply(world, entity);
        let area = world.get::<InfluenceArea>(entity).unwrap();
        assert!(area.sorting.is_none());
        assert_eq!(area.strength, 20.0);
        assert!(!area.changes.leave.is_empty());
        Remove(Behavior::Immunity).apply(world, entity);
        let area = world.get::<InfluenceArea>(entity).unwrap();
        assert_eq!(area.immunity, crate::area_effects::Immunity::None);
        assert!(area.protein.is_some());
        assert!(!area.changes.leave.is_empty());
    }

    #[test]
    fn kanban_source_behaviors_remain_accessible_below_the_columns() {
        let (mut app, root) = editing_app();
        let world = app.world_mut();
        let owner = crate::kanban::spawn(world, root, 1, DVec2::ZERO).unwrap();
        let source_id = world
            .get::<crate::kanban::Kanban>(owner)
            .unwrap()
            .source
            .clone();
        let source = world
            .query::<(Entity, &InfluenceArea)>()
            .iter(world)
            .find(|(_, area)| area.id == source_id)
            .unwrap()
            .0;
        update(world);
        let panel = world.get::<Summary>(source).unwrap().panel;
        let Val::Px(top) = world.get::<Node>(panel).unwrap().top else {
            panic!("Missing summary position")
        };
        let area = world.get::<InfluenceArea>(source).unwrap();
        assert!(f64::from(top) - area.size[1] * 0.5 > 320.0);
        let row = world.get::<Children>(panel).unwrap()[0];
        let button = world.get::<BehaviorRow>(row).unwrap().button;
        let mut hits = bevy::picking::hover::HoverMap::default();
        hits.insert(
            bevy::picking::pointer::PointerId::Mouse,
            [(
                row,
                bevy::picking::backend::HitData::new(root, 0.0, None, None),
            )]
            .into(),
        );
        world.insert_resource(hits);
        hover(world);
        assert_eq!(world.get::<Node>(button).unwrap().display, Display::Flex);
        Remove(Behavior::Immunity).apply(world, source);
        assert_eq!(
            world.get::<InfluenceArea>(source).unwrap().immunity,
            crate::area_effects::Immunity::None
        );
        assert!(
            world
                .get::<InfluenceArea>(source)
                .unwrap()
                .protein
                .is_some()
        );
        world.despawn(source);
        update(world);
        assert!(world.get_entity(panel).is_err());
    }

    #[test]
    fn hovering_a_behavior_reveals_its_remove_button() {
        use bevy::picking::{backend::HitData, hover::HoverMap};
        let (mut app, root) = editing_app();
        let world = app.world_mut();
        world.insert_resource(HoverMap::default());
        let area = world.spawn(ChildOf(root)).id();
        let button = world.spawn(Node::default()).id();
        let row = world
            .spawn((
                Node::default(),
                BehaviorRow {
                    area,
                    full_text: "A complete behavior description".into(),
                    button,
                },
            ))
            .id();
        world
            .resource_mut::<HoverMap>()
            .entry(super::super::input::CONTENT_POINTER)
            .or_default()
            .insert(row, HitData::new(Entity::PLACEHOLDER, 0.0, None, None));
        hover(world);
        assert_eq!(world.get::<Node>(button).unwrap().display, Display::Flex);
        assert!(over_behavior(world));
        let popup = world.spawn_empty().id();
        world.resource_mut::<HoverPreview>().popup = Some(popup);
        world
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = false;
        hover(world);
        assert!(!over_behavior(world));
        assert_eq!(world.get::<Pickable>(row), Some(&Pickable::IGNORE));
        assert_eq!(world.get::<Node>(button).unwrap().display, Display::None);
        assert!(world.get_entity(popup).is_err());
        world
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        hover(world);
        assert!(over_behavior(world));
        world.resource_mut::<HoverMap>().clear();
        hover(world);
        assert_eq!(world.get::<Node>(button).unwrap().display, Display::None);
    }

    #[test]
    fn previews_stay_beside_the_hovered_row_at_different_scales() {
        for scale in [0.5, 1.0, 2.0] {
            for x in [10.0, 680.0] {
                let bounds = Rect::new(x, 80.0, x + 304.0, 108.0);
                let (left, top, width) = preview_position(bounds, Vec2::new(1000.0, 600.0), scale);
                assert_eq!(top, bounds.min.y * scale);
                assert!(width > 0.0);
                assert!(left >= 8.0);
                assert!(left + width <= 1000.0 * scale - 8.0);
                assert!(left >= bounds.max.x * scale || left + width <= bounds.min.x * scale);
            }
        }
    }

    #[test]
    fn rectangles_fit_small_round_and_concave_areas() {
        let shapes = [
            InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(10.0)),
            InfluenceArea::new(
                AreaShape::Circle,
                DVec2::new(250.0, -70.0),
                DVec2::splat(100.0),
            ),
            InfluenceArea::drawn(&[
                DVec2::ZERO,
                DVec2::new(100.0, 0.0),
                DVec2::new(100.0, 30.0),
                DVec2::new(30.0, 30.0),
                DVec2::new(30.0, 100.0),
                DVec2::new(0.0, 100.0),
            ])
            .unwrap(),
        ];
        for area in shapes {
            let (offset, size, scale) = bounds(&area, 5);
            let center =
                offset + DVec2::from_array(area.center) - DVec2::from_array(area.size) * 0.5;
            assert!(scale > 0.0 && scale < 1.0);
            assert!(inside(&area, center, size.as_dvec2() * 0.499));
        }
    }
}
