use super::{Member, Room, runtime};
use crate::native::{
    actions::Action,
    canvas_host::{self, Identity},
    sand_panel,
    workspace::WorkspaceMember,
};
use bevy::{math::DQuat, prelude::*};
use nucleus::canvas::{self as api, room as model};

mod proxies;
mod records;

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "glam",
        author: "Cameron Hart and contributors",
        license: include_str!("../../../../licenses/native/glam-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
];

#[derive(Component)]
struct Panel {
    owner: Entity,
    signature: String,
}
#[derive(Component)]
struct Field {
    owner: Entity,
    key: u8,
}
#[derive(Component, Default)]
struct Page(usize);
#[derive(Component, Default)]
struct History(Vec<Vec<String>>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Create,
    Proxy(model::Role),
    Cleaning,
    ImportCloud,
    CancelImports,
    Inspect(Entity),
    MakeRoom,
    Attach(Entity, Option<Entity>),
    Mount(Entity, Option<Entity>),
    Detach,
    Role,
    Apply,
    Refresh,
    ResetLocal,
    ResetRoom,
    Ambient,
    Undo,
    RecordDefault,
    RecordUndo,
    Page,
}

#[derive(Component)]
struct Button(Control);

#[derive(Clone, Copy)]
enum FocusKey {
    Field(u8),
    Button(Control),
    First,
}

fn button(
    world: &mut World,
    panel: Entity,
    owner: Entity,
    caption: &str,
    action: Control,
) -> Entity {
    let entity = sand_panel::button(world, panel, owner, caption, action);
    world.entity_mut(entity).insert(Button(action));
    entity
}

fn below(world: &World, mut entity: Entity, panel: Entity) -> bool {
    for _ in 0..128 {
        if entity == panel {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

fn focused(world: &World, panel: Entity) -> Option<FocusKey> {
    let mut entity = world
        .get_resource::<bevy::input_focus::InputFocus>()?
        .get()?;
    if !below(world, entity, panel) {
        return None;
    }
    for _ in 0..128 {
        if let Some(field) = world.get::<Field>(entity) {
            return Some(FocusKey::Field(field.key));
        }
        if let Some(button) = world.get::<Button>(entity) {
            return Some(FocusKey::Button(button.0));
        }
        if entity == panel {
            break;
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
    Some(FocusKey::First)
}

fn restore_focus(world: &mut World, panel: Entity, key: FocusKey) {
    let field = if let FocusKey::Field(key) = key {
        world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(entity, field)| field.key == key && below(world, *entity, panel))
            .map(|(entity, _)| entity)
    } else {
        None
    };
    let button = field.or_else(|| {
        world
            .query::<(Entity, &Button)>()
            .iter(world)
            .find(|(entity, button)| {
                below(world, *entity, panel)
                    && match key {
                        FocusKey::Button(action) => button.0 == action,
                        _ => true,
                    }
            })
            .map(|(entity, _)| entity)
    });
    let entity = button.or_else(|| {
        world
            .query_filtered::<Entity, With<Button>>()
            .iter(world)
            .find(|entity| below(world, *entity, panel))
    });
    if let Some(entity) = entity
        && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
    {
        focus.set(entity, bevy::input_focus::FocusCause::Navigated);
    }
}

fn scope(world: &World, owner: Entity) -> Option<(Entity, u64)> {
    Some((
        world.get::<ChildOf>(owner)?.parent(),
        world.get::<WorkspaceMember>(owner)?.0,
    ))
}

fn find(world: &mut World, root: Entity, id: &str) -> Option<Entity> {
    world
        .query::<(Entity, &Identity, &ChildOf)>()
        .iter(world)
        .find(|(_, identity, parent)| identity.0 == id && parent.parent() == root)
        .map(|(entity, _, _)| entity)
}

fn transact(world: &mut World, root: Entity, mutation: api::Mutation) -> Result<String, String> {
    let revision = match canvas_host::handle(
        world,
        root,
        &api::Request::Inspect {
            workspace: None,
            selected_only: false,
            offset: 0,
            limit: 1,
        },
    )? {
        api::Response::Snapshot { snapshot } => snapshot.revision,
        _ => return Err("The room canvas is unavailable.".into()),
    };
    match canvas_host::handle(
        world,
        root,
        &api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: revision,
            mutation,
        },
    )? {
        api::Response::Receipt { receipt } => Ok(receipt.request_id),
        _ => Err("The room edit did not return a receipt.".into()),
    }
}

fn metadata(
    world: &mut World,
    root: Entity,
    owner: Entity,
) -> Result<(String, api::Metadata), String> {
    let snapshot = canvas_host::capture(world, root)?;
    let id = world
        .get::<Identity>(owner)
        .ok_or("Select a placed Sand or its producing Castle.")?
        .0
        .clone();
    let value = snapshot
        .placements
        .into_iter()
        .find(|item| item.id == id)
        .ok_or("The placement is unavailable.")?
        .metadata;
    Ok((id, value))
}

fn input(world: &mut World, owner: Entity, key: u8) -> Result<String, String> {
    let entity = world
        .query::<(Entity, &Field)>()
        .iter(world)
        .find(|(_, field)| field.owner == owner && field.key == key)
        .map(|(entity, _)| entity)
        .ok_or("Reopen the room controls before applying these fields.")?;
    sand_panel::value(world, entity)
}

fn numbers(value: &str) -> Result<[f64; 3], String> {
    value
        .split([',', ' '])
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or("Enter three finite numbers separated by commas.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "Enter exactly three numbers.".into())
}

fn invalidate(world: &mut World, owner: Entity) {
    let panels: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .filter(|(_, panel)| panel.owner == owner)
        .map(|(entity, _)| entity)
        .collect();
    for panel in panels {
        world.get_mut::<Panel>(panel).unwrap().signature.clear();
    }
}

fn perform(world: &mut World, owner: Entity, control: Control) -> Result<(), String> {
    if let Control::Inspect(target) = control {
        let (root, _) = scope(world, owner).ok_or("The room canvas is unavailable.")?;
        if scope(world, owner) != scope(world, target)
            || world
                .get::<crate::native::canvas::CanvasItem>(target)
                .is_none()
        {
            return Err("The room or Castle is no longer available in this workspace.".into());
        }
        crate::native::canvas_item::select(world, root, target);
        return Ok(());
    }
    let root = if matches!(control, Control::Create) {
        owner
    } else {
        scope(world, owner).ok_or("Select a placed Sand.")?.0
    };
    if !world
        .get::<crate::native::edit_mode::EditMode>(root)
        .is_some_and(|mode| mode.enabled)
    {
        return Err("Open Edit mode to change room placement.".into());
    }
    if super::hosted(world, owner) {
        return Err("Use the acting Actor's Workspace Sync to edit this hosted placement.".into());
    }
    if matches!(control, Control::Page | Control::Refresh) {
        if matches!(control, Control::Page) {
            let page = world.get::<Page>(owner).map_or(0, |page| page.0);
            world.entity_mut(owner).insert(Page(page.wrapping_add(1)));
        }
        invalidate(world, owner);
        return Ok(());
    }
    if world
        .get::<crate::native::protein_area::RecordBinding>(owner)
        .is_some()
    {
        return records::perform(world, owner, control);
    }
    match control {
        Control::Proxy(role) => return create_proxy(world, owner, role).map(|_| ()),
        Control::Cleaning => {
            if !world
                .get::<Member>(owner)
                .is_some_and(|member| member.0.role == model::Role::Floor)
            {
                return Err("Select a room floor proxy for cleaning effects.".into());
            }
            if world
                .get::<crate::native::shader_effects::Surface>(owner)
                .is_none()
            {
                world
                    .entity_mut(owner)
                    .insert(crate::native::shader_effects::Surface::cleaning());
                crate::native::workspace::request_save(world);
            }
            crate::native::shader_effects::OpenSurface(owner).apply(world, owner);
            return Ok(());
        }
        Control::ImportCloud => {
            let path = std::path::PathBuf::from(input(world, owner, 3)?.trim());
            return crate::native::topology::assets::import_into_room(world, owner, path);
        }
        Control::CancelImports => {
            if world.get::<Room>(owner).is_none() {
                return Err("Select a room.".into());
            }
            crate::native::topology::assets::cancel_room_imports(world, owner);
            return Ok(());
        }
        _ => {}
    }
    if matches!(control, Control::Undo) {
        let receipts = world
            .get::<History>(root)
            .and_then(|history| history.0.last())
            .cloned()
            .ok_or("No room edit is available to undo.")?;
        canvas_host::handle(
            world,
            root,
            &api::Request::Inspect {
                workspace: None,
                selected_only: false,
                offset: 0,
                limit: 1,
            },
        )?;
        let mut preview = world
            .get::<crate::native::workspace::Workspaces>(root)
            .and_then(|spaces| spaces.canvas_state.clone())
            .ok_or("The canvas undo state is unavailable.")?;
        let before = preview.snapshot.clone();
        for receipt in receipts.iter().rev() {
            preview.handle(&api::Request::Mutate {
                request_id: nucleus::new_uid("request"),
                expected_revision: preview.snapshot.revision,
                mutation: api::Mutation::UndoChanges {
                    receipt: receipt.clone(),
                },
            })?;
        }
        for item in &before.placements {
            if let Some(entity) = find(world, root, &item.id)
                && world
                    .get::<crate::native::workspace_sync::HostedReplica>(entity)
                    .is_some()
                && preview
                    .snapshot
                    .placements
                    .iter()
                    .find(|next| next.id == item.id)
                    != Some(item)
            {
                return Err(
                    "Use the acting Actor's Workspace Sync to undo hosted placement edits.".into(),
                );
            }
        }
        for receipt in receipts.into_iter().rev() {
            transact(world, root, api::Mutation::UndoChanges { receipt })?;
        }
        world.get_mut::<History>(root).unwrap().0.pop();
        return Ok(());
    }
    super::synchronize(world);
    let mut receipts = Vec::new();
    let target = if matches!(control, Control::Create) {
        let workspace = world
            .get::<crate::native::workspace::Workspaces>(root)
            .ok_or("The canvas is unavailable.")?
            .active;
        let center = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(bevy::math::DVec2::ZERO, |view| view.center);
        let id = nucleus::new_uid("placement");
        receipts.push(transact(
            world,
            root,
            api::Mutation::Add {
                placement: id.clone(),
                workspace,
                component: api::Component::Native {
                    kind: "square".into(),
                    settings: std::collections::BTreeMap::from([(
                        "appearance".into(),
                        serde_json::Value::String(
                            serde_json::to_string(&boundary_style())
                                .map_err(|error| error.to_string())?,
                        ),
                    )]),
                    bindings: Vec::new(),
                },
                geometry: api::Geometry {
                    position: center.to_array(),
                    size: [1200.0, 900.0],
                },
            },
        )?);
        find(world, root, &id).ok_or("The new room placement was not created.")?
    } else {
        owner
    };
    let (id, mut value) = metadata(world, root, target)?;
    match control {
        Control::Create | Control::MakeRoom => {
            if value.room.is_some() {
                return Err("This Sand is already a room.".into());
            }
            if value.room_member.is_some() {
                return Err("Detach this Sand before making it a room.".into());
            }
            if value.pinned.is_some() {
                return Err("Unpin this Sand from the screen before making it a room.".into());
            }
            value.room = Some(model::Room {
                home: runtime::world_pose(world, target).ok_or("The room pose is unavailable.")?,
                ..default()
            });
            if matches!(control, Control::Create) {
                value.spatial.depth = Some(2.0);
            }
        }
        Control::Attach(room, anchor) | Control::Mount(room, anchor) => {
            if value.room.is_some() {
                return Err("A room cannot be parented to another room.".into());
            }
            if value.pinned.is_some() || value.binding.is_some() || value.follow_slope {
                return Err("Release screen pinning and terrain bindings before using room-local placement.".into());
            }
            if scope(world, room) != scope(world, target) || world.get::<Room>(room).is_none() {
                return Err("Select a room in this workspace.".into());
            }
            let room_id = world
                .get::<Identity>(room)
                .ok_or("The room identity is unavailable.")?
                .0
                .clone();
            let parent = if let Some(anchor) = anchor {
                let member = world
                    .get::<Member>(anchor)
                    .ok_or("The anchor is unavailable.")?;
                if scope(world, anchor) != scope(world, target)
                    || member.0.room != room_id
                    || member.0.role == model::Role::Content
                {
                    return Err("Select an anchor in this room and workspace.".into());
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
            let parent_pose = runtime::world_pose(world, anchor.unwrap_or(room))
                .ok_or("The parent pose is unavailable.")?;
            let current =
                runtime::world_pose(world, target).ok_or("The Sand pose is unavailable.")?;
            value.room_member = Some(model::Member {
                room: room_id,
                parent,
                local: if matches!(control, Control::Mount(..)) {
                    mount_pose(
                        world,
                        anchor.ok_or("Choose a wall, desk or computer anchor.")?,
                    )?
                } else {
                    runtime::relative(&parent_pose, &current)
                },
                role: value
                    .room_member
                    .as_ref()
                    .map_or(model::Role::Content, |member| member.role),
            });
        }
        Control::Detach => value.room_member = None,
        Control::Role => {
            let member = value.room_member.as_mut().ok_or("Join a room first.")?;
            member.role = match member.role {
                model::Role::Content => model::Role::Floor,
                model::Role::Floor => model::Role::Wall,
                model::Role::Wall => model::Role::Desk,
                model::Role::Desk => model::Role::Computer,
                model::Role::Computer => model::Role::Content,
            };
        }
        Control::Apply => {
            if let Some(room) = &mut value.room {
                room.name = input(world, target, 2)?.trim().to_string();
            } else {
                let member = value.room_member.as_mut().ok_or("Join a room first.")?;
                let position = numbers(&input(world, target, 0)?)?;
                let angles = numbers(&input(world, target, 1)?)?.map(f64::to_radians);
                member.local = api::Pose {
                    position,
                    rotation: DQuat::from_euler(EulerRot::XYZ, angles[0], angles[1], angles[2])
                        .normalize()
                        .to_array(),
                };
            }
        }
        Control::ResetLocal => {
            value
                .room_member
                .as_mut()
                .ok_or("Join a room first.")?
                .local = model::identity_pose()
        }
        Control::ResetRoom => {
            let home = value.room.as_ref().ok_or("Select a room.")?.home.clone();
            receipts.push(transact(
                world,
                root,
                api::Mutation::Move {
                    placement: id.clone(),
                    workspace: scope(world, target).unwrap().1,
                    position: [home.position[0], home.position[2]],
                },
            )?);
            value.spatial.elevation = home.position[1];
            value.spatial.rotation = home.rotation;
        }
        Control::Ambient => {
            let room = value.room.as_mut().ok_or("Select a room.")?;
            room.ambient_enabled = !room.ambient_enabled;
        }
        Control::Page | Control::Refresh | Control::Undo => unreachable!(),
        Control::RecordDefault | Control::RecordUndo => {
            return Err("Select a generated Record.".into());
        }
        Control::Proxy(_)
        | Control::Cleaning
        | Control::ImportCloud
        | Control::CancelImports
        | Control::Inspect(_) => {
            unreachable!()
        }
    }
    let mutation = if matches!(
        control,
        Control::Attach(..) | Control::Mount(..) | Control::Detach | Control::ResetLocal
    ) || matches!(control, Control::Apply) && value.room_member.is_some()
    {
        let snapshot = canvas_host::capture(world, root)?;
        let selected = snapshot
            .placements
            .iter()
            .find(|item| item.id == id)
            .ok_or("The placement is unavailable.")?;
        let before =
            runtime::world_pose(world, target).ok_or("The placement pose is unavailable.")?;
        let parent = value
            .room_member
            .as_ref()
            .map(|member| {
                let parent = find(world, root, member.parent.as_ref().unwrap_or(&member.room))
                    .ok_or("The room parent is unavailable.")?;
                runtime::world_pose(world, parent).ok_or("The room parent pose is unavailable.")
            })
            .transpose()?;
        let after = value
            .room_member
            .as_ref()
            .zip(parent.as_ref())
            .map(|(member, parent)| runtime::compose(parent, &member.local));
        let mut members = std::collections::BTreeMap::new();
        for item in &snapshot.placements {
            if item.id != id
                && !(selected.group.is_some()
                    && item.group == selected.group
                    && item.workspace == selected.workspace)
            {
                continue;
            }
            let member = if let Some(template) = value.room_member.as_ref() {
                let entity =
                    find(world, root, &item.id).ok_or("A grouped placement is unavailable.")?;
                let mut point =
                    runtime::world_pose(world, entity).ok_or("A grouped pose is unavailable.")?;
                if matches!(
                    control,
                    Control::Apply | Control::ResetLocal | Control::Mount(..)
                ) {
                    point = runtime::compose(
                        after.as_ref().unwrap(),
                        &runtime::relative(&before, &point),
                    );
                }
                Some(model::Member {
                    local: runtime::relative(parent.as_ref().unwrap(), &point),
                    role: item
                        .metadata
                        .room_member
                        .as_ref()
                        .map_or(model::Role::Content, |member| member.role),
                    ..template.clone()
                })
            } else {
                None
            };
            members.insert(item.id.clone(), member);
        }
        api::Mutation::SetRoomMembers { members }
    } else {
        api::Mutation::SetMetadata {
            placement: id,
            metadata: value,
        }
    };
    match transact(world, root, mutation) {
        Ok(receipt) => receipts.push(receipt),
        Err(error) => {
            for receipt in receipts.into_iter().rev() {
                transact(world, root, api::Mutation::UndoChanges { receipt })?;
            }
            return Err(error);
        }
    }
    if world.get::<History>(root).is_none() {
        world.entity_mut(root).insert(History::default());
    }
    let mut history = world.get_mut::<History>(root).unwrap();
    history.0.push(receipts);
    if history.0.len() > 8 {
        history.0.remove(0);
    }
    if matches!(control, Control::Create)
        && let Some(mut inspection) = world.get_mut::<crate::native::inspection::Inspection>(root)
    {
        inspection.selected = Some(target);
    }
    Ok(())
}

pub fn create_proxy(world: &mut World, room: Entity, role: model::Role) -> Result<Entity, String> {
    let (root, _) = scope(world, room).ok_or("Select a placed room.")?;
    if world.get::<Room>(room).is_none() || super::hosted(world, room) {
        return Err("Choose a locally editable room.".into());
    }
    if !world
        .get::<crate::native::edit_mode::EditMode>(root)
        .is_some_and(|mode| mode.enabled)
    {
        return Err("Open Edit mode to add a room proxy.".into());
    }
    proxies::create(world, room, role)
}

fn mount_pose(world: &World, anchor: Entity) -> Result<api::Pose, String> {
    let role = world
        .get::<Member>(anchor)
        .ok_or("The room anchor is unavailable.")?
        .0
        .role;
    let clearance = match role {
        model::Role::Wall | model::Role::Floor => 12.0,
        model::Role::Desk | model::Role::Computer => 80.0,
        model::Role::Content => return Err("Choose a wall, desk or computer anchor.".into()),
    };
    Ok(api::Pose {
        position: [0.0, clearance, 0.0],
        ..model::identity_pose()
    })
}

pub fn mount(world: &mut World, owner: Entity, anchor: Entity) -> Result<(), String> {
    let (root, _) = scope(world, owner).ok_or("Select a placed Sand or Record.")?;
    if scope(world, owner) != scope(world, anchor) {
        return Err("Choose an anchor in this workspace.".into());
    }
    let id = world
        .get::<Member>(anchor)
        .ok_or("Select a room anchor.")?
        .0
        .room
        .clone();
    let room = find(world, root, &id).ok_or("The room is unavailable.")?;
    perform(world, owner, Control::Mount(room, Some(anchor)))
}

pub fn boundary_style() -> crate::native::tokens::TokenOverrides {
    use crate::native::tokens::{Token, TokenOverrides, TokenValue};
    TokenOverrides(std::collections::BTreeMap::from([
        (Token::SandBackground, TokenValue::Color([0, 0, 0, 0])),
        (Token::BorderWidth, TokenValue::Number(1.0)),
    ]))
}

impl Action for Control {
    fn practice_intent(&self) -> crate::native::actions::PracticeIntent {
        if matches!(self, Self::Inspect(_)) {
            crate::native::actions::PracticeIntent::Navigation
        } else {
            crate::native::actions::PracticeIntent::Target
        }
    }
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Err(error) = perform(world, owner, *self) {
            crate::native::notifications::report(world, "Rooms", &error);
        }
    }
}

fn suffix(id: &str) -> String {
    id.chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn anchor_caption(world: &World, entity: Entity, role: model::Role, id: &str) -> String {
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.iter() {
            if world
                .get::<crate::native::sand_text::SandText>(child)
                .is_none()
            {
                continue;
            }
            let value = world
                .get::<bevy::text::EditableText>(child)
                .map(|text| text.value().chars().take(40).collect::<String>())
                .or_else(|| {
                    world
                        .get::<Text>(child)
                        .map(|text| text.0.chars().take(40).collect::<String>())
                });
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                return format!(
                    "{role:?} · {}",
                    value.trim().chars().take(40).collect::<String>()
                );
            }
        }
    }
    format!("{role:?} · {}", suffix(id))
}

fn choices(world: &mut World, owner: Entity) -> Vec<(Entity, String, Option<Entity>)> {
    let Some(scope) = scope(world, owner) else {
        return Vec::new();
    };
    let mut rooms: Vec<_> = world
        .query::<(Entity, &Room, &Identity, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(_, _, _, parent, workspace)| (parent.parent(), workspace.0) == scope)
        .map(|(entity, room, id, _, _)| {
            (entity, format!("{} · {}", room.0.name, suffix(&id.0)), None)
        })
        .collect();
    let available: std::collections::BTreeMap<_, _> = rooms
        .iter()
        .filter_map(|(entity, name, _)| {
            Some((
                world.get::<Identity>(*entity)?.0.clone(),
                (*entity, name.clone()),
            ))
        })
        .collect();
    {
        let anchors: Vec<_> = world
            .query::<(Entity, &Member, &Identity, &ChildOf, &WorkspaceMember)>()
            .iter(world)
            .filter(|(entity, anchor, _, parent, workspace)| {
                *entity != owner
                    && available.contains_key(&anchor.0.room)
                    && anchor.0.role != model::Role::Content
                    && (parent.parent(), workspace.0) == scope
            })
            .map(|(entity, anchor, id, _, _)| {
                (
                    available[&anchor.0.room].0,
                    format!(
                        "{} / {}",
                        available[&anchor.0.room].1,
                        anchor_caption(world, entity, anchor.0.role, &id.0)
                    ),
                    Some(entity),
                )
            })
            .collect();
        rooms.extend(anchors);
    }
    rooms.sort_by(|a, b| a.1.cmp(&b.1));
    rooms
}

fn signature(world: &mut World, owner: Entity) -> String {
    let room = world.get::<Room>(owner).map(|room| room.0.clone());
    let member = world.get::<Member>(owner).map(|member| {
        (
            member.0.room.clone(),
            member.0.parent.clone(),
            member.0.role,
        )
    });
    let page = world.get::<Page>(owner).map_or(0, |page| page.0);
    let history = scope(world, owner)
        .and_then(|(root, _)| world.get::<History>(root))
        .map_or(0, |history| history.0.len());
    format!(
        "{room:?}:{member:?}:{page}:{history}:{:?}:{:?}:{}",
        choices(world, owner),
        world.get::<super::WorldPlacement>(owner).is_some(),
        records::has_undo(world, owner)
    )
}

fn label(world: &mut World, panel: Entity, text: &str) {
    crate::native::edit_mode::label(world, panel, text, 13.0);
}

fn fill(world: &mut World, panel: Entity, owner: Entity) {
    label(world, panel, "Room and local placement");
    crate::native::sand_panel::credits(world, panel, panel, CREDITS);
    if let Some(member) = world.get::<Member>(owner).cloned()
        && let Some((root, _)) = scope(world, owner)
        && let Some(room) = find(world, root, &member.0.room)
    {
        button(world, panel, owner, "Inspect room", Control::Inspect(room));
    }
    if let Some(source) = world
        .get::<crate::native::protein_area::RecordBinding>(owner)
        .map(|binding| binding.area)
    {
        button(
            world,
            panel,
            owner,
            "Inspect producing Castle",
            Control::Inspect(source),
        );
    }
    if super::hosted(world, owner) {
        label(
            world,
            panel,
            "Hosted placement · edit through Workspace Sync",
        );
        return;
    }
    if let Some(room) = world.get::<Room>(owner).cloned() {
        let field = sand_panel::field(world, panel, "Room name", &room.0.name);
        world.entity_mut(field).insert(Field { owner, key: 2 });
        button(world, panel, owner, "Apply room name", Control::Apply);
        label(world, panel, "Room appearance");
        let field = sand_panel::field(world, panel, "Gaussian room file (.gcloud)", "");
        world.entity_mut(field).insert(Field { owner, key: 3 });
        button(
            world,
            panel,
            owner,
            "Import Gaussian into room",
            Control::ImportCloud,
        );
        button(
            world,
            panel,
            owner,
            "Cancel this room's imports",
            Control::CancelImports,
        );
        label(
            world,
            panel,
            "Import preserves source coordinates. Select the imported Sand to edit local XYZ, rotation and scale.",
        );
        label(
            world,
            panel,
            "Proxies provide editable surfaces and collision; the Gaussian supplies appearance.",
        );
        for (caption, role) in [
            ("Add floor proxy", model::Role::Floor),
            ("Add wall proxy", model::Role::Wall),
            ("Add desk anchor", model::Role::Desk),
            ("Add computer anchor", model::Role::Computer),
        ] {
            button(world, panel, owner, caption, Control::Proxy(role));
        }
        button(
            world,
            panel,
            owner,
            "Reset room placement",
            Control::ResetRoom,
        );
        button(
            world,
            panel,
            owner,
            if room.0.ambient_enabled {
                "Disable room ambient effects"
            } else {
                "Enable room ambient effects"
            },
            Control::Ambient,
        );
    } else {
        let record = world
            .get::<crate::native::protein_area::RecordBinding>(owner)
            .is_some();
        let member = world.get::<Member>(owner).cloned();
        let pose = member
            .as_ref()
            .map(|member| member.0.local.clone())
            .or_else(|| {
                world
                    .get::<super::WorldPlacement>(owner)
                    .map(|pose| pose.0.clone())
            });
        if let Some(pose) = pose {
            if let Some(member) = &member
                && !record
            {
                label(world, panel, &format!("Anchor role: {:?}", member.0.role));
                button(world, panel, owner, "Next room anchor role", Control::Role);
                if member.0.role == model::Role::Floor {
                    button(
                        world,
                        panel,
                        owner,
                        "Configure cleaning effect",
                        Control::Cleaning,
                    );
                    label(
                        world,
                        panel,
                        "Choose a cleaning Record and source in the surface editor, then explicitly set task/progress start and end. Missing data stays clean. Reset and Disable remain in the surface controls.",
                    );
                }
            }
            let angles = DQuat::from_array(pose.rotation).to_euler(EulerRot::XYZ);
            for (key, caption, values) in [
                (
                    0,
                    if member.is_some() {
                        "Local position XYZ"
                    } else {
                        "World position XYZ"
                    },
                    pose.position,
                ),
                (
                    1,
                    if member.is_some() {
                        "Local rotation XYZ degrees"
                    } else {
                        "World rotation XYZ degrees"
                    },
                    [
                        angles.0.to_degrees(),
                        angles.1.to_degrees(),
                        angles.2.to_degrees(),
                    ],
                ),
            ] {
                let field = sand_panel::field(
                    world,
                    panel,
                    caption,
                    &values.map(|value| value.to_string()).join(", "),
                );
                world.entity_mut(field).insert(Field { owner, key });
            }
            for (caption, action) in [
                ("Apply placement", Control::Apply),
                ("Refresh placement fields", Control::Refresh),
            ] {
                button(world, panel, owner, caption, action);
            }
            if member.is_some() {
                button(
                    world,
                    panel,
                    owner,
                    "Reset local placement",
                    Control::ResetLocal,
                );
                button(world, panel, owner, "Detach from room", Control::Detach);
            }
        } else if !record {
            button(
                world,
                panel,
                owner,
                "Make this Sand a room",
                Control::MakeRoom,
            );
        } else if super::frame(world, owner).is_some() {
            label(world, panel, "Placement follows the producing Castle");
            button(world, panel, owner, "Detach from room", Control::Detach);
        }
        if record {
            button(
                world,
                panel,
                owner,
                "Use Castle placement",
                Control::RecordDefault,
            );
            if records::has_undo(world, owner) {
                button(
                    world,
                    panel,
                    owner,
                    "Undo Record placement",
                    Control::RecordUndo,
                );
            }
        }
        let choices = choices(world, owner);
        if !choices.is_empty() {
            label(
                world,
                panel,
                "Parent keeps the current position. Align moves this Sand and its group onto the anchor. Adjust local XYZ and rotation afterwards.",
            );
        }
        let page = world.get::<Page>(owner).map_or(0, |page| page.0);
        let pages = choices.len().div_ceil(8).max(1);
        for (room, caption, anchor) in choices.iter().skip((page % pages) * 8).take(8) {
            button(
                world,
                panel,
                owner,
                &format!("Parent to {caption}"),
                Control::Attach(*room, *anchor),
            );
            if anchor.is_some() {
                button(
                    world,
                    panel,
                    owner,
                    &format!("Align with {caption}"),
                    Control::Mount(*room, *anchor),
                );
            }
        }
        if pages > 1 {
            button(world, panel, owner, "More room parents", Control::Page);
        }
    }
    if world
        .get::<crate::native::protein_area::RecordBinding>(owner)
        .is_none()
        && scope(world, owner)
            .and_then(|(root, _)| world.get::<History>(root))
            .is_some_and(|history| !history.0.is_empty())
    {
        button(world, panel, owner, "Undo room edit", Control::Undo);
    }
}

pub(crate) fn controls(world: &mut World, parent: Entity, owner: Entity) {
    let signature = signature(world, owner);
    let panel = sand_panel::column(world, parent);
    world.entity_mut(panel).insert(Panel { owner, signature });
    fill(world, panel, owner);
}

pub(crate) fn store_controls(world: &mut World, root: Entity, parent: Entity) {
    button(world, parent, root, "Create room", Control::Create);
}

pub(crate) fn update_controls(world: &mut World) {
    let panels: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .map(|(entity, panel)| (entity, panel.owner, panel.signature.clone()))
        .collect();
    for (panel, owner, old) in panels {
        if world.get_entity(owner).is_err() {
            continue;
        }
        let signature = signature(world, owner);
        if signature == old {
            continue;
        }
        let focus = focused(world, panel);
        sand_panel::clear(world, panel);
        fill(world, panel, owner);
        world.get_mut::<Panel>(panel).unwrap().signature = signature;
        if let Some(focus) = focus {
            restore_focus(world, panel, focus);
        }
    }
}

#[cfg(test)]
mod tests;
