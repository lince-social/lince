use crate::actions::Action;
use crate::{
    area::{AreaShape, InfluenceArea, MAX_POINTS, ShapeKind},
    area_panel::{AreaAction, AreaEditor},
    canvas::CanvasView,
    edit_mode::{EditAction, EditMode},
    workspace::Workspaces,
};
use bevy::{
    ecs::message::MessageCursor,
    input_focus::InputFocus,
    math::DVec2,
    picking::{
        hover::{HoverMap, generate_hovermap},
        pointer::{PointerAction, PointerButton, PointerId, PointerInput, PointerPress},
    },
    prelude::*,
    window::WindowEvent,
};

pub struct AreaInputPlugin;

#[derive(Resource, Default)]
struct Gesture(Option<Entity>);

impl Plugin for AreaInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gesture>().add_systems(
            PreUpdate,
            (hit_areas, input)
                .chain()
                .after(generate_hovermap)
                .after(crate::canvas_selection::SelectInput)
                .before(crate::inspection::InspectInput),
        );
    }
}

pub(crate) fn canvas_point(world: &World, root: Entity, screen: Vec2) -> Option<DVec2> {
    let view = world.get::<CanvasView>(root)?;
    let node = world.get::<ComputedNode>(root)?;
    let transform = world.get::<UiGlobalTransform>(root)?;
    let scale = node.inverse_scale_factor();
    let center = transform.translation * scale;
    let size = node.size() * scale;
    if !screen.is_finite()
        || !view.center.is_finite()
        || !view.zoom.is_finite()
        || view.zoom <= 0.0
        || !Rect::from_center_size(center, size).contains(screen)
    {
        return None;
    }
    let point = view.center + (screen - center).as_dvec2() / view.zoom;
    point.is_finite().then_some(point)
}

pub(crate) fn regular(kind: ShapeKind, start: DVec2, end: DVec2) -> Option<InfluenceArea> {
    let delta = end - start;
    let side = delta.abs().max_element();
    let center = start
        + DVec2::new(
            if delta.x < 0.0 { -side } else { side },
            if delta.y < 0.0 { -side } else { side },
        ) * 0.5;
    let shape = match kind {
        ShapeKind::Square => AreaShape::Square,
        ShapeKind::Circle => AreaShape::Circle,
        ShapeKind::Drawn => return None,
    };
    let area = InfluenceArea::new(shape, center, DVec2::splat(side));
    area.validate().then_some(area)
}

fn hit_areas(world: &mut World) {
    let hit = world
        .resource::<HoverMap>()
        .get(&PointerId::Mouse)
        .and_then(|hits| {
            hits.iter()
                .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
                .map(|(entity, hit)| (*entity, hit.clone()))
        });
    let Some((root, hit)) = hit else { return };
    if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled)
        || world
            .get::<AreaEditor>(root)
            .is_some_and(|editor| editor.tool.is_some())
    {
        return;
    }
    let point = world
        .query::<(&PointerId, &bevy::picking::pointer::PointerLocation)>()
        .iter(world)
        .find_map(|(id, pointer)| {
            (*id == PointerId::Mouse)
                .then(|| pointer.location().map(|location| location.position))
                .flatten()
        });
    let Some(screen) = point else { return };
    let Some(point) = canvas_point(world, root, screen) else {
        return;
    };
    let zoom = world.get::<CanvasView>(root).unwrap().zoom;
    let selected = world
        .get::<AreaEditor>(root)
        .and_then(|editor| editor.selected);
    let mut candidates: Vec<_> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter(|(entity, area)| {
            if !crate::area_panel::owns(world, root, *entity) {
                return false;
            }
            let local = ((point - DVec2::from_array(area.center)) * zoom).as_vec2();
            let bounds = Rect::from_center_size(
                Vec2::ZERO,
                DVec2::from_array(area.size).as_vec2() * zoom as f32,
            );
            area.contains(point)
                || selected == Some(*entity)
                    && crate::canvas_resize::Edges::at(local, bounds)
                        .cursor()
                        .is_some()
        })
        .map(|(entity, area)| (entity, area.size[0] * area.size[1], area.id.clone()))
        .collect();
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)));
    if let Some((entity, _, _)) = candidates.first() {
        let mut hover = world.resource_mut::<HoverMap>();
        let hits = hover.get_mut(&PointerId::Mouse).unwrap();
        hits.remove(&root);
        hits.insert(*entity, hit);
    }
}

fn sample(points: &mut Vec<DVec2>, point: DVec2, zoom: f64, final_point: bool) {
    if points
        .last()
        .is_some_and(|last| last.distance(point) * zoom < if final_point { 0.01 } else { 3.0 })
    {
        return;
    }
    if points.len() >= MAX_POINTS - 1 {
        let last = *points.last().unwrap();
        *points = points.iter().step_by(2).copied().collect();
        if points.last() != Some(&last) {
            points.push(last);
        }
    }
    points.push(point);
}

fn input(
    world: &mut World,
    mut cursor: Local<MessageCursor<PointerInput>>,
    mut window_cursor: Local<MessageCursor<WindowEvent>>,
) {
    let interrupted = window_cursor
        .read(world.resource::<Messages<WindowEvent>>())
        .any(|event| {
            matches!(event, WindowEvent::WindowFocused(event) if !event.focused)
                || matches!(event, WindowEvent::CursorLeft(_))
        });
    let roots: Vec<_> = world
        .query::<(Entity, &AreaEditor)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect();
    for root in roots {
        let editor = world.get::<AreaEditor>(root).unwrap();
        let reset = interrupted
            || !world
                .get::<EditMode>(root)
                .is_some_and(|mode| mode.enabled && mode.areas)
            || world
                .get::<Workspaces>(root)
                .is_none_or(|spaces| editor.workspace != spaces.active);
        if reset && editor.tool.is_some() {
            world.get_mut::<AreaEditor>(root).unwrap().cancel();
        }
    }
    if interrupted
        || world.resource::<Gesture>().0.is_some_and(|root| {
            world
                .get::<AreaEditor>(root)
                .is_none_or(|editor| editor.tool.is_none())
        })
    {
        world.resource_mut::<Gesture>().0 = None;
    }
    let mut events = world.remove_resource::<Messages<PointerInput>>().unwrap();
    for event in cursor.read_mut(&mut events) {
        if event.pointer_id != PointerId::Mouse {
            continue;
        }
        let active = world.resource::<Gesture>().0;
        if active.is_none()
            && world
                .resource::<ButtonInput<KeyCode>>()
                .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        {
            continue;
        }
        let hit = world
            .resource::<HoverMap>()
            .get(&PointerId::Mouse)
            .and_then(|hits| {
                hits.iter()
                    .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
                    .map(|(entity, _)| *entity)
            });
        let root = active.or_else(|| {
            hit.and_then(|entity| {
                if world.get::<CanvasView>(entity).is_some() {
                    Some(entity)
                } else if world.get::<InfluenceArea>(entity).is_some() {
                    world.get::<ChildOf>(entity).map(ChildOf::parent)
                } else {
                    None
                }
            })
        });
        let Some(root) = root else { continue };
        if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled) {
            world.resource_mut::<Gesture>().0 = None;
            continue;
        }
        let point = canvas_point(world, root, event.location.position);
        let mut consume = active.is_some();
        match event.action {
            PointerAction::Press(PointerButton::Primary) => {
                let Some(point) = point else { continue };
                let tool = world.get::<AreaEditor>(root).and_then(|editor| editor.tool);
                if let Some(tool) = tool {
                    let zoom = world.get::<CanvasView>(root).unwrap().zoom;
                    let mut editor = world.get_mut::<AreaEditor>(root).unwrap();
                    if tool == ShapeKind::Drawn {
                        if editor.dragging {
                            sample(&mut editor.points, point, zoom, true);
                            if editor.points.len() > 3
                                && editor.points.first() == editor.points.last()
                            {
                                editor.points.pop();
                            }
                            editor.dragging = false;
                            drop(editor);
                            EditAction::Area(AreaAction::Finish).apply(world, root);
                        } else {
                            editor.points = vec![point];
                            editor.cursor = Some(point);
                            editor.dragging = true;
                        }
                    } else {
                        editor.points = vec![point];
                        editor.dragging = true;
                        editor.cursor = Some(point);
                    }
                    consume = true;
                } else {
                    if let Some(entity) =
                        hit.filter(|entity| world.get::<InfluenceArea>(*entity).is_some())
                    {
                        EditAction::Area(AreaAction::Select(entity)).apply(world, root);
                    }
                }
                if consume {
                    world.resource_mut::<Gesture>().0 = Some(root);
                }
            }
            PointerAction::Move { .. } => {
                if let Some(mut editor) = world.get_mut::<AreaEditor>(root)
                    && editor.tool.is_some()
                    && editor.cursor != point
                {
                    editor.cursor = point;
                    if editor.tool == Some(ShapeKind::Drawn)
                        && editor.dragging
                        && let Some(point) = point
                    {
                        let zoom = world.get::<CanvasView>(root).unwrap().zoom;
                        let mut editor = world.get_mut::<AreaEditor>(root).unwrap();
                        sample(&mut editor.points, point, zoom, false);
                    }
                }
            }
            PointerAction::Release(PointerButton::Primary) => {
                let drawn = world.get::<AreaEditor>(root).and_then(|editor| {
                    if editor.dragging && editor.tool != Some(ShapeKind::Drawn) {
                        Some((editor.tool?, *editor.points.first()?))
                    } else {
                        None
                    }
                });
                if let Some((tool, start)) = drawn {
                    let area = point.and_then(|point| regular(tool, start, point));
                    if let Some(area) = area {
                        crate::area_panel::insert(world, root, area);
                    } else {
                        let mut editor = world.get_mut::<AreaEditor>(root).unwrap();
                        editor.dragging = false;
                        editor.points.clear();
                        editor.notice =
                            "Draw an area between 1 and 100000 canvas units wide.".into();
                    }
                    crate::edit_mode::render_panel(world, root);
                }
                if !world
                    .get::<AreaEditor>(root)
                    .is_some_and(|editor| editor.tool == Some(ShapeKind::Drawn) && editor.dragging)
                {
                    world.resource_mut::<Gesture>().0 = None;
                }
            }
            PointerAction::Cancel => {
                if let Some(mut editor) = world.get_mut::<AreaEditor>(root) {
                    editor.cancel();
                }
                world.resource_mut::<Gesture>().0 = None;
            }
            _ => {}
        }
        if consume {
            event.action = PointerAction::Cancel;
            world.resource_mut::<InputFocus>().clear();
            let mut pointers = world.query::<(&PointerId, &mut PointerPress)>();
            for (id, mut press) in pointers.iter_mut(world) {
                if *id == PointerId::Mouse {
                    *press = PointerPress::default();
                }
            }
        }
    }
    world.insert_resource(events);
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn freehand_sampling_keeps_endpoints_and_closes_with_a_straight_edge() {
        let mut points = vec![DVec2::ZERO];
        for i in 1..2000 {
            sample(&mut points, DVec2::new(f64::from(i), 0.0), 1.0, false);
        }
        for i in 1..500 {
            sample(&mut points, DVec2::new(1999.0, f64::from(i)), 1.0, false);
        }
        let end = DVec2::new(500.0, 499.0);
        sample(&mut points, end, 1.0, true);
        assert_eq!(points.first(), Some(&DVec2::ZERO));
        assert_eq!(points.last(), Some(&end));
        assert!(points.len() < MAX_POINTS);
        let area = InfluenceArea::drawn(&points).unwrap();
        let outline = area.outline();
        assert!(outline[0].distance(DVec2::ZERO) < 1e-8);
        assert!(outline[outline.len() - 2].distance(end) < 1e-8);
        assert_eq!(outline.first(), outline.last());
    }

    crate::laboratory_cases! {
        freehand_sampling_keeps_endpoints_and_closes_with_a_straight_edge,
    }
}
