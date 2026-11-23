//! Model documents in Edit → Workspaces. State belongs to the desktop workspace, not the panel.
pub(crate) mod accepted;
pub(crate) mod bridge;
mod library;

use super::label;
use crate::native::{
    actions::Action,
    sand_panel::{column, field, status, value},
    workspace::Workspaces,
};
use bevy::prelude::*;
use nucleus::{
    canvas::scene::{Handle, PART_ORIGIN, Scene},
    karma::{CanonicalHash, DecimalValue},
};
use std::{collections::BTreeMap, path::PathBuf};
use store::workspace_scene;

#[derive(Component, Default)]
struct Documents(BTreeMap<u64, Session>);

#[derive(Default)]
struct Session {
    accepted: accepted::State,
    library: library::State,
    directory: PathBuf,
    scene: Option<Scene>,
    revision: Option<CanonicalHash>,
    selected: Option<Handle>,
    undo: Vec<Scene>,
    redo: Vec<Scene>,
    dirty: bool,
    collapsed: bool,
    message: String,
}

#[derive(Component, Clone, Copy)]
struct Form {
    root: Entity,
    workspace: u64,
    section: Entity,
    path: Entity,
    status: Entity,
    name: Option<Entity>,
    location: Option<[Entity; 3]>,
    record: Option<Entity>,
    request_id: Option<Entity>,
    source_directory: Option<Entity>,
}

#[derive(Clone)]
enum Command {
    Accepted(accepted::Action),
    Library(library::Action),
    Example,
    New,
    Open,
    Save,
    SaveCopy,
    Add,
    Place,
    Select(Handle),
    Apply,
    Bind,
    Undo,
    Redo,
}

fn button(world: &mut World, page: Entity, caption: &str, action: impl Action) -> Entity {
    crate::native::layout::desktop::input_settings::button(world, page, page, caption, action).0
}

pub(super) fn panel(world: &mut World, root: Entity, parent: Entity, workspace: u64) {
    let section = column(world, parent);
    if world
        .get::<Documents>(root)
        .is_some_and(|d| d.0.get(&workspace).is_some_and(|s| !s.collapsed))
    {
        draw(world, section, root, workspace);
    } else {
        button(world, section, "Scene document", Show { root, workspace });
    }
}

struct Show {
    root: Entity,
    workspace: u64,
}

impl Action for Show {
    fn apply(&self, world: &mut World, section: Entity) {
        if world
            .get::<Workspaces>(self.root)
            .is_none_or(|w| w.active != self.workspace)
        {
            return;
        }
        let path = crate::native::workspace_config::path(world, self.workspace).map_or_else(
            || PathBuf::from("scene.workspace"),
            |p| p.with_file_name("scene.workspace"),
        );
        world
            .entity_mut(self.root)
            .entry::<Documents>()
            .or_default()
            .into_mut()
            .0
            .entry(self.workspace)
            .or_insert_with(|| Session {
                directory: path,
                ..default()
            })
            .collapsed = false;
        draw(world, section, self.root, self.workspace);
    }
}

struct Hide;
impl Action for Hide {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(form) = world.get::<Form>(page).copied() else {
            return;
        };
        let Some(mut documents) = world.get_mut::<Documents>(form.root) else {
            return;
        };
        if let Some(s) = documents.0.get_mut(&form.workspace) {
            s.collapsed = true;
        }
        crate::native::sand_panel::clear(world, form.section);
        button(
            world,
            form.section,
            "Scene document",
            Show {
                root: form.root,
                workspace: form.workspace,
            },
        );
    }
}

fn draw(world: &mut World, section: Entity, root: Entity, workspace: u64) {
    crate::native::sand_panel::clear(world, section);
    let page = column(world, section);
    let session = &world.get::<Documents>(root).unwrap().0[&workspace];
    let directory = session.directory.display().to_string();
    let scene = session.scene.clone();
    let selected = session.selected.clone();
    let message = session.message.clone();
    let dirty = session.dirty;
    let undo = !session.undo.is_empty();
    let redo = !session.redo.is_empty();
    label(world, page, "Scene document", 20.0);
    button(world, page, "Close scene inspector", Hide);
    let path = field(world, page, "Scene directory", &directory);
    if !dirty {
        button(world, page, "New scene", Command::New);
        if showoff_bracket(world, root, workspace).is_some() {
            button(world, page, "Try the bracket example", Command::Example);
        }
    }
    button(
        world,
        page,
        if dirty {
            "Discard scene edits and open directory"
        } else {
            "Open directory"
        },
        Command::Open,
    );
    let status = label(world, page, &message, 14.0);
    let mut form = Form {
        root,
        workspace,
        section,
        path,
        status,
        name: None,
        location: None,
        record: None,
        request_id: None,
        source_directory: None,
    };
    if let Some(scene) = &scene {
        button(world, page, "Save scene", Command::Save);
        button(world, page, "Save to a new directory", Command::SaveCopy);
        form.request_id = accepted::controls(world, root, workspace, page);
        form.source_directory = library::controls(world, root, workspace, page);
        if undo {
            button(world, page, "Undo scene edit", Command::Undo);
        }
        if redo {
            button(world, page, "Redo scene edit", Command::Redo);
        }
        label(world, page, "Parts and placements", 17.0);
        form.name = Some(field(world, page, "New part name", ""));
        button(world, page, "Create part and place it", Command::Add);
        for part in scene.parts.iter() {
            label(world, page, &part.name, 16.0);
            let pin = part
                .pin(&scene.document)
                .expect("validated scene definition");
            for slot in scene.slots.iter().filter(|s| s.pin(&scene.document) == pin) {
                let handle = scene.handle(slot);
                let marker = if selected.as_ref() == Some(&handle) {
                    "Selected"
                } else {
                    "Placement"
                };
                button(
                    world,
                    page,
                    &format!("{marker} {}", slot.id.as_str()),
                    Command::Select(handle),
                );
            }
        }
        if let Some(slot) = selected.as_ref().and_then(|h| scene.slot(h)) {
            label(world, page, "Selected placement", 17.0);
            let details = label(
                world,
                page,
                &format!(
                    "Root {}\nSlot {}\nDefinition {}\nSource document {}\nRevision {}\nLocal origin element {}",
                    scene.root.as_str(),
                    slot.id.as_str(),
                    slot.part.as_str(),
                    slot.source.as_ref().unwrap_or(&scene.document).as_str(),
                    slot.revision.as_str(),
                    PART_ORIGIN.0
                ),
                13.0,
            );
            world.entity_mut(details).insert(TextLayout::linebreak(
                bevy::text::LineBreak::WordOrCharacter,
            ));
            button(
                world,
                page,
                "Place another copy of this part",
                Command::Place,
            );
            if let Some(binding) = scene
                .canvas
                .as_ref()
                .filter(|b| b.handle == scene.handle(slot))
            {
                let details = label(
                    world,
                    page,
                    &format!(
                        "Canvas placement {} in Room {}",
                        binding.placement, binding.room
                    ),
                    13.0,
                );
                world.entity_mut(details).insert(TextLayout::linebreak(
                    bevy::text::LineBreak::WordOrCharacter,
                ));
            } else if scene.canvas.is_none() {
                button(
                    world,
                    page,
                    "Link selected Room member to this placement",
                    Command::Bind,
                );
            }
            form.location = Some(std::array::from_fn(|i| {
                field(
                    world,
                    page,
                    ["X (m)", "Y (m)", "Z (m)"][i],
                    &slot.metres[i].to_string(),
                )
            }));
            form.record = Some(field(
                world,
                page,
                "Record UID (optional)",
                slot.record.as_deref().unwrap_or_default(),
            ));
            button(world, page, "Apply to selected placement", Command::Apply);
        }
    }
    world.entity_mut(page).insert(form);
}

impl Action for Command {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(form) = world.get::<Form>(page).copied() else {
            return;
        };
        if world
            .get::<Workspaces>(form.root)
            .is_none_or(|w| w.active != form.workspace)
        {
            return;
        }
        let Some(mut session) = world
            .get_mut::<Documents>(form.root)
            .and_then(|mut d| d.0.remove(&form.workspace))
        else {
            return;
        };
        let result = session.run(world, form, self);
        let success = result.is_ok();
        session.message = result.unwrap_or_else(|e| e);
        let redraw = success || matches!(self, Command::Library(_));
        if !redraw {
            status(world, form.status, &session.message);
        }
        world
            .get_mut::<Documents>(form.root)
            .unwrap()
            .0
            .insert(form.workspace, session);
        if success {
            let connected = world
                .get::<Documents>(form.root)
                .unwrap()
                .0
                .values()
                .any(|s| s.scene.as_ref().is_some_and(|s| s.canvas.is_some()));
            if connected {
                world.entity_mut(form.root).insert(bridge::Connected);
            } else {
                world.entity_mut(form.root).remove::<bridge::Connected>();
            }
        }
        if redraw {
            draw(world, form.section, form.root, form.workspace);
        }
    }
}

fn showoff_bracket(world: &World, root: Entity, workspace: u64) -> Option<&str> {
    world
        .get::<Workspaces>(root)?
        .entries
        .iter()
        .find(|s| s.id == workspace)?
        .showoff_bracket
        .as_deref()
}

fn coordinates(world: &World, form: Form) -> Result<[DecimalValue; 3], String> {
    let [x, y, z] = form.location.ok_or("Select a placement.")?;
    let coordinate = |field| {
        DecimalValue::parse_inferred(value(world, field)?.trim()).map_err(|e| e.to_string())
    };
    Ok([coordinate(x)?, coordinate(y)?, coordinate(z)?])
}

impl Session {
    fn remember(&mut self, previous: Scene) {
        if self.undo.len() == 16 {
            self.undo.remove(0);
        }
        self.accepted.invalidate();
        self.undo.push(previous);
        self.redo.clear();
        self.dirty = true;
    }

    fn run(&mut self, world: &mut World, form: Form, command: &Command) -> Result<String, String> {
        match command {
            Command::Accepted(action) => accepted::run(self, world, form, *action),
            Command::Library(action) => library::run(self, world, form, action),
            Command::Example => {
                let id = showoff_bracket(world, form.root, form.workspace)
                    .ok_or("Open the model workbench in the showoff workspace.")?;
                let item = bridge::find(world, form.root, form.workspace, id)?;
                let mut scene = Scene::default();
                let first = scene.add_part("Bracket")?;
                let second = scene.place_again(&first)?;
                bridge::bind_item(world, form, &mut scene, &second, item)?;
                bridge::select_item(world, form.root, form.workspace, &scene, &second);
                *self = Self {
                    selected: Some(second),
                    scene: Some(scene),
                    dirty: true,
                    directory: self.directory.clone(),
                    ..default()
                };
                Ok("Drag the Bracket and undo. Save the scene, then open Accepted edits and measurements to try recovering a lost reply.".into())
            }
            Command::New | Command::Open | Command::Save | Command::SaveCopy => {
                let text = value(world, form.path)?;
                if text.trim().is_empty() {
                    return Err("Enter a scene directory.".into());
                }
                let path = PathBuf::from(text.trim());
                if matches!(command, Command::New | Command::Open) {
                    let opened = if matches!(command, Command::Open) {
                        Some(workspace_scene::open(&path).map_err(|e| e.to_string())?)
                    } else {
                        None
                    };
                    let (scene, revision, last) = opened.map_or_else(
                        || (Scene::default(), None, None),
                        |o| (o.scene, Some(o.revision), o.last_receipt),
                    );
                    let projection = bridge::project(world, form.root, form.workspace, &scene);
                    let selected = scene
                        .canvas
                        .as_ref()
                        .map(|b| b.handle.clone())
                        .or_else(|| scene.slots.first().map(|s| scene.handle(s)));
                    let mut accepted = accepted::State::opened(last);
                    // A derived sidecar cannot prevent access to the authored scene.
                    let recovered = match (&revision, &selected) {
                        (Some(revision), Some(handle)) => {
                            accepted.recover_measurement(&path, &scene, revision, handle)
                        }
                        _ => Ok(()),
                    };
                    *self = Self {
                        accepted,
                        directory: path,
                        scene: Some(scene),
                        dirty: revision.is_none(),
                        revision,
                        selected,
                        ..default()
                    };
                    if let Err(reason) = projection {
                        return Ok(format!(
                            "Opened the scene. Canvas binding unavailable: {reason}"
                        ));
                    }
                    if let Err(reason) = recovered {
                        return Ok(format!(
                            "Opened the scene. Saved measurement unavailable: {reason}"
                        ));
                    }
                    return Ok(if self.dirty {
                        "New scene. Add a part, then save."
                    } else {
                        "Opened the published scene."
                    }
                    .into());
                }
                let expected = if matches!(command, Command::SaveCopy) {
                    None
                } else {
                    if path != self.directory {
                        return Err("Use Save to a new directory for another location.".into());
                    }
                    self.revision.as_ref()
                };
                let revision = workspace_scene::save(
                    &path,
                    self.scene.as_ref().ok_or("Create or open a scene first.")?,
                    expected,
                )
                .map_err(|e| e.to_string())?;
                if self.directory != path || self.revision.as_ref() != Some(&revision) {
                    self.accepted.invalidate();
                }
                if matches!(command, Command::SaveCopy) {
                    self.accepted = default();
                }
                self.revision = Some(revision);
                self.directory = path;
                self.dirty = false;
                Ok("Saved the scene.".into())
            }
            Command::Select(handle) => {
                self.scene
                    .as_ref()
                    .and_then(|s| s.slot(handle))
                    .ok_or("Placement is unavailable.")?;
                bridge::select_item(
                    world,
                    form.root,
                    form.workspace,
                    self.scene.as_ref().unwrap(),
                    handle,
                );
                if self.selected.as_ref() != Some(handle) {
                    self.accepted.invalidate();
                }
                self.selected = Some(handle.clone());
                Ok("Editing this placement.".into())
            }
            Command::Undo | Command::Redo => {
                let (from, to) = if matches!(command, Command::Undo) {
                    (&mut self.undo, &mut self.redo)
                } else {
                    (&mut self.redo, &mut self.undo)
                };
                let next = from.last().ok_or("No scene edit to restore.")?;
                if let Some(binding) = &next.canvas
                    && self.scene.as_ref().is_none_or(|s| {
                        s.canvas != next.canvas
                            || s.slot(&binding.handle) != next.slot(&binding.handle)
                    })
                {
                    bridge::project(world, form.root, form.workspace, next)?;
                }
                let next = from.pop().unwrap();
                let scene = self.scene.as_mut().ok_or("Scene is unavailable.")?;
                to.push(std::mem::replace(scene, next));
                if self
                    .selected
                    .as_ref()
                    .is_none_or(|h| scene.slot(h).is_none())
                {
                    self.selected = scene.slots.first().map(|s| scene.handle(s));
                }
                self.accepted.invalidate();
                self.dirty = true;
                Ok("Restored the scene edit.".into())
            }
            Command::Add | Command::Place | Command::Apply | Command::Bind => {
                let scene = self.scene.as_mut().ok_or("Create or open a scene first.")?;
                let previous = scene.clone();
                match command {
                    Command::Add => {
                        self.selected = Some(scene.add_part(
                            value(world, form.name.ok_or("Missing part name.")?)?.trim(),
                        )?);
                    }
                    Command::Place => {
                        self.selected =
                            Some(scene.place_again(
                                self.selected.as_ref().ok_or("Select a placement.")?,
                            )?);
                    }
                    Command::Bind => {
                        bridge::bind(
                            world,
                            form,
                            scene,
                            self.selected.as_ref().ok_or("Select a placement.")?,
                        )?;
                    }
                    Command::Apply => {
                        let metres = coordinates(world, form)?;
                        let record = value(world, form.record.ok_or("Missing Record field.")?)?;
                        let record = record.trim();
                        let handle = self.selected.as_ref().ok_or("Select a placement.")?;
                        let mut next = scene.clone();
                        next.edit(
                            handle,
                            metres,
                            (!record.is_empty()).then(|| record.to_owned()),
                        )?;
                        if next.canvas.as_ref().is_some_and(|b| &b.handle == handle) {
                            bridge::project(world, form.root, form.workspace, &next)?;
                        }
                        *scene = next;
                    }
                    _ => unreachable!(),
                }
                self.remember(previous);
                Ok("Unsaved scene edit.".into())
            }
        }
    }
}

#[cfg(test)]
mod tests;
