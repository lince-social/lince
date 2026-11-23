//! A local source directory grants access to its published catalogue. Import reads only the
//! permitted dependency closure of the chosen pin. Placements, Records and jobs stay at source.
use super::*;
use nucleus::canvas::scene::{DefinitionKind, MAX_PARTS, Pin, definitions::DEFINITION_DOMAIN};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const PROFILE: &str = "definition-library/1";
const INDEX: &str = "lince.workspace.definition-index.v1";
pub const MAX_CLOSURE: usize = 32;
pub const MAX_DEPTH: usize = 8;
pub const MAX_IMPORT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub pin: Pin,
    /// Private bodies have no published name.
    pub name: Option<String>,
}

pub(super) fn needs_index(scene: &Scene) -> bool {
    scene.parts.iter().any(|p| {
        p.source.is_some()
            || p.local_only
            || !p.requires.is_empty()
            || p.intrinsic != DefinitionKind::Part
    }) || scene
        .parts
        .iter()
        .map(|p| &p.id)
        .collect::<BTreeSet<_>>()
        .len()
        != scene.parts.len()
}

pub(super) fn write_all(writer: &mut Writer<'_>, scene: &Scene) -> io::Result<CanonicalHash> {
    let mut rows = Vec::with_capacity(scene.parts.len());
    for part in scene.parts.iter() {
        let revision = writer.page(DEFINITION_DOMAIN, part.content().as_ref())?;
        rows.push(Entry {
            pin: Pin {
                document: part.source.as_ref().unwrap_or(&scene.document).clone(),
                part: part.id.clone(),
                revision,
            },
            name: (!part.local_only).then(|| part.name.clone()),
        });
    }
    writer.page(INDEX, &rows)
}

fn verify_index(rows: &[Entry]) -> io::Result<()> {
    let mut pins = BTreeSet::new();
    if rows.len() > MAX_PARTS
        || rows.iter().any(|r| {
            !r.pin.valid()
                || !pins.insert(&r.pin)
                || r.name.as_ref().is_some_and(|s| {
                    s.trim().is_empty() || s.len() > 128 || s.chars().any(char::is_control)
                })
        })
    {
        return Err(invalid("Invalid or duplicate definition catalogue entry."));
    }
    Ok(())
}

fn unpack(mut body: Part, row: &Entry, owner: &DocumentId) -> io::Result<Part> {
    body.validate().map_err(invalid)?;
    if body.source.is_some()
        || body.local_only
        || body.id != row.pin.part
        || row.name.as_ref().is_some_and(|name| name != &body.name)
    {
        return Err(invalid(
            "Definition body disagrees with its catalogue entry.",
        ));
    }
    body.source = (&row.pin.document != owner).then(|| row.pin.document.clone());
    body.local_only = row.name.is_none();
    Ok(body)
}

pub(super) fn read_all(dir: &Path, manifest: &Manifest) -> io::Result<Vec<Part>> {
    let rows: Vec<Entry> = page(dir, INDEX, &manifest.definitions)?;
    verify_index(&rows)?;
    rows.iter()
        .map(|r| {
            unpack(
                page(dir, DEFINITION_DOMAIN, &r.pin.revision)?,
                r,
                &manifest.document_id,
            )
        })
        .collect()
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct ReadCost {
    pub files: usize,
    pub bytes: usize,
    pub largest_input_bytes: usize,
}

impl ReadCost {
    fn take<T: DeserializeOwned>(&mut self, path: &Path) -> io::Result<T> {
        let input = file_bytes(path, LIMIT.min(MAX_IMPORT_BYTES.saturating_sub(self.bytes)))?;
        let bytes = input.len();
        self.files += 1;
        self.bytes += bytes;
        self.largest_input_bytes = self.largest_input_bytes.max(bytes);
        serde_json::from_slice(&input).map_err(invalid)
    }
    fn page<T: DeserializeOwned + Serialize>(
        &mut self,
        dir: &Path,
        purpose: &str,
        hash: &CanonicalHash,
    ) -> io::Result<T> {
        let value = self.take(&page_path(dir, hash))?;
        if digest(purpose, &value)? != *hash {
            return Err(invalid("Definition page digest mismatch."));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    dir: PathBuf,
    pub document: DocumentId,
    pub revision: CanonicalHash,
    rows: Vec<Entry>,
    legacy: BTreeMap<Pin, Part>,
    pub cost: ReadCost,
}

impl Catalog {
    pub fn open(dir: &Path) -> io::Result<Self> {
        pages_directory(dir)?;
        let mut cost = ReadCost::default();
        let head: Head = cost.take(&dir.join("HEAD"))?;
        let manifest: Manifest = cost.page(dir, ROOT, head.revision())?;
        let mut legacy = BTreeMap::new();
        let rows: Vec<Entry> = if manifest.indexed()? {
            cost.page(dir, INDEX, &manifest.definitions)?
        } else {
            let parts: Vec<Part> = cost.page(dir, PARTS, &manifest.definitions)?;
            if parts.len() > MAX_PARTS {
                return Err(invalid("Too many definitions in the source."));
            }
            let mut rows = Vec::new();
            let mut ids = BTreeSet::new();
            for part in parts {
                part.validate().map_err(invalid)?;
                if part.source.is_some()
                    || part.local_only
                    || !part.requires.is_empty()
                    || part.intrinsic != DefinitionKind::Part
                    || !ids.insert(part.id.clone())
                {
                    return Err(invalid("Invalid legacy definition catalogue."));
                }
                let pin = part.pin(&manifest.document_id).map_err(invalid)?;
                rows.push(Entry {
                    pin: pin.clone(),
                    name: Some(part.name.clone()),
                });
                legacy.insert(pin, part);
            }
            rows
        };
        verify_index(&rows)?;
        Ok(Self {
            dir: dir.to_owned(),
            revision: head.revision().clone(),
            document: manifest.document_id,
            rows,
            legacy,
            cost,
        })
    }

    pub fn available(&self) -> impl Iterator<Item = &Entry> {
        self.rows.iter().filter(|e| e.name.is_some())
    }

    fn current(&self, cost: &mut ReadCost) -> io::Result<()> {
        let head: Head = cost.take(&self.dir.join("HEAD"))?;
        if head.revision() == &self.revision {
            Ok(())
        } else {
            Err(io::Error::other(
                "The source or its import permissions changed. Browse the source again.",
            ))
        }
    }

    pub fn preview(&self, root: &Pin, dest: &Scene) -> io::Result<Review> {
        self.resolve(root, dest).map(|(review, _)| review)
    }

    pub fn import(&self, root: &Pin, dest: &mut Scene, preview: &Review) -> io::Result<usize> {
        let (mut verified, added) = self.resolve(root, dest)?;
        if &verified.items != &preview.items || !verified.complete() {
            return Err(io::Error::other(
                "Incomplete or changed import. Review the required definitions again.",
            ));
        }
        if added.is_empty() {
            return Ok(0);
        }
        let mut next = dest.clone();
        let count = added.len();
        Arc::make_mut(&mut next.parts).extend(added);
        next.validate().map_err(invalid)?;
        self.current(&mut verified.cost)?;
        *dest = next;
        Ok(count)
    }

    fn resolve(&self, root: &Pin, dest: &Scene) -> io::Result<(Review, Vec<Part>)> {
        let mut walk = Walk {
            catalog: self,
            dest,
            seen: BTreeSet::new(),
            visiting: BTreeSet::new(),
            added: Vec::new(),
            review: Review {
                items: Vec::new(),
                cost: self.cost,
            },
        };
        self.current(&mut walk.review.cost)?;
        walk.visit(root, 0)?;
        if dest.parts.len() + walk.added.len() > MAX_PARTS {
            return Err(invalid(
                "Import exceeds the 64 definition revisions in a scene.",
            ));
        }
        self.current(&mut walk.review.cost)?;
        Ok((walk.review, walk.added))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    BringIn,
    Reuse,
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub pin: Pin,
    pub name: Option<String>,
    pub intrinsic: Option<DefinitionKind>,
    pub status: Status,
    pub conflicts: Vec<Pin>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Review {
    pub items: Vec<Item>,
    pub cost: ReadCost,
}
impl Review {
    pub fn complete(&self) -> bool {
        self.items
            .iter()
            .all(|r| !matches!(r.status, Status::Unavailable(_)))
    }
}
struct Walk<'a> {
    catalog: &'a Catalog,
    dest: &'a Scene,
    seen: BTreeSet<Pin>,
    visiting: BTreeSet<Pin>,
    added: Vec<Part>,
    review: Review,
}
impl Walk<'_> {
    fn read(&mut self, pin: &Pin) -> io::Result<Part> {
        let row = self
            .catalog
            .rows
            .iter()
            .find(|entry| &entry.pin == pin)
            .ok_or_else(|| invalid("Missing required definition."))?;
        if row.name.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "The source keeps this definition local.",
            ));
        }
        if let Some(part) = self.catalog.legacy.get(pin) {
            let mut part = part.clone();
            part.source = (pin.document != self.dest.document).then(|| pin.document.clone());
            return Ok(part);
        }
        let body = self
            .review
            .cost
            .page(&self.catalog.dir, DEFINITION_DOMAIN, &pin.revision)?;
        unpack(body, row, &self.dest.document)
    }

    fn visit(&mut self, pin: &Pin, depth: usize) -> io::Result<()> {
        if self.visiting.contains(pin) {
            return Err(invalid("Cycle in required definitions."));
        }
        if self.seen.contains(pin) {
            return Ok(());
        }
        if depth >= MAX_DEPTH || self.seen.len() >= MAX_CLOSURE {
            return Err(invalid(
                "Definition import exceeds its depth or closure bound.",
            ));
        }
        self.seen.insert(pin.clone());
        let part = match self.read(pin) {
            Ok(part) => part,
            Err(error) => {
                self.review.items.push(Item {
                    pin: pin.clone(),
                    name: None,
                    intrinsic: None,
                    status: Status::Unavailable(error.to_string()),
                    conflicts: vec![],
                });
                return Ok(());
            }
        };
        self.visiting.insert(pin.clone());
        for dep in &part.requires {
            self.visit(dep, depth + 1)?;
        }
        self.visiting.remove(pin);
        let mut conflicts = Vec::new();
        let mut reused = false;
        for other in self.dest.parts.iter().chain(&self.added) {
            if other.id != part.id && other.name != part.name {
                continue;
            }
            let other_pin = other.pin(&self.dest.document).map_err(invalid)?;
            if other_pin == *pin {
                if other.content() != part.content() {
                    return Err(invalid("Pinned definition content differs."));
                }
                reused = true;
            } else if (other.id == part.id && other_pin.document == pin.document)
                || other.name == part.name
            {
                conflicts.push(other_pin);
            }
        }
        self.review.items.push(Item {
            pin: pin.clone(),
            name: Some(part.name.clone()),
            intrinsic: Some(part.intrinsic.clone()),
            status: if reused {
                Status::Reuse
            } else {
                Status::BringIn
            },
            conflicts,
        });
        if !reused {
            self.added.push(part);
        }
        Ok(())
    }
}
