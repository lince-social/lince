use super::*;
use crate::native::topology::rooms::WorldPlacement;
use crate::native::{protein_area::RecordBinding, topology};

#[derive(Clone)]
struct State {
    identity: (Entity, String, crate::native::protein_area::Source),
    member: Option<model::Member>,
    world: Option<api::Pose>,
    reference: Entity,
    local: api::Pose,
}

#[derive(Component, Default)]
struct Edits(Vec<(State, State)>);

pub(super) fn has_undo(world: &World, owner: Entity) -> bool {
    world
        .get::<Edits>(owner)
        .is_some_and(|edits| !edits.0.is_empty())
}

fn capture(world: &World, owner: Entity) -> Result<State, String> {
    let binding = world
        .get::<RecordBinding>(owner)
        .ok_or("The Record is unavailable.")?;
    let source = binding.area;
    let reference = crate::native::protein_area::room_frame(world, owner).unwrap_or(source);
    let parent =
        runtime::world_pose(world, reference).ok_or("The producing Castle is unavailable.")?;
    let point = runtime::world_pose(world, owner).ok_or("The Record pose is unavailable.")?;
    Ok(State {
        identity: (source, binding.uid.clone(), binding.source.clone()),
        member: world.get::<Member>(owner).map(|member| member.0.clone()),
        world: world
            .get::<WorldPlacement>(owner)
            .map(|pose| pose.0.clone()),
        reference,
        local: runtime::relative(&parent, &point),
    })
}

fn fields(world: &mut World, owner: Entity) -> Result<api::Pose, String> {
    let position = numbers(&input(world, owner, 0)?)?;
    let angles = numbers(&input(world, owner, 1)?)?.map(f64::to_radians);
    Ok(api::Pose {
        position,
        rotation: DQuat::from_euler(EulerRot::XYZ, angles[0], angles[1], angles[2])
            .normalize()
            .to_array(),
    })
}

pub(super) fn perform(world: &mut World, owner: Entity, control: Control) -> Result<(), String> {
    super::super::synchronize(world);
    let before = capture(world, owner)?;
    let mut member = before.member.clone();
    let mut point = before.world.clone();
    match control {
        Control::Attach(room, anchor) | Control::Mount(room, anchor) => {
            if scope(world, room) != scope(world, owner) || world.get::<Room>(room).is_none() {
                return Err("Select a room in this workspace.".into());
            }
            if world
                .get::<crate::native::sand_placement::Pinned>(owner)
                .is_some()
                || world.get::<topology::terrain::Bound>(owner).is_some()
                || world.get::<topology::terrain::FollowSlope>(owner).is_some()
            {
                return Err("Release screen pinning and terrain bindings before using room-local placement.".into());
            }
            let room_id = world
                .get::<Identity>(room)
                .ok_or("The room identity is unavailable.")?
                .0
                .clone();
            let parent = if let Some(anchor) = anchor {
                let value = world
                    .get::<Member>(anchor)
                    .ok_or("The room anchor is unavailable.")?;
                if scope(world, anchor) != scope(world, owner)
                    || value.0.room != room_id
                    || value.0.role == model::Role::Content
                {
                    return Err("Select an anchor in this room and workspace.".into());
                }
                let mut cursor = Some(anchor);
                let mut depth = 1;
                while let Some(entity) = cursor {
                    depth += 1;
                    if depth > model::MAX_DEPTH {
                        return Err("Room parenting supports at most eight levels.".into());
                    }
                    let parent = world
                        .get::<Member>(entity)
                        .and_then(|member| member.0.parent.clone());
                    cursor = parent
                        .as_ref()
                        .and_then(|id| find(world, scope(world, owner).unwrap().0, id));
                }
                Some(
                    world
                        .get::<Identity>(anchor)
                        .ok_or("The anchor identity is unavailable.")?
                        .0
                        .clone(),
                )
            } else {
                None
            };
            let owner_scope = scope(world, owner);
            let count = world
                .query_filtered::<Entity, With<Member>>()
                .iter(world)
                .filter(|entity| scope(world, *entity) == owner_scope)
                .count();
            if before.member.is_none() && count >= model::MAX_MEMBERS {
                return Err("A workspace supports at most 256 room members.".into());
            }
            let parent_pose = runtime::world_pose(world, anchor.unwrap_or(room))
                .ok_or("The parent pose is unavailable.")?;
            member = Some(model::Member {
                room: room_id,
                parent,
                role: model::Role::Content,
                local: if matches!(control, Control::Mount(..)) {
                    mount_pose(world, anchor.ok_or("Select a room anchor.")?)?
                } else {
                    runtime::relative(
                        &parent_pose,
                        &runtime::world_pose(world, owner)
                            .ok_or("The Record pose is unavailable.")?,
                    )
                },
            });
            point = None;
        }
        Control::Detach => {
            point =
                Some(runtime::world_pose(world, owner).ok_or("The Record pose is unavailable.")?);
            member = None;
        }
        Control::Apply => {
            let pose = fields(world, owner)?;
            if let Some(member) = &mut member {
                member.local = pose;
            } else if point.is_some() {
                point = Some(pose);
            } else {
                return Err("Choose a room parent first.".into());
            }
        }
        Control::ResetLocal => {
            member.as_mut().ok_or("Join a room first.")?.local = model::identity_pose()
        }
        Control::RecordDefault => {
            member = None;
            point = None;
        }
        Control::RecordUndo => {
            let (old, after) = world
                .get::<Edits>(owner)
                .and_then(|edits| edits.0.last())
                .cloned()
                .ok_or("No Record placement edit is available to undo.")?;
            if before.identity != after.identity
                || before.member != after.member
                || before.world != after.world
            {
                return Err("Record placement changed after this edit.".into());
            }
            let parent = runtime::world_pose(world, old.reference)
                .ok_or("The original Castle is unavailable.")?;
            let fallback = runtime::compose(&parent, &old.local);
            apply(world, owner, old.member, old.world);
            if world.get::<Member>(owner).is_none() && world.get::<WorldPlacement>(owner).is_none()
            {
                runtime::set_world_pose(world, owner, &fallback);
                let members = topology::groups::members(world, owner);
                if members.len() > 1 {
                    topology::groups::attach(world, &members);
                }
            }
            world.get_mut::<Edits>(owner).unwrap().0.pop();
            save(world, owner);
            return Ok(());
        }
        _ => return Err("Select a room or a placed Sand for this control.".into()),
    }
    apply(world, owner, member, point);
    let after = capture(world, owner)?;
    if world.get::<Edits>(owner).is_none() {
        world.entity_mut(owner).insert(Edits::default());
    }
    let mut edits = world.get_mut::<Edits>(owner).unwrap();
    edits.0.push((before, after));
    if edits.0.len() > 8 {
        edits.0.remove(0);
    }
    save(world, owner);
    Ok(())
}

fn apply(
    world: &mut World,
    owner: Entity,
    member: Option<model::Member>,
    point: Option<api::Pose>,
) {
    crate::native::canvas_selection::detach(world, owner);
    world.entity_mut(owner).remove::<(Member, WorldPlacement)>();
    if let Some(member) = member {
        world.entity_mut(owner).insert(Member(member));
    } else if let Some(point) = point {
        world.entity_mut(owner).insert(WorldPlacement(point));
    } else {
        crate::native::protein_area::room_default(world, owner);
    }
    super::super::synchronize(world);
}

fn save(world: &mut World, owner: Entity) {
    crate::native::record_presentation::save(world, owner);
    crate::native::workspace::request_save(world);
    invalidate(world, owner);
}
