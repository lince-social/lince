//! Local directory storage for the bounded composition profile. HEAD is the only mutable reference.
use nucleus::{
    canvas::scene::{CanvasBinding, DocumentId, Part, RootId, Scene, Slot},
    karma::{CanonicalHash, canonical_hash, canonical_json_bytes},
};
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
    pub scene: Scene,
    pub revision: CanonicalHash,
}

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn read<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > LIMIT as u64 {
        return Err(invalid("Expected a bounded scene file."));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(path)?
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > LIMIT {
        return Err(invalid("Scene page exceeds 64 KiB."));
    }
    serde_json::from_slice(&bytes).map_err(invalid)
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

fn profiles(bound: bool) -> Vec<String> {
    let mut profiles = vec!["composition/1".into()];
    if bound {
        profiles.push("canvas-room/1".into());
    }
    profiles
}

pub fn open(dir: &Path) -> io::Result<Opened> {
    pages_directory(dir)?;
    let revision: CanonicalHash = read(&dir.join("HEAD"))?;
    let manifest: Manifest = page(dir, ROOT, &revision)?;
    if manifest.format != "lince.workspace.model"
        || manifest.schema != 1
        || manifest.required_profiles != profiles(manifest.canvas.is_some())
        || manifest.unit != "m"
    {
        return Err(invalid(
            "Unsupported scene format, required profile or unit.",
        ));
    }
    let parts: Vec<Part> = page(dir, PARTS, &manifest.definitions)?;
    let slots: Vec<Slot> = page(dir, SLOTS, &manifest.placements)?;
    let scene = Scene {
        document: manifest.document_id,
        root: manifest.root_occurrence,
        canvas: manifest.canvas,
        parts: Arc::new(parts),
        slots: Arc::new(slots),
    };
    scene.validate().map_err(invalid)?;
    Ok(Opened { scene, revision })
}

fn staged(dir: &Path, bytes: &[u8]) -> io::Result<tempfile::NamedTempFile> {
    let mut file = tempfile::Builder::new()
        .prefix(".stage-")
        .tempfile_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn write_page<T: Serialize + DeserializeOwned>(
    dir: &Path,
    purpose: &str,
    value: &T,
) -> io::Result<CanonicalHash> {
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
        }
        Err(e) => return Err(e),
    }
    Ok(hash)
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
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(".scene-lock"))?;
    lock.try_lock()
        .map_err(|e| io::Error::other(format!("Cannot lock this scene for saving: {e}")))?;
    let head = dir.join("HEAD");
    let actual: Option<CanonicalHash> = match read(&head) {
        Ok(revision) => Some(revision),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if expected != actual.as_ref() {
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
    let manifest = Manifest {
        format: "lince.workspace.model".into(),
        schema: 1,
        required_profiles: profiles(scene.canvas.is_some()),
        canvas: scene.canvas.clone(),
        document_id: scene.document.clone(),
        parent: actual,
        root_occurrence: scene.root.clone(),
        unit: "m".into(),
        definitions: write_page(dir, PARTS, &*scene.parts)?,
        placements: write_page(dir, SLOTS, &*scene.slots)?,
    };
    let revision = write_page(dir, ROOT, &manifest)?;
    File::open(dir.join("pages"))?.sync_all()?;
    let next = staged(dir, &canonical_json_bytes(&revision).map_err(invalid)?)?;
    before_head()?;
    next.persist(head)?;
    File::open(dir)?.sync_all()?;
    Ok(revision)
}

#[cfg(test)]
mod tests;
