//! Local directory storage for the bounded composition profile. HEAD is the only mutable reference.
use nucleus::{
    canvas::scene::{CanvasBinding, DocumentId, Part, RootId, Scene, Slot},
    karma::{CanonicalHash, canonical_hash, canonical_json_bytes},
};
pub mod acceptance;
pub mod library;
pub mod measurement;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const LIMIT: usize = 65_536;
const ROOT: &str = "lince.workspace.manifest.v1";
const PARTS: &str = "lince.workspace.parts.v1";
const SLOTS: &str = "lince.workspace.slots.v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    schema: u32,
    required_profiles: Vec<String>,
    document_id: DocumentId,
    parent: Option<CanonicalHash>,
    root_occurrence: RootId,
    unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canvas: Option<CanvasBinding>,
    definitions: CanonicalHash,
    placements: CanonicalHash,
}

#[derive(Debug)]
pub struct Opened {
    pub last_receipt: Option<nucleus::canvas::scene::operation::Receipt>,
    pub scene: Scene,
    pub revision: CanonicalHash,
}

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Head {
    Legacy(CanonicalHash),
    Journal(IndexedHead),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexedHead {
    revision: CanonicalHash,
    receipts: CanonicalHash,
    last_request: String,
}

impl Head {
    fn revision(&self) -> &CanonicalHash {
        match self {
            Self::Legacy(hash) => hash,
            Self::Journal(head) => &head.revision,
        }
    }
    fn advance(self, revision: CanonicalHash) -> Self {
        match self {
            Self::Legacy(_) => Self::Legacy(revision),
            Self::Journal(h) => Self::Journal(IndexedHead { revision, ..h }),
        }
    }
}

fn read<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    serde_json::from_slice(&file_bytes(path, LIMIT)?).map_err(invalid)
}

fn file_bytes(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return Err(invalid("Expected a bounded scene file."));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("Scene page exceeds its read budget."));
    }
    Ok(bytes)
}

fn digest<T: Serialize>(purpose: &str, value: &T) -> io::Result<CanonicalHash> {
    canonical_hash(purpose, value).map_err(invalid)
}

fn page_path(dir: &Path, hash: &CanonicalHash) -> PathBuf {
    dir.join("pages").join(format!(
        "{}.json",
        hash.as_str().trim_start_matches("sha256:")
    ))
}

fn page<T: DeserializeOwned + Serialize>(
    dir: &Path,
    purpose: &str,
    hash: &CanonicalHash,
) -> io::Result<T> {
    let value = read(&page_path(dir, hash))?;
    if digest(purpose, &value)? != *hash {
        return Err(invalid("Scene page digest mismatch."));
    }
    Ok(value)
}

fn pages_directory(dir: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(dir.join("pages"))?.is_dir() {
        return Err(invalid("Expected a scene pages directory."));
    }
    Ok(())
}

fn profiles(bound: bool, indexed: bool) -> Vec<String> {
    let mut profiles = vec!["composition/1".into()];
    if bound {
        profiles.push("canvas-room/1".into());
    }
    if indexed {
        profiles.push(library::PROFILE.into());
    }
    profiles
}

pub fn open(dir: &Path) -> io::Result<Opened> {
    pages_directory(dir)?;
    let head: Head = read(&dir.join("HEAD"))?;
    open_head(dir, &head)
}

fn open_head(dir: &Path, head: &Head) -> io::Result<Opened> {
    let scene = load_revision(dir, head.revision())?;
    let last_receipt = match head {
        Head::Journal(indexed) => Some(acceptance::last(dir, indexed, &scene.document)?),
        Head::Legacy(_) => None,
    };
    Ok(Opened {
        last_receipt,
        scene,
        revision: head.revision().clone(),
    })
}

fn load_revision(dir: &Path, revision: &CanonicalHash) -> io::Result<Scene> {
    let manifest: Manifest = page(dir, ROOT, revision)?;
    let indexed = manifest.indexed()?;
    let parts = if indexed {
        library::read_all(dir, &manifest)?
    } else {
        page::<Vec<Part>>(dir, PARTS, &manifest.definitions)?
    };
    let slots: Vec<Slot> = page(dir, SLOTS, &manifest.placements)?;
    let scene = Scene {
        document: manifest.document_id,
        root: manifest.root_occurrence,
        canvas: manifest.canvas,
        parts: Arc::new(parts),
        slots: Arc::new(slots),
    };
    if !indexed && library::needs_index(&scene) {
        return Err(invalid(
            "Extended definitions require the definition library profile.",
        ));
    }
    scene.validate().map_err(invalid)?;
    Ok(scene)
}

impl Manifest {
    fn indexed(&self) -> io::Result<bool> {
        let indexed = self.required_profiles.iter().any(|p| p == library::PROFILE);
        if self.format != "lince.workspace.model"
            || self.schema != 1
            || self.required_profiles != profiles(self.canvas.is_some(), indexed)
            || self.unit != "m"
            || !self.document_id.valid()
            || !self.root_occurrence.valid()
        {
            return Err(invalid(
                "Unsupported scene format, required profile or unit.",
            ));
        }
        Ok(indexed)
    }
}

fn staged(dir: &Path, bytes: &[u8]) -> io::Result<tempfile::NamedTempFile> {
    let mut file = tempfile::Builder::new()
        .prefix(".stage-")
        .tempfile_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    Ok(file)
}

/// Encoded bytes actually staged, including the pointer published last.
#[derive(Debug, Default)]
pub struct WriteMetrics {
    pub staged_bytes: usize,
    pub largest_staged_file_bytes: usize,
}

impl WriteMetrics {
    fn count(&mut self, bytes: &[u8]) {
        self.staged_bytes += bytes.len();
        self.largest_staged_file_bytes = self.largest_staged_file_bytes.max(bytes.len());
    }
}

struct Writer<'a> {
    dir: &'a Path,
    metrics: WriteMetrics,
}

impl<'a> Writer<'a> {
    fn new(dir: &'a Path) -> Self {
        Self {
            dir,
            metrics: WriteMetrics::default(),
        }
    }

    fn page<T: Serialize + DeserializeOwned>(
        &mut self,
        purpose: &str,
        value: &T,
    ) -> io::Result<CanonicalHash> {
        let dir = self.dir;
        let bytes = canonical_json_bytes(value).map_err(invalid)?;
        if bytes.len() > LIMIT {
            return Err(invalid("Scene page exceeds 64 KiB."));
        }
        let hash = digest(purpose, value)?;
        let path = page_path(dir, &hash);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                page::<T>(dir, purpose, &hash)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                staged(&dir.join("pages"), &bytes)?.persist_noclobber(&path)?;
                self.metrics.count(&bytes);
            }
            Err(e) => return Err(e),
        }
        Ok(hash)
    }

    fn scene(&mut self, scene: &Scene, parent: Option<CanonicalHash>) -> io::Result<CanonicalHash> {
        let indexed = library::needs_index(scene);
        let manifest = Manifest {
            format: "lince.workspace.model".into(),
            schema: 1,
            required_profiles: profiles(scene.canvas.is_some(), indexed),
            canvas: scene.canvas.clone(),
            document_id: scene.document.clone(),
            parent,
            root_occurrence: scene.root.clone(),
            unit: "m".into(),
            definitions: if indexed {
                library::write_all(self, scene)?
            } else {
                self.page(PARTS, &*scene.parts)?
            },
            placements: self.page(SLOTS, &*scene.slots)?,
        };
        self.page(ROOT, &manifest)
    }

    fn publish(
        &mut self,
        head: &Head,
        before_head: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        File::open(self.dir.join("pages"))?.sync_all()?;
        let bytes = canonical_json_bytes(head).map_err(invalid)?;
        let next = staged(self.dir, &bytes)?;
        before_head()?;
        next.persist(self.dir.join("HEAD"))?;
        self.metrics.count(&bytes);
        File::open(self.dir)?.sync_all()
    }
}

#[cfg(test)]
fn write_page<T: Serialize + DeserializeOwned>(
    dir: &Path,
    purpose: &str,
    value: &T,
) -> io::Result<CanonicalHash> {
    Writer::new(dir).page(purpose, value)
}

fn ensure_directory(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let parent = dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure_directory(parent)?;
    match fs::create_dir(dir) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => (),
        Err(e) => return Err(e),
    }
    File::open(parent)?.sync_all()
}

/// A first publication expects no HEAD. Subsequent writes must name the root they edited.
pub fn save(
    dir: &Path,
    scene: &Scene,
    expected: Option<&CanonicalHash>,
) -> io::Result<CanonicalHash> {
    publish(dir, scene, expected, || Ok(()))
}

fn publish(
    dir: &Path,
    scene: &Scene,
    expected: Option<&CanonicalHash>,
    before_head: impl FnOnce() -> io::Result<()>,
) -> io::Result<CanonicalHash> {
    scene.validate().map_err(invalid)?;
    ensure_directory(dir)?;
    let _lock = lock(dir)?;
    let head = dir.join("HEAD");
    let actual: Option<Head> = match read(&head) {
        Ok(revision) => Some(revision),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if expected != actual.as_ref().map(Head::revision) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Scene revision conflict. Open the published version or save your draft to a new directory.",
        ));
    }
    if actual.is_some() {
        let previous = open(dir)?;
        if previous.scene.document != scene.document || previous.scene.root != scene.root {
            return Err(invalid("Scene identity differs from the published root."));
        }
        if previous.scene == *scene {
            return Ok(previous.revision);
        }
    }
    ensure_directory(&dir.join("pages"))?;
    pages_directory(dir)?;
    let mut writer = Writer::new(dir);
    let revision = writer.scene(scene, expected.cloned())?;
    let next = actual.map_or_else(
        || Head::Legacy(revision.clone()),
        |h| h.advance(revision.clone()),
    );
    writer.publish(&next, before_head)?;
    Ok(revision)
}

fn lock(dir: &Path) -> io::Result<File> {
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(".scene-lock"))?;
    lock.try_lock()
        .map_err(|e| io::Error::other(format!("Cannot lock this scene for saving: {e}")))?;
    Ok(lock)
}

#[cfg(test)]
mod tests;
