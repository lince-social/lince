//! A scene and its exact receipt share one atomic HEAD publication. Existing scene directories
//! keep their original HEAD representation until the first acknowledged operation.
use super::*;
use nucleus::{
    canvas::scene::operation::{Edit, Effect, Operation, Receipt},
    component::composition::Origin,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const INDEX: &str = "lince.workspace.request-index.v1";
const RECEIPT: &str = "lince.workspace.edit-receipt.v1";

// Prefixes are read from the entropy end of the validated request ULID. A leaf/page never
// grows past 32 entries, and a lookup reads at most 26 branches even for colliding prefixes.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Index {
    Leaf(BTreeMap<String, CanonicalHash>),
    Branch(BTreeMap<u8, CanonicalHash>),
}

fn digit(id: &str, depth: usize) -> io::Result<u8> {
    id.as_bytes()
        .iter()
        .rev()
        .take(26)
        .nth(depth)
        .copied()
        .ok_or_else(|| invalid("Invalid request index depth."))
}

fn lookup(dir: &Path, mut index: CanonicalHash, id: &str) -> io::Result<Option<Receipt>> {
    for depth in 0..=26 {
        match page(dir, INDEX, &index)? {
            Index::Leaf(entries) => {
                return entries
                    .get(id)
                    .map(|hash| {
                        let receipt: Receipt = page(dir, RECEIPT, hash)?;
                        if receipt.edit.request_id != id {
                            return Err(invalid("Receipt identity mismatch."));
                        }
                        Ok(receipt)
                    })
                    .transpose();
            }
            Index::Branch(children) => match children.get(&digit(id, depth)?) {
                Some(hash) => index = hash.clone(),
                None => return Ok(None),
            },
        }
    }
    Err(invalid("Invalid receipt index."))
}

fn pack(
    writer: &mut Writer<'_>,
    entries: BTreeMap<String, CanonicalHash>,
    depth: usize,
) -> io::Result<CanonicalHash> {
    if entries.len() <= 32 {
        return writer.page(INDEX, &Index::Leaf(entries));
    }
    let mut groups = BTreeMap::<u8, BTreeMap<_, _>>::new();
    for (id, hash) in entries {
        groups
            .entry(digit(&id, depth)?)
            .or_default()
            .insert(id, hash);
    }
    let mut children = BTreeMap::new();
    for (digit, group) in groups {
        children.insert(digit, pack(writer, group, depth + 1)?);
    }
    writer.page(INDEX, &Index::Branch(children))
}

fn insert(
    writer: &mut Writer<'_>,
    index: Option<&CanonicalHash>,
    id: &str,
    receipt: CanonicalHash,
    depth: usize,
) -> io::Result<CanonicalHash> {
    let node = index
        .map(|hash| page(writer.dir, INDEX, hash))
        .transpose()?
        .unwrap_or_else(|| Index::Leaf(BTreeMap::new()));
    match node {
        Index::Leaf(mut entries) => {
            entries.insert(id.to_owned(), receipt);
            pack(writer, entries, depth)
        }
        Index::Branch(mut children) => {
            let key = digit(id, depth)?;
            let child = insert(writer, children.get(&key), id, receipt, depth + 1)?;
            children.insert(key, child);
            writer.page(INDEX, &Index::Branch(children))
        }
    }
}

pub(super) fn last(dir: &Path, head: &IndexedHead, document: &DocumentId) -> io::Result<Receipt> {
    by_id(dir, head, document, &head.last_request)?
        .ok_or_else(|| invalid("Missing last accepted receipt."))
}

fn by_id(
    dir: &Path,
    head: &IndexedHead,
    document: &DocumentId,
    id: &str,
) -> io::Result<Option<Receipt>> {
    nucleus::canvas::request_identifier(id).map_err(invalid)?;
    let receipt = lookup(dir, head.receipts.clone(), id)?;
    if receipt
        .as_ref()
        .is_some_and(|r| &r.edit.document != document)
    {
        return Err(invalid("Receipt belongs to another document."));
    }
    Ok(receipt)
}

pub fn receipt(dir: &Path, document: &DocumentId, request_id: Option<&str>) -> io::Result<Receipt> {
    let head: Head = read(&dir.join("HEAD"))?;
    if load_revision(dir, head.revision())?.document != *document {
        return Err(invalid("Scene document changed."));
    }
    if let Head::Journal(head) = head
        && let Some(receipt) = by_id(
            dir,
            &head,
            document,
            request_id.unwrap_or(&head.last_request),
        )?
    {
        return Ok(receipt);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No accepted receipt for this request.",
    ))
}

#[derive(Debug)]
pub struct Accepted {
    pub current: Opened,
    pub receipt: Receipt,
    pub replayed: bool,
    pub writes: WriteMetrics,
    pub processing: Duration,
}

/// The guard rechecks projection eligibility and authority immediately before publishing.
/// A retry reads the original receipt while returning the current scene, even after other edits.
pub fn accept(
    dir: &Path,
    edit: &Edit,
    fiote: Option<&Origin>,
    guard: impl FnOnce(&Scene) -> io::Result<()>,
) -> io::Result<Accepted> {
    let start = Instant::now();
    edit.validate().map_err(invalid)?;
    if fiote
        .is_some_and(|f| !nucleus::valid_uid(&f.agent, "r") || !nucleus::valid_uid(&f.thread, "r"))
    {
        return Err(invalid("Invalid Fiote origin."));
    }
    pages_directory(dir)?;
    let _lock = lock(dir)?;
    let head: Head = read(&dir.join("HEAD"))?;
    let current = open_head(dir, &head)?;
    if current.scene.document != edit.document {
        return Err(invalid("The captured document is unavailable."));
    }
    let mut index = None;
    if let Head::Journal(head) = &head {
        if let Some(receipt) = by_id(dir, head, &edit.document, &edit.request_id)? {
            if &receipt.edit != edit || receipt.fiote.as_ref() != fiote {
                return Err(invalid(
                    "The request ID already belongs to a different command or Actor.",
                ));
            }
            guard(&current.scene)?;
            return Ok(Accepted {
                receipt,
                current,
                replayed: true,
                writes: WriteMetrics::default(),
                processing: start.elapsed(),
            });
        }
        index = Some(&head.receipts);
    }
    if edit.expected != current.revision {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Scene revision conflict. Inspect the accepted revision and retain your draft.",
        ));
    }
    let mut next = current.scene;
    let (handle, effect) = match &edit.operation {
        Operation::PlaceCopy { source } => {
            (next.place_again(source).map_err(invalid)?, Effect::Created)
        }
        Operation::Move { target, metres } => {
            let slot = next
                .slot(target)
                .ok_or_else(|| invalid("The captured placement is unavailable."))?;
            let old = slot.metres;
            next.edit(target, *metres, slot.record.clone())
                .map_err(invalid)?;
            if next.slot(target).unwrap().metres == old {
                return Err(invalid("The placement already has these coordinates."));
            }
            (target.clone(), Effect::Moved)
        }
        Operation::Undo { request_id } => {
            let Head::Journal(head) = &head else {
                return Err(invalid("No accepted edit to undo."));
            };
            let prior = by_id(dir, head, &edit.document, request_id)?
                .ok_or_else(|| invalid("Unknown accepted edit."))?;
            if prior.revision != current.revision {
                return Err(invalid("Undo requires the current accepted edit."));
            }
            let restored = load_revision(dir, &prior.edit.expected)?;
            if restored.document != next.document || restored.root != next.root {
                return Err(invalid("Undo source belongs to another scene."));
            }
            next = restored;
            (prior.handle, prior.effect.inverse())
        }
    };
    next.validate().map_err(invalid)?;
    let mut writer = Writer::new(dir);
    let revision = writer.scene(&next, Some(current.revision))?;
    let receipt = Receipt {
        edit: edit.clone(),
        fiote: fiote.cloned(),
        revision: revision.clone(),
        handle,
        effect,
    };
    let artifact = writer.page(RECEIPT, &receipt)?;
    let receipts = insert(&mut writer, index, &edit.request_id, artifact, 0)?;
    let head = Head::Journal(IndexedHead {
        revision: revision.clone(),
        receipts,
        last_request: edit.request_id.clone(),
    });
    writer.publish(&head, || guard(&next))?;
    Ok(Accepted {
        current: Opened {
            last_receipt: Some(receipt.clone()),
            scene: next,
            revision,
        },
        receipt,
        writes: writer.metrics,
        replayed: false,
        processing: start.elapsed(),
    })
}
