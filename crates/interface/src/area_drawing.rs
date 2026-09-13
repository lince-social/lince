use crate::{
    area::{AreaForces, InfluenceArea, ReachMode, ReachShape, ShapeKind},
    area_panel::AreaEditor,
    canvas::{CanvasItem, CanvasView},
    edit_mode::EditMode,
};
use bevy::{math::DVec2, prelude::*};

#[derive(Clone, PartialEq)]
enum Mark {
    Line(Vec2, Vec2, bool),
    Reach(Vec2, Vec2),
    Force(Vec2, Vec2),
    Label(Vec2, String),
}

#[derive(Component)]
struct Drawing {
    layer: Entity,
    marks: Vec<Mark>,
    entities: Vec<Entity>,
}

#[derive(Component)]
struct ReachDrawing {
    shape: crate::area::AreaShape,
    area_center: [f64; 2],
    area_size: [f64; 2],
    reach: crate::area::Reach,
    center: DVec2,
    zoom: f64,
    size: Vec2,
    marks: Vec<Mark>,
}

pub struct AreaDrawingPlugin;

#[derive(Resource, Default)]
struct Redraw(bool);

impl Plugin for AreaDrawingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Redraw>().add_systems(
            PostUpdate,
            (dirty, draw).chain().before(bevy::ui::UiSystems::Prepare),
        );
    }
}

fn dirty(
    roots: Query<
        (),
        (
            With<CanvasView>,
            Or<(
                Changed<CanvasView>,
                Changed<crate::topology::view::View>,
                Changed<ComputedNode>,
                Changed<EditMode>,
                Changed<AreaEditor>,
                Changed<crate::workspace::Workspaces>,
            )>,
        ),
    >,
    items: Query<
        (),
        Or<(
            Changed<InfluenceArea>,
            Changed<AreaForces>,
            Changed<CanvasItem>,
            Changed<crate::topology::Spatial>,
            Changed<crate::workspace::WorkspaceMember>,
            Changed<ChildOf>,
        )>,
    >,
    mut removed_areas: RemovedComponents<InfluenceArea>,
    mut removed_forces: RemovedComponents<AreaForces>,
    mut removed_items: RemovedComponents<CanvasItem>,
    mut removed_members: RemovedComponents<crate::workspace::WorkspaceMember>,
    mut redraw: ResMut<Redraw>,
) {
    let removed = removed_areas.read().count()
        + removed_forces.read().count()
        + removed_items.read().count()
        + removed_members.read().count();
    redraw.0 = removed > 0 || !roots.is_empty() || !items.is_empty();
}

fn screen(view: &CanvasView, size: Vec2, point: DVec2) -> Vec2 {
    ((point - view.center) * view.zoom + size.as_dvec2() * 0.5).as_vec2()
}

fn clip(start: Vec2, end: Vec2, size: Vec2) -> Option<(Vec2, Vec2)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let delta = end - start;
    let mut near: f32 = 0.0;
    let mut far: f32 = 1.0;
    for (p, q) in [
        (-delta.x, start.x),
        (delta.x, size.x - start.x),
        (-delta.y, start.y),
        (delta.y, size.y - start.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            near = near.max(q / p);
        } else {
            far = far.min(q / p);
        }
    }
    (near <= far).then_some((start + delta * near, start + delta * far))
}

fn outline(marks: &mut Vec<Mark>, view: &CanvasView, size: Vec2, points: &[DVec2], selected: bool) {
    for pair in points.windows(2) {
        if let Some((a, b)) = clip(
            screen(view, size, pair[0]),
            screen(view, size, pair[1]),
            size,
        ) {
            marks.push(Mark::Line(a, b, selected));
        }
    }
}

fn contour(
    marks: &mut Vec<Mark>,
    area: &InfluenceArea,
    view: &CanvasView,
    size: Vec2,
    bounds: Rect,
    depth: u8,
) {
    let sample = |point: Vec2| {
        let world = (point - size * 0.5).as_dvec2() / view.zoom + view.center;
        (area.signed_distance(world) - area.reach.radius) * view.zoom
    };
    let middle = bounds.center();
    if sample(middle).abs() > f64::from(bounds.half_size().length()) {
        return;
    }
    if bounds.size().max_element() > 3.0 && depth < 14 {
        for corner in [
            bounds.min,
            Vec2::new(bounds.max.x, bounds.min.y),
            bounds.max,
            Vec2::new(bounds.min.x, bounds.max.y),
        ] {
            contour(
                marks,
                area,
                view,
                size,
                Rect::from_corners(corner, middle),
                depth + 1,
            );
        }
        return;
    }
    let corners = [
        bounds.min,
        Vec2::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Vec2::new(bounds.min.x, bounds.max.y),
        bounds.min,
    ];
    let mut crossings = Vec::with_capacity(4);
    for edge in corners.windows(2) {
        let a = sample(edge[0]);
        let b = sample(edge[1]);
        if (a <= 0.0) != (b <= 0.0) {
            crossings.push(edge[0].lerp(edge[1], (a / (a - b)) as f32));
        }
    }
    for pair in crossings.chunks_exact(2) {
        marks.push(Mark::Reach(pair[0], pair[1]));
    }
}

fn reach(
    world: &mut World,
    root: Entity,
    area: &InfluenceArea,
    view: &CanvasView,
    size: Vec2,
) -> Vec<Mark> {
    if let Some(cache) = world.get::<ReachDrawing>(root)
        && cache.shape == area.shape
        && cache.area_center == area.center
        && cache.area_size == area.size
        && cache.reach == area.reach
        && cache.center == view.center
        && cache.zoom == view.zoom
        && cache.size == size
    {
        return cache.marks.clone();
    }
    let mut marks = Vec::new();
    if area.reach.mode == ReachMode::Unlimited {
        marks.push(Mark::Label(
            Vec2::new(12.0, size.y - 28.0),
            "Reach · Unlimited".into(),
        ));
    } else if area.reach.shape == ReachShape::Square {
        let half = area.size[0].max(area.size[1]) * 0.5 + area.reach.radius;
        let center = DVec2::from_array(area.center);
        let points = [
            DVec2::new(-half, -half),
            DVec2::new(half, -half),
            DVec2::new(half, half),
            DVec2::new(-half, half),
            DVec2::new(-half, -half),
        ];
        for pair in points.windows(2) {
            if let Some((a, b)) = clip(
                screen(view, size, center + pair[0]),
                screen(view, size, center + pair[1]),
                size,
            ) {
                marks.push(Mark::Reach(a, b));
            }
        }
    } else if area.reach.radius > 0.0 {
        contour(
            &mut marks,
            area,
            view,
            size,
            Rect::from_corners(Vec2::ZERO, size),
            0,
        );
    }
    world.entity_mut(root).insert(ReachDrawing {
        shape: area.shape.clone(),
        area_center: area.center,
        area_size: area.size,
        reach: area.reach,
        center: view.center,
        zoom: view.zoom,
        size,
        marks: marks.clone(),
    });
    marks
}

fn draw(world: &mut World) {
    if !world.resource::<Redraw>().0 {
        return;
    }
    let roots: Vec<_> = world
        .query::<(Entity, &EditMode, &CanvasView, &ComputedNode)>()
        .iter(world)
        .map(|(entity, mode, view, computed)| {
            (
                entity,
                mode.enabled,
                *view,
                computed.size() * computed.inverse_scale_factor(),
            )
        })
        .collect();
    for (root, enabled, view, size) in roots {
        let enabled = enabled
            && !world
                .get::<crate::topology::view::View>(root)
                .is_some_and(|v| v.spatial);
        if !size.is_finite() || size.min_element() <= 0.0 {
            continue;
        }
        let mut marks = Vec::new();
        if enabled && view.center.is_finite() && view.zoom.is_finite() && view.zoom > 0.0 {
            let selected = world
                .get::<AreaEditor>(root)
                .and_then(|editor| editor.selected);
            let mut areas: Vec<_> = world
                .query::<(Entity, &InfluenceArea)>()
                .iter(world)
                .filter(|(entity, area)| {
                    crate::area_panel::owns(world, root, *entity) && area.validate()
                })
                .map(|(entity, area)| (entity, area.clone()))
                .collect();
            areas.sort_by(|a, b| a.1.id.cmp(&b.1.id));
            let mut reach_marks = Vec::new();
            for (entity, area) in areas {
                let placement = crate::topology::spatial(world, entity);
                let center = DVec2::from_array(area.center);
                let project = |point: DVec2| {
                    let local = point - center;
                    let point = placement.position(center)
                        + placement.rotation() * bevy::math::DVec3::new(local.x, 0.0, local.y);
                    DVec2::new(point.x, point.z)
                };
                if selected == Some(entity) {
                    reach_marks = reach(world, root, &area, &view, size);
                    for mark in &mut reach_marks {
                        if let Mark::Reach(a, b) = mark {
                            let project_screen = |point: Vec2| {
                                screen(
                                    &view,
                                    size,
                                    project(
                                        view.center + (point - size * 0.5).as_dvec2() / view.zoom,
                                    ),
                                )
                            };
                            *a = project_screen(*a);
                            *b = project_screen(*b);
                        }
                    }
                }
                let points: Vec<_> = area.outline().into_iter().map(project).collect();
                outline(&mut marks, &view, size, &points, selected == Some(entity));
                let position = screen(
                    &view,
                    size,
                    project(center - DVec2::from_array(area.size) * 0.5),
                );
                if Rect::from_corners(Vec2::ZERO, size).contains(position) {
                    marks.push(Mark::Label(
                        position,
                        format!(
                            "{} · {}",
                            area.name,
                            if area.sorting.is_some() {
                                "Sorting"
                            } else if area.immunity != crate::area_effects::Immunity::None {
                                "Immunity"
                            } else if area.scale != 1.0 {
                                "Size"
                            } else if area.strength == 0.0
                                || (area.rules.is_empty() && area.filter.is_none())
                            {
                                "No force"
                            } else if area.direction == crate::area::Direction::Attract {
                                "Attract"
                            } else {
                                "Repel"
                            }
                        ),
                    ));
                }
            }
            if let Some(editor) = world.get::<AreaEditor>(root) {
                if editor.tool == Some(ShapeKind::Drawn) {
                    let mut points = editor.points.clone();
                    if let Some(cursor) = editor.cursor {
                        points.push(cursor);
                    }
                    if let Some(first) = points.first().copied() {
                        points.push(first);
                    }
                    outline(&mut marks, &view, size, &points, true);
                    if let Some(first) = editor.points.first() {
                        let position = screen(&view, size, *first);
                        if Rect::from_corners(Vec2::ZERO, size).contains(position) {
                            marks.push(Mark::Label(position, "Start".into()));
                        }
                    }
                } else if let (Some(kind), Some(start), Some(end)) =
                    (editor.tool, editor.points.first(), editor.cursor)
                    && let Some(area) = crate::area_input::regular(kind, *start, end)
                {
                    outline(&mut marks, &view, size, &area.outline(), true);
                }
            }
            if let Some(selected) = selected {
                for (item, forces) in world.query::<(&CanvasItem, &AreaForces)>().iter(world) {
                    if let Some(force) = forces.0.iter().find(|force| force.area == selected) {
                        let start = screen(&view, size, item.position);
                        let direction = force.force.normalize_or_zero().as_vec2();
                        let end = start + direction * 40.0;
                        if let Some((a, b)) = clip(start, end, size) {
                            marks.push(Mark::Force(a, b));
                            for sign in [-1.0, 1.0] {
                                let tip = end - direction * 9.0 + direction.perp() * 5.0 * sign;
                                if let Some((a, b)) = clip(end, tip, size) {
                                    marks.push(Mark::Force(a, b));
                                }
                            }
                        }
                    }
                }
            }
            marks.extend(reach_marks);
        }
        if world
            .get::<Drawing>(root)
            .is_some_and(|drawing| drawing.marks == marks)
        {
            continue;
        }
        let layer = if let Some(drawing) = world.get::<Drawing>(root) {
            let layer = drawing.layer;
            layer
        } else {
            world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Pickable::IGNORE,
                    ZIndex(-1),
                    ChildOf(root),
                ))
                .id()
        };
        let mut entities = world
            .get::<Drawing>(root)
            .map(|drawing| drawing.entities.clone())
            .unwrap_or_default();
        let accent = crate::token_style::resolve(world, root, crate::tokens::Token::Accent)
            .0
            .color();
        let surface = crate::token_style::resolve(world, root, crate::tokens::Token::Surface)
            .0
            .color();
        for (index, mark) in marks.iter().enumerate() {
            let previous = world
                .get::<Drawing>(root)
                .and_then(|drawing| drawing.marks.get(index));
            if previous == Some(mark) {
                continue;
            }
            let same_kind = previous.is_some_and(|previous| {
                matches!((previous, mark), (Mark::Label(..), Mark::Label(..)))
                    || matches!(
                        (previous, mark),
                        (
                            Mark::Line(..) | Mark::Reach(..) | Mark::Force(..),
                            Mark::Line(..) | Mark::Reach(..) | Mark::Force(..)
                        )
                    )
            });
            let entity = if same_kind {
                entities[index]
            } else {
                let entity = world.spawn((Pickable::IGNORE, ChildOf(layer))).id();
                if index < entities.len() {
                    world.despawn(entities[index]);
                    entities[index] = entity;
                } else {
                    entities.push(entity);
                }
                entity
            };
            if !matches!(mark, Mark::Force(..)) {
                world.entity_mut(entity).remove::<GlobalZIndex>();
            }

            match mark {
                Mark::Line(a, b, _) | Mark::Reach(a, b) | Mark::Force(a, b) => {
                    let delta = *b - *a;
                    let center = (*a + *b) * 0.5;
                    let width = if matches!(mark, Mark::Line(_, _, true) | Mark::Force(..)) {
                        2.0
                    } else {
                        1.0
                    };
                    let mut entity = world.entity_mut(entity);
                    entity.insert((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(center.x - delta.length() * 0.5),
                            top: px(center.y - width * 0.5),
                            width: px(delta.length()),
                            height: px(width),
                            ..default()
                        },
                        UiTransform {
                            rotation: Rot2::radians(delta.y.atan2(delta.x)),
                            ..default()
                        },
                        (
                            BackgroundColor(accent),
                            crate::token_style::BackgroundToken(crate::tokens::Token::Accent),
                        ),
                        Pickable::IGNORE,
                        ChildOf(layer),
                    ));
                    if matches!(mark, Mark::Force(..)) {
                        entity.insert(GlobalZIndex(2));
                    }
                }
                Mark::Label(position, value) => {
                    let font = world.resource::<crate::theme::Typography>().text(12.0);
                    world.entity_mut(entity).insert((
                        Text::new(value),
                        UiTransform::default(),
                        font,
                        (
                            TextColor(accent),
                            crate::token_style::TextToken(crate::tokens::Token::Accent),
                        ),
                        (
                            BackgroundColor(surface),
                            crate::token_style::BackgroundToken(crate::tokens::Token::Surface),
                        ),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(position.x),
                            top: px(position.y),
                            max_width: px(240),
                            ..default()
                        },
                        Pickable::IGNORE,
                        ChildOf(layer),
                    ));
                }
            }
        }
        for entity in entities.drain(marks.len()..) {
            world.despawn(entity);
        }
        world.entity_mut(root).insert(Drawing {
            layer,
            marks,
            entities,
        });
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use crate::actions::Action;

    #[cfg_attr(test, test)]
    fn reach_contours_follow_the_expanded_shape_and_reuse_unchanged_geometry() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Circle,
            DVec2::ZERO,
            DVec2::splat(200.0),
        );
        area.reach.radius = 50.0;
        let view = CanvasView::default();
        let size = Vec2::splat(600.0);
        let marks = reach(&mut world, root, &area, &view, size);
        assert!(!marks.is_empty());
        for mark in &marks {
            let Mark::Reach(a, b) = mark else {
                panic!("Expected a reach boundary")
            };
            for point in [a, b] {
                assert!(((point.as_dvec2() - size.as_dvec2() * 0.5).length() - 150.0).abs() < 0.1);
            }
        }
        world.clear_trackers();
        assert!(reach(&mut world, root, &area, &view, size) == marks);
        area.target = crate::area::AttractionTarget::Point([200.0, 300.0]);
        area.depth = 42.0;
        assert!(reach(&mut world, root, &area, &view, size) == marks);
        assert!(
            !world
                .entity(root)
                .get_ref::<ReachDrawing>()
                .unwrap()
                .is_changed()
        );
        area.reach.mode = ReachMode::Unlimited;
        let unlimited = reach(&mut world, root, &area, &view, size);
        assert!(matches!(unlimited.as_slice(), [Mark::Label(_, _)]));
        area.reach.mode = ReachMode::Limited;
        area.reach.shape = ReachShape::Square;
        assert_eq!(reach(&mut world, root, &area, &view, size).len(), 4);
    }

    #[cfg_attr(test, test)]
    fn camera_motion_retains_lines_and_shape_changes_do_not_leave_text_on_them() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                AreaDrawingPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        app.world_mut().get_mut::<ComputedNode>(root).unwrap().size = Vec2::splat(1000.0);
        let area = crate::area::spawn_area(
            app.world_mut(),
            root,
            1,
            InfluenceArea::new(
                crate::area::AreaShape::Square,
                DVec2::ZERO,
                DVec2::splat(200.0),
            ),
        )
        .unwrap();
        app.update();
        let original = app.world().get::<Drawing>(root).unwrap().entities.clone();
        assert_eq!(original.len(), 5);
        for _ in 0..12 {
            app.world_mut().get_mut::<CanvasView>(root).unwrap().center += DVec2::splat(0.25);
            app.update();
            assert_eq!(app.world().get::<Drawing>(root).unwrap().entities, original);
        }
        app.world_mut().entity_mut(root).insert(AreaEditor {
            selected: Some(area),
            ..default()
        });
        app.world_mut()
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .reach
            .radius = 80.0;
        for _ in 0..12 {
            app.world_mut().get_mut::<CanvasView>(root).unwrap().center += DVec2::splat(0.25);
            app.update();
            assert_eq!(
                &app.world().get::<Drawing>(root).unwrap().entities[..5],
                original.as_slice()
            );
        }
        app.world_mut()
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .shape = crate::area::AreaShape::Circle;
        app.update();
        assert!(app.world().get_entity(original[4]).is_err());
        let drawing = app.world().get::<Drawing>(root).unwrap();
        for (mark, entity) in drawing.marks.iter().zip(&drawing.entities) {
            assert_eq!(
                app.world().get::<Text>(*entity).is_some(),
                matches!(mark, Mark::Label(..))
            );
        }
    }

    crate::laboratory_cases! {
        reach_contours_follow_the_expanded_shape_and_reuse_unchanged_geometry,
        camera_motion_retains_lines_and_shape_changes_do_not_leave_text_on_them,
    }
}
