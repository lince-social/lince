use super::*;
use bevy::{
    ecs::message::MessageCursor,
    picking::{
        hover::{HoverMap, generate_hovermap},
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerPress},
    },
    window::WindowEvent,
};

#[derive(Component, Clone)]
pub(super) struct ListRow {
    pub owner: Entity,
    pub uid: String,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ReorderInput;

#[derive(Resource, Default)]
struct DragState(Option<Drag>);

struct Drag {
    pointer: PointerId,
    row: ListRow,
    start: Location,
    moved: bool,
    target: Option<(Entity, bool)>,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<DragState>()
        .init_resource::<HoverMap>()
        .add_message::<PointerInput>()
        .add_message::<WindowEvent>()
        .add_systems(
            PreUpdate,
            input
                .in_set(ReorderInput)
                .after(generate_hovermap)
                .after(crate::inspection::InspectInput)
                .before(bevy::picking::events::pointer_events)
                .before(bevy::picking::hover::update_interactions)
                .before(crate::topology::input::gestures),
        );
}

fn hit(world: &World, pointer: PointerId) -> Option<Entity> {
    let mut entity = *world
        .get_resource::<HoverMap>()?
        .get(&pointer)?
        .iter()
        .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))?
        .0;
    loop {
        if world.get::<ListRow>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

fn clear_marker(world: &mut World, drag: &Drag) {
    if let Some((entity, _)) = drag.target
        && let Some(mut node) = world.get_mut::<Node>(entity)
    {
        node.border = UiRect::ZERO;
    }
}

fn cancel(world: &mut World, drag: Drag) {
    clear_marker(world, &drag);
    if let Some(mut view) = world.get_mut::<View>(drag.row.owner) {
        view.dragging = false;
    }
}

pub(super) fn move_row(world: &mut World, owner: Entity, uid: &str, target: &str, after: bool) {
    if uid == target || crate::laboratory::suspended(world, owner) {
        return;
    }
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    if view.batch.is_some() {
        return;
    }
    let Some(from) = view.rows.iter().position(|row| row.uid == uid) else {
        return;
    };
    let Some(to) = view.rows.iter().position(|row| row.uid == target) else {
        return;
    };
    let insertion = to + usize::from(after);
    let insertion = insertion - usize::from(from < insertion);
    if insertion == from {
        return;
    }
    let row = view.rows.remove(from);
    view.rows.insert(insertion, row);
    view.manual_order = view.rows.iter().map(|row| row.uid.clone()).collect();
    view.message = "Order changed · Renumber to save".into();
    ui::render(world, owner);
    ui::status(world, owner);
}

fn input(
    world: &mut World,
    mut cursor: Local<MessageCursor<PointerInput>>,
    mut windows: Local<MessageCursor<WindowEvent>>,
) {
    let interrupted = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape))
        || windows
            .read(world.resource::<Messages<WindowEvent>>())
            .any(|event| {
                matches!(event, WindowEvent::WindowFocused(event) if !event.focused)
                    || matches!(event, WindowEvent::CursorLeft(_))
            });
    let mut drag = world.resource_mut::<DragState>().0.take();
    if interrupted
        || drag.as_ref().is_some_and(|drag| {
            world.get::<View>(drag.row.owner).is_none()
                || crate::laboratory::suspended(world, drag.row.owner)
        })
    {
        if let Some(active) = drag.take() {
            cancel(world, active);
        }
    }
    let mut events = world.remove_resource::<Messages<PointerInput>>().unwrap();
    let mut consumed = Vec::new();
    for event in cursor.read_mut(&mut events) {
        let content = crate::topology::input::CONTENT_POINTER;
        if event.pointer_id == PointerId::Mouse
            && (drag.as_ref().is_some_and(|drag| drag.pointer == content)
                || hit(world, content).is_some()
                    && matches!(event.action, PointerAction::Press(PointerButton::Primary)))
            && matches!(
                event.action,
                PointerAction::Press(PointerButton::Primary)
                    | PointerAction::Release(PointerButton::Primary)
                    | PointerAction::Move { .. }
            )
        {
            event.action = PointerAction::Cancel;
            consumed.push(event.pointer_id);
            continue;
        }
        if let Some(active) = &drag
            && active.pointer != event.pointer_id
        {
            continue;
        }
        match event.action {
            PointerAction::Press(PointerButton::Primary) if !interrupted => {
                let Some(entity) = hit(world, event.pointer_id) else {
                    continue;
                };
                let row = world.get::<ListRow>(entity).unwrap().clone();
                if crate::laboratory::suspended(world, row.owner)
                    || world
                        .get::<View>(row.owner)
                        .is_none_or(|view| view.batch.is_some())
                {
                    continue;
                }
                if let Some(active) = drag.take() {
                    cancel(world, active);
                }
                world.get_mut::<View>(row.owner).unwrap().dragging = true;
                drag = Some(Drag {
                    pointer: event.pointer_id,
                    row,
                    start: event.location.clone(),
                    moved: false,
                    target: None,
                });
            }
            PointerAction::Move { .. } if drag.is_some() => {
                let active = drag.as_mut().unwrap();
                clear_marker(world, active);
                active.target = None;
                active.moved |= event.location.position.distance(active.start.position) >= 5.0;
                if active.moved
                    && event.location.target == active.start.target
                    && let Some(target) = hit(world, event.pointer_id)
                    && let Some(row) = world.get::<ListRow>(target)
                    && row.owner == active.row.owner
                    && row.uid != active.row.uid
                {
                    let after = world
                        .get::<UiGlobalTransform>(target)
                        .is_some_and(|transform| {
                            transform
                                .affine()
                                .inverse()
                                .transform_point2(event.location.position)
                                .y
                                >= 0.0
                        });
                    active.target = Some((target, after));
                    world.get_mut::<Node>(target).unwrap().border = if after {
                        UiRect::bottom(px(3))
                    } else {
                        UiRect::top(px(3))
                    };
                }
            }
            PointerAction::Release(PointerButton::Primary) if drag.is_some() => {
                let active = drag.take().unwrap();
                let target = active.target.and_then(|(entity, after)| {
                    world
                        .get::<ListRow>(entity)
                        .map(|row| (row.uid.clone(), after))
                });
                let clicked = !active.moved
                    && hit(world, event.pointer_id).is_some_and(|entity| {
                        world.get::<ListRow>(entity).is_some_and(|row| {
                            row.owner == active.row.owner && row.uid == active.row.uid
                        })
                    });
                let owner = active.row.owner;
                let uid = active.row.uid.clone();
                cancel(world, active);
                if let Some((target, after)) = target {
                    move_row(world, owner, &uid, &target, after);
                } else if clicked
                    && let (Some(frame), Some(config)) =
                        (world.get::<Frame>(owner), config(world, owner))
                {
                    crate::full_record::Open(RecordBinding {
                        area: frame.area,
                        uid,
                        source: config.source,
                    })
                    .apply(world, owner);
                }
            }
            PointerAction::Cancel if drag.is_some() => {
                cancel(world, drag.take().unwrap());
            }
            _ => continue,
        }
        event.action = PointerAction::Cancel;
        consumed.push(event.pointer_id);
    }
    world.insert_resource(events);
    world.resource_mut::<DragState>().0 = drag;
    if !consumed.is_empty() {
        for (id, mut press) in world
            .query::<(&PointerId, &mut PointerPress)>()
            .iter_mut(world)
        {
            if consumed.contains(id) {
                *press = PointerPress::default();
            }
        }
    }
}
