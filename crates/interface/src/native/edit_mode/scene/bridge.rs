//! Project the tiny scene's root into a local Room and one occurrence onto its existing Sand.
use super::{Documents, Form, Session, draw};
use crate::native::{
    canvas_host::Identity,
    canvas_selection::{self, SandGroup},
    item_properties::Attachment,
    protein_area::RecordBinding,
    topology::{
        self,
        appearance::CANVAS_UNITS_PER_METER,
        rooms::{self, Member, Room},
    },
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec3, prelude::*};
use nucleus::{
    canvas::{
        Pose, room,
        scene::{CanvasBinding, Handle, Scene},
    },
    karma::DecimalValue,
};

/// Enables gesture capture only on desktops with a linked scene.
#[derive(Component)]
pub(crate) struct Connected;

#[derive(Component)]
struct Drag {
    workspace: u64,
    target: Target,
    before: Scene,
    position: [f64; 3],
}

#[derive(Clone, Copy, PartialEq)]
struct Target {
    item: Entity,
    room: Entity,
}

pub(super) fn find(
    world: &World,
    root: Entity,
    workspace: u64,
    id: &str,
) -> Result<Entity, String> {
    let mut matches = world
        .get::<Children>(root)
        .into_iter()
        .flat_map(|c| c.iter())
        .filter(|e| {
            world
                .get::<WorkspaceMember>(*e)
                .is_some_and(|w| w.0 == workspace)
                && world.get::<Identity>(*e).is_some_and(|i| i.0 == id)
        });
    let entity = matches
        .next()
        .ok_or("Linked canvas placement is missing from this workspace.")?;
    if matches.next().is_some() {
        return Err("Canvas identity is ambiguous in this workspace.".into());
    }
    Ok(entity)
}

fn record(world: &World, item: Entity) -> Result<Option<String>, String> {
    let attached = world.get::<Attachment>(item).map(|a| &a.uid);
    let produced = world.get::<RecordBinding>(item).map(|b| &b.uid);
    if attached.is_some() && produced.is_some() && attached != produced {
        return Err("The selected item has two different Record bindings.".into());
    }
    Ok(attached.or(produced).cloned())
}

fn resolve(
    world: &World,
    root: Entity,
    workspace: u64,
    b: &CanvasBinding,
) -> Result<Target, String> {
    let item = find(world, root, workspace, &b.placement)?;
    let room = find(world, root, workspace, &b.room)?;
    if item == room
        || world
            .get::<crate::native::canvas::CanvasItem>(item)
            .is_none()
        || world
            .get::<crate::native::canvas::CanvasItem>(room)
            .is_none()
        || world.get::<Room>(item).is_some()
        || world.get::<Member>(room).is_some()
        || !world.get::<Room>(room).is_some_and(|r| r.0.valid())
        || !world
            .get::<Member>(item)
            .is_some_and(|m| m.0.valid() && m.0.room == b.room && m.0.parent.is_none())
    {
        return Err("Select a direct member of a valid Room in this workspace.".into());
    }
    if rooms::hosted(world, item)
        || rooms::hosted(world, room)
        || world.get::<SandGroup>(item).is_some()
    {
        return Err("The scene bridge needs a local, ungrouped Room member.".into());
    }
    Ok(Target { item, room })
}

fn metres(position: [f64; 3]) -> Result<[DecimalValue; 3], String> {
    let mut values = [DecimalValue::from_integer(0); 3];
    for (value, units) in values.iter_mut().zip(position) {
        let m = units / f64::from(CANVAS_UNITS_PER_METER);
        *value = DecimalValue::parse_inferred(&m.to_string())
            .or_else(|_| {
                DecimalValue::parse_inferred(
                    format!("{m:.18}")
                        .trim_end_matches('0')
                        .trim_end_matches('.'),
                )
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(values)
}

pub(super) fn bind(
    world: &World,
    form: Form,
    scene: &mut Scene,
    handle: &Handle,
) -> Result<(), String> {
    let selection = canvas_selection::selected(world, form.root);
    let [entity] = selection.as_slice() else {
        return Err("Select one existing canvas item that is a direct Room member.".into());
    };
    bind_item(world, form, scene, handle, *entity)
}

pub(super) fn bind_item(
    world: &World,
    form: Form,
    scene: &mut Scene,
    handle: &Handle,
    entity: Entity,
) -> Result<(), String> {
    if scene.canvas.is_some() {
        return Err("This scene already has a canvas binding.".into());
    }
    let member = world
        .get::<Member>(entity)
        .ok_or("Join the selected canvas item to a Room first.")?;
    let binding = CanvasBinding {
        handle: handle.clone(),
        room: member.0.room.clone(),
        placement: world
            .get::<Identity>(entity)
            .ok_or("The item needs a saved placement identity.")?
            .0
            .clone(),
    };
    let target = resolve(world, form.root, form.workspace, &binding)?;
    let record = record(world, target.item)?;
    let slot = scene.slot(handle).ok_or("Select a model placement.")?;
    if slot.record.is_some() && slot.record != record {
        return Err(
            "The selected placement and canvas item have different Record bindings.".into(),
        );
    }
    let mut next = scene.clone();
    next.edit(handle, metres(member.0.local.position)?, record)?;
    next.canvas = Some(binding);
    next.validate()?;
    *scene = next;
    Ok(())
}

fn pose(world: &World, entity: Entity) -> Result<Pose, String> {
    rooms::world_pose(world, entity).ok_or_else(|| "Linked canvas pose is unavailable.".into())
}

fn target(world: &World, root: Entity, workspace: u64, scene: &Scene) -> Result<Target, String> {
    let b = scene.canvas.as_ref().ok_or("No canvas binding.")?;
    let t = resolve(world, root, workspace, b)?;
    if record(world, t.item)?
        != scene
            .slot(&b.handle)
            .ok_or("Missing linked occurrence.")?
            .record
    {
        return Err("The canvas Record binding differs from the linked placement.".into());
    }
    Ok(t)
}

/// Resolve and validate everything before changing either local or world translation.
pub(super) fn project(
    world: &mut World,
    root: Entity,
    workspace: u64,
    scene: &Scene,
) -> Result<(), String> {
    let Some((t, local, point)) = projection(world, root, workspace, scene)? else {
        return Ok(());
    };
    world.get_mut::<Member>(t.item).unwrap().0.local = local;
    topology::set_position(world, t.item, DVec3::from_array(point.position));
    crate::native::record_presentation::save(world, t.item);
    crate::native::workspace::request_save(world);
    Ok(())
}

pub(super) fn check_projection(
    world: &World,
    root: Entity,
    workspace: u64,
    scene: &Scene,
) -> Result<(), String> {
    projection(world, root, workspace, scene).map(|_| ())
}

fn projection(
    world: &World,
    root: Entity,
    workspace: u64,
    scene: &Scene,
) -> Result<Option<(Target, Pose, Pose)>, String> {
    let Some(b) = &scene.canvas else {
        return Ok(None);
    };
    let t = target(world, root, workspace, scene)?;
    let mut local = world.get::<Member>(t.item).unwrap().0.local.clone();
    let coordinates = scene.slot(&b.handle).unwrap().metres;
    local.position = coordinates.map(|d| {
        d.mantissa() as f64 / 10_f64.powi(i32::from(d.scale())) * f64::from(CANVAS_UNITS_PER_METER)
    });
    let point = room::compose(&pose(world, t.room)?, &local);
    if !room::valid_pose(&point) {
        return Err("The model position exceeds the canvas range.".into());
    }
    Ok(Some((t, local, point)))
}

fn session(world: &World, root: Entity) -> Option<(u64, &Session)> {
    let workspace = world.get::<Workspaces>(root)?.active;
    Some((workspace, world.get::<Documents>(root)?.0.get(&workspace)?))
}

pub(super) fn refresh(world: &mut World, root: Entity, workspace: u64) {
    let section = world
        .query::<&Form>()
        .iter(world)
        .find(|f| f.root == root && f.workspace == workspace)
        .map(|f| f.section);
    if let Some(section) = section {
        draw(world, section, root, workspace);
    }
}

pub(crate) fn selected(world: &mut World, root: Entity, item: Entity) {
    let Some((workspace, session)) = session(world, root) else {
        return;
    };
    let Some(scene) = &session.scene else { return };
    let Some(b) = &scene.canvas else { return };
    if session.selected.as_ref() == Some(&b.handle)
        || world
            .get::<Identity>(item)
            .is_none_or(|id| id.0 != b.placement)
        || !target(world, root, workspace, scene).is_ok_and(|t| t.item == item)
    {
        return;
    }
    let handle = b.handle.clone();
    let mut documents = world.get_mut::<Documents>(root).unwrap();
    let session = documents.0.get_mut(&workspace).unwrap();
    session.accepted.invalidate();
    session.selected = Some(handle);
    refresh(world, root, workspace);
}

pub(super) fn select_item(
    world: &mut World,
    root: Entity,
    workspace: u64,
    scene: &Scene,
    handle: &Handle,
) {
    if scene.canvas.as_ref().is_some_and(|b| b.handle == *handle)
        && let Ok(t) = target(world, root, workspace, scene)
    {
        crate::native::canvas_item::select(world, root, t.item);
    }
}

pub(crate) fn begin(world: &mut World, root: Entity, item: Entity) -> bool {
    let Some((workspace, session)) = session(world, root) else {
        return false;
    };
    let Some(scene) = &session.scene else {
        return false;
    };
    if scene.canvas.as_ref().is_none_or(|b| {
        world
            .get::<Identity>(item)
            .is_none_or(|id| id.0 != b.placement)
    }) {
        return false;
    }
    let Ok(t) = target(world, root, workspace, scene) else {
        return false;
    };
    if t.item != item {
        return false;
    }
    let position = world.get::<Member>(item).unwrap().0.local.position;
    let drag = Drag {
        workspace,
        target: t,
        before: scene.clone(),
        position,
    };
    world.entity_mut(root).insert(drag);
    true
}

pub(crate) fn finish(world: &mut World, root: Entity, commit: bool) {
    let Some(drag) = world
        .get_entity_mut(root)
        .ok()
        .and_then(|mut e| e.take::<Drag>())
    else {
        return;
    };
    let workspace = drag.workspace;
    let Some(current) = world
        .get::<Documents>(root)
        .and_then(|d| d.0.get(&workspace))
        .and_then(|s| s.scene.as_ref())
    else {
        return;
    };
    let active = world
        .get::<Workspaces>(root)
        .is_some_and(|w| w.active == workspace);
    if current.canvas != drag.before.canvas
        || !target(world, root, workspace, current).is_ok_and(|t| t == drag.target)
    {
        return;
    }
    let current = current.clone();
    let handle = current.canvas.as_ref().unwrap().handle.clone();
    let slot = current.slot(&handle).unwrap();
    let edit = || -> Result<Option<Scene>, String> {
        if !active || !commit || current.slot(&handle) != drag.before.slot(&handle) {
            return Err("Canvas drag cancelled or superseded.".into());
        }
        let parent = pose(world, drag.target.room)?;
        let point = pose(world, drag.target.item)?;
        let mut start = room::identity_pose();
        start.position = drag.position;
        if room::compose(&parent, &start).position == point.position {
            return Ok(None);
        }
        let local = room::relative(&parent, &point);
        let mut coordinates = slot.metres;
        for (i, value) in metres(local.position)?.into_iter().enumerate() {
            if local.position[i] != drag.position[i] {
                coordinates[i] = value;
            }
        }
        let mut next = current.clone();
        next.edit(&handle, coordinates, slot.record.clone())?;
        Ok((next != current).then_some(next))
    };
    let (next, message) = match edit() {
        Ok(Some(next)) => (Some(next), "Moved the linked canvas placement.".to_owned()),
        Err(message) => (None, message),
        Ok(None) => return,
    };
    if project(world, root, workspace, next.as_ref().unwrap_or(&current)).is_err() {
        return;
    }
    let mut docs = world.get_mut::<Documents>(root).unwrap();
    let session = docs.0.get_mut(&workspace).unwrap();
    if let Some(next) = next {
        session.remember(current);
        session.scene = Some(next);
        session.selected = Some(handle.clone());
    }
    session.message = message;
    refresh(world, root, workspace);
}
