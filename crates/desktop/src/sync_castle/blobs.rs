use crate::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use engine::blob_sync::{Target, Transfer};
use lince_interface::blob_sync::{size, state};
use std::path::PathBuf;

const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "iroh",
        author: "n0 team and contributors",
        license: include_str!("../../licenses/blob-sync/iroh-BSD-3-Clause.txt"),
    },
    crate::credits::Attribution {
        name: "iroh-blobs",
        author: "n0 team and contributors",
        license: include_str!("../../licenses/blob-sync/iroh-blobs-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "bao-tree",
        author: "Rüdiger Klaehn and contributors",
        license: include_str!("../../licenses/blob-sync/bao-tree-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "rfd",
        author: "Poly Meilex and contributors",
        license: include_str!("../../licenses/document/rfd-MIT.txt"),
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
    snapshot: Option<Snapshot>,
}

#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    targets: Vec<Target>,
    transfers: Vec<Transfer>,
}

#[derive(Component)]
struct Watch {
    receiver: tokio::sync::watch::Receiver<Option<Result<Snapshot, String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Component)]
struct Job(tokio::sync::oneshot::Receiver<Result<Outcome, String>>);

#[derive(Component)]
struct TransferRow(Transfer);

#[derive(Component)]
struct FileList(Vec<nucleus::blob_sync::Entry>, bool);

#[derive(Clone)]
struct ToggleFiles;

impl Action for ToggleFiles {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut list) = world.get_mut::<FileList>(owner) else {
            return;
        };
        list.1 = !list.1;
        let entries = if list.1 { list.0.clone() } else { Vec::new() };
        panel::clear(world, owner);
        for entry in entries {
            label(
                world,
                owner,
                &format!("{} · {}", entry.path, size(entry.size)),
            );
        }
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
    });
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::edit_mode::label(world, parent, text, 14.0)
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let status = view.status;
        if crate::laboratory::active(world) {
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
        let Some(runtime) = world
            .get_resource::<crate::app::CellHandle>()
            .map(|handle| handle.0.clone())
        else {
            panel::status(world, status, "The local Cell is unavailable");
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            panel::status(world, status, "The runtime is unavailable");
            return;
        };
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let command = self.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        handle.spawn(async move {
            let result = run(command, runtime, paths, target).await;
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        world.entity_mut(owner).insert(Job(receiver));
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
) -> Result<Outcome, String> {
    match command {
        Command::Files | Command::Folder => {
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
            let directory = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .set_title("Save Blob Sync copy in")
                    .pick_folder()
            })
            .await
            .map_err(|error| error.to_string())?;
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

pub(super) fn update(world: &mut World) {
    if crate::laboratory::active(world) {
        return;
    }
    let owners = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect::<Vec<_>>();
    for owner in owners {
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
        if let Some(update) = update {
            let view = world.get::<View>(owner).unwrap();
            match update {
                Ok(snapshot) if view.snapshot.as_ref() != Some(&snapshot) => {
                    render(world, owner, snapshot)
                }
                Err(error) => {
                    let status = view.status;
                    panel::status(world, status, error);
                }
                _ => {}
            }
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
    let Some(runtime) = world
        .get_resource::<crate::app::CellHandle>()
        .map(|handle| handle.0.clone())
    else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = tokio::sync::watch::channel(None);
    let task = handle.spawn(async move {
        let Ok(blobs) = runtime.engine.blob_sync() else {
            let _ = sender.send(Some(Err("Blob Sync is unavailable on this Cell".into())));
            if let Some(wake) = &wake {
                wake.ring();
            }
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
                Ok(Snapshot { targets, transfers })
            }
            .await;
            if sender.send(Some(result)).is_err() {
                break;
            }
            if let Some(wake) = &wake {
                wake.ring();
            }
        }
    });
    world.entity_mut(owner).insert(Watch { receiver, task });
}

fn render(world: &mut World, owner: Entity, snapshot: Snapshot) {
    let view = world.get::<View>(owner).unwrap();
    let (targets, transfers, status) = (view.targets, view.transfers, view.status);
    let first = view.snapshot.is_none();
    if view
        .snapshot
        .as_ref()
        .is_none_or(|old| old.targets != snapshot.targets)
    {
        panel::clear(world, targets);
        if snapshot.targets.is_empty() {
            label(
                world,
                targets,
                "No recipients. Add a contact or enable LAN discovery in Organ settings.",
            );
        }
        for target in &snapshot.targets {
            let caption = format!(
                "{} · {} · {}",
                target.label,
                if target.nearby { "Nearby" } else { "Contact" },
                target.node_id.chars().take(8).collect::<String>()
            );
            panel::button(
                world,
                targets,
                owner,
                &caption,
                Command::Select(target.node_id.clone(), target.label.clone()),
            );
        }
    }
    let children = world
        .get::<Children>(transfers)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    for child in children {
        if world.get::<TransferRow>(child).is_none_or(|row| {
            !snapshot
                .transfers
                .iter()
                .any(|transfer| transfer.id == row.0.id)
        }) {
            world.despawn(child);
        }
    }
    if snapshot.transfers.is_empty() {
        label(world, transfers, "No copies sent or received yet.");
    }
    for transfer in &snapshot.transfers {
        let existing = world
            .get::<Children>(transfers)
            .into_iter()
            .flatten()
            .find_map(|child| {
                world
                    .get::<TransferRow>(*child)
                    .filter(|row| row.0.id == transfer.id)
                    .map(|row| (*child, row.0 == *transfer))
            });
        if existing.is_some_and(|(_, unchanged)| unchanged) {
            continue;
        }
        let row = if let Some((row, _)) = existing {
            panel::clear(world, row);
            row
        } else {
            panel::column(world, transfers)
        };
        world.entity_mut(row).insert(TransferRow(transfer.clone()));
        let incoming = transfer.direction == "incoming";
        label(
            world,
            row,
            &format!(
                "{} {} · {} · {}",
                if incoming { "From" } else { "To" },
                transfer.label,
                size(transfer.manifest.bytes()),
                state(&transfer.state, incoming)
            ),
        );
        label(
            world,
            row,
            &format!("Peer {}", transfer.peer.chars().take(8).collect::<String>()),
        );
        for entry in transfer.manifest.entries.iter().take(12) {
            label(
                world,
                row,
                &format!(
                    "{}{}",
                    entry.path,
                    if entry.hash.is_none() { "/" } else { "" }
                ),
            );
        }
        if transfer.manifest.entries.len() > 12 {
            let details = panel::column(world, row);
            world.entity_mut(details).insert(FileList(
                transfer.manifest.entries.iter().skip(12).cloned().collect(),
                false,
            ));
            panel::button(
                world,
                row,
                details,
                &format!(
                    "Show / hide {} more entries",
                    transfer.manifest.entries.len() - 12
                ),
                ToggleFiles,
            );
        }
        if transfer.state == "accepted" {
            label(
                world,
                row,
                &format!(
                    "{} / {}",
                    size(transfer.progress),
                    size(transfer.manifest.bytes())
                ),
            );
        }
        if let Some(destination) = &transfer.destination {
            label(world, row, destination);
        }
        if !transfer.error.is_empty() {
            label(
                world,
                row,
                &format!("{} · Lince will retry automatically.", transfer.error),
            );
        }
        let controls = panel::row(world, row);
        if incoming && transfer.state == "accepted" && !transfer.error.is_empty() {
            panel::button(
                world,
                controls,
                owner,
                "Change save folder…",
                Command::Accept(transfer.id.clone()),
            );
        }
        if incoming && transfer.state == "offered" {
            panel::button(
                world,
                controls,
                owner,
                "Accept and save…",
                Command::Accept(transfer.id.clone()),
            );
            panel::button(
                world,
                controls,
                owner,
                "Decline",
                Command::Stop(transfer.id.clone()),
            );
        } else if matches!(transfer.state.as_str(), "offered" | "accepted") {
            panel::button(
                world,
                controls,
                owner,
                "Cancel copy",
                Command::Stop(transfer.id.clone()),
            );
        }
    }
    if first {
        panel::status(world, status, "Ready");
    }
    world.get_mut::<View>(owner).unwrap().snapshot = Some(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_have_acceptance_controls_and_unrelated_progress_keeps_them_stable() {
        let mut app = crate::sand_panel::tests::app();
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
                transfers: vec![incoming.clone(), outgoing.clone()],
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
                transfers: vec![incoming, outgoing],
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
