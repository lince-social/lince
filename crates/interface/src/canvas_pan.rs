use crate::canvas::{CanvasItem, CanvasView};
use crate::canvas_resize::{Edges, Resize, ResizeCursor};
use bevy::{
    camera::NormalizedRenderTarget,
    ecs::entity::ContainsEntity,
    input_focus::InputFocus,
    picking::{
        PickingSystems,
        events::pointer_events,
        hover::{HoverMap, generate_hovermap, update_interactions},
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerPress},
    },
    prelude::*,
    window::{CursorIcon, WindowEvent},
};

#[derive(Resource, Default)]
struct CanvasPan(
    Option<Gesture>,
    Option<Location>,
    Vec<(Entity, CanvasItem, CanvasItem)>,
);

pub(crate) fn dragged(world: &World) -> Option<Entity> {
    world.get_resource::<CanvasPan>()?.0.as_ref()?.item
}

struct Gesture {
    button: PointerButton,
    view: Entity,
    item: Option<Entity>,
    last: Location,
    resize: Option<Resize>,
}

pub struct CanvasPanPlugin;

impl Plugin for CanvasPanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CanvasPan>()
            .init_resource::<ResizeCursor>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<HoverMap>()
            .add_message::<PointerInput>()
            .add_message::<WindowEvent>()
            .add_systems(
                PreUpdate,
                (pan, move_selection)
                    .chain()
                    .in_set(PickingSystems::Hover)
                    .after(bevy::input::InputSystems)
                    .after(generate_hovermap)
                    .after(crate::inspection::InspectInput)
                    .before(update_interactions)
                    .before(pointer_events),
            );
    }
}

fn pan(
    mut input: MessageMutator<PointerInput>,
    mut windows: MessageReader<WindowEvent>,
    keys: Res<ButtonInput<KeyCode>>,
    hover: Res<HoverMap>,
    mut gesture: ResMut<CanvasPan>,
    mut views: Query<&mut CanvasView>,
    mut items: Query<(
        &mut CanvasItem,
        Option<&mut crate::sand_placement::Pinned>,
        Option<&mut crate::area::InfluenceArea>,
    )>,
    modes: Query<&crate::edit_mode::EditMode>,
    parents: Query<&ChildOf>,
    mut pointers: Query<(&PointerId, &mut PointerPress)>,
    mut focus: ResMut<InputFocus>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform, &Node)>,
    window_cursors: Query<(Entity, &Window, Option<&CursorIcon>)>,
    mut cursor: ResMut<ResizeCursor>,
    mut commands: Commands,
    text_areas: Query<(&crate::sand_text::SandText, &ChildOf, &Node)>,
) {
    for event in windows.read() {
        if matches!(event, WindowEvent::WindowFocused(event) if !event.focused)
            || matches!(event, WindowEvent::CursorLeft(_))
        {
            gesture.0 = None;
            gesture.1 = None;
        }
    }
    if gesture.0.as_ref().is_some_and(|active| {
        active.button == PointerButton::Primary
            && active.item.is_some()
            && !modes.get(active.view).is_ok_and(|mode| mode.enabled)
    }) {
        gesture.0 = None;
    }
    let control = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let mut consumed = false;
    let mut moves = Vec::new();
    for event in input.read() {
        if event.pointer_id != PointerId::Mouse {
            continue;
        }
        gesture.1 = Some(event.location.clone());
        match event.action {
            PointerAction::Press(button @ (PointerButton::Primary | PointerButton::Secondary)) => {
                gesture.0 = None;
                let hit = hover.get(&PointerId::Mouse).and_then(|hits| {
                    hits.iter()
                        .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
                        .map(|(entity, _)| *entity)
                });
                let Some(hit) = hit else { continue };
                let mut candidate = hit;
                let mut sand = None;
                loop {
                    if items.contains(candidate) {
                        sand = Some(candidate);
                    }
                    if let Ok(view) = views.get(candidate) {
                        let moving = (button == PointerButton::Secondary || !control)
                            && sand.is_some()
                            && (button == PointerButton::Secondary
                                || modes.get(candidate).is_ok_and(|mode| mode.enabled));
                        if (candidate == hit || control && sand.is_some() || moving)
                            && view.center.is_finite()
                            && view.zoom.is_finite()
                            && view.zoom > 0.0
                            && event.location.position.is_finite()
                        {
                            let resize = if moving && button == PointerButton::Primary {
                                sand.and_then(|entity| {
                                    let (item, pin, _) = items.get(entity).ok()?;
                                    let effective = CanvasView {
                                        zoom: pin.map_or(view.zoom, |pin| pin.scale),
                                        ..*view
                                    };
                                    let edges = edges_at(
                                        entity,
                                        event.location.position,
                                        &effective,
                                        item,
                                        &geometry,
                                    );
                                    edges.cursor()?;
                                    let mut minimum = Vec2::splat(48.0);
                                    for (area, parent, node) in &text_areas {
                                        if parent.parent() == entity
                                            && area.editable
                                            && area.overflow == crate::sand_text::TextOverflow::Grow
                                        {
                                            let height = if let Val::Px(height) = node.height {
                                                height
                                            } else {
                                                area.size[1]
                                            };
                                            minimum.y =
                                                minimum.y.max(area.offset[1] + height + 16.0);
                                        }
                                    }
                                    Some(Resize {
                                        edges,
                                        start: event.location.position,
                                        zoom: effective.zoom,
                                        original: *item,
                                        minimum,
                                    })
                                })
                            } else {
                                None
                            };
                            gesture.0 = Some(Gesture {
                                button,
                                view: candidate,
                                item: if moving { sand } else { None },
                                last: event.location.clone(),
                                resize,
                            });
                            event.action = PointerAction::Cancel;
                            consumed = true;
                            focus.clear();
                        }
                        break;
                    }
                    let Ok(parent) = parents.get(candidate) else {
                        break;
                    };
                    candidate = parent.parent();
                }
            }
            PointerAction::Move { .. } => {
                let Some(active) = gesture.0.as_mut() else {
                    continue;
                };
                if event.location.target != active.last.target
                    || !event.location.position.is_finite()
                {
                    gesture.0 = None;
                    continue;
                }
                let Ok(mut view) = views.get_mut(active.view) else {
                    gesture.0 = None;
                    continue;
                };
                let delta = (event.location.position - active.last.position).as_dvec2();
                if let Some(entity) = active.item {
                    if active.button == PointerButton::Primary
                        && !modes.get(active.view).is_ok_and(|mode| mode.enabled)
                    {
                        gesture.0 = None;
                        continue;
                    }
                    if !parents
                        .get(entity)
                        .is_ok_and(|parent| parent.parent() == active.view)
                        || geometry
                            .get(entity)
                            .is_ok_and(|(_, _, node)| node.display == Display::None)
                    {
                        gesture.0 = None;
                        continue;
                    }
                    let Ok((mut item, mut pin, area)) = items.get_mut(entity) else {
                        gesture.0 = None;
                        continue;
                    };
                    let zoom = pin.as_ref().map_or(view.zoom, |pin| pin.scale);
                    let before = *item;
                    let original = item.position;
                    if let Some(resize) = &active.resize {
                        if resize.zoom != zoom {
                            gesture.0 = None;
                            continue;
                        }
                        if let Some(resized) = if area
                            .as_ref()
                            .is_some_and(|area| area.shape.kind() != crate::area::ShapeKind::Drawn)
                        {
                            resize.apply_square(event.location.position)
                        } else {
                            resize.apply(event.location.position)
                        } {
                            *item = resized;
                        }
                    } else {
                        let position = item.position + delta / zoom;
                        if position.is_finite() && zoom.is_finite() && zoom > 0.0 {
                            item.position = position;
                        }
                    }
                    if let Some(pin) = pin.as_mut()
                        && let Ok((computed, _, _)) = geometry.get(active.view)
                    {
                        pin.moved(
                            (item.position - original) * zoom,
                            computed.size() * computed.inverse_scale_factor(),
                        );
                    }
                    if let Some(mut area) = area {
                        area.center = item.position.to_array();
                        area.size = item.size.as_dvec2().to_array();
                    }
                    moves.push((entity, before, *item));
                    active.last = event.location.clone();
                    consumed = true;
                    continue;
                }
                let center = view.center - delta / view.zoom;
                if center.is_finite() && view.zoom.is_finite() && view.zoom > 0.0 {
                    view.center = center;
                }
                active.last = event.location.clone();
                consumed = true;
            }
            PointerAction::Release(button)
                if gesture
                    .0
                    .as_ref()
                    .is_some_and(|active| active.button == button) =>
            {
                gesture.0 = None;
                event.action = PointerAction::Cancel;
                consumed = true;
            }
            PointerAction::Cancel => gesture.0 = None,
            _ => {}
        }
    }
    gesture.2.extend(moves);
    if consumed || gesture.0.is_some() {
        for (id, mut press) in &mut pointers {
            if *id == PointerId::Mouse {
                *press = PointerPress::default();
            }
        }
    }
    let mut next_cursor = None;
    if let Some(location) = &gesture.1
        && let NormalizedRenderTarget::Window(window) = &location.target
        && window_cursors
            .get(window.entity())
            .is_ok_and(|(_, window, _)| window.focused)
    {
        if let Some(active) = &gesture.0 {
            next_cursor = active
                .resize
                .as_ref()
                .and_then(|resize| resize.edges.cursor());
        } else if !control {
            let hit = hover.get(&PointerId::Mouse).and_then(|hits| {
                hits.iter()
                    .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
                    .map(|(entity, _)| *entity)
            });
            let mut candidate = hit;
            let mut sand = None;
            while let Some(entity) = candidate {
                if items.contains(entity) {
                    sand = Some(entity);
                }
                if let Ok(view) = views.get(entity) {
                    if modes.get(entity).is_ok_and(|mode| mode.enabled)
                        && let Some(sand) = sand
                        && let Ok((item, pin, _)) = items.get(sand)
                    {
                        let effective = CanvasView {
                            zoom: pin.map_or(view.zoom, |pin| pin.scale),
                            ..*view
                        };
                        next_cursor =
                            edges_at(sand, location.position, &effective, item, &geometry).cursor();
                    }
                    break;
                }
                candidate = parents.get(entity).ok().map(ChildOf::parent);
            }
        }
        cursor.update(
            next_cursor.map(|icon| (window.entity(), icon)),
            &window_cursors,
            &mut commands,
        );
    } else {
        cursor.update(None, &window_cursors, &mut commands);
    }
}

fn edges_at(
    entity: Entity,
    position: Vec2,
    view: &CanvasView,
    item: &CanvasItem,
    geometry: &Query<(&ComputedNode, &UiGlobalTransform, &Node)>,
) -> Edges {
    let Ok((computed, transform, node)) = geometry.get(entity) else {
        return Edges::default();
    };
    if node.display == Display::None || !view.zoom.is_finite() || view.zoom <= 0.0 {
        return Edges::default();
    }
    let center = transform.translation * computed.inverse_scale_factor();
    Edges::at(
        position,
        Rect::from_center_size(center, item.size * view.zoom as f32),
    )
}

fn move_selection(world: &mut World) {
    let moves = std::mem::take(&mut world.resource_mut::<CanvasPan>().2);
    for (entity, before, after) in moves {
        crate::canvas_selection::transform_members(world, entity, before, after);
    }
}
