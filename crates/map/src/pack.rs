use crate::{
    Bounds, Tile,
    source::{MAX_DECODED_BYTES, MAX_TILE_BYTES, normalize},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_PACK_FILES: usize = 50_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Directory,
    Pmtiles,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub bytes: u64,
    pub expanded_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub format: Format,
    pub schema: String,
    pub dataset: String,
    pub source: String,
    pub date: String,
    pub coverage: Bounds,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub license: String,
    pub credits: String,
    pub files: Vec<File>,
}

fn message(error: impl ToString) -> String {
    error.to_string()
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.schema != "shortbread-1.1"
            || !self.coverage.valid()
            || self.min_zoom > self.max_zoom
            || self.max_zoom > crate::geometry::MAX_DATA_ZOOM
            || self.files.is_empty()
            || self.files.len() > MAX_PACK_FILES
            || [
                &self.dataset,
                &self.source,
                &self.date,
                &self.license,
                &self.credits,
            ]
            .into_iter()
            .any(|text| {
                text.trim().is_empty()
                    || text.len() > 4096
                    || text
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
            })
        {
            return Err("Use a version 1 Shortbread pack with valid coverage, zoom, source, date and credits".into());
        }
        if self.format == Format::Pmtiles && self.files.len() != 1 {
            return Err("A PMTiles pack contains one archive".into());
        }
        let mut paths = BTreeSet::new();
        let mut total = 0_u64;
        for file in &self.files {
            if !valid_relative(&file.path)
                || !paths.insert(file.path.clone())
                || file.bytes == 0
                || file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(
                    "Pack paths, sizes and SHA-256 checksums must be valid and unique".into(),
                );
            }
            total = total.checked_add(file.bytes).ok_or("Pack size overflows")?;
            if total > 65_536 * 1024 * 1024 {
                return Err("Split map packs larger than 64 GiB into regions".into());
            }
            if self.format == Format::Directory {
                if file.bytes > MAX_TILE_BYTES as u64
                    || file.expanded_bytes > MAX_DECODED_BYTES as u64
                    || file.expanded_bytes == 0
                {
                    return Err("A pack tile exceeds its encoded or expanded limit".into());
                }
                let parts: Vec<_> = file.path.split('/').collect();
                let tile = (|| {
                    Some(Tile {
                        z: parts.first()?.parse().ok()?,
                        x: parts.get(1)?.parse().ok()?,
                        y: parts.get(2)?.strip_suffix(".pbf")?.parse().ok()?,
                    })
                })();
                if parts.len() != 3
                    || tile.is_none_or(|tile| {
                        !tile.valid() || tile.z < self.min_zoom || tile.z > self.max_zoom
                    })
                {
                    return Err(
                        "Directory tiles use z/x/y.pbf within the declared zoom range".into(),
                    );
                }
            } else if !file.path.ends_with(".pmtiles") || file.expanded_bytes < file.bytes {
                return Err("Use a PMTiles archive with its declared stored size".into());
            }
        }
        Ok(())
    }

    pub fn identity(&self) -> Result<String, String> {
        self.validate()?;
        Ok(hex(&Sha256::digest(
            serde_json::to_vec(self).map_err(message)?,
        )))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 256
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn safe(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !valid_relative(relative) {
        return Err("Pack paths must remain inside their directory".into());
    }
    if fs::symlink_metadata(root).map_err(message)?.is_symlink() {
        return Err("Map pack roots cannot be symbolic links".into());
    }
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        path.push(component);
        if fs::symlink_metadata(&path).map_err(message)?.is_symlink() {
            return Err("Map packs cannot contain symbolic links".into());
        }
    }
    if !path
        .canonicalize()
        .map_err(message)?
        .starts_with(root.canonicalize().map_err(message)?)
    {
        return Err("Map pack path escapes its directory".into());
    }
    Ok(path)
}

pub fn manifest(root: &Path) -> Result<Manifest, String> {
    let path = safe(root, "manifest.json")?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(message)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(message)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Map manifest exceeds 4 MiB".into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(message)?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn archive_identity(path: &Path) -> Result<String, String> {
    let root = path
        .parent()
        .ok_or("Choose an archive inside a manifest directory")?;
    let manifest = manifest(root)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid map archive name")?;
    if manifest.format != Format::Pmtiles || manifest.files[0].path != name {
        return Err("The archive must match its pack manifest".into());
    }
    manifest.identity()
}

fn verified(root: &Path, file: &File) -> Result<Vec<u8>, String> {
    let path = safe(root, &file.path)?;
    if file.bytes > MAX_TILE_BYTES as u64 {
        return Err("Map tile exceeds its size limit".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(message)?
        .take(file.bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(message)?;
    if bytes.len() as u64 != file.bytes || hex(&Sha256::digest(&bytes)) != file.sha256 {
        return Err("Map tile size or checksum differs from its manifest".into());
    }
    if normalize(&bytes)?.len() as u64 > file.expanded_bytes {
        return Err("Map tile exceeds its declared expanded size".into());
    }
    Ok(bytes)
}

pub fn read_tile(root: &Path, tile: &str) -> Result<Option<Vec<u8>>, String> {
    let manifest = manifest(root)?;
    if manifest.format != Format::Directory {
        return Err("Choose a directory pack for local tiles".into());
    }
    manifest
        .files
        .iter()
        .find(|file| file.path == tile)
        .map(|file| verified(root, file))
        .transpose()
}

#[derive(Default)]
pub struct InstallProgress {
    pub copied: AtomicU64,
    pub total: AtomicU64,
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct CancelOnDrop(Arc<AtomicBool>, bool);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.1 {
            self.0.store(true, Ordering::Relaxed);
        }
    }
}

fn copy_file(
    root: &Path,
    target: Option<&Path>,
    file: &File,
    cancelled: &AtomicBool,
    progress: &InstallProgress,
) -> Result<(), String> {
    let path = safe(root, &file.path)?;
    let mut input = fs::File::open(path).map_err(message)?;
    if input.metadata().map_err(message)?.len() != file.bytes {
        return Err("Map file size differs from its manifest".into());
    }
    let mut output = target
        .map(|target| {
            let path = target.join(&file.path);
            fs::create_dir_all(path.parent().ok_or("Invalid pack destination")?)
                .map_err(message)?;
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(message)
        })
        .transpose()?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    let mut count = 0_u64;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Map pack import cancelled".into());
        }
        let read = input.read(&mut buffer).map_err(message)?;
        if read == 0 {
            break;
        }
        count = count.checked_add(read as u64).ok_or("Map size overflows")?;
        if count > file.bytes {
            return Err("Map file grew during import".into());
        }
        hash.update(&buffer[..read]);
        if let Some(output) = &mut output {
            output.write_all(&buffer[..read]).map_err(message)?;
        }
        progress.copied.fetch_add(read as u64, Ordering::Relaxed);
    }
    if count != file.bytes || hex(&hash.finalize()) != file.sha256 {
        return Err("Map file checksum differs from its manifest".into());
    }
    if let Some(output) = output {
        output.sync_all().map_err(message)?;
    }
    if file.path.ends_with(".pbf") {
        verified(target.unwrap_or(root), file)?;
    }
    Ok(())
}

fn contents(
    root: &Path,
    allowed: &BTreeSet<String>,
    directory: &Path,
    depth: usize,
    seen: &mut usize,
) -> Result<u64, String> {
    if depth > 4 {
        return Err("Map pack directories are too deeply nested".into());
    }
    let mut size = 0_u64;
    for entry in fs::read_dir(directory).map_err(message)? {
        *seen += 1;
        if *seen > MAX_PACK_FILES * 4 {
            return Err("Map pack has too many directory entries".into());
        }
        let entry = entry.map_err(message)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(message)?;
        if metadata.is_symlink() {
            return Err("Map packs cannot contain symbolic links".into());
        }
        if metadata.is_dir() {
            size = size
                .checked_add(contents(root, allowed, &entry.path(), depth + 1, seen)?)
                .ok_or("Pack size overflows")?;
        } else if metadata.is_file() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(message)?
                .to_str()
                .ok_or("Invalid pack filename")?
                .replace('\\', "/");
            if !allowed.contains(&relative) {
                return Err(format!("Unlisted file in map pack: {relative}"));
            }
            size = size
                .checked_add(metadata.len())
                .ok_or("Pack size overflows")?;
        } else {
            return Err("Map packs contain only normal files and directories".into());
        }
    }
    Ok(size)
}

pub async fn verify_archive(path: PathBuf) -> Result<(), String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(cancelled.clone(), true);
    tokio::task::spawn_blocking(move || {
        archive_identity(&path)?;
        let root = path.parent().ok_or("Invalid archive directory")?;
        let manifest = manifest(root)?;
        copy_file(
            root,
            None,
            &manifest.files[0],
            &cancelled,
            &InstallProgress::default(),
        )
    })
    .await
    .map_err(message)?
}

pub async fn install(
    source: PathBuf,
    managed: PathBuf,
    quota: u64,
    cancelled: Arc<AtomicBool>,
    progress: Arc<InstallProgress>,
) -> Result<PathBuf, String> {
    let quota_root = managed.parent().unwrap_or(&managed).to_path_buf();
    let gate = crate::source::quota_lock(&quota_root);
    let _quota = gate.lock().await;
    let mut guard = CancelOnDrop(cancelled.clone(), true);
    let result = tokio::task::spawn_blocking(move || {
        let manifest = manifest(&source)?;
        let identity = manifest.identity()?;
        let mut allowed = manifest
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        allowed.insert("manifest.json".into());
        let total = contents(&source, &allowed, &source, 0, &mut 0)?;
        if total > quota {
            return Err("This map pack exceeds the configured storage quota".into());
        }
        progress.total.store(
            manifest.files.iter().map(|file| file.bytes).sum(),
            Ordering::Relaxed,
        );
        progress.copied.store(0, Ordering::Relaxed);
        fs::create_dir_all(&managed).map_err(message)?;
        let destination = managed.join(&identity);
        if destination.exists() {
            if crate::pack::manifest(&destination)?.identity()? != identity {
                return Err("The installed pack manifest changed".into());
            }
            for file in &manifest.files {
                copy_file(&destination, None, file, &cancelled, &progress)?;
            }
            return Ok(destination);
        }
        let used = crate::source::directory_size(&quota_root)?;
        if used.checked_add(total).is_none_or(|used| used > quota) {
            return Err("Map pack storage is full; the active pack was kept".into());
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(message)?
            .as_nanos();
        let stage_path = managed.join(format!(".install-{}-{nonce}", std::process::id()));
        fs::create_dir(&stage_path).map_err(message)?;
        let stage = Stage(stage_path);
        for file in &manifest.files {
            copy_file(&source, Some(&stage.0), file, &cancelled, &progress)?;
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("Map pack import cancelled".into());
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(stage.0.join("manifest.json"))
            .map_err(message)?;
        file.write_all(&serde_json::to_vec_pretty(&manifest).map_err(message)?)
            .map_err(message)?;
        file.sync_all().map_err(message)?;
        #[cfg(unix)]
        fs::File::open(&stage.0)
            .map_err(message)?
            .sync_all()
            .map_err(message)?;
        fs::rename(&stage.0, &destination).map_err(message)?;
        #[cfg(unix)]
        fs::File::open(&managed)
            .map_err(message)?
            .sync_all()
            .map_err(message)?;
        Ok(destination)
    })
    .await
    .map_err(message)?;
    guard.1 = false;
    result
}

pub fn package(destination: &Path) -> Result<(), String> {
    use geozero::mvt::{Message, Tile as VectorTile, tile::Layer};
    let maps = destination.join("maps");
    let root = maps.join("tiles");
    fs::create_dir_all(root.join("0/0")).map_err(message)?;
    let bytes = VectorTile {
        layers: vec![
            Layer {
                name: "streets".into(),
                version: 2,
                extent: Some(4096),
                ..Default::default()
            },
            Layer {
                name: "water_polygons".into(),
                version: 2,
                extent: Some(4096),
                ..Default::default()
            },
        ],
    }
    .encode_to_vec();
    let manifest = Manifest { version: 1, format: Format::Directory, schema: "shortbread-1.1".into(), dataset: "lince-empty-example-v1".into(), source: "Lince format example. This tile contains no surveyed roads or terrain; the bundled Natural Earth overview supplies geography.".into(), date: "2026-10-17".into(), coverage: Bounds { west: -180.0, south: -85.0, east: 180.0, north: 85.0 }, min_zoom: 0, max_zoom: 0, license: include_str!("../../../LICENSE").into(), credits: "Lince contributors. Natural Earth overview by Tom Patterson, Nathaniel Vaughn Kelso and contributors, public domain.".into(), files: vec![File { path: "0/0/0.pbf".into(), bytes: bytes.len() as u64, expanded_bytes: bytes.len() as u64, sha256: hex(&Sha256::digest(&bytes)) }] };
    manifest.validate()?;
    for (path, bytes) in [
        (root.join("0/0/0.pbf"), bytes),
        (root.join("manifest.json"), serde_json::to_vec_pretty(&manifest).map_err(message)?),
        (maps.join("world-land.geojson"), include_bytes!("../data/world-land.geojson").to_vec()),
        (maps.join("CREDITS.txt"), b"Natural Earth overview: public domain. The example pack contains no survey detail. External maps are optional and require explicit configuration. OpenStreetMap data requires ODbL attribution.\n".to_vec()),
    ] {
        let mut file = fs::OpenOptions::new().create_new(true).write(true).open(path).map_err(message)?;
        file.write_all(&bytes).map_err(message)?;
        file.sync_all().map_err(message)?;
    }
    let licenses = maps.join("licenses");
    fs::create_dir_all(&licenses).map_err(message)?;
    for (name, license) in [
        (
            "Natural-Earth.txt",
            include_str!("../licenses/natural-earth.txt"),
        ),
        (
            "Galileo-MIT.txt",
            include_str!("../licenses/galileo-MIT.txt"),
        ),
        ("wgpu-MIT.txt", include_str!("../licenses/wgpu-MIT.txt")),
        (
            "PMTiles-MIT.txt",
            include_str!("../licenses/pmtiles-MIT.txt"),
        ),
        (
            "GeoZero-MIT.txt",
            include_str!("../licenses/geozero-MIT.txt"),
        ),
        (
            "GeoJSON-MIT.txt",
            include_str!("../licenses/geojson-MIT.txt"),
        ),
        ("ODbL.txt", include_str!("../licenses/ODbL.txt")),
        (
            "ESA-WorldCover.txt",
            include_str!("../licenses/ESA-WorldCover.txt"),
        ),
    ] {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(licenses.join(name))
            .map_err(message)?;
        file.write_all(license.as_bytes()).map_err(message)?;
    }
    Ok(())
}
