use crate::{
    actions::Action,
    canvas::{CanvasItem, CanvasView},
    edit_mode::EditMode,
    inspection::{Inspection, InspectionExcluded},
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{
    ecs::message::MessageCursor,
    picking::{
        hover::{HoverMap, generate_hovermap},
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerPress},
    },
    prelude::*,
    window::WindowEvent,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SandGroup(pub [u8; 16]);

#[derive(Component, Default)]
pub struct SandSelection(pub Vec<Entity>);

#[derive(Resource, Default)]
struct SelectionGesture(Option<Rectangle>);

struct Rectangle {
    root: Entity,
    workspace: u64,
    start: Location,
    end: Vec2,
    previous: Vec<Entity>,
}

#[derive(Resource, Default)]
struct Drawing(Vec<Entity>);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SelectInput;

pub struct CanvasSelectionPlugin;

impl Plugin for CanvasSelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionGesture>()
            .init_resource::<Drawing>()
            .add_systems(
                PreUpdate,
                input
                    .in_set(SelectInput)
                    .after(generate_hovermap)
                    .before(crate::inspection::InspectInput),
            )
            .add_systems(PostUpdate, draw.before(bevy::ui::UiSystems::Prepare));
    }
}

pub(crate) fn eligible(world: &World, root: Entity, entity: Entity) -> bool {
    world.get::<CanvasItem>(entity).is_some()
        && world.get::<crate::area::InfluenceArea>(entity).is_none()
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

pub(crate) fn group_members(world: &World, root: Entity, entity: Entity) -> Vec<Entity> {
    let group = world.get::<SandGroup>(entity);
    world
        .get::<Children>(root)
        .map_or_else(Vec::new, |children| {
            children
                .iter()
                .filter(|candidate| {
                    eligible(world, root, *candidate)
                        && (*candidate == entity
                            || group.is_some() && world.get::<SandGroup>(*candidate) == group)
                })
                .collect()
        })
}

fn expand_groups(world: &World, root: Entity, selection: Vec<Entity>) -> Vec<Entity> {
    let selected: HashSet<_> = selection
        .into_iter()
        .filter(|entity| eligible(world, root, *entity))
        .collect();
    let groups: HashSet<_> = selected
        .iter()
        .filter_map(|entity| world.get::<SandGroup>(*entity).copied())
        .collect();
    let mut result = Vec::new();
    if let Some(children) = world.get::<Children>(root) {
        for entity in children.iter() {
            if eligible(world, root, entity)
                && (selected.contains(&entity)
                    || world
                        .get::<SandGroup>(entity)
                        .is_some_and(|group| groups.contains(group)))
            {
                result.push(entity);
            }
        }
    }
    result.sort();
    result
}

pub(crate) fn selected(world: &World, root: Entity) -> Vec<Entity> {
    world
        .get::<SandSelection>(root)
        .map_or_else(Vec::new, |selection| {
            selection
                .0
                .iter()
                .copied()
                .filter(|entity| eligible(world, root, *entity))
                .collect()
        })
}

fn set_selection(world: &mut World, root: Entity, mut entities: Vec<Entity>) {
    entities.sort();
    entities.dedup();
    if world
        .get::<SandSelection>(root)
        .is_none_or(|selection| selection.0 != entities)
    {
        if let Some(mut inspection) = world.get_mut::<Inspection>(root) {
            inspection.selected = entities.first().copied();
        }
        world.entity_mut(root).insert(SandSelection(entities));
    }
}

pub(crate) fn clear(world: &mut World, root: Entity) {
    if let Some(mut selection) = world.get_mut::<SandSelection>(root)
        && !selection.0.is_empty()
    {
        selection.0.clear();
    }
    if world
        .get_resource::<SelectionGesture>()
        .is_some_and(|gesture| {
            gesture
                .0
                .as_ref()
                .is_some_and(|gesture| gesture.root == root)
        })
    {
        world.resource_mut::<SelectionGesture>().0 = None;
    }
}

pub(crate) fn screen_bounds(world: &World, root: Entity, entity: Entity) -> Option<Rect> {
    let viewport = crate::inspection::bounds(world, root)?;
    let item = world.get::<CanvasItem>(entity)?;
    let view = *world.get::<CanvasView>(root)?;
    let pin = world.get::<Pinned>(entity);
    let view = pin.map_or(view, |pin| pin.view(item, viewport.size()));
    let top = view.screen_position(item, viewport.size())? + viewport.min;
    Some(Rect::from_corners(top, top + item.size * view.zoom as f32))
}

fn inside(world: &World, root: Entity, rect: Rect) -> Vec<Entity> {
    let Some(children) = world.get::<Children>(root) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for entity in children.iter() {
        if eligible(world, root, entity)
            && world.get::<Visibility>(entity) != Some(&Visibility::Hidden)
            && let Some(bounds) = screen_bounds(world, root, entity)
            && rect.contains(bounds.min)
            && rect.contains(bounds.max)
        {
            result.push(entity);
        }
    }
    expand_groups(world, root, result)
}

fn hit(world: &World) -> Option<(Entity, Option<Entity>)> {
    let mut entity = world
        .resource::<HoverMap>()
        .get(&PointerId::Mouse)?
        .iter()
        .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
        .map(|(entity, _)| *entity)?;
    let mut sand = None;
    loop {
        if world.get::<InspectionExcluded>(entity).is_some() {
            return None;
        }
        if world.get::<CanvasItem>(entity).is_some() {
            sand = Some(entity);
        }
        if world.get::<CanvasView>(entity).is_some() {
            return Some((entity, sand));
        }
        let parent = world.get::<ChildOf>(entity)?.parent();
        if world.get::<CanvasView>(parent).is_some() && sand.is_none() {
            return None;
        }
        entity = parent;
    }
}

fn input(
    world: &mut World,
    mut cursor: Local<MessageCursor<PointerInput>>,
    mut windows: Local<MessageCursor<WindowEvent>>,
) {
    let interrupted = windows
        .read(world.resource::<Messages<WindowEvent>>())
        .any(|event| {
            matches!(event, WindowEvent::WindowFocused(event) if !event.focused)
                || matches!(event, WindowEvent::CursorLeft(_))
        })
        || world
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape);
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<SandSelection>>()
        .iter(world)
        .collect();
    for root in roots {
        if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled) {
            clear(world, root);
        } else {
            let selection = selected(world, root);
            set_selection(world, root, selection);
        }
    }
    if world
        .resource::<SelectionGesture>()
        .0
        .as_ref()
        .is_some_and(|gesture| {
            interrupted
                || world
                    .get::<Workspaces>(gesture.root)
                    .is_some_and(|spaces| spaces.active != gesture.workspace)
                || !world
                    .get::<EditMode>(gesture.root)
                    .is_some_and(|mode| mode.enabled)
        })
        && let Some(gesture) = world.resource_mut::<SelectionGesture>().0.take()
    {
        let previous = gesture
            .previous
            .into_iter()
            .filter(|entity| eligible(world, gesture.root, *entity))
            .collect();
        set_selection(world, gesture.root, previous);
    }
    let control = world
        .resource::<ButtonInput<KeyCode>>()
        .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let mut events = world.remove_resource::<Messages<PointerInput>>().unwrap();
    let mut consumed = false;
    for event in cursor.read_mut(&mut events) {
        if event.pointer_id != PointerId::Mouse {
            continue;
        }
        if let Some(mut gesture) = world.resource_mut::<SelectionGesture>().0.take() {
            if event.location.target != gesture.start.target
                || !event.location.position.is_finite()
                || matches!(event.action, PointerAction::Cancel)
            {
                set_selection(world, gesture.root, gesture.previous);
                continue;
            }
            gesture.end = event.location.position;
            let entities = inside(
                world,
                gesture.root,
                Rect::from_corners(gesture.start.position, gesture.end),
            );
            set_selection(world, gesture.root, entities);
            let finished = matches!(
                event.action,
                PointerAction::Release(PointerButton::Secondary)
            );
            if !finished {
                world.resource_mut::<SelectionGesture>().0 = Some(gesture);
            }
            event.action = PointerAction::Cancel;
            consumed = true;
            continue;
        }
        let Some((root, sand)) = hit(world) else {
            continue;
        };
        if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled) {
            continue;
        }
        if control
            && matches!(event.action, PointerAction::Press(PointerButton::Secondary))
            && event.location.position.is_finite()
        {
            let previous = selected(world, root);
            let workspace = world
                .get::<Workspaces>(root)
                .map_or(1, |spaces| spaces.active);
            world.resource_mut::<SelectionGesture>().0 = Some(Rectangle {
                root,
                workspace,
                start: event.location.clone(),
                end: event.location.position,
                previous,
            });
            set_selection(world, root, Vec::new());
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .clear();
            if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root) {
                editor.cancel();
            }
            event.action = PointerAction::Cancel;
            consumed = true;
        } else if matches!(
            event.action,
            PointerAction::Press(PointerButton::Primary | PointerButton::Secondary)
        ) {
            let entities = sand
                .filter(|sand| eligible(world, root, *sand))
                .map_or_else(Vec::new, |sand| {
                    let selection = selected(world, root);
                    if selection.contains(&sand) {
                        selection
                    } else {
                        group_members(world, root, sand)
                    }
                });
            set_selection(world, root, entities);
        }
    }
    world.insert_resource(events);
    if consumed {
        for (id, mut press) in world
            .query::<(&PointerId, &mut PointerPress)>()
            .iter_mut(world)
        {
            if *id == PointerId::Mouse {
                *press = PointerPress::default();
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum GroupAction {
    Group,
    Ungroup,
}

pub(crate) fn options(world: &World, root: Entity, target: Entity) -> (bool, bool) {
    if !eligible(world, root, target) {
        return (false, false);
    }
    let selection = selected(world, root);
    let selection = if selection.contains(&target) {
        selection
    } else {
        group_members(world, root, target)
    };
    let group = world.get::<SandGroup>(target);
    (
        selection.len() > 1
            && (group.is_none()
                || selection
                    .iter()
                    .any(|entity| world.get::<SandGroup>(*entity) != group)),
        selection
            .iter()
            .any(|entity| world.get::<SandGroup>(*entity).is_some()),
    )
}

impl Action for GroupAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        vec![crate::inspection::Connection {
            target,
            name: match self {
                Self::Group => "Group selected Sands",
                Self::Ungroup => "Ungroup Sands",
            }
            .into(),
        }]
    }

    fn apply(&self, world: &mut World, target: Entity) {
        let Some(root) = world.get::<ChildOf>(target).map(ChildOf::parent) else {
            return;
        };
        if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled)
            || !eligible(world, root, target)
        {
            return;
        }
        let mut selection = selected(world, root);
        if !selection.contains(&target) {
            selection = group_members(world, root, target);
        }
        let expanded = expand_groups(world, root, selection);
        match self {
            Self::Group => {
                if expanded.len() < 2 {
                    return;
                }
                let mut id = [0; 16];
                if getrandom::fill(&mut id).is_err() {
                    return;
                }
                let groups: Vec<_> = expanded
                    .iter()
                    .filter_map(|entity| world.get::<SandGroup>(*entity).copied())
                    .collect();
                crate::workspace::regroup_saved(world, root, &groups, Some(SandGroup(id)));
                for entity in &expanded {
                    world.entity_mut(*entity).insert(SandGroup(id));
                }
            }
            Self::Ungroup => {
                let groups: Vec<_> = expanded
                    .iter()
                    .filter_map(|entity| world.get::<SandGroup>(*entity).copied())
                    .collect();
                crate::workspace::regroup_saved(world, root, &groups, None);
                for entity in &expanded {
                    world.entity_mut(*entity).remove::<SandGroup>();
                }
            }
        }
        set_selection(world, root, expanded);
    }
}

fn draw(world: &mut World) {
    let roots: Vec<_> = world
        .query::<(Entity, &EditMode)>()
        .iter(world)
        .filter(|(_, mode)| mode.enabled)
        .map(|(root, _)| root)
        .collect();
    let mut marks = Vec::new();
    for root in roots {
        let Some(viewport) = crate::inspection::bounds(world, root) else {
            continue;
        };
        for entity in selected(world, root) {
            if let Some(rect) = screen_bounds(world, root, entity) {
                marks.push((
                    root,
                    Rect::from_corners(rect.min - viewport.min, rect.max - viewport.min),
                ));
            }
        }
        if let Some(gesture) = &world.resource::<SelectionGesture>().0
            && gesture.root == root
        {
            marks.push((
                root,
                Rect::from_corners(
                    gesture.start.position - viewport.min,
                    gesture.end - viewport.min,
                ),
            ));
        }
    }
    let mut drawing = std::mem::take(&mut world.resource_mut::<Drawing>().0);
    drawing.retain(|entity| world.get_entity(*entity).is_ok());
    while drawing.len() > marks.len() {
        world.despawn(drawing.pop().unwrap());
    }
    for (index, (root, rect)) in marks.into_iter().enumerate() {
        let node = Node {
            position_type: PositionType::Absolute,
            left: px(rect.min.x),
            top: px(rect.min.y),
            width: px(rect.width()),
            height: px(rect.height()),
            border: UiRect::all(px(1)),
            ..default()
        };
        if let Some(entity) = drawing.get(index).copied() {
            if world.get::<Node>(entity) != Some(&node) {
                world.entity_mut(entity).insert(node);
            }
            if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(root) {
                world.entity_mut(entity).insert(ChildOf(root));
            }
        } else {
            drawing.push(
                world
                    .spawn((
                        node,
                        ChildOf(root),
                        InspectionExcluded,
                        Pickable::IGNORE,
                        GlobalZIndex(22),
                        crate::token_style::border(crate::tokens::Token::Accent),
                    ))
                    .id(),
            );
        }
    }
    world.resource_mut::<Drawing>().0 = drawing;
}

pub(crate) fn companions(world: &World, root: Entity, entity: Entity) -> Vec<Entity> {
    let selection = selected(world, root);
    if selection.contains(&entity) {
        selection
    } else {
        group_members(world, root, entity)
    }
}

pub(crate) fn transform_members(
    world: &mut World,
    entity: Entity,
    before: CanvasItem,
    after: CanvasItem,
) {
    let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
        return;
    };
    if !eligible(world, root, entity) {
        return;
    }
    let Some(view) = world.get::<CanvasView>(root).copied() else {
        return;
    };
    let Some(viewport) = crate::inspection::bounds(world, root).map(|rect| rect.size().as_dvec2())
    else {
        return;
    };
    let pin = world.get::<Pinned>(entity).copied();
    let zoom = pin.map_or(view.zoom, |pin| pin.scale);
    let delta = (after.position - before.position) * zoom;
    let scale = (after.size / before.size).as_dvec2();
    let origin = pin.map_or((before.position - view.center) * zoom, |pin| {
        (bevy::math::DVec2::from_array(pin.anchor) - bevy::math::DVec2::splat(0.5)) * viewport
            - delta
    });
    for member in companions(world, root, entity) {
        if member == entity {
            continue;
        }
        let item = *world.get::<CanvasItem>(member).unwrap();
        let member_pin = world.get::<Pinned>(member).copied();
        let member_zoom = member_pin.map_or(view.zoom, |pin| pin.scale);
        let center = member_pin.map_or((item.position - view.center) * member_zoom, |pin| {
            (bevy::math::DVec2::from_array(pin.anchor) - bevy::math::DVec2::splat(0.5)) * viewport
        });
        let movement = delta + (center - origin) * (scale - bevy::math::DVec2::ONE);
        let position = item.position + movement / member_zoom;
        let size = item.size * scale.as_vec2();
        if !position.is_finite() || !size.is_finite() || size.min_element() <= 0.0 {
            continue;
        }
        *world.get_mut::<CanvasItem>(member).unwrap() = CanvasItem { position, size };
        if let Some(mut pin) = world.get_mut::<Pinned>(member) {
            pin.moved(movement, viewport.as_vec2());
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::{camera::NormalizedRenderTarget, math::DVec2, picking::backend::HitData};

    fn fixture() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<HoverMap>()
            .add_message::<PointerInput>()
            .add_message::<WindowEvent>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                CanvasSelectionPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        app.world_mut().entity_mut(root).insert(ComputedNode {
            size: Vec2::new(800.0, 600.0),
            ..default()
        });
        let first = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(-80.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(80.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        app.world_mut()
            .resource_mut::<HoverMap>()
            .entry(PointerId::Mouse)
            .or_default()
            .insert(root, HitData::new(root, 0.0, None, None));
        (app, root, first, second)
    }

    fn send(app: &mut App, action: PointerAction, position: Vec2) {
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::None {
                    width: 800,
                    height: 600,
                },
                position,
            },
            action,
        ));
        app.update();
    }

    #[cfg_attr(test, test)]
    fn selection_uses_zoomed_bounds_and_screen_pins_and_retains_overlay_entities() {
        let (mut app, root, first, second) = fixture();
        app.world_mut().get_mut::<CanvasView>(root).unwrap().zoom = 2.0;
        let rect = Rect::from_corners(Vec2::new(-205.0, -45.0), Vec2::new(-115.0, 45.0));
        assert_eq!(inside(app.world(), root, rect), vec![first]);
        app.world_mut().entity_mut(first).insert(Pinned {
            anchor: [0.3, 0.5],
            scale: 2.0,
        });
        app.world_mut().get_mut::<CanvasView>(root).unwrap().center = DVec2::splat(1e12);
        assert_eq!(inside(app.world(), root, rect), vec![first]);
        app.world_mut().entity_mut(first).remove::<Pinned>();
        app.world_mut().get_mut::<CanvasView>(root).unwrap().center = DVec2::ZERO;
        set_selection(app.world_mut(), root, vec![first, second]);
        app.update();
        let overlays = app.world().resource::<Drawing>().0.clone();
        assert_eq!(overlays.len(), 2);
        app.world_mut().clear_trackers();
        app.update();
        assert_eq!(app.world().resource::<Drawing>().0, overlays);
        for entity in &overlays {
            assert!(
                !app.world()
                    .entity(*entity)
                    .get_ref::<Node>()
                    .unwrap()
                    .is_changed()
            );
        }
        app.world_mut()
            .get_mut::<CanvasView>(root)
            .unwrap()
            .center
            .x += 10.0;
        app.update();
        assert_eq!(app.world().resource::<Drawing>().0, overlays);
        crate::workspace::create(app.world_mut(), root);
        assert!(selected(app.world(), root).is_empty());
    }

    #[cfg_attr(test, test)]
    fn rectangle_requires_full_containment_and_ignores_other_workspaces_hidden_sands_and_areas() {
        let (mut app, root, first, second) = fixture();
        assert_eq!(
            inside(
                app.world(),
                root,
                Rect::from_corners(Vec2::new(-105.0, -25.0), Vec2::new(95.0, 25.0))
            ),
            vec![first]
        );
        let rect = Rect::from_corners(Vec2::new(110.0, 30.0), Vec2::new(-110.0, -30.0));
        assert_eq!(inside(app.world(), root, rect).len(), 2);
        app.world_mut()
            .entity_mut(second)
            .insert(WorkspaceMember(2));
        assert_eq!(inside(app.world(), root, rect), vec![first]);
        app.world_mut().entity_mut(first).insert(Visibility::Hidden);
        assert!(inside(app.world(), root, rect).is_empty());
        app.world_mut().entity_mut(first).remove::<Visibility>();
        app.world_mut()
            .entity_mut(first)
            .insert(crate::area::InfluenceArea::new(
                crate::area::AreaShape::Square,
                DVec2::ZERO,
                DVec2::splat(40.0),
            ));
        assert!(inside(app.world(), root, rect).is_empty());
    }

    #[cfg_attr(test, test)]
    fn right_drag_selects_live_closes_on_release_and_escape_restores_the_previous_selection() {
        let (mut app, root, first, second) = fixture();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        send(
            &mut app,
            PointerAction::Press(PointerButton::Secondary),
            Vec2::new(-110.0, -30.0),
        );
        send(
            &mut app,
            PointerAction::Move { delta: Vec2::ZERO },
            Vec2::new(110.0, 30.0),
        );
        assert_eq!(selected(app.world(), root).len(), 2);
        assert!(app.world().resource::<SelectionGesture>().0.is_some());
        send(
            &mut app,
            PointerAction::Release(PointerButton::Secondary),
            Vec2::new(110.0, 30.0),
        );
        assert!(app.world().resource::<SelectionGesture>().0.is_none());
        send(
            &mut app,
            PointerAction::Press(PointerButton::Secondary),
            Vec2::splat(150.0),
        );
        assert!(selected(app.world(), root).is_empty());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let restored = selected(app.world(), root);
        assert!(restored.contains(&first) && restored.contains(&second));
        assert!(app.world().resource::<SelectionGesture>().0.is_none());
        assert_eq!(
            app.world().get::<CanvasView>(root).unwrap().center,
            DVec2::ZERO
        );
    }

    #[cfg_attr(test, test)]
    fn groups_keep_entities_and_contents_and_cannot_change_another_workspace() {
        let (mut app, root, first, second) = fixture();
        let text = app
            .world_mut()
            .spawn((Text::new("Keep this"), ChildOf(first)))
            .id();
        set_selection(app.world_mut(), root, vec![first, second]);
        GroupAction::Group.apply(app.world_mut(), first);
        let group = *app.world().get::<SandGroup>(first).unwrap();
        assert_eq!(app.world().get::<SandGroup>(second), Some(&group));
        assert_eq!(options(app.world(), root, first), (false, true));
        let saved = crate::sand_placement::Placement::capture(app.world(), first);
        let saved: crate::sand_placement::Placement =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(saved.group, Some(group));
        app.world_mut()
            .entity_mut(second)
            .insert(WorkspaceMember(2));
        GroupAction::Ungroup.apply(app.world_mut(), second);
        assert_eq!(app.world().get::<SandGroup>(first), Some(&group));
        GroupAction::Ungroup.apply(app.world_mut(), first);
        assert!(app.world().get::<SandGroup>(first).is_none());
        assert_eq!(app.world().get::<SandGroup>(second), Some(&group));
        assert_eq!(app.world().get::<Text>(text).unwrap().0, "Keep this");
        assert_eq!(app.world().get::<ChildOf>(text).unwrap().parent(), first);
    }

    #[cfg_attr(test, test)]
    fn group_movement_and_resize_preserve_offsets_at_distant_coordinates() {
        let (mut app, root, first, second) = fixture();
        let offset = DVec2::splat(1e12);
        app.world_mut().get_mut::<CanvasView>(root).unwrap().center = offset;
        for sand in [first, second] {
            app.world_mut()
                .get_mut::<CanvasItem>(sand)
                .unwrap()
                .position += offset;
        }
        set_selection(app.world_mut(), root, vec![first, second]);
        GroupAction::Group.apply(app.world_mut(), first);
        let before = *app.world().get::<CanvasItem>(first).unwrap();
        let after = CanvasItem {
            position: before.position + DVec2::new(10.0, -5.0),
            size: before.size * 2.0,
        };
        *app.world_mut().get_mut::<CanvasItem>(first).unwrap() = after;
        transform_members(app.world_mut(), first, before, after);
        let item = app.world().get::<CanvasItem>(second).unwrap();
        assert_eq!(item.position, offset + DVec2::new(250.0, -5.0));
        assert_eq!(item.size, Vec2::splat(80.0));
        assert_eq!(app.world().get::<ChildOf>(second).unwrap().parent(), root);
    }

    crate::laboratory_cases! {
        selection_uses_zoomed_bounds_and_screen_pins_and_retains_overlay_entities,
        rectangle_requires_full_containment_and_ignores_other_workspaces_hidden_sands_and_areas,
        right_drag_selects_live_closes_on_release_and_escape_restores_the_previous_selection,
        groups_keep_entities_and_contents_and_cannot_change_another_workspace,
        group_movement_and_resize_preserve_offsets_at_distant_coordinates,
    }
}
