mod guide;
mod highlight;
mod observation;
pub(crate) mod tests;
mod view;

pub use highlight::TutorialHighlight;

use crate::{
    actions::Action,
    area::{AreaForces, InfluenceArea, RecordProperties},
    canvas::CanvasItem,
    protein_area::RecordBinding,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};

pub struct TutorialPlugin;
impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update.after(crate::protein_area::UpdateProteinAreas),
        );
        app.add_systems(
            PostUpdate,
            highlight::position.after(bevy::ui::UiSystems::PostLayout),
        );
    }
}

#[derive(Component)]
pub struct Tutorial;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TutorialField {
    Query(Entity, String),
    Quantity(Entity, bool),
}

#[derive(Component)]
struct Session {
    workspace: u64,
    existing: Vec<Entity>,
    sample_prefix: String,
    shell: Entity,
    content: Entity,
    step: usize,
    unlocked: usize,
    records: Vec<String>,
    request: Option<String>,
    error: Option<String>,
    spawn: Option<Entity>,
    force: Option<Entity>,
    change: Option<Entity>,
    status: Entity,
    next: Entity,
    entered: bool,
    left: bool,
    completed: bool,
    hidden: bool,
}

#[derive(Clone)]
pub struct Start;
impl Action for Start {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        let mut root = owner;
        while world.get::<Workspaces>(root).is_none() {
            let Some(parent) = world.get::<ChildOf>(root) else {
                return;
            };
            root = parent.parent();
        }
        let workspace = world.get::<Workspaces>(root).unwrap().active;
        if let Some(session) = world.get::<Session>(root) {
            let workspace = session.workspace;
            if world
                .get::<Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.id == workspace)
            {
                crate::workspace::switch(world, root, workspace);
                let mut session = world.get_mut::<Session>(root).unwrap();
                session.hidden = false;
                return;
            }
            let shell = world.get::<Session>(root).unwrap().shell;
            world.despawn(shell);
            world.entity_mut(root).remove::<Session>();
        }
        let existing = world
            .query_filtered::<Entity, With<InfluenceArea>>()
            .iter(world)
            .collect();
        let sample_prefix = format!(
            "Area lesson {:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let shell = crate::fiote::spawn(world, root);
        let mut node = world.get_mut::<Node>(shell).unwrap();
        node.right = Val::Auto;
        node.left = px(16);
        node.max_width = percent(48);
        let content = world.get::<crate::fiote::Fiote>(shell).unwrap().bubble;
        let content = world
            .spawn((
                Tutorial,
                Node {
                    width: px(330),
                    max_width: percent(100),
                    max_height: Val::Vh(72.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                },
                ChildOf(content),
            ))
            .id();
        world.entity_mut(root).insert(Session {
            workspace,
            existing,
            sample_prefix,
            shell,
            content,
            step: 0,
            unlocked: 0,
            records: Vec::new(),
            request: None,
            error: None,
            spawn: None,
            force: None,
            change: None,
            status: Entity::PLACEHOLDER,
            next: Entity::PLACEHOLDER,
            entered: false,
            left: false,
            completed: false,
            hidden: false,
        });
        request_sample(world, root);
        view::render(world, root);
    }
}

fn request_sample(world: &mut World, root: Entity) {
    let session = world.get::<Session>(root).unwrap();
    if session.request.is_some() || session.records.len() == 2 {
        return;
    }
    let index = session.records.len();
    let id = format!("tutorial-{}-{}-{index}", root.to_bits(), session.workspace);
    let action = engine::actions::Action::CreateRecord {
        slug: None,
        kind: nucleus::RecordKind::Plain,
        head: format!("{} {}", session.sample_prefix, index + 1),
        body: "A sample Record for the **Areas of Influence** tutorial.".into(),
        quantity: 0.0,
    };
    let result = world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .ok_or("The local Organ is not connected. Reconnect, then retry.")
        .and_then(|bridge| {
            bridge
                .outgoing
                .try_send(ClientMessage::Act {
                    id: id.clone(),
                    action,
                })
                .map_err(|_| "The local Organ is busy or disconnected. Retry when connected.")
        });
    let mut session = world.get_mut::<Session>(root).unwrap();
    match result {
        Ok(()) => {
            session.request = Some(id);
            session.error = None;
        }
        Err(error) => session.error = Some(error.into()),
    }
}

#[derive(Clone, Copy)]
enum Command {
    Retry,
    CopySample,
    Step(usize),
    Next,
    Close,
}
impl Action for Command {
    fn apply(&self, world: &mut World, root: Entity) {
        let Some(session) = world.get::<Session>(root) else {
            return;
        };
        if matches!(self, Self::Close) {
            world.get_mut::<Session>(root).unwrap().hidden = true;
            highlight::clear(world, root);
            return;
        }
        if world.get::<Workspaces>(root).unwrap().active != session.workspace {
            return;
        }
        match *self {
            Self::CopySample => {
                let text = if session.step == 0 {
                    session.sample_prefix.clone()
                } else {
                    format!("{} 1", session.sample_prefix)
                };
                let result = world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .map(|mut clipboard| clipboard.set_text(text));
                crate::notifications::report(
                    world,
                    "Tutorial",
                    if matches!(result, Some(Ok(()))) {
                        "Copied. Paste this text into the field shown in the instructions."
                    } else {
                        "Could not copy. Type the lesson text shown in the instructions."
                    },
                );
                return;
            }
            Self::Retry => {
                world.get_mut::<Session>(root).unwrap().error = None;
                request_sample(world, root);
            }
            Self::Step(step) if step <= session.unlocked && step < 5 => {
                world.get_mut::<Session>(root).unwrap().step = step
            }
            Self::Next => {
                if verify(world, root).is_ok() {
                    let mut session = world.get_mut::<Session>(root).unwrap();
                    if session.step == 4 {
                        session.completed = true;
                    } else {
                        session.step += 1;
                        session.unlocked = session.unlocked.max(session.step);
                    }
                }
            }
            _ => {}
        }
        view::render(world, root);
    }
}

fn rows(world: &mut World, root: Entity) -> Vec<(Entity, String, DVec2, serde_json::Value)> {
    let session = world.get::<Session>(root).unwrap();
    let (workspace, source, records) = (session.workspace, session.spawn, session.records.clone());
    world
        .query::<(
            Entity,
            &RecordBinding,
            &CanvasItem,
            &WorkspaceMember,
            &ChildOf,
            &RecordProperties,
        )>()
        .iter(world)
        .filter(|(_, binding, _, member, parent, _)| {
            Some(binding.area) == source
                && binding.source == crate::protein_area::Source::Local
                && member.0 == workspace
                && parent.parent() == root
                && records.contains(&binding.uid)
        })
        .map(|(entity, binding, item, _, _, record)| {
            (entity, binding.uid.clone(), item.position, record.0.clone())
        })
        .collect()
}

fn owned(world: &World, root: Entity, entity: Option<Entity>) -> Option<&InfluenceArea> {
    let entity = entity?;
    let session = world.get::<Session>(root)?;
    if world.get::<ChildOf>(entity)?.parent() != root
        || world.get::<WorkspaceMember>(entity)?.0 != session.workspace
    {
        return None;
    }
    world
        .get::<InfluenceArea>(entity)
        .filter(|area| area.enabled)
}

fn verify(world: &mut World, root: Entity) -> Result<String, String> {
    observation::selected(world, root);
    let session = world
        .get::<Session>(root)
        .ok_or("Open the tutorial again.")?;
    if let Some(error) = &session.error {
        return Err(error.clone());
    }
    if session.records.len() != 2 {
        return Err("Waiting for the local Organ to create two sample Records…".into());
    }
    let step = session.step;
    let (spawn, force, change, workspace, entered, left) = (
        session.spawn,
        session.force,
        session.change,
        session.workspace,
        session.entered,
        session.left,
    );
    if world.get::<Workspaces>(root).unwrap().active != workspace {
        return Err("Return to the workspace where you started this tutorial.".into());
    }
    let sands = rows(world, root);
    if step == 0 {
        let area = owned(world, root, spawn).ok_or(
            "Click Edit mode > Areas of influence > Add square. Keep the new area selected.",
        )?;
        let config = area
            .protein
            .as_ref()
            .ok_or("In the selected area's Protein section, click + (Make this a Protein Area).")?;
        if !config.enabled {
            return Err("In the Protein query Castle, finish the Text contains condition, then click Run to apply it to the area.".into());
        }
        if config.source != crate::protein_area::Source::Local {
            return Err("Choose the local Organ for the practice Records.".into());
        }
        if !["head", "body", "quantity"].iter().all(|key| {
            config
                .bindings
                .iter()
                .any(|binding| binding.property == *key)
        }) {
            return Err("Under Row template, use Add property (+) to include Title, Description and Quantity. Quantity lets you see the saved changes in steps 4–5.".into());
        }
        let ready =
            crate::protein_area::ordered_records(world, spawn.unwrap()).is_some_and(|(_, ids)| {
                ids.len() == 2
                    && world
                        .get::<Session>(root)
                        .unwrap()
                        .records
                        .iter()
                        .all(|uid| ids.contains(uid))
            });
        if !ready || sands.len() != 2 {
            return Err(format!(
                "Click the Protein pencil. In Filters, add Text contains with the lesson text above. Both sample Records must appear, with no other Records. {}",
                crate::protein_area::calendar_status(world, spawn.unwrap())
            ));
        }
        return Ok("Both Records are visible with Title and Description.".into());
    }
    if sands.is_empty() {
        return Err(
            "The practice Sands are missing. Return to step 1 and enable their Protein.".into(),
        );
    }
    if step < 3 {
        let area = owned(world, root, force).ok_or(
            "In Areas of influence, click Add circle to create and select a separate force area.",
        )?;
        observation::matching(world, root, area)?;
        let direction = if step == 1 {
            crate::area::Direction::Attract
        } else {
            crate::area::Direction::Repel
        };
        if !area.attraction_enabled || area.strength <= 0.0 {
            return Err("Select the force area, enable Attraction, then raise the Strength slider above zero.".into());
        }
        if area.direction != direction {
            return Err(format!(
                "Set the force direction to {}.",
                if step == 1 { "Attract" } else { "Repel" }
            ));
        }
        if !crate::workspace_config::enabled(world, root, workspace) {
            return Err("At the top of Areas of influence, turn on workspace Physics.".into());
        }
        let target = area.target_position();
        let sign = if step == 1 { 1.0 } else { -1.0 };
        if !sands.iter().any(|(entity, _, position, _)| {
            world.get::<AreaForces>(*entity).is_some_and(|forces| {
                forces.0.iter().any(|entry| {
                    Some(entry.area) == force
                        && entry.force.length_squared() > 0.0001
                        && entry.force.dot(target - *position) * sign > 0.0
                })
            })
        }) {
            return Err("Move a practice Sand within reach of the force area and away from its center. Waiting for the physics force…".into());
        }
        return Ok(if step == 1 {
            "The area is pulling a practice Sand toward it."
        } else {
            "The area is pushing a practice Sand away."
        }
        .into());
    }
    let area = owned(world, root, change)
        .ok_or("Click Add square for a separate property area. Under Filter, add a Title rule for sample 1.")?;
    observation::matching(world, root, area)?;
    if area.changes.enter.quantity.as_deref() != Some("1")
        || area.changes.leave.quantity.as_deref() != Some("0")
    {
        return Err("Set entry Quantity to 1 and exit Quantity to 0.".into());
    }
    if !area.changes_enabled || !crate::area_mutation::armed(world, change.unwrap()) {
        let status = world
            .get::<crate::area_mutation::MutationStatus>(change.unwrap())
            .map(|status| status.0.as_str())
            .unwrap_or_default();
        return Err(format!(
            "{status} Keep sample 1 outside the square, then switch Change properties off and on to resume. Wait for each change to finish before crossing again."
        ));
    }
    let expected = if step == 3 { 1.0 } else { 0.0 };
    let applied = if step == 3 { entered } else { left };
    if !applied
        || !sands.iter().any(|(_, uid, position, value)| {
            uid == &world.get::<Session>(root).unwrap().records[0]
                && area.contains(*position) == (step == 3)
                && value["quantity"]
                    .as_str()
                    .and_then(|value| value.parse::<f64>().ok())
                    .or_else(|| value["quantity"].as_f64())
                    == Some(expected)
        })
    {
        let status = world
            .get::<crate::area_mutation::MutationStatus>(change.unwrap())
            .map(|status| status.0.as_str())
            .unwrap_or_default();
        return Err(format!(
            "Move sample 1 {} the property area and wait for Quantity {expected} to be saved. {status}",
            if step == 3 { "inside" } else { "outside" }
        ));
    }
    Ok(format!(
        "The Organ confirmed the {} change. Sample 1 now has Quantity {expected}.",
        if step == 3 { "entry" } else { "exit" }
    ))
}

fn update(
    world: &mut World,
    mut messages: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
    mut transitions: Local<
        bevy::ecs::message::MessageCursor<crate::area_mutation::TransitionApplied>,
    >,
) {
    let events: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|events| messages.read(events).map(|event| event.0.clone()).collect())
        .unwrap_or_default();
    let changes: Vec<_> = world
        .get_resource::<Messages<crate::area_mutation::TransitionApplied>>()
        .map(|events| transitions.read(events).cloned().collect())
        .unwrap_or_default();
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Session>>()
        .iter(world)
        .collect();
    for root in roots {
        observation::selected(world, root);
        let mut redraw = false;
        for event in &events {
            let request = world.get::<Session>(root).unwrap().request.clone();
            match event {
                ServerMessage::ActionOk { id, created, .. } if Some(id) == request.as_ref() => {
                    let mut session = world.get_mut::<Session>(root).unwrap();
                    session.request = None;
                    if let Some(uid) = created {
                        session.records.push(uid.clone());
                    } else {
                        session.error =
                            Some("The Organ did not return the sample Record. Retry.".into());
                    }
                    redraw = true;
                    if world.get::<Session>(root).unwrap().error.is_none() {
                        request_sample(world, root);
                    }
                }
                ServerMessage::Error { id, message, .. } if Some(id) == request.as_ref() => {
                    let mut session = world.get_mut::<Session>(root).unwrap();
                    session.request = None;
                    session.error = Some(format!("Could not create the sample Record: {message}"));
                    redraw = true;
                }
                _ => {}
            }
        }
        for event in &changes {
            let mut session = world.get_mut::<Session>(root).unwrap();
            if Some(event.area) == session.change && session.records.first() == Some(&event.record)
            {
                if event.inside {
                    session.entered = true;
                    session.left = false;
                } else if session.entered {
                    session.left = true;
                }
            }
        }
        if redraw {
            view::render(world, root);
        }
        let session = world.get::<Session>(root).unwrap();
        let (shell, status, next) = (session.shell, session.status, session.next);
        let visible = !session.hidden
            && world
                .get::<Workspaces>(root)
                .is_some_and(|spaces| spaces.active == session.workspace);
        let display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(mut node) = world.get_mut::<Node>(shell)
            && node.display != display
        {
            node.display = display;
        }
        if !visible {
            highlight::clear(world, root);
            continue;
        }
        let result = if world.get::<Session>(root).unwrap().completed {
            Ok("All five steps verified.".into())
        } else {
            verify(world, root)
        };
        let disabled = world.get::<bevy::ui::InteractionDisabled>(next).is_some();
        guide::update(world, root, result.is_ok());
        if result.is_ok() && disabled {
            world
                .entity_mut(next)
                .remove::<bevy::ui::InteractionDisabled>();
        } else if result.is_err() && !disabled {
            world.entity_mut(next).insert(bevy::ui::InteractionDisabled);
        }
        let value = result.unwrap_or_else(|error| error);
        if let Some(mut text) = world.get_mut::<Text>(status)
            && text.0 != value
        {
            text.0 = value;
        }
    }
}
