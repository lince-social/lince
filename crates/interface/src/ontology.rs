#[cfg(test)]
mod tests;
mod ui;

use crate::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::Value;
use std::collections::HashSet;

const SOURCES: [protein::Source; 4] = [
    protein::Source::Lingua,
    protein::Source::Concept,
    protein::Source::Record,
    protein::Source::Assertion,
];
const PAGE: usize = 30;

#[derive(Component)]
pub struct OntologySand {
    editor: Entity,
    list: Entity,
    status: Entity,
    tab: usize,
    page: usize,
    name: Entity,
    choices: [String; 6],
    choice_pages: [usize; 6],
    choosers: Vec<(Entity, usize)>,
    rows: [Vec<Value>; 4],
    ids: [String; 4],
    requested: [bool; 4],
    ready: [bool; 4],
    pending: Option<String>,
    dirty: bool,
}

#[derive(Resource, Default)]
struct Subscriptions(HashSet<String>);

pub struct OntologyPlugin;
impl Plugin for OntologyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Subscriptions>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Subscriptions>();
    ui::populate(world, sand);
    sand
}

#[derive(Clone)]
enum Command {
    Tab(usize),
    Choose(usize, String),
    ChoicePage(usize, bool),
    Page(bool),
    Refresh,
    Create,
    Rename,
    Delete,
    Include(bool),
    Parent(bool),
    Assert,
    Identity,
    Retract(String),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<OntologySand>(owner) else {
            return;
        };
        let status = view.status;
        if view.pending.is_some() {
            panel::status(world, status, "Wait for the current change to finish");
            return;
        }
        match self {
            Self::ChoicePage(slot, next) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                let page = &mut view.choice_pages[*slot];
                *page = if *next {
                    page.saturating_add(1)
                } else {
                    page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::Tab(tab) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.tab = *tab;
                view.page = 0;
                view.dirty = true;
                ui::editor(world, owner);
            }
            Self::Choose(slot, uid) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.choices[*slot] = uid.clone();
                view.dirty = true;
                if *slot == view.tab && *slot < 2 {
                    let name = view.name;
                    let value = view.rows[*slot]
                        .iter()
                        .find(|row| row["uid"] == *uid)
                        .and_then(|row| row["name"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(name) {
                        text.editor.set_text(&value);
                    }
                }
            }
            Self::Page(next) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.page = if *next {
                    view.page.saturating_add(1)
                } else {
                    view.page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::Refresh => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.requested = [false; 4];
                view.ready = [false; 4];
                panel::status(world, status, "Refreshing…");
            }
            _ => {
                let result = mutation(world, owner, self).and_then(|action| {
                    let id = nucleus::new_uid("ontology-action");
                    panel::send(
                        world,
                        ClientMessage::Act {
                            id: id.clone(),
                            action,
                        },
                    )?;
                    world.get_mut::<OntologySand>(owner).unwrap().pending = Some(id);
                    Ok(())
                });
                panel::status(
                    world,
                    status,
                    result.err().unwrap_or_else(|| "Saving…".into()),
                );
            }
        }
    }
}

fn selected(view: &OntologySand, slot: usize) -> Result<String, String> {
    let source = match slot {
        0 => 0,
        1 | 2 => 1,
        3 | 4 => 2,
        _ => return Err("Invalid selection".into()),
    };
    let uid = &view.choices[slot];
    view.rows[source]
        .iter()
        .any(|row| row["uid"].as_str() == Some(uid))
        .then(|| uid.clone())
        .ok_or_else(|| "Choose an existing entry first".into())
}

fn mutation(
    world: &World,
    owner: Entity,
    command: &Command,
) -> Result<engine::actions::Action, String> {
    use engine::actions::Action as Backend;
    let view = world
        .get::<OntologySand>(owner)
        .ok_or("Ontology is closed")?;
    if !view.ready.iter().all(|ready| *ready) {
        return Err("Wait for the ontology to load, or refresh after an error".into());
    }
    let name = || {
        let name = panel::value(world, view.name)?.trim().to_owned();
        if name.is_empty() || name.chars().count() > 256 {
            Err("Enter a name of 1–256 characters".to_owned())
        } else {
            Ok(name)
        }
    };
    Ok(match command {
        Command::Create if view.tab == 0 => Backend::CreateLingua {
            name: name()?,
            visibility: view.choices[5].clone(),
        },
        Command::Create => Backend::CreateConcept {
            lingua: selected(view, 0)?,
            name: name()?,
            parents: vec![],
        },
        Command::Rename if view.tab == 0 => Backend::RenameLingua {
            lingua: selected(view, 0)?,
            name: name()?,
        },
        Command::Rename => Backend::RenameConcept {
            concept: selected(view, 1)?,
            name: name()?,
        },
        Command::Delete if view.tab == 0 => {
            let lingua = selected(view, 0)?;
            if lingua == "g_local" {
                return Err("The local Lingua is permanent".into());
            }
            Backend::DeleteLingua { lingua }
        }
        Command::Delete => Backend::DeleteConcept {
            concept: selected(view, 1)?,
        },
        Command::Include(true) => Backend::AdoptConcept {
            lingua: selected(view, 0)?,
            concept: selected(view, 1)?,
        },
        Command::Include(false) => Backend::RemoveConceptFromLingua {
            lingua: selected(view, 0)?,
            concept: selected(view, 1)?,
        },
        Command::Parent(true) => Backend::AddConceptParent {
            concept: selected(view, 1)?,
            parent: selected(view, 2)?,
        },
        Command::Parent(false) => Backend::RemoveConceptParent {
            concept: selected(view, 1)?,
            parent: selected(view, 2)?,
        },
        Command::Assert => Backend::AssertRecord {
            subject: selected(view, 3)?,
            predicate: selected(view, 1)?,
            object: if view.choices[4].is_empty() {
                None
            } else {
                Some(selected(view, 4)?)
            },
            quantity: None,
            unit: None,
        },
        Command::Identity => Backend::SetIdentity {
            subject: selected(view, 3)?,
            predicate: Some(selected(view, 1)?),
        },
        Command::Retract(uid) => {
            if !view.rows[3].iter().any(|row| row["uid"] == *uid) {
                return Err("This assertion is no longer available".into());
            }
            Backend::RetractAssertion {
                assertion: uid.clone(),
            }
        }
        _ => return Err("Choose an operation".into()),
    })
}

fn query(source: protein::Source) -> protein::Protein {
    protein::Protein {
        source,
        filter: vec![],
        fields: (source == protein::Source::Record)
            .then(|| ["uid", "head", "slug"].map(str::to_owned).into()),
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
    let view = world.get::<OntologySand>(owner).unwrap();
    let status = view.status;
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if view.ids.contains(id) =>
        {
            let index = view
                .ids
                .iter()
                .position(|candidate| candidate == id)
                .unwrap();
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            view.rows[index] = rows.clone();
            view.ready[index] = true;
            view.dirty = true;
            if view.ready.iter().all(|ready| *ready) && view.pending.is_none() {
                panel::status(world, status, "Live");
            }
        }
        ServerMessage::ActionOk { id, warnings, .. } if view.pending.as_ref() == Some(id) => {
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            view.pending = None;
            view.requested = [false; 4];
            view.ready = [false; 4];
            panel::status(
                world,
                status,
                if warnings.is_empty() {
                    "Saved".into()
                } else {
                    format!("Saved · {}", warnings.join(" · "))
                },
            );
        }
        ServerMessage::Error { id, message, .. }
            if view.ids.contains(id)
                || view.pending.as_ref() == Some(id)
                || id == crate::cell_bridge::CONNECTION =>
        {
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            if let Some(index) = view.ids.iter().position(|candidate| candidate == id) {
                view.ready[index] = false;
            }
            if id == crate::cell_bridge::CONNECTION {
                view.ready = [false; 4];
            }
            if view.pending.as_ref() == Some(id) || id == crate::cell_bridge::CONNECTION {
                view.pending = None;
            }
            panel::status(world, status, message);
        }
        _ => {}
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<OntologySand>>()
        .iter(world)
        .collect();
    let active: HashSet<_> = owners
        .iter()
        .flat_map(|owner| world.get::<OntologySand>(*owner).unwrap().ids.clone())
        .collect();
    let stale: Vec<_> = world
        .resource::<Subscriptions>()
        .0
        .difference(&active)
        .cloned()
        .collect();
    for id in stale {
        if panel::send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Subscriptions>().0.remove(&id);
        }
    }
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for owner in owners {
        for message in &messages {
            receive(world, owner, message);
        }
        for (index, source) in SOURCES.iter().enumerate() {
            let view = world.get::<OntologySand>(owner).unwrap();
            if !view.requested[index] && !crate::laboratory::active(world) {
                let id = view.ids[index].clone();
                if panel::send(
                    world,
                    ClientMessage::Subscribe {
                        id: id.clone(),
                        protein: query(*source),
                    },
                )
                .is_ok()
                {
                    world.resource_mut::<Subscriptions>().0.insert(id);
                    world.get_mut::<OntologySand>(owner).unwrap().requested[index] = true;
                }
            }
        }
        if world.get::<OntologySand>(owner).unwrap().dirty {
            ui::render(world, owner);
        }
    }
}
