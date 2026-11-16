use crate::native::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use engine::blob_sync::{Target, Transfer};
use lince_interface::blob_sync::{size, state};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

mod presentation;

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "iroh",
        author: "n0 team and contributors",
        license: include_str!("../../../licenses/native/blob-sync/iroh-BSD-3-Clause.txt"),
    },
    crate::native::credits::Attribution {
        name: "iroh-blobs",
        author: "n0 team and contributors",
        license: include_str!("../../../licenses/native/blob-sync/iroh-blobs-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "bao-tree",
        author: "Rüdiger Klaehn and contributors",
        license: include_str!("../../../licenses/native/blob-sync/bao-tree-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "rfd",
        author: "Poly Meilex and contributors",
        license: include_str!("../../../licenses/native/document/rfd-MIT.txt"),
    },
];

#[derive(Component)]
struct View {
    selection: Entity,
    recipient: Entity,
    status: Entity,
    targets: Entity,
    transfers: Entity,
    paths: Vec<PathBuf>,
    target: Option<String>,
    snapshot: Option<Arc<Snapshot>>,
    shown: Option<Arc<Snapshot>>,
    rows: presentation::Rows,
}

#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    targets: Vec<Target>,
    transfers: Vec<Arc<Transfer>>,
}

impl Snapshot {
    fn from_rows(targets: Vec<Target>, transfers: Vec<Transfer>, previous: Option<&Self>) -> Self {
        let _timing = crate::host::performance::Span::new(
            crate::host::performance::Stage::NativeBlobSnapshotPreparation,
        );
        let previous: HashMap<_, _> = previous
            .into_iter()
            .flat_map(|snapshot| &snapshot.transfers)
            .map(|transfer| (transfer.id.as_str(), transfer))
            .collect();
        let mut reused = 0;
        let transfers = transfers
            .into_iter()
            .map(|transfer| {
                if let Some(previous) = previous.get(transfer.id.as_str())
                    && previous.as_ref() == &transfer
                {
                    reused += 1;
                    Arc::clone(previous)
                } else {
                    Arc::new(transfer)
                }
            })
            .collect();
        crate::host::performance::add(
            crate::host::performance::Count::BlobTransferModelsReused,
            reused,
        );
        Self { targets, transfers }
    }
}

#[derive(Component)]
struct Watch {
    receiver: tokio::sync::watch::Receiver<Option<Result<Arc<Snapshot>, String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Component)]
struct Job(
    tokio::sync::oneshot::Receiver<Result<Outcome, String>>,
    tokio::task::JoinHandle<()>,
);

impl Drop for Job {
    fn drop(&mut self) {
        self.1.abort();
    }
}

enum Outcome {
    Files(Vec<PathBuf>),
    Done(String),
    Nothing,
}

#[derive(Clone)]
enum Command {
    Files,
    Folder,
    Clear,
    Select(String, String),
    Send,
    Accept(String),
    Stop(String),
}

pub(super) fn populate(world: &mut World, parent: Entity) {
    let owner = panel::column(world, parent);
    label(world, owner, "Blob Sync");
    label(
        world,
        owner,
        "Send a fixed copy. The recipient must accept each offer.",
    );
    let controls = panel::row(world, owner);
    for (caption, command) in [
        ("Choose files", Command::Files),
        ("Choose folder", Command::Folder),
        ("Clear selection", Command::Clear),
    ] {
        panel::button(world, controls, owner, caption, command);
    }
    let selection = label(world, owner, "No files selected");
    let recipient = label(world, owner, "Choose a nearby Organ or contact");
    let targets = panel::column(world, owner);
    panel::button(world, owner, owner, "Send copy", Command::Send);
    let status = label(world, owner, "Loading Blob Sync…");
    let transfers = panel::column(world, owner);
    panel::credits(world, controls, owner, CREDITS);
    world.entity_mut(owner).insert(View {
        selection,
        recipient,
        targets,
        transfers,
        status,
        paths: Vec::new(),
        target: None,
        snapshot: None,
        shown: None,
        rows: Default::default(),
    });
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::native::edit_mode::label(world, parent, text, 14.0)
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let status = view.status;
        if crate::native::laboratory::blocked(world, owner) {
            panel::status(world, status, "Blob Sync is unavailable in the Laboratory");
            return;
        }
        if world.get::<Job>(owner).is_some() {
            return;
        }
        match self {
            Self::Select(node, name) => {
                let recipient = view.recipient;
                world.get_mut::<View>(owner).unwrap().target = Some(node.clone());
                panel::status(world, recipient, format!("To: {name}"));
                return;
            }
            Self::Clear => {
                let selection = view.selection;
                world.get_mut::<View>(owner).unwrap().paths.clear();
                panel::status(world, selection, "No files selected");
                return;
            }
            _ => {}
        }
        let paths = view.paths.clone();
        let target = view.target.clone();
        let Some(runtime) = crate::native::practice_cells::runtime(world, owner) else {
            panel::status(world, status, "The local Cell is unavailable");
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            panel::status(world, status, "The runtime is unavailable");
            return;
        };
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
        let prepared_directory =
            crate::native::practice_cells::source(world, owner).and_then(|source| {
                world
                    .get_resource::<crate::native::practice_cells::PracticeCells>()?
                    .directories
                    .get(&source)
                    .cloned()
            });
        let command = self.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = handle.spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                run(command, runtime, paths, target, prepared_directory),
            )
            .await
            .unwrap_or_else(|_| Err("This operation timed out. Retry or cancel the copy.".into()));
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        world.entity_mut(owner).insert(Job(receiver, task));
        panel::status(
            world,
            status,
            if matches!(self, Self::Send) {
                "Preparing a fixed copy…"
            } else {
                "Working…"
            },
        );
    }
}

async fn run(
    command: Command,
    runtime: cell::CellRuntime,
    paths: Vec<PathBuf>,
    target: Option<String>,
    prepared_directory: Option<PathBuf>,
) -> Result<Outcome, String> {
    match command {
        Command::Files | Command::Folder => {
            if let Some(directory) = prepared_directory {
                return Ok(Outcome::Files(vec![directory.join("blob-sample.txt")]));
            }
            let paths = tokio::task::spawn_blocking(move || {
                if matches!(command, Command::Files) {
                    rfd::FileDialog::new().pick_files()
                } else {
                    rfd::FileDialog::new().pick_folder().map(|path| vec![path])
                }
            })
            .await
            .map_err(|error| error.to_string())?;
            Ok(paths.map_or(Outcome::Nothing, Outcome::Files))
        }
        Command::Send => {
            let target = target.ok_or("Choose a recipient")?;
            if paths.is_empty() {
                return Err("Choose files or a folder".into());
            }
            let wire = runtime
                .wire
                .read()
                .await
                .clone()
                .ok_or("Peer connections are unavailable")?;
            wire.send_blob_copy(&target, paths)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Outcome::Done(
                "Copy prepared. Waiting for the recipient to accept.".into(),
            ))
        }
        Command::Accept(id) => {
            let directory = if let Some(directory) = prepared_directory {
                let directory = directory.join("received-files");
                tokio::fs::create_dir_all(&directory)
                    .await
                    .map_err(|error| error.to_string())?;
                Some(directory)
            } else {
                tokio::task::spawn_blocking(|| {
                    rfd::FileDialog::new()
                        .set_title("Save Blob Sync copy in")
                        .pick_folder()
                })
                .await
                .map_err(|error| error.to_string())?
            };
            let Some(directory) = directory else {
                return Ok(Outcome::Nothing);
            };
            runtime
                .engine
                .accept_blob_copy(&id, &directory)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Outcome::Done(
                "Accepted. The copy will arrive in its own folder.".into(),
            ))
        }
        Command::Stop(id) => {
            runtime
                .engine
                .stop_blob_copy(&id)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Outcome::Done("Copy stopped.".into()))
        }
        _ => Ok(Outcome::Nothing),
    }
}

fn within(world: &World, ancestor: Entity, mut entity: Entity) -> bool {
    loop {
        if entity == ancestor {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
}

fn control(world: &mut World, ancestor: Entity) -> Option<Entity> {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .filter(|entity| within(world, ancestor, *entity))
        .collect();
    match owners.as_slice() {
        [owner] => Some(*owner),
        _ => None,
    }
}

pub(crate) fn prepare_copy(world: &mut World, ancestor: Entity, file: PathBuf, target: &str) {
    let Some(owner) = control(world, ancestor) else {
        return;
    };
    if world.get::<Job>(owner).is_some() {
        return;
    }
    let selection = world.get::<View>(owner).unwrap().selection;
    world.get_mut::<View>(owner).unwrap().paths = vec![file];
    panel::status(world, selection, "blob-sample.txt · bundled practice file");
    Command::Select(target.into(), "Prepared peer".into()).apply(world, owner);
}

pub(crate) fn send_copy(world: &mut World, ancestor: Entity) {
    if let Some(owner) = control(world, ancestor) {
        Command::Send.apply(world, owner);
    }
}

pub(crate) fn transfer(
    world: &mut World,
    ancestor: Entity,
    direction: &str,
    state: &str,
) -> Option<String> {
    let owner = control(world, ancestor)?;
    let snapshot = world.get::<View>(owner)?.snapshot.as_ref()?;
    snapshot
        .transfers
        .iter()
        .find(|transfer| {
            transfer.direction == direction
                && transfer.state == state
                && transfer
                    .manifest
                    .entries
                    .iter()
                    .any(|entry| entry.path == "blob-sample.txt")
        })
        .map(|transfer| transfer.id.clone())
}

pub(crate) fn accept_copy(world: &mut World, ancestor: Entity) {
    if let Some(id) = transfer(world, ancestor, "incoming", "offered")
        && let Some(owner) = control(world, ancestor)
    {
        Command::Accept(id).apply(world, owner);
    }
}

pub(super) fn update(world: &mut World) {
    if !crate::native::laboratory::normal(world) {
        return;
    }
    let owners = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect::<Vec<_>>();
    for owner in owners {
        if crate::native::laboratory::blocked(world, owner) {
            continue;
        }
        if world.get::<Watch>(owner).is_none() {
            start_watch(world, owner);
        }
        let update = world.get_mut::<Watch>(owner).and_then(|mut watch| {
            if watch.receiver.has_changed().unwrap_or(false) {
                watch.receiver.borrow_and_update().clone()
            } else {
                None
            }
        });
        let snapshot = match update {
            Some(Ok(snapshot)) => Some(snapshot),
            Some(Err(error)) => {
                let status = world.get::<View>(owner).unwrap().status;
                panel::status(world, status, error);
                None
            }
            None => None,
        }
        .or_else(|| {
            let view = world.get::<View>(owner).unwrap();
            view.snapshot
                .as_ref()
                .filter(|snapshot| {
                    view.shown
                        .as_ref()
                        .is_none_or(|shown| !Arc::ptr_eq(shown, snapshot))
                })
                .cloned()
        });
        if let Some(snapshot) = snapshot {
            render(world, owner, snapshot);
        }
        let result = world
            .get_mut::<Job>(owner)
            .and_then(|mut job| match job.0.try_recv() {
                Ok(result) => Some(result),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Some(Err("The operation stopped. Try again.".into()))
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            world.entity_mut(owner).remove::<Job>();
            let view = world.get::<View>(owner).unwrap();
            let (status, selection) = (view.status, view.selection);
            match result {
                Ok(Outcome::Files(paths)) => {
                    let names = paths
                        .iter()
                        .filter_map(|path| path.file_name())
                        .map(|name| name.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ");
                    world.get_mut::<View>(owner).unwrap().paths = paths;
                    panel::status(world, selection, names);
                    panel::status(world, status, "Choose a recipient, then send the copy.");
                }
                Ok(Outcome::Done(message)) | Err(message) => panel::status(world, status, message),
                Ok(Outcome::Nothing) => panel::status(world, status, ""),
            }
        }
    }
}

fn start_watch(world: &mut World, owner: Entity) {
    let Some(runtime) = crate::native::practice_cells::runtime(world, owner) else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let (sender, receiver) = tokio::sync::watch::channel(None);
    let task = handle.spawn(async move {
        let Ok(blobs) = runtime.engine.blob_sync() else {
            super::snapshots::publish(
                &sender,
                Err("Blob Sync is unavailable on this Cell".into()),
                wake.as_ref(),
            );
            return;
        };
        let mut changes = blobs.watch();
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = timer.tick() => {}
                changed = changes.changed() => { if changed.is_err() { break; } }
            }
            let result = async {
                let targets = match runtime.wire.read().await.clone() {
                    Some(wire) => wire
                        .blob_targets()
                        .await
                        .map_err(|error| error.to_string())?,
                    None => Vec::new(),
                };
                let transfers = runtime
                    .engine
                    .blob_transfers()
                    .await
                    .map_err(|error| error.to_string())?;
                let previous = sender.borrow();
                let previous = previous
                    .as_ref()
                    .and_then(|snapshot| snapshot.as_ref().ok());
                Ok(Arc::new(Snapshot::from_rows(
                    targets,
                    transfers,
                    previous.map(Arc::as_ref),
                )))
            }
            .await;
            if !super::snapshots::publish(&sender, result, wake.as_ref()) {
                break;
            }
        }
    });
    world.entity_mut(owner).insert(Watch { receiver, task });
}

fn render(world: &mut World, owner: Entity, snapshot: impl Into<Arc<Snapshot>>) {
    let snapshot = snapshot.into();
    let view = world.get::<View>(owner).unwrap();
    let first = view.snapshot.is_none();
    let status = view.status;
    let shown = view
        .shown
        .as_ref()
        .is_some_and(|shown| Arc::ptr_eq(shown, &snapshot));
    if view
        .snapshot
        .as_ref()
        .is_none_or(|current| !Arc::ptr_eq(current, &snapshot))
    {
        world.get_mut::<View>(owner).unwrap().snapshot = Some(snapshot.clone());
    }
    if first {
        panel::status(world, status, "Ready");
    }
    // Snapshot consumers (including practice steps) stay current independently
    // of presentation. Retain the old UI until it has real output demand.
    if shown || !crate::native::output_views::presents_entity(world, owner) {
        return;
    }
    let (previous, mut rows) = {
        let mut view = world.get_mut::<View>(owner).unwrap();
        (view.shown.take(), std::mem::take(&mut view.rows))
    };
    presentation::render(world, owner, previous.as_deref(), &snapshot, &mut rows);
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.rows = rows;
    view.shown = Some(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_have_acceptance_controls_and_unrelated_progress_keeps_them_stable() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let parent = world.spawn_empty().id();
        populate(world, parent);
        let owner = world
            .query_filtered::<Entity, With<View>>()
            .single(world)
            .unwrap();
        let incoming = Transfer {
            id: "incoming".into(),
            direction: "incoming".into(),
            peer: "recipient".into(),
            peer_organ: None,
            label: "Friend".into(),
            manifest: nucleus::blob_sync::Manifest {
                id: "incoming".into(),
                entries: vec![nucleus::blob_sync::Entry {
                    path: "photo.png".into(),
                    hash: Some("0".repeat(64)),
                    size: 128,
                }],
            },
            state: "offered".into(),
            destination: None,
            progress: 0,
            error: String::new(),
            settled: false,
        };
        let mut outgoing = incoming.clone();
        outgoing.id = "outgoing".into();
        outgoing.direction = "outgoing".into();
        outgoing.state = "accepted".into();
        render(
            world,
            owner,
            Snapshot {
                targets: Vec::new(),
                transfers: vec![Arc::new(incoming.clone()), Arc::new(outgoing.clone())],
            },
        );
        let accept = world
            .query::<(Entity, &Text)>()
            .iter(world)
            .find(|(_, text)| text.0 == "Accept and save…")
            .unwrap()
            .0;
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "Decline")
        );
        outgoing.progress = 64;
        render(
            world,
            owner,
            Snapshot {
                targets: Vec::new(),
                transfers: vec![Arc::new(incoming), Arc::new(outgoing)],
            },
        );
        assert_eq!(world.get::<Text>(accept).unwrap().0, "Accept and save…");
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "64 B / 128 B")
        );
    }
}
