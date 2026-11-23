//! The inspector and the existing local Canvas Action publish to the same open scene.
use super::{
    Command, Documents, Form, Session, bridge, button, coordinates, label, showoff_bracket,
};
use crate::native::workspace::Workspaces;
use bevy::prelude::*;
use nucleus::{
    canvas::{
        self,
        scene::{
            Scene,
            operation::{Availability, Edit, Inspection, Operation, Receipt, Request},
        },
    },
    component::composition::Origin,
};
use store::workspace_scene::{
    self, acceptance,
    measurement::{self, DistanceJob},
};

#[derive(Clone, Copy)]
pub(super) enum Action {
    Toggle,
    Move,
    Copy,
    DropCopyReply,
    Retry,
    Recover,
    Find,
    Undo,
    Hold,
    Measure,
    Deliver,
    Cancel,
}

#[derive(Default)]
pub(super) struct State {
    expanded: bool,
    pending: Option<Edit>,
    receipt: Option<Receipt>,
    generation: Option<String>,
    held: Option<DistanceJob>,
    measured: Option<(DistanceJob, f64)>,
}

impl State {
    pub(super) fn opened(receipt: Option<Receipt>) -> Self {
        Self {
            receipt,
            ..Self::default()
        }
    }

    pub(super) fn recover_measurement(
        &mut self,
        directory: &std::path::Path,
        scene: &Scene,
        revision: &nucleus::karma::CanonicalHash,
        handle: &nucleus::canvas::scene::Handle,
    ) -> Result<(), String> {
        self.measured = measurement::recover(directory, &scene.document, revision, handle)
            .map_err(|e| e.to_string())?;
        self.generation = self
            .measured
            .as_ref()
            .map(|(job, _)| job.generation.id.clone());
        Ok(())
    }
    pub(super) fn invalidate(&mut self) {
        self.generation = None;
        self.measured = None;
    }
}

pub(super) fn controls(
    world: &mut World,
    root: Entity,
    workspace: u64,
    page: Entity,
) -> Option<Entity> {
    let s = &world.get::<Documents>(root).unwrap().0[&workspace];
    let revision = s.revision.as_ref()?;
    let scene = s.scene.as_ref()?;
    let expanded = s.accepted.expanded;
    let pending = s.accepted.pending.is_some();
    let held = s.accepted.held.is_some();
    let measured = s.accepted.measured.as_ref().map(|(job, metres)| {
        format!(
            "Distance from origin: {metres:.6} m\nGeneration {}\nSource {}",
            job.generation.id,
            job.generation.revision.as_str()
        )
    });
    let clean = !s.dirty;
    let selected = s.selected.is_some();
    let details = s.accepted.receipt.as_ref().map(describe);
    let available = Availability::for_scene(
        scene,
        revision,
        clean && !pending,
        s.accepted.receipt.as_ref(),
    );
    button(
        world,
        page,
        if expanded {
            "Close accepted edits"
        } else {
            "Accepted edits and measurements"
        },
        Command::Accepted(Action::Toggle),
    );
    if !expanded {
        return None;
    }
    let showcase = showoff_bracket(world, root, workspace).is_some();
    if let Some(details) = details {
        let text = label(world, page, &details, 14.);
        world.entity_mut(text).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
    }
    if let Some(distance) = measured {
        let text = label(world, page, &distance, 14.);
        world.entity_mut(text).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
    }
    let request_id = super::field(world, page, "Request UID", "");
    let mut add = |caption, action| {
        button(world, page, caption, Command::Accepted(action));
    };
    add("Find accepted receipt", Action::Find);
    add("Recover last accepted receipt", Action::Recover);
    if pending {
        add("Retry the same request", Action::Retry);
    }
    if clean && selected && !pending {
        add("Publish placement coordinates", Action::Move);
        if available.place_copy {
            add("Publish another copy", Action::Copy);
        }
        if available.undo_request.is_some() {
            add("Undo the accepted edit", Action::Undo);
        }
        add("Measure distance from origin", Action::Measure);
        if showcase {
            if available.place_copy {
                add(
                    "Publish a copy and interrupt its reply",
                    Action::DropCopyReply,
                );
            }
            if !held {
                add("Hold a distance measurement", Action::Hold);
            }
        }
    }
    if held {
        add("Finish the held measurement", Action::Deliver);
        add("Cancel the held measurement", Action::Cancel);
    }
    if !clean {
        label(
            world,
            page,
            "Save the draft to publish another operation.",
            14.,
        );
    }
    Some(request_id)
}

fn describe(receipt: &Receipt) -> String {
    let actor = receipt.fiote.as_ref().map_or_else(
        || "Local operator".into(),
        |f| format!("Fiote {} · thread {}", f.agent, f.thread),
    );
    format!(
        "{actor} · {:?}\n{}\nSlot {}\nAccepted {}",
        receipt.effect,
        receipt.edit.request_id,
        receipt.handle.slot.as_str(),
        receipt.revision.as_str()
    )
}

pub(super) fn run(
    s: &mut Session,
    world: &mut World,
    form: Form,
    action: Action,
) -> Result<String, String> {
    match action {
        Action::Toggle => {
            s.accepted.expanded = !s.accepted.expanded;
            Ok("Published edits return a durable receipt.".into())
        }
        Action::Recover | Action::Find => {
            let scene = s.scene.as_ref().ok_or("Open a saved scene.")?;
            let id = if matches!(action, Action::Find) {
                Some(super::value(
                    world,
                    form.request_id.ok_or("Open accepted edits first.")?,
                )?)
            } else {
                None
            };
            let receipt =
                acceptance::receipt(&s.directory, &scene.document, id.as_deref().map(str::trim))
                    .map_err(|e| e.to_string())?;
            s.accepted.receipt = Some(receipt);
            Ok("Recovered the stored receipt.".into())
        }
        Action::Hold | Action::Measure => {
            if s.dirty {
                return Err("Save the scene before measuring its published placement.".into());
            }
            let handle = s.selected.clone().ok_or("Select a placement.")?;
            let scene = s.scene.as_ref().ok_or("Open the scene first.")?;
            let revision = s.revision.as_ref().ok_or("Save the scene first.")?;
            let job = measurement::begin(&s.directory, &scene.document, revision, &handle)
                .map_err(|e| e.to_string())?;
            s.accepted.generation = Some(job.generation.id.clone());
            if matches!(action, Action::Hold) {
                s.accepted.held = Some(job);
                Ok("Measurement held. Run a newer measurement or edit the placement, then finish this one.".into())
            } else {
                finish(s, job)
            }
        }
        Action::Deliver => {
            let job = s.accepted.held.take().ok_or("No held measurement.")?;
            finish(s, job)
        }
        Action::Cancel => {
            if let Some(job) = &s.accepted.held {
                measurement::cancel(&s.directory, &job.generation).map_err(|e| e.to_string())?;
                if s.accepted.generation.as_ref() == Some(&job.generation.id) {
                    s.accepted.generation = None;
                }
            }
            s.accepted.held = None;
            Ok("Cancelled the held measurement.".into())
        }
        Action::Move | Action::Copy | Action::DropCopyReply | Action::Retry | Action::Undo => {
            let edit = if matches!(action, Action::Retry) {
                s.accepted
                    .pending
                    .clone()
                    .ok_or("No interrupted request.")?
            } else {
                let selected = s.selected.clone().ok_or("Select a placement.")?;
                let operation = match action {
                    Action::Move => Operation::Move {
                        target: selected,
                        metres: coordinates(world, form)?,
                    },
                    Action::Undo => Operation::Undo {
                        request_id: s
                            .accepted
                            .receipt
                            .as_ref()
                            .ok_or("Recover the accepted edit first.")?
                            .edit
                            .request_id
                            .clone(),
                    },
                    _ => Operation::PlaceCopy { source: selected },
                };
                Edit {
                    request_id: nucleus::new_uid("request"),
                    document: s.scene.as_ref().ok_or("Open a scene.")?.document.clone(),
                    expected: s.revision.clone().ok_or("Save the scene first.")?,
                    operation,
                }
            };
            let interrupted = matches!(action, Action::DropCopyReply);
            let result = submit(
                s,
                world,
                form.root,
                form.workspace,
                &edit,
                None,
                || false,
                interrupted,
            );
            if result.is_ok() {
                s.accepted.pending = interrupted.then_some(edit);
            }
            result.map(|receipt| match receipt {
                None => format!(
                    "Reply interrupted after publication. Retry the same request to recover {}.",
                    s.accepted.pending.as_ref().unwrap().request_id
                ),
                Some(_) if matches!(action, Action::Retry) => {
                    "Recovered the original receipt.".into()
                }
                Some(_) => "Published the scene edit and stored its receipt.".into(),
            })
        }
    }
}

fn finish(s: &mut Session, job: DistanceJob) -> Result<String, String> {
    let scene = s.scene.as_ref().ok_or("Open a saved scene.")?;
    let revision = s.revision.as_ref().ok_or("Save the scene first.")?;
    let eligible = !s.dirty
        && job.generation.current(
            s.accepted.generation.as_deref(),
            &scene.document,
            revision,
            s.selected.as_ref(),
        );
    let distance = measurement::finish(&s.directory, &job, eligible).map_err(|e| e.to_string())?;
    let Some(distance) = distance else {
        return Ok("Discarded the late measurement. Its captured generation is obsolete.".into());
    };
    s.accepted.measured = Some((job, distance));
    Ok(format!(
        "Measured distance from the scene origin: {distance:.6} m."
    ))
}

fn bound_changed(before: &Scene, after: &Scene) -> bool {
    before.canvas != after.canvas
        || after
            .canvas
            .as_ref()
            .is_some_and(|b| before.slot(&b.handle) != after.slot(&b.handle))
}

fn submit(
    s: &mut Session,
    world: &mut World,
    root: Entity,
    workspace: u64,
    edit: &Edit,
    origin: Option<&Origin>,
    cancelled: impl Fn() -> bool,
    drop_reply: bool,
) -> Result<Option<Receipt>, String> {
    let scene = s
        .scene
        .as_ref()
        .ok_or("Open the scene in the local workspace first.")?;
    use engine::canvas::scene_edits::{LocalScene, Outcome};
    let local = LocalScene {
        directory: &s.directory,
        document: &scene.document,
        dirty: s.dirty,
    };
    let result = match local.submit(edit, origin, cancelled, |next| {
        if bound_changed(scene, next) {
            bridge::check_projection(world, root, workspace, next)?;
        }
        Ok(())
    })? {
        Outcome::RecoveredWithDraft(receipt) => return Ok(Some(receipt)),
        Outcome::Published(result) => result,
    };
    if drop_reply {
        return Ok(None);
    }
    let current = result.current;
    let advanced = s.revision.as_ref() != Some(&current.revision);
    if advanced {
        if bound_changed(scene, &current.scene) {
            bridge::project(world, root, workspace, &current.scene)?;
        }
        if s.revision.as_ref() == Some(&edit.expected)
            && current.revision == result.receipt.revision
        {
            s.remember(scene.clone());
        } else {
            s.undo.clear();
            s.redo.clear();
            s.accepted.invalidate();
        }
    }
    let handle = &result.receipt.handle;
    if advanced
        && current.revision == result.receipt.revision
        && current.scene.slot(handle).is_some()
    {
        s.selected = Some(handle.clone());
        bridge::select_item(world, root, workspace, &current.scene, handle);
    } else if s
        .selected
        .as_ref()
        .is_none_or(|h| current.scene.slot(h).is_none())
    {
        s.selected = current
            .scene
            .slots
            .first()
            .map(|slot| current.scene.handle(slot));
    }
    s.scene = Some(current.scene);
    s.revision = Some(current.revision);
    s.dirty = false;
    s.accepted.receipt = Some(result.receipt.clone());
    Ok(Some(result.receipt))
}

pub(crate) fn request(
    world: &mut World,
    root: Entity,
    workspace: u64,
    query: &Request,
    origin: Option<&Origin>,
    cancelled: impl Fn() -> bool,
) -> Result<canvas::Response, String> {
    if cancelled()
        || world
            .get::<Workspaces>(root)
            .is_none_or(|w| w.active != workspace)
    {
        return Err("Scene request cancelled or outside the active local workspace.".into());
    }
    let mut s = world
        .get_mut::<Documents>(root)
        .and_then(|mut docs| docs.0.remove(&workspace))
        .ok_or("Open a saved scene in this workspace first.")?;
    let result = (|| {
        let scene = s.scene.as_ref().ok_or("Open a saved scene first.")?;
        let revision = s.revision.as_ref().ok_or("Save this scene first.")?;
        match query {
            Request::Inspect => {
                let current = workspace_scene::open(&s.directory).map_err(|e| e.to_string())?;
                if current.scene.document != scene.document {
                    return Err("The open document changed.".into());
                }
                let editable = !s.dirty && *revision == current.revision;
                Ok(canvas::Response::Scene {
                    inspection: Inspection::new(
                        &current.scene,
                        current.revision,
                        editable,
                        current.last_receipt.as_ref(),
                    ),
                })
            }
            Request::Receipt {
                document,
                request_id,
            } => {
                if document != &scene.document {
                    return Err("Receipt is outside the open document.".into());
                }
                let receipt = acceptance::receipt(&s.directory, document, Some(request_id))
                    .map_err(|e| e.to_string())?;
                if origin.is_some() && receipt.fiote.as_ref() != origin {
                    return Err("This receipt belongs to another Actor.".into());
                }
                Ok(canvas::Response::SceneReceipt { receipt })
            }
            Request::Submit { edit } => {
                let receipt = submit(
                    &mut s, world, root, workspace, edit, origin, cancelled, false,
                )?
                .ok_or("Reply interrupted.")?;
                s.message = format!("Scene operation · {}", describe(&receipt));
                Ok(canvas::Response::SceneReceipt { receipt })
            }
        }
    })();
    world
        .get_mut::<Documents>(root)
        .unwrap()
        .0
        .insert(workspace, s);
    if matches!(query, Request::Submit { .. }) && result.is_ok() {
        bridge::refresh(world, root, workspace);
    }
    result
}
