// non-lean-yet
// Scope: struct View, struct Form, type Snapshot, struct Watch.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 82, 84, 132, 139,
//   147, 156, 171, 184, 216, 233, 286, 310, 332, 337, 341, 357, 362, 370, 376, 435.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (244, 250,
//   266, 312, 340, 345, 446, 465).

use crate::native::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use engine::file_sync::{DirectorySync, FileFormat, Request, Status};
use std::{sync::Arc, time::Duration};

#[derive(Component)]
struct View {
    castle: Entity,
    list: Entity,
    editor: Entity,
    status: Entity,
    entries: Arc<Vec<Status>>,
    form: Option<Form>,
    removal: Option<String>,
}

struct Form {
    name: Entity,
    origin: Result<String, Entity>,
    directory: Entity,
    format: FileFormat,
    format_label: Entity,
    protein: Option<String>,
    protein_label: Entity,
    enabled: bool,
}

type Snapshot = Result<Arc<Vec<Status>>, String>;

#[derive(Component)]
struct Watch {
    receiver: tokio::sync::watch::Receiver<Option<Snapshot>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Component)]
struct Job(tokio::sync::oneshot::Receiver<Snapshot>);

#[derive(Clone)]
enum Command {
    Open(Option<DirectorySync>),
    Format(FileFormat),
    Protein(Option<String>, String),
    Save,
    SetEnabled(String, bool),
    ConfirmRemoval(String),
    Remove(String),
    Close,
}

pub(super) fn populate(world: &mut World, parent: Entity, castle: Entity) {
    let owner = panel::column(world, parent);
    super::label(world, owner, "Folder syncs", 18.0);
    panel::button(world, owner, owner, "Add folder sync", Command::Open(None));
    let status = super::label(world, owner, "", 13.0);
    let list = panel::column(world, owner);
    let editor = panel::column(world, owner);
    world.entity_mut(owner).insert(View {
        castle,
        list,
        editor,
        status,
        entries: Arc::default(),
        form: None,
        removal: None,
    });
}

fn render(world: &mut World, owner: Entity, entries: Arc<Vec<Status>>, force: bool) {
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    if !force && view.entries == entries {
        return;
    }
    view.entries = entries.clone();
    let list = view.list;
    let removal = view.removal.clone();
    panel::clear(world, list);
    for item in entries.iter() {
        let settings = &item.settings;
        let row = panel::column(world, list);
        let state = if !settings.enabled {
            "Stopped"
        } else if !item.checked {
            "Starting"
        } else if item.conflicts.is_empty() {
            "Running"
        } else {
            "Needs attention"
        };
        super::label(world, row, &format!("{} · {state}", settings.name), 16.0);
        super::label(world, row, &settings.path, 13.0);
        super::label(
            world,
            row,
            &format!(
                "{} · File origin {}",
                super::format_name(settings.format),
                settings.uid
            ),
            13.0,
        );
        for conflict in item.conflicts.iter().take(5) {
            super::label(
                world,
                row,
                &format!("{} · {}", conflict.path, conflict.reason),
                13.0,
            );
        }
        if item.conflicts.len() > 5 {
            super::label(
                world,
                row,
                &format!("{} more conflicts", item.conflicts.len() - 5),
                13.0,
            );
        }
        let buttons = panel::row(world, row);
        panel::button(
            world,
            buttons,
            owner,
            "Edit",
            Command::Open(Some(settings.clone())),
        );
        panel::button(
            world,
            buttons,
            owner,
            if settings.enabled { "Stop" } else { "Start" },
            Command::SetEnabled(settings.uid.clone(), !settings.enabled),
        );
        if removal.as_ref() == Some(&settings.uid) {
            panel::button(
                world,
                buttons,
                owner,
                "Confirm removal",
                Command::Remove(settings.uid.clone()),
            );
            panel::button(world, buttons, owner, "Cancel", Command::Close);
        } else {
            panel::button(
                world,
                buttons,
                owner,
                "Remove",
                Command::ConfirmRemoval(settings.uid.clone()),
            );
        }
    }
}

fn open(world: &mut World, owner: Entity, settings: Option<&DirectorySync>) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let editor = view.editor;
    let mut proteins: Vec<_> = world
        .get::<super::SyncCastle>(view.castle)
        .into_iter()
        .flat_map(|castle| castle.names.iter())
        .map(|(id, name)| (name.clone(), id.clone()))
        .collect();
    proteins.sort();
    panel::clear(world, editor);
    let name = panel::field(
        world,
        editor,
        "Name",
        settings.map_or("", |item| &item.name),
    );
    let origin = match settings {
        Some(item) => {
            super::label(world, editor, &format!("File origin {}", item.uid), 13.0);
            Ok(item.uid.clone())
        }
        None => Err(panel::field(
            world,
            editor,
            "File origin (e.g. institute)",
            "",
        )),
    };
    let directory = panel::field(
        world,
        editor,
        "Absolute directory",
        settings.map_or("", |item| &item.path),
    );
    let format = settings.map_or(FileFormat::Lingua, |item| item.format);
    let format_label = super::label(world, editor, super::format_name(format), 14.0);
    let formats = panel::row(world, editor);
    panel::button(
        world,
        formats,
        owner,
        ".lingua (includes subfolders)",
        Command::Format(FileFormat::Lingua),
    );
    panel::button(
        world,
        formats,
        owner,
        "Markdown (.md)",
        Command::Format(FileFormat::Markdown),
    );
    let protein = settings.and_then(|item| item.protein.clone());
    let all = "All Records with this file origin";
    let selected = protein
        .as_deref()
        .map_or(all, |uid| {
            proteins
                .iter()
                .find(|(_, id)| id == uid)
                .map_or(uid, |(name, _)| name)
        })
        .to_owned();
    let mut choices = vec![(
        all.into(),
        crate::native::actions![Command::Protein(None, all.into())],
    )];
    choices.extend(proteins.into_iter().map(|(name, uid)| {
        (
            name.clone(),
            crate::native::actions![Command::Protein(Some(uid), name)],
        )
    }));
    super::label(world, editor, "Export Protein", 14.0);
    let dropdown =
        crate::native::dropdown::spawn(world, editor, owner, "Export Protein", &selected, choices);
    let protein_label = world
        .get::<Children>(dropdown)
        .and_then(|children| children.first())
        .copied()
        .expect("dropdown caption");
    let row = panel::row(world, editor);
    panel::button(world, row, owner, "Save", Command::Save);
    panel::button(world, row, owner, "Cancel", Command::Close);
    world
        .get_mut::<View>(owner)
        .expect("mounted folder editor")
        .form = Some(Form {
        name,
        origin,
        directory,
        format,
        format_label,
        protein,
        protein_label,
        enabled: settings.is_some_and(|item| item.enabled),
    });
}

fn request(world: &mut World, owner: Entity, request: Request) {
    let status = world
        .get::<View>(owner)
        .expect("mounted folder syncs")
        .status;
    let (Some(runtime), Ok(handle)) = (
        crate::native::practice_cells::runtime(world, owner),
        tokio::runtime::Handle::try_current(),
    ) else {
        panel::status(world, status, "Local Cell unavailable");
        return;
    };
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let result = runtime
            .engine
            .act(engine::actions::Action::DirectorySync { request }, None)
            .await
            .map_err(|error| error.to_string())
            .and_then(|outcome| {
                serde_json::from_value(outcome.data.unwrap_or_default()["directories"].clone())
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            });
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    world.entity_mut(owner).insert(Job(receiver));
    panel::status(world, status, "Saving…");
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<Job>(owner).is_some() || crate::native::laboratory::blocked(world, owner) {
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        match self {
            Self::Open(settings) => open(world, owner, settings.as_ref()),
            Self::Close => {
                let (editor, entries) = (view.editor, view.entries.clone());
                panel::clear(world, editor);
                let mut view = world.get_mut::<View>(owner).expect("mounted folder syncs");
                view.form = None;
                if view.removal.take().is_some() {
                    render(world, owner, entries, true);
                }
            }
            Self::Format(value) => {
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                let Some(form) = &mut view.form else { return };
                form.format = *value;
                let label = form.format_label;
                panel::status(world, label, super::format_name(*value));
            }
            Self::Protein(uid, name) => {
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                let Some(form) = &mut view.form else { return };
                form.protein = uid.clone();
                let label = form.protein_label;
                panel::status(world, label, name);
            }
            Self::ConfirmRemoval(uid) => {
                let entries = view.entries.clone();
                world
                    .get_mut::<View>(owner)
                    .expect("mounted folder syncs")
                    .removal = Some(uid.clone());
                render(world, owner, entries, true);
                let status = world
                    .get::<View>(owner)
                    .expect("mounted folder syncs")
                    .status;
                panel::status(
                    world,
                    status,
                    "Removing this sync keeps its files and Records.",
                );
            }
            Self::SetEnabled(uid, enabled) => request(
                world,
                owner,
                Request::SetEnabled {
                    uid: uid.clone(),
                    enabled: *enabled,
                },
            ),
            Self::Remove(uid) if view.removal.as_ref() == Some(uid) => {
                request(world, owner, Request::Remove { uid: uid.clone() });
            }
            Self::Remove(_) => {}
            Self::Save => {
                let Some(form) = &view.form else { return };
                let settings = (|| -> Result<_, String> {
                    Ok(DirectorySync {
                        uid: match &form.origin {
                            Ok(uid) => uid.clone(),
                            Err(entity) => panel::value(world, *entity)?.trim().to_owned(),
                        },
                        name: panel::value(world, form.name)?,
                        path: panel::value(world, form.directory)?,
                        format: form.format,
                        protein: form.protein.clone(),
                        enabled: form.enabled,
                    })
                })();
                match settings {
                    Ok(settings) => request(
                        world,
                        owner,
                        Request::Save {
                            settings,
                            create: form.origin.is_err(),
                        },
                    ),
                    Err(error) => panel::status(world, view.status, error),
                }
            }
        }
    }
}

pub(super) fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        if super::snapshots::needs_watch::<Watch>(world, owner)
            && !crate::native::laboratory::blocked(world, owner)
            && let (Some(runtime), Ok(handle)) = (
                crate::native::practice_cells::runtime(world, owner),
                tokio::runtime::Handle::try_current(),
            )
        {
            let wake = world
                .get_resource::<crate::native::wake::WakeSignal>()
                .cloned();
            let (sender, receiver) = tokio::sync::watch::channel(None);
            let task = handle.spawn(async move {
                let mut refresh =
                    super::snapshots::QueryRefresh::new(&runtime.engine, Duration::from_secs(3));
                while refresh.next().await {
                    let value = runtime
                        .engine
                        .directory_sync_statuses()
                        .await
                        .map(Arc::new)
                        .map_err(|error| error.to_string());
                    if !super::snapshots::publish(&sender, value, wake.as_ref()) {
                        break;
                    }
                }
            });
            world.entity_mut(owner).insert(Watch { receiver, task });
        }
        let snapshot = world.get_mut::<Watch>(owner).and_then(|mut watch| {
            watch
                .receiver
                .has_changed()
                .unwrap_or(false)
                .then(|| watch.receiver.borrow_and_update().clone())
                .flatten()
        });
        if world.get::<Job>(owner).is_none()
            && let Some(snapshot) = snapshot
        {
            match snapshot {
                Ok(entries) => render(world, owner, entries, false),
                Err(error) => {
                    let status = world
                        .get::<View>(owner)
                        .expect("mounted folder syncs")
                        .status;
                    panel::status(world, status, error);
                }
            }
        }
        let result = world
            .get_mut::<Job>(owner)
            .and_then(|mut job| match job.0.try_recv() {
                Ok(result) => Some(result),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(_) => Some(Err(
                    "Request interrupted. Reopen Sync to inspect the current settings.".into(),
                )),
            });
        if let Some(result) = result {
            world.entity_mut(owner).remove::<Job>();
            let status = world
                .get::<View>(owner)
                .expect("mounted folder syncs")
                .status;
            match result {
                Ok(entries) => {
                    Command::Close.apply(world, owner);
                    render(world, owner, entries, true);
                    panel::status(world, status, "Saved");
                }
                Err(error) => panel::status(world, status, error),
            }
        }
    }
}
