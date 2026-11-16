use super::*;
use crate::host::performance::{self, Count, Stage};
use crate::native::retained;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Recipient {
    Empty,
    Target(String, String, bool),
}

#[derive(Default)]
pub(super) struct Rows {
    recipients: retained::Children<Recipient>,
    transfers: retained::Children<Option<String>>,
}

#[derive(Component, Clone)]
struct TransferRow {
    current: Option<Arc<Transfer>>,
    bytes: u64,
    heading: Entity,
    peer: Entity,
    files: Entity,
    extra: Option<Entity>,
    progress: Option<Entity>,
    destination: Option<Entity>,
    error: Option<Entity>,
    controls: Entity,
}

#[derive(Component)]
struct FileList {
    transfer: Arc<Transfer>,
    expanded: bool,
    built: bool,
}

#[derive(Clone)]
struct ToggleFiles;

impl Action for ToggleFiles {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut list) = world.get_mut::<FileList>(owner) else {
            return;
        };
        list.expanded = !list.expanded;
        let expanded = list.expanded;
        let build = (expanded && !list.built).then(|| list.transfer.clone());
        list.built |= expanded;
        if let Some(transfer) = build {
            for entry in transfer.manifest.entries.iter().skip(12) {
                label(
                    world,
                    owner,
                    &format!("{} · {}", entry.path, size(entry.size)),
                );
            }
            performance::add(
                Count::BlobFileLabelsCreated,
                transfer.manifest.entries.len().saturating_sub(12) as u64,
            );
        }
        display(world, owner, expanded);
    }
}

fn display(world: &mut World, entity: Entity, visible: bool) {
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    if let Some(mut node) = world.get_mut::<Node>(entity)
        && node.display != display
    {
        node.display = display;
    }
}

fn optional_label(
    world: &mut World,
    parent: Entity,
    entity: &mut Option<Entity>,
    text: Option<String>,
) {
    if let Some(text) = text {
        if let Some(entity) = *entity {
            panel::status(world, entity, text);
            display(world, entity, true);
        } else {
            *entity = Some(label(world, parent, &text));
        }
    } else if let Some(entity) = *entity {
        display(world, entity, false);
    }
}

fn file_list(world: &mut World, parent: Entity, transfer: &Arc<Transfer>) -> Option<Entity> {
    panel::clear(world, parent);
    for entry in transfer.manifest.entries.iter().take(12) {
        label(
            world,
            parent,
            &format!(
                "{}{}",
                entry.path,
                if entry.hash.is_none() { "/" } else { "" }
            ),
        );
    }
    performance::add(
        Count::BlobFileLabelsCreated,
        transfer.manifest.entries.len().min(12) as u64,
    );
    if transfer.manifest.entries.len() <= 12 {
        return None;
    }
    let details = panel::column(world, parent);
    world.entity_mut(details).insert(FileList {
        transfer: transfer.clone(),
        expanded: false,
        built: false,
    });
    display(world, details, false);
    panel::button(
        world,
        parent,
        details,
        &format!(
            "Show / hide {} more entries",
            transfer.manifest.entries.len() - 12
        ),
        ToggleFiles,
    );
    Some(details)
}

// Action identity depends on the transfer ID and these predicates, not on its
// changing progress or status label. Keep held/focused controls until that
// identity changes; backend actions still revalidate their current authority.
fn actions(transfer: &Transfer) -> (bool, bool, bool) {
    let incoming = transfer.direction == "incoming";
    let retry = incoming && transfer.state == "accepted" && !transfer.error.is_empty();
    let offer = incoming && transfer.state == "offered";
    let cancel = !offer && matches!(transfer.state.as_str(), "offered" | "accepted");
    (retry, offer, cancel)
}

fn controls(world: &mut World, parent: Entity, owner: Entity, transfer: &Transfer) {
    panel::clear(world, parent);
    let (retry, offer, cancel) = actions(transfer);
    if retry {
        panel::button(
            world,
            parent,
            owner,
            "Change save folder…",
            Command::Accept(transfer.id.clone()),
        );
    }
    if offer {
        panel::button(
            world,
            parent,
            owner,
            "Accept and save…",
            Command::Accept(transfer.id.clone()),
        );
        panel::button(
            world,
            parent,
            owner,
            "Decline",
            Command::Stop(transfer.id.clone()),
        );
    } else if cancel {
        panel::button(
            world,
            parent,
            owner,
            "Cancel copy",
            Command::Stop(transfer.id.clone()),
        );
    }
}

fn create(world: &mut World, parent: Entity, owner: Entity, transfer: &Arc<Transfer>) -> Entity {
    let root = panel::column(world, parent);
    let mut row = TransferRow {
        current: None,
        bytes: 0,
        heading: label(world, root, ""),
        peer: label(world, root, ""),
        files: panel::column(world, root),
        extra: None,
        progress: None,
        destination: None,
        error: None,
        controls: panel::row(world, root),
    };
    apply(world, root, owner, &mut row, transfer);
    world.entity_mut(root).insert(row);
    performance::count(Count::BlobTransferRowsCreated);
    root
}

fn update(world: &mut World, root: Entity, owner: Entity, transfer: &Arc<Transfer>) {
    performance::count(Count::BlobTransferRowsRetained);
    let row = world.get::<TransferRow>(root).unwrap();
    if row
        .current
        .as_ref()
        .is_some_and(|old| Arc::ptr_eq(old, transfer))
    {
        return;
    }
    let mut row = row.clone();
    apply(world, root, owner, &mut row, transfer);
    world.entity_mut(root).insert(row);
}

fn apply(
    world: &mut World,
    root: Entity,
    owner: Entity,
    row: &mut TransferRow,
    transfer: &Arc<Transfer>,
) {
    let previous = row.current.take();
    let previous = previous.as_deref();
    let manifest_changed = previous.is_none_or(|old| old.manifest != transfer.manifest);
    if manifest_changed {
        row.bytes = transfer.manifest.bytes();
        row.extra = file_list(world, row.files, transfer);
    } else if let Some(extra) = row.extra {
        // Repoint retained details at the latest owned model, so a long-lived
        // expanded list does not pin a separate old transfer/manifest copy.
        world.get_mut::<FileList>(extra).unwrap().transfer = transfer.clone();
    }
    if manifest_changed
        || previous.is_none_or(|old| {
            old.direction != transfer.direction
                || old.label != transfer.label
                || old.state != transfer.state
        })
    {
        let incoming = transfer.direction == "incoming";
        panel::status(
            world,
            row.heading,
            format!(
                "{} {} · {} · {}",
                if incoming { "From" } else { "To" },
                transfer.label,
                size(row.bytes),
                state(&transfer.state, incoming)
            ),
        );
    }
    if previous.is_none_or(|old| old.peer != transfer.peer) {
        panel::status(
            world,
            row.peer,
            format!("Peer {}", transfer.peer.chars().take(8).collect::<String>()),
        );
    }
    if manifest_changed
        || previous
            .is_none_or(|old| old.state != transfer.state || old.progress != transfer.progress)
    {
        optional_label(
            world,
            root,
            &mut row.progress,
            (transfer.state == "accepted")
                .then(|| format!("{} / {}", size(transfer.progress), size(row.bytes))),
        );
    }
    if previous.is_none_or(|old| old.destination != transfer.destination) {
        optional_label(
            world,
            root,
            &mut row.destination,
            transfer.destination.clone(),
        );
    }
    if previous.is_none_or(|old| old.error != transfer.error) {
        optional_label(
            world,
            root,
            &mut row.error,
            (!transfer.error.is_empty())
                .then(|| format!("{} · Lince will retry automatically.", transfer.error)),
        );
    }
    if previous.is_none_or(|old| actions(old) != actions(transfer)) {
        controls(world, row.controls, owner, transfer);
    }
    retained::order(
        world,
        root,
        [
            Some(row.heading),
            Some(row.peer),
            Some(row.files),
            row.progress,
            row.destination,
            row.error,
            Some(row.controls),
        ]
        .into_iter()
        .flatten()
        .collect(),
    );
    row.current = Some(transfer.clone());
    performance::count(Count::BlobTransferRowsUpdated);
}

pub(super) fn render(
    world: &mut World,
    owner: Entity,
    previous: Option<&Snapshot>,
    snapshot: &Snapshot,
    rows: &mut Rows,
) {
    let _timing = performance::Span::new(Stage::NativeBlobPresentation);
    let view = world.get::<View>(owner).unwrap();
    let (targets, transfers) = (view.targets, view.transfers);
    if previous.is_none_or(|old| old.targets != snapshot.targets) {
        let items = snapshot
            .targets
            .iter()
            .map(|target| {
                (
                    Recipient::Target(target.node_id.clone(), target.label.clone(), target.nearby),
                    Some(target),
                )
            })
            .chain(
                snapshot
                    .targets
                    .is_empty()
                    .then_some((Recipient::Empty, None)),
            );
        rows.recipients.reconcile(
            world,
            targets,
            items,
            |world, parent, target| {
                if let Some(target) = target {
                    panel::button(
                        world,
                        parent,
                        owner,
                        &format!(
                            "{} · {} · {}",
                            target.label,
                            if target.nearby { "Nearby" } else { "Contact" },
                            target.node_id.chars().take(8).collect::<String>()
                        ),
                        Command::Select(target.node_id.clone(), target.label.clone()),
                    )
                } else {
                    label(
                        world,
                        parent,
                        "No recipients. Add a contact or enable LAN discovery in Organ settings.",
                    )
                }
            },
            |_, _, _| {},
        );
    }
    let items = snapshot
        .transfers
        .iter()
        .map(|transfer| (Some(transfer.id.clone()), Some(transfer)))
        .chain(snapshot.transfers.is_empty().then_some((None, None)));
    rows.transfers.reconcile(
        world,
        transfers,
        items,
        |world, parent, transfer| {
            if let Some(transfer) = transfer {
                create(world, parent, owner, transfer)
            } else {
                label(world, parent, "No copies sent or received yet.")
            }
        },
        |world, row, transfer| {
            if let Some(transfer) = transfer {
                update(world, row, owner, transfer);
            }
        },
    );
}
