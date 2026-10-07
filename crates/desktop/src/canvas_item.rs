use crate::{
    actions::{Action, ActionButton},
    canvas::CanvasView,
    inspection::Inspection,
    sand_placement::{Pinned, PlacementAction},
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec2, prelude::*};

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
#[require(Node)]
pub struct CanvasItem {
    pub position: DVec2,
    pub size: Vec2,
}

pub(crate) fn eligible(world: &World, root: Entity, entity: Entity) -> bool {
    crate::instinct::practice::permits_target(world, entity)
        && world.get::<CanvasItem>(entity).is_some()
        && !crate::inspection::excluded(world, entity)
        && world
            .get::<ChildOf>(entity)
            .is_some_and(|parent| parent.parent() == root)
        && world.get::<Workspaces>(root).is_none_or(|spaces| {
            world
                .get::<WorkspaceMember>(entity)
                .map_or(spaces.entries[0].id, |member| member.0)
                == spaces.active
        })
}

#[derive(Clone, Copy)]
pub struct DeleteItem;

impl Action for DeleteItem {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        vec![crate::inspection::Connection {
            target,
            name: "Canvas Item Clicked Delete".into(),
        }]
    }

    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            return;
        };
        if !eligible(world, root, entity)
            || !world
                .get::<crate::edit_mode::EditMode>(root)
                .is_some_and(|mode| mode.enabled)
        {
            return;
        }
        let targets = crate::canvas_selection::companions(world, root, entity);
        crate::deletion::request(world, root, targets);
    }
}

pub(crate) fn select(world: &mut World, root: Entity, entity: Entity) {
    if eligible(world, root, entity) {
        let targets = crate::canvas_selection::companions(world, root, entity);
        crate::canvas_selection::set_selection(world, root, targets);
        if let Some(mut inspection) = world.get_mut::<Inspection>(root) {
            inspection.selected = Some(entity);
        }
        let area = world.get::<crate::area::InfluenceArea>(entity).is_some();
        if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root) {
            editor.selected = area.then_some(entity);
        }
    }
}

pub(crate) fn area_pickable(world: &World, root: Entity) -> Pickable {
    if world
        .get::<crate::topology::presentation::SpatialRoot>(root)
        .is_some()
    {
        Pickable::default()
    } else {
        Pickable::IGNORE
    }
}

fn synchronize_picking(
    roots: Query<(), With<crate::topology::presentation::SpatialRoot>>,
    mut areas: Query<(&ChildOf, &mut Pickable), With<crate::area::InfluenceArea>>,
) {
    for (parent, mut pickable) in &mut areas {
        pickable.set_if_neq(if roots.contains(parent.parent()) {
            Pickable::default()
        } else {
            Pickable::IGNORE
        });
    }
}

#[derive(Component)]
pub(crate) struct CanvasItemMenu;

#[derive(Resource, Default)]
struct Menu {
    target: Option<Entity>,
    entity: Option<Entity>,
    pinned: bool,
    grouping: (bool, bool),
    layout_linked: bool,
    date_boundary: bool,
    click: Option<Vec2>,
    anchor: Vec2,
}

pub struct CanvasItemPlugin;

impl Plugin for CanvasItemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Menu>()
            .add_systems(
                First,
                synchronize_picking.before(bevy::picking::PickingSystems::Input),
            )
            .add_systems(
                PostUpdate,
                menu.after(crate::actions::ApplyActions)
                    .after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

fn menu(world: &mut World) {
    let mut roots = world.query::<(Entity, &crate::edit_mode::EditMode, &Inspection)>();
    let candidate = roots.iter(world).find_map(|(root, mode, state)| {
        mode.enabled
            .then_some((
                root,
                state
                    .selected
                    .or(if state.hover { state.hovered } else { None }),
            ))
            .and_then(|(root, candidate)| candidate.map(|candidate| (root, candidate)))
    });
    let target = candidate.and_then(|(root, mut candidate)| {
        loop {
            let parent = world.get::<ChildOf>(candidate)?.parent();
            if parent == root {
                break world
                    .get::<CanvasItem>(candidate)
                    .map(|_| (root, candidate));
            }
            candidate = parent;
        }
    });
    let target = target.or_else(|| retained_menu_target(world));
    let target = target.filter(|(root, entity)| eligible(world, *root, *entity));
    let pinned = target.is_some_and(|(root, entity)| {
        let members: Vec<_> = crate::canvas_selection::companions(world, root, entity)
            .into_iter()
            .filter(|member| world.get::<crate::area::InfluenceArea>(*member).is_none())
            .collect();
        !members.is_empty()
            && members
                .iter()
                .all(|member| world.get::<Pinned>(*member).is_some())
    });
    let grouping = target.map_or((false, false), |(root, target)| {
        crate::canvas_selection::options(world, root, target)
    });
    let date_boundary = target.is_some_and(|(root, target)| {
        crate::canvas_selection::group_members(world, root, target)
            .iter()
            .any(|entity| {
                world
                    .get::<crate::scoped_events::EventBoundary>(*entity)
                    .is_some_and(|boundary| {
                        boundary
                            .0
                            .iter()
                            .any(|name| name == crate::calendar::DATE_SELECTED)
                    })
            })
    });
    let current = world.resource::<Menu>();
    let layout_linked = target.is_some_and(|(_, entity)| crate::layout::linked(world, entity));
    if current.target == target.map(|(_, entity)| entity)
        && current.pinned == pinned
        && current.grouping == grouping
        && current.layout_linked == layout_linked
        && current.date_boundary == date_boundary
        && current
            .entity
            .is_none_or(|entity| world.get_entity(entity).is_ok())
    {
        if let (Some((root, target)), Some(panel)) = (target, current.entity) {
            position_menu(world, root, target, panel);
        }
        return;
    }
    let anchor = current.anchor;
    let click = current.click;
    let same_target = current.target == target.map(|(_, entity)| entity);
    if let Some(entity) = world.resource_mut::<Menu>().entity.take() {
        world.despawn(entity);
    }
    *world.resource_mut::<Menu>() = Menu {
        target: target.map(|(_, entity)| entity),
        pinned,
        grouping,
        layout_linked,
        date_boundary,
        entity: None,
        click: if same_target { click } else { None },
        anchor: if same_target {
            anchor
        } else {
            Vec2::splat(0.5)
        },
    };
    let Some((root, target)) = target else { return };
    let area = world.get::<crate::area::InfluenceArea>(target).is_some();
    let deletable = eligible(world, root, target);
    let panel = world
        .spawn((
            CanvasItemMenu,
            crate::inspection::InspectionExcluded,
            crate::sand::Square,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(4)),
                column_gap: px(4),
                row_gap: px(4),
                border: UiRect::all(px(1)),
                ..default()
            },
            GlobalZIndex(23),
            ChildOf(root),
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
        ))
        .id();
    use crate::icons::{Icon, IconButton, IconStyle};
    world.spawn((
        IconButton::new(Icon::Grow, "Layout: size, growth, scrolling and parent"),
        IconStyle {
            size: 20.0,
            padding: 6.0,
            ..default()
        },
        ActionButton::new(
            target,
            crate::actions![crate::layout::panel::LayoutAction::Open],
        ),
        ChildOf(panel),
    ));
    if area {
        world.spawn((
            IconButton::new(Icon::General, "Configure this area and its behaviors"),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(
                root,
                crate::actions![crate::edit_mode::EditAction::Area(
                    crate::area_panel::AreaAction::Select(target)
                )],
            ),
            ChildOf(panel),
        ));
    }
    let mut buttons = Vec::new();
    if !area {
        buttons.extend([
            (
                PlacementAction::Pin,
                Icon::Pin,
                if pinned {
                    "Unpin from screen"
                } else {
                    "Pin to screen"
                },
            ),
            (PlacementAction::Front, Icon::BringHere, "Bring to front"),
            (PlacementAction::Forward, Icon::Forward, "Bring forward"),
            (PlacementAction::Backward, Icon::Backward, "Send backward"),
            (PlacementAction::Back, Icon::Back, "Send to back"),
        ]);
    }
    for (action, icon, name) in buttons {
        if matches!(action, PlacementAction::Pin) && layout_linked && !pinned {
            continue;
        }
        world.spawn((
            IconButton::new(icon, name),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(target, crate::actions![action]),
            ChildOf(panel),
        ));
    }
    if deletable {
        world.spawn((
            IconButton::new(Icon::Delete, "Delete"),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(target, crate::actions![DeleteItem]),
            ChildOf(panel),
        ));
    }
    for (show, action, icon, name) in [
        (
            grouping.1,
            crate::canvas_selection::GroupAction::Detach,
            Icon::Detach,
            "Ungroup this component",
        ),
        (
            grouping.0,
            crate::canvas_selection::GroupAction::Group,
            Icon::Group,
            "Group selected Sands",
        ),
        (
            grouping.1,
            crate::canvas_selection::GroupAction::Ungroup,
            Icon::Ungroup,
            "Ungroup Sands",
        ),
    ] {
        if show {
            world.spawn((
                IconButton::new(icon, name),
                IconStyle {
                    size: 20.0,
                    padding: 6.0,
                    ..default()
                },
                ActionButton::new(target, crate::actions![action]),
                ChildOf(panel),
            ));
        }
    }
    if !area {
        world.spawn((
            IconButton::new(
                if date_boundary {
                    Icon::EventsLocal
                } else {
                    Icon::EventsShared
                },
                if date_boundary {
                    "Date events stay in this group. Click to let them leave."
                } else {
                    "Date events can leave this group. Click to keep them inside."
                },
            ),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(
                target,
                crate::actions![crate::scoped_events::ToggleDateBoundary],
            ),
            ChildOf(panel),
        ));
    }
    let controls: Vec<_> = world.get::<Children>(panel).unwrap().iter().collect();
    for control in controls {
        world.entity_mut(control).insert((
            crate::icons::InlineTooltip,
            crate::icons::TooltipIcon { source: control },
        ));
    }
    world.resource_mut::<Menu>().entity = Some(panel);
    if !same_target {
        let point = world
            .get::<Inspection>(root)
            .filter(|state| state.selected.is_some())
            .and_then(|state| state.selected_point)
            .or_else(|| {
                world
                    .query::<&Window>()
                    .iter(world)
                    .filter(|window| window.focused)
                    .find_map(Window::cursor_position)
            });
        set_menu_anchor(world, target, point);
    }
    position_menu(world, root, target, panel);
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn menu_position(viewport: Rect, sand: Rect, size: Vec2, point: Vec2) -> Vec2 {
    let margin = Vec2::splat(8.0);
    let min = viewport.min + margin;
    let max = (viewport.max - size - margin).max(min);
    let point = point.clamp(sand.min, sand.max);
    let along = (point - size * 0.5).clamp(min, max);
    let positions = [
        Vec2::new(along.x, sand.min.y - size.y - margin.y),
        Vec2::new(along.x, sand.max.y + margin.y),
        Vec2::new(sand.min.x - size.x - margin.x, along.y),
        Vec2::new(sand.max.x + margin.x, along.y),
    ];
    let score = |position: Vec2| {
        let overflow = (min - position).max(Vec2::ZERO)
            + (position + size + margin - viewport.max).max(Vec2::ZERO);
        let nearest = point.clamp(position, position + size);
        (overflow.element_sum(), nearest.distance_squared(point))
    };
    positions
        .into_iter()
        .min_by(|a, b| {
            let a = score(*a);
            let b = score(*b);
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        })
        .unwrap()
        - viewport.min
}

fn set_menu_anchor(world: &mut World, target: Entity, point: Option<Vec2>) {
    let Some(root) = world.get::<ChildOf>(target).map(ChildOf::parent) else {
        return;
    };
    let Some(sand) = menu_bounds(world, root, target) else {
        return;
    };
    let scale = world.get_resource::<UiScale>().map_or(1.0, |scale| scale.0);
    let anchor = point.map_or(Vec2::splat(0.5), |point| {
        ((point / scale - sand.min) / sand.size()).clamp(Vec2::ZERO, Vec2::ONE)
    });
    let mut menu = world.resource_mut::<Menu>();
    menu.anchor = anchor;
    menu.click = point;
}

fn menu_bounds(world: &World, root: Entity, target: Entity) -> Option<Rect> {
    crate::canvas_selection::group_members(world, root, target)
        .into_iter()
        .filter_map(|member| {
            crate::inspection::bounds(world, member)
                .or_else(|| crate::canvas_selection::screen_bounds(world, root, member))
        })
        .reduce(|a, b| Rect::from_corners(a.min.min(b.min), a.max.max(b.max)))
}

fn retained_menu_target(world: &mut World) -> Option<(Entity, Entity)> {
    let menu = world.resource::<Menu>();
    let target = menu.target?;
    let panel = menu.entity?;
    let root = world.get::<ChildOf>(target)?.parent();
    if !world.get::<crate::edit_mode::EditMode>(root)?.enabled {
        return None;
    }
    let sand = menu_bounds(world, root, target)?;
    let controls = crate::inspection::bounds(world, panel)?;
    let corridor = Rect::from_corners(sand.min.min(controls.min), sand.max.max(controls.max));
    world
        .query::<&Window>()
        .iter(world)
        .filter(|window| window.focused)
        .filter_map(Window::cursor_position)
        .any(|point| !sand.contains(point) && corridor.contains(point))
        .then_some((root, target))
}

fn position_menu(world: &mut World, root: Entity, target: Entity, panel: Entity) {
    let Some(viewport) = crate::inspection::bounds(world, root) else {
        return;
    };
    let Some(sand) = menu_bounds(world, root, target) else {
        return;
    };
    let click = world
        .get::<Inspection>(root)
        .filter(|state| state.selected.is_some())
        .and_then(|state| state.selected_point);
    if click.is_some() && click != world.resource::<Menu>().click {
        set_menu_anchor(world, target, click);
    }
    let size =
        crate::inspection::bounds(world, panel).map_or(Vec2::new(216.0, 48.0), |rect| rect.size());
    let point = sand.min + sand.size() * world.resource::<Menu>().anchor;
    let position = menu_position(viewport, sand, size, point);
    let controls = world.get::<Children>(panel).unwrap();
    let width = controls
        .iter()
        .map(|control| match world.get::<Node>(control).unwrap().width {
            Val::Px(width) => width,
            _ => 34.0,
        })
        .sum::<f32>()
        + 4.0 * controls.len().saturating_sub(1) as f32
        + 10.0;
    let width = px(width.min((viewport.width() - 16.0).max(44.0)));
    let mut node = world.get_mut::<Node>(panel).unwrap();
    if node.left != px(position.x) || node.top != px(position.y) || node.width != width {
        node.left = px(position.x);
        node.top = px(position.y);
        node.width = width;
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    } else if crate::inspection::bounds(world, panel).is_some() {
        world
            .get_mut::<Visibility>(panel)
            .unwrap()
            .set_if_neq(Visibility::Inherited);
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn controls_stay_outside_the_sand_near_the_clicked_edge() {
        let viewport = Rect::from_corners(Vec2::new(20.0, 30.0), Vec2::new(1220.0, 830.0));
        let sand = Rect::from_corners(Vec2::new(400.0, 250.0), Vec2::new(800.0, 600.0));
        let size = Vec2::new(216.0, 48.0);
        for (point, expected) in [
            (Vec2::new(520.0, 255.0), Vec2::new(412.0, 194.0)),
            (Vec2::new(680.0, 595.0), Vec2::new(572.0, 608.0)),
            (Vec2::new(405.0, 430.0), Vec2::new(176.0, 406.0)),
            (Vec2::new(795.0, 430.0), Vec2::new(808.0, 406.0)),
        ] {
            assert_eq!(
                menu_position(viewport, sand, size, point) + viewport.min,
                expected
            );
        }
        for sand in [
            Rect::from_corners(Vec2::new(30.0, 40.0), Vec2::new(1190.0, 140.0)),
            Rect::from_corners(Vec2::new(30.0, 740.0), Vec2::new(1190.0, 820.0)),
            Rect::from_corners(Vec2::new(30.0, 40.0), Vec2::new(500.0, 820.0)),
            Rect::from_corners(Vec2::new(720.0, 40.0), Vec2::new(1210.0, 820.0)),
        ] {
            for point in [sand.min, sand.center(), sand.max] {
                let position = menu_position(viewport, sand, size, point) + viewport.min;
                assert!(position.cmpge(viewport.min + Vec2::splat(8.0)).all());
                assert!(
                    (position + size)
                        .cmple(viewport.max - Vec2::splat(8.0))
                        .all()
                );
                assert!(
                    position.x + size.x <= sand.min.x - 8.0
                        || position.x >= sand.max.x + 8.0
                        || position.y + size.y <= sand.min.y - 8.0
                        || position.y >= sand.max.y + 8.0
                );
            }
        }
        let position = menu_position(viewport, viewport, size, viewport.center()) + viewport.min;
        assert!(position.y + size.y <= viewport.min.y - 8.0);
    }

    #[cfg_attr(test, test)]
    fn menu_uses_the_whole_group_and_keeps_the_click_anchor_when_it_moves() {
        let mut world = World::new();
        world.init_resource::<Menu>();
        world.insert_resource(UiScale(2.0));
        let root = world.spawn(CanvasView::default()).id();
        let mut members = Vec::new();
        for (group, size, position) in [
            (1, Vec2::new(300.0, 80.0), Vec2::new(400.0, 150.0)),
            (1, Vec2::new(300.0, 400.0), Vec2::new(400.0, 400.0)),
            (2, Vec2::splat(100.0), Vec2::new(900.0, 900.0)),
        ] {
            members.push(
                world
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size,
                        },
                        crate::canvas_selection::SandGroup([group; 16]),
                        ComputedNode { size, ..default() },
                        UiGlobalTransform::from(bevy::math::Affine2::from_translation(position)),
                        ChildOf(root),
                    ))
                    .id(),
            );
        }
        let bounds = menu_bounds(&world, root, members[0]).unwrap();
        assert_eq!(bounds.min, Vec2::new(250.0, 110.0));
        assert_eq!(bounds.max, Vec2::new(550.0, 600.0));
        set_menu_anchor(&mut world, members[0], Some(Vec2::new(1040.0, 240.0)));
        let anchor = world.resource::<Menu>().anchor;
        let point = bounds.min + bounds.size() * anchor;
        assert!(point.distance(Vec2::new(520.0, 120.0)) < 0.01);
        for member in &members[..2] {
            let position = world.get::<UiGlobalTransform>(*member).unwrap().translation;
            world.entity_mut(*member).insert(UiGlobalTransform::from(
                bevy::math::Affine2::from_translation(position + Vec2::splat(50.0)),
            ));
        }
        let moved = menu_bounds(&world, root, members[0]).unwrap();
        assert!((moved.min + moved.size() * anchor).distance(point + Vec2::splat(50.0)) < 0.01);
    }

    #[cfg_attr(test, test)]
    fn grouped_sand_controls_have_distinct_icons_and_direct_tooltips() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                crate::icons::IconPlugin,
                CanvasItemPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        let mut sands = Vec::new();
        for group in [1, 1, 2] {
            sands.push(
                app.world_mut()
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size: Vec2::splat(100.0),
                        },
                        crate::canvas_selection::SandGroup([group; 16]),
                        ChildOf(root),
                    ))
                    .id(),
            );
        }
        app.world_mut()
            .entity_mut(root)
            .insert(crate::canvas_selection::SandSelection(sands.clone()));
        app.world_mut()
            .get_mut::<Inspection>(root)
            .unwrap()
            .selected = Some(sands[0]);
        for enabled in [false, true] {
            app.world_mut()
                .resource_mut::<crate::icons::TooltipSettings>()
                .enabled = enabled;
            for _ in 0..3 {
                app.update();
            }
            let panel = app.world().resource::<Menu>().entity.unwrap();
            assert_eq!(app.world().get::<ChildOf>(panel).unwrap().parent(), root);
            let controls = app.world().get::<Children>(panel).unwrap();
            let mut icons = Vec::new();
            for control in controls.iter() {
                let button = app
                    .world()
                    .get::<crate::icons::IconButton>(control)
                    .unwrap();
                assert!(
                    !icons.contains(&button.icon),
                    "duplicate icon: {}",
                    button.label
                );
                icons.push(button.icon);
                assert_eq!(
                    app.world().get::<crate::icons::Tooltip>(control).unwrap().0,
                    button.label
                );
                assert!(
                    app.world()
                        .get::<crate::icons::InlineTooltip>(control)
                        .is_some()
                );
                assert_eq!(
                    app.world()
                        .get::<crate::icons::TooltipIcon>(control)
                        .unwrap()
                        .source,
                    control
                );
                assert!(
                    app.world()
                        .get::<Children>(control)
                        .unwrap()
                        .iter()
                        .all(|child| {
                            app.world()
                                .get::<crate::icons::TooltipIcon>(child)
                                .is_none()
                        })
                );
            }
            assert_eq!(icons.len(), 11);
        }
    }

    #[cfg_attr(test, test)]
    fn spatial_area_hits_reach_selection_after_hover_generation() {
        use bevy::picking::{
            backend::{HitData, PointerHits},
            hover::{HoverMap, PreviousHoverMap, generate_hovermap},
            pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
        };
        let (mut app, root, _, area) = deletion_fixture();
        app.init_resource::<HoverMap>()
            .init_resource::<PreviousHoverMap>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_message::<PointerHits>()
            .add_message::<PointerInput>()
            .add_message::<bevy::window::WindowEvent>()
            .add_plugins(crate::canvas_selection::CanvasSelectionPlugin)
            .add_systems(
                PreUpdate,
                generate_hovermap.before(crate::canvas_selection::SelectInput),
            );
        app.world_mut().spawn(PointerId::Mouse);
        let camera = app.world_mut().spawn_empty().id();
        for spatial in [false, true, false] {
            crate::canvas_selection::set_selection(app.world_mut(), root, vec![]);
            if spatial {
                app.world_mut()
                    .entity_mut(root)
                    .insert(crate::topology::presentation::SpatialRoot);
            } else {
                app.world_mut()
                    .entity_mut(root)
                    .remove::<crate::topology::presentation::SpatialRoot>();
            }
            app.world_mut().write_message(PointerHits::new(
                PointerId::Mouse,
                vec![(area, HitData::new(camera, 0.0, Some(Vec3::ZERO), None))],
                -1.0,
            ));
            app.world_mut().write_message(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target: bevy::camera::NormalizedRenderTarget::None {
                        width: 800,
                        height: 600,
                    },
                    position: Vec2::new(400.0, 300.0),
                },
                PointerAction::Press(PointerButton::Primary),
            ));
            app.update();
            let hovered = app
                .world()
                .resource::<HoverMap>()
                .get(&PointerId::Mouse)
                .unwrap();
            assert_eq!(hovered.contains_key(&area), spatial);
            assert_eq!(
                crate::canvas_selection::selected(app.world(), root),
                if spatial { vec![area] } else { vec![] }
            );
        }
        select(app.world_mut(), root, area);
        crate::deletion::DeleteSelected.apply(app.world_mut(), root);
        crate::deletion::Decision(true).apply(app.world_mut(), root);
        assert!(app.world().get_entity(area).is_err());
    }

    fn deletion_fixture() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                CanvasItemPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        let sand = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let area = crate::area::spawn_area(
            app.world_mut(),
            root,
            1,
            crate::area::InfluenceArea::new(
                crate::area::AreaShape::Circle,
                DVec2::ZERO,
                DVec2::splat(200.0),
            ),
        )
        .unwrap();
        (app, root, sand, area)
    }

    #[cfg_attr(test, test)]
    fn sand_and_area_controls_delete_through_the_same_action() {
        for member in [false, true] {
            let (mut app, root, sand, area) = deletion_fixture();
            let target = if member { area } else { sand };
            let kept = if member { sand } else { area };
            select(app.world_mut(), root, target);
            menu(app.world_mut());
            let panel = app.world().resource::<Menu>().entity.unwrap();
            let control = app
                .world()
                .get::<Children>(panel)
                .unwrap()
                .iter()
                .find(|control| {
                    app.world()
                        .get::<crate::icons::IconButton>(*control)
                        .is_some_and(|button| button.icon == crate::icons::Icon::Delete)
                })
                .unwrap();
            let action = app.world().get::<ActionButton>(control).unwrap().clone();
            action.actions.run(app.world_mut(), action.target);
            assert!(app.world().get_entity(target).is_ok());
            crate::deletion::Decision(true).apply(app.world_mut(), root);
            assert!(app.world().get_entity(target).is_err());
            assert!(app.world().get_entity(kept).is_ok());
        }
    }

    #[cfg_attr(test, test)]
    fn mixed_selection_shares_deletion_but_keeps_screen_pinning_on_sands() {
        let (mut app, root, sand, area) = deletion_fixture();
        app.world_mut().entity_mut(root).insert((
            CanvasView::default(),
            ComputedNode {
                size: Vec2::splat(1000.0),
                ..default()
            },
            UiGlobalTransform::default(),
        ));
        crate::canvas_selection::set_selection(app.world_mut(), root, vec![sand, area]);
        PlacementAction::Pin.apply(app.world_mut(), sand);
        assert!(app.world().get::<Pinned>(sand).is_some());
        assert!(app.world().get::<Pinned>(area).is_none());
        DeleteItem.apply(app.world_mut(), area);
        crate::deletion::Decision(false).apply(app.world_mut(), root);
        assert!(app.world().get_entity(sand).is_ok());
        assert!(app.world().get_entity(area).is_ok());
        crate::deletion::DeleteSelected.apply(app.world_mut(), root);
        crate::deletion::Decision(true).apply(app.world_mut(), root);
        assert!(app.world().get_entity(sand).is_err());
        assert!(app.world().get_entity(area).is_err());
    }

    #[cfg_attr(test, test)]
    fn area_deletion_respects_edit_mode_and_workspace_ownership() {
        let (mut app, root, _, area) = deletion_fixture();
        app.world_mut()
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = false;
        DeleteItem.apply(app.world_mut(), area);
        crate::deletion::Decision(true).apply(app.world_mut(), root);
        assert!(app.world().get_entity(area).is_ok());
        app.world_mut()
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        app.world_mut().get_mut::<WorkspaceMember>(area).unwrap().0 = 2;
        DeleteItem.apply(app.world_mut(), area);
        crate::deletion::Decision(true).apply(app.world_mut(), root);
        assert!(app.world().get_entity(area).is_ok());
        app.world_mut().get_mut::<WorkspaceMember>(area).unwrap().0 = 1;
        DeleteItem.apply(app.world_mut(), area);
        app.world_mut().get_mut::<WorkspaceMember>(area).unwrap().0 = 2;
        crate::deletion::Decision(true).apply(app.world_mut(), root);
        assert!(app.world().get_entity(area).is_ok());
    }

    crate::laboratory_cases! {
        controls_stay_outside_the_sand_near_the_clicked_edge,
        menu_uses_the_whole_group_and_keeps_the_click_anchor_when_it_moves,
        grouped_sand_controls_have_distinct_icons_and_direct_tooltips,
        sand_and_area_controls_delete_through_the_same_action,
        mixed_selection_shares_deletion_but_keeps_screen_pinning_on_sands,
        area_deletion_respects_edit_mode_and_workspace_ownership,
        spatial_area_hits_reach_selection_after_hover_generation,
    }
}
