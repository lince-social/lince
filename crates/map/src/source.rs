use crate::geometry::{MAX_DATA_ZOOM, Tile};
use bytes::Bytes;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use futures::{StreamExt, stream};
use geozero::mvt::{Message, Tile as VectorTile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, Semaphore};

pub const MAX_TILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 16 * 1024 * 1024;
pub const PUBLIC_SERVER: &str = "https://tiles.versatiles.org/tiles/osm/{z}/{x}/{y}";

pub(crate) fn quota_lock(root: &Path) -> Arc<AsyncMutex<()>> {
    static GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>> = OnceLock::new();
    let path = root
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(root));
    let mut gates = GATES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(&path).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(AsyncMutex::new(()));
    gates.insert(path, Arc::downgrade(&gate));
    gate
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub local_directory: PathBuf,
    pub archives: Vec<PathBuf>,
    pub server: String,
    pub dataset_version: String,
    pub online: bool,
    pub max_zoom: u8,
    pub storage_megabytes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            local_directory: PathBuf::new(),
            archives: Vec::new(),
            server: String::new(),
            dataset_version: String::new(),
            online: false,
            max_zoom: MAX_DATA_ZOOM,
            storage_megabytes: 1024,
        }
    }
}

impl Settings {
    pub fn save_atomic(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        self.validate_paths()?;
        save_json_atomic(self, path)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.dataset_version.len() > 256
            || self.dataset_version.chars().any(char::is_control)
            || self.online && self.dataset_version.trim().is_empty()
        {
            return Err("Choose a dataset version before enabling external downloads".into());
        }
        if self.max_zoom > MAX_DATA_ZOOM
            || !(16..=65_536).contains(&self.storage_megabytes)
            || self.archives.len() > 16
        {
            return Err("Use data zoom 0–14, storage 16–65536 MB, and at most 16 archives".into());
        }
        if self.online || !self.server.is_empty() {
            if !["{z}", "{x}", "{y}"]
                .iter()
                .all(|key| self.server.contains(key))
                || self.server.len() > 2048
            {
                return Err("The tile server must contain {z}, {x}, and {y}".into());
            }
            let parsed = reqwest::Url::parse(
                &self
                    .server
                    .replace("{z}", "0")
                    .replace("{x}", "0")
                    .replace("{y}", "0"),
            )
            .map_err(|_| "Invalid tile server URL")?;
            let local = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
            if parsed.scheme() != "https" && !(parsed.scheme() == "http" && local) {
                return Err("Use an HTTPS tile server, or HTTP on localhost".into());
            }
            if !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.fragment().is_some()
            {
                return Err("The tile server cannot contain credentials or a fragment".into());
            }
        }
        Ok(())
    }

    pub fn source_id(&self) -> String {
        let local = if self.local_directory.as_os_str().is_empty() {
            None
        } else {
            crate::pack::manifest(&self.local_directory)
                .and_then(|manifest| manifest.identity())
                .ok()
        };
        let archives = self
            .archives
            .iter()
            .map(|path| crate::pack::archive_identity(path).ok())
            .collect::<Vec<_>>();
        Sha256::digest(
            serde_json::to_vec(&(
                "shortbread-1.1-no-buildings",
                &self.server,
                &self.dataset_version,
                local,
                archives,
            ))
            .unwrap_or_default(),
        )
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
    }

    fn validate_paths(&self) -> Result<(), String> {
        if !self.local_directory.as_os_str().is_empty()
            && crate::pack::manifest(&self.local_directory)?.format
                != crate::pack::Format::Directory
        {
            return Err("Choose a directory pack for the primary local source".into());
        }
        for archive in &self.archives {
            crate::pack::archive_identity(archive)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub downloaded_bytes: u64,
    pub expected_bytes: u64,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

impl Progress {
    pub fn finished(&self) -> bool {
        self.completed + self.failed >= self.total || self.cancelled
    }
}

#[derive(Clone, Debug, Default)]
struct TileProgress {
    finished: bool,
    error: Option<String>,
    bytes: u64,
    expected: u64,
}

pub struct TileStore {
    settings: Settings,
    root: PathBuf,
    client: reqwest::Client,
    slots: Semaphore,
    in_flight: AsyncMutex<HashMap<Tile, Arc<OnceCell<Result<Bytes, String>>>>>,
    progress: Mutex<BTreeMap<Tile, TileProgress>>,
    quota_root: PathBuf,
    storage: Arc<AsyncMutex<()>>,
    archives: OnceCell<Vec<pmtiles::AsyncPmTilesReader<LocalArchive>>>,
}

impl TileStore {
    pub fn new(settings: Settings, cache: impl AsRef<Path>) -> Result<Arc<Self>, String> {
        settings.validate()?;
        settings.validate_paths()?;
        let root = cache.as_ref().join(settings.source_id());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Lince/0.8 map")
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Arc::new(Self {
            settings,
            root,
            client,
            slots: Semaphore::new(4),
            in_flight: AsyncMutex::default(),
            progress: Mutex::default(),
            quota_root: cache.as_ref().to_path_buf(),
            storage: quota_lock(cache.as_ref()),
            archives: OnceCell::new(),
        }))
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub async fn validate_sources(&self) -> Result<(), String> {
        self.local_archives().await.map(|_| ())
    }

    async fn local_archives(
        &self,
    ) -> Result<&Vec<pmtiles::AsyncPmTilesReader<LocalArchive>>, String> {
        self.archives
            .get_or_try_init(|| async {
                let mut readers = Vec::new();
                for archive in &self.settings.archives {
                    crate::pack::verify_archive(archive.clone()).await?;
                    let backend = LocalArchive::open(archive.clone()).await?;
                    let metadata = backend.metadata().await?;
                    let reader = pmtiles::AsyncPmTilesReader::try_from_source(backend)
                        .await
                        .map_err(|error| format!("Cannot open local map archive: {error}"))?;
                    if reader.get_header().tile_type != pmtiles::TileType::Mvt
                        || !matches!(
                            reader.get_header().tile_compression,
                            pmtiles::Compression::None | pmtiles::Compression::Gzip
                        )
                    {
                        return Err(
                            "Use a Shortbread vector PMTiles archive with plain or gzip tiles"
                                .to_string(),
                        );
                    }
                    if metadata.len() > 1_048_576 || !shortbread_metadata(&metadata) {
                        return Err(
                            "The archive does not declare the supported Shortbread layers"
                                .to_string(),
                        );
                    }
                    readers.push(reader);
                }
                Ok(readers)
            })
            .await
    }

    pub fn progress(&self, tiles: &[Tile]) -> Progress {
        let states = self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut result = Progress {
            total: tiles.len(),
            ..Default::default()
        };
        for tile in tiles {
            if let Some(state) = states.get(tile) {
                result.downloaded_bytes += state.bytes;
                result.expected_bytes += state.expected;
                if let Some(error) = &state.error {
                    result.failed += 1;
                    if result.errors.len() < 3 && !result.errors.contains(error) {
                        result.errors.push(error.clone());
                    }
                } else if state.finished {
                    result.completed += 1;
                }
            }
        }
        result
    }

    fn update(&self, tile: Tile, update: impl FnOnce(&mut TileProgress)) {
        let mut states = self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if states.len() >= 50_512 && !states.contains_key(&tile) {
            states.retain(|_, progress| !progress.finished && progress.error.is_none());
        }
        update(states.entry(tile).or_default());
    }

    pub async fn load(self: &Arc<Self>, tile: Tile) -> Result<Bytes, String> {
        if !tile.valid() || tile.z > self.settings.max_zoom {
            return Err("Tile is outside the configured detail range".into());
        }
        let cell = {
            let mut requests = self.in_flight.lock().await;
            if requests.len() >= 1024 {
                requests.retain(|_, cell| Arc::strong_count(cell) > 1);
            }
            requests.entry(tile).or_default().clone()
        };
        let result = cell
            .get_or_init(|| async { self.load_one(tile).await })
            .await
            .clone();
        let mut requests = self.in_flight.lock().await;
        if requests
            .get(&tile)
            .is_some_and(|current| Arc::ptr_eq(current, &cell))
        {
            requests.remove(&tile);
        }
        drop(requests);
        self.update(tile, |state| {
            state.finished = result.is_ok();
            state.error = result.as_ref().err().cloned();
        });
        result
    }

    async fn load_one(&self, tile: Tile) -> Result<Bytes, String> {
        let _slot = self
            .slots
            .acquire()
            .await
            .map_err(|_| "Map loading stopped")?;
        self.update(tile, |state| *state = TileProgress::default());
        let relative = PathBuf::from(format!("{}/{}/{}.pbf", tile.z, tile.x, tile.y));
        if !self.settings.local_directory.as_os_str().is_empty() {
            let root = self.settings.local_directory.clone();
            let tile_path = relative.to_string_lossy().replace('\\', "/");
            if let Some(bytes) =
                tokio::task::spawn_blocking(move || crate::pack::read_tile(&root, &tile_path))
                    .await
                    .map_err(|error| error.to_string())??
            {
                return normalize(&bytes);
            }
        }
        let archives = self.local_archives().await?;
        for reader in archives {
            let coordinate = pmtiles::TileCoord::new(tile.z, tile.x, tile.y)
                .map_err(|error| error.to_string())?;
            if let Some(bytes) = reader
                .get_tile(coordinate)
                .await
                .map_err(|error| error.to_string())?
            {
                return normalize(&bytes);
            }
        }
        if let Some(bytes) = read_bounded(&self.root.join(&relative)).await? {
            match normalize(&bytes) {
                Ok(bytes) => return Ok(bytes),
                Err(_) => {
                    let _ = tokio::fs::remove_file(self.root.join(&relative)).await;
                }
            }
        }
        if !self.settings.online {
            return Err(
                "This detail is not available locally. Enable a map server in Settings".into(),
            );
        }
        let url = self
            .settings
            .server
            .replace("{z}", &tile.z.to_string())
            .replace("{x}", &tile.x.to_string())
            .replace("{y}", &tile.y.to_string());
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "Cannot reach the map server")?
            .error_for_status()
            .map_err(|error| {
                format!(
                    "Map server returned {}",
                    error
                        .status()
                        .map_or("an error".into(), |status| status.to_string())
                )
            })?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_TILE_BYTES as u64)
        {
            return Err("Map tile exceeds the download limit".into());
        }
        self.update(tile, |state| {
            state.expected = response.content_length().unwrap_or_default()
        });
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Map download was interrupted")?
        {
            if bytes.len() + chunk.len() > MAX_TILE_BYTES {
                return Err("Map tile exceeds the download limit".into());
            }
            bytes.extend_from_slice(&chunk);
            self.update(tile, |state| state.bytes = bytes.len() as u64);
        }
        let normalized = normalize(&bytes)?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder
            .write_all(&normalized)
            .map_err(|error| error.to_string())?;
        let stored = encoder.finish().map_err(|error| error.to_string())?;
        let _storage = self.storage.lock().await;
        let root = self.quota_root.clone();
        let used = tokio::task::spawn_blocking(move || directory_size(&root))
            .await
            .map_err(|error| error.to_string())??;
        if used + stored.len() as u64 > self.settings.storage_megabytes * 1024 * 1024 {
            return Err(
                "Map storage is full. Increase its limit in Settings or remove a downloaded pack"
                    .into(),
            );
        }
        let destination = self.root.join(relative);
        tokio::fs::create_dir_all(destination.parent().ok_or("Invalid tile path")?)
            .await
            .map_err(|error| error.to_string())?;
        static NEXT_TEMPORARY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let temporary = Temporary(destination.with_extension(format!(
            "{}.{}.part",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        )));
        tokio::fs::write(&temporary.0, stored)
            .await
            .map_err(|error| error.to_string())?;
        tokio::fs::rename(&temporary.0, &destination)
            .await
            .map_err(|error| error.to_string())?;
        Ok(normalized)
    }

    pub async fn download(
        self: Arc<Self>,
        tiles: Vec<Tile>,
        cancelled: Arc<AtomicBool>,
    ) -> Progress {
        let mut requests = stream::iter(tiles.iter().copied())
            .map(|tile| self.load(tile))
            .buffer_unordered(4);
        loop {
            if cancelled.load(Ordering::Relaxed) {
                let mut progress = self.progress(&tiles);
                progress.cancelled = true;
                return progress;
            }
            tokio::select! {
                result = requests.next() => {
                    match result {
                        None => break,
                        Some(Err(_)) => break,
                        _ => {}
                    }
                },
                _ = async {
                    while !cancelled.load(Ordering::Relaxed) { tokio::time::sleep(Duration::from_millis(100)).await; }
                } => {
                    let mut progress = self.progress(&tiles);
                    progress.cancelled = true;
                    return progress;
                }
            }
        }
        self.progress(&tiles)
    }
}

async fn read_bounded(path: &Path) -> Result<Option<Bytes>, String> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.len() > MAX_TILE_BYTES as u64 => {
            Err("Local map tile exceeds its size limit".into())
        }
        Ok(_) => tokio::fs::read(path)
            .await
            .map(|bytes| Some(Bytes::from(bytes)))
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn directory_size(path: &Path) -> Result<u64, String> {
    directory_size_bounded(path, 0, &mut 0)
}

fn directory_size_bounded(path: &Path, depth: usize, entries: &mut usize) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    if depth > 8
        || std::fs::symlink_metadata(path)
            .map_err(|error| error.to_string())?
            .is_symlink()
    {
        return Err(
            "Map storage paths cannot be symbolic links or exceed eight directory levels".into(),
        );
    }
    let mut total = 0_u64;
    for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
        *entries += 1;
        if *entries > 250_000 {
            return Err(
                "Map storage contains too many files; remove unused packs or cache files".into(),
            );
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.is_symlink() {
            return Err("Downloaded map storage cannot contain symbolic links".into());
        }
        if metadata.is_dir() {
            total = total
                .checked_add(directory_size_bounded(&entry.path(), depth + 1, entries)?)
                .ok_or("Map storage size overflows")?;
        } else {
            total = total
                .checked_add(metadata.len())
                .ok_or("Map storage size overflows")?;
        }
    }
    Ok(total)
}

fn shortbread_metadata(metadata: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(metadata)
        .ok()
        .and_then(|value| value["vector_layers"].as_array().cloned())
        .is_some_and(|layers| {
            layers.iter().any(|layer| {
                matches!(
                    layer["id"].as_str(),
                    Some("streets" | "water_polygons" | "ocean")
                )
            })
        })
}

pub fn normalize(bytes: &[u8]) -> Result<Bytes, String> {
    if bytes.len() > MAX_TILE_BYTES {
        return Err("Map tile exceeds its size limit".into());
    }
    let decoded = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut output = Vec::new();
        GzDecoder::new(bytes)
            .take(MAX_DECODED_BYTES as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|_| "Invalid compressed map tile")?;
        if output.len() > MAX_DECODED_BYTES {
            return Err("Expanded map tile exceeds its size limit".into());
        }
        output
    } else {
        bytes.to_vec()
    };
    let mut tile = VectorTile::decode(decoded.as_slice()).map_err(|_| "Invalid vector map tile")?;
    if tile.layers.len() > 128
        || tile
            .layers
            .iter()
            .map(|layer| layer.features.len())
            .sum::<usize>()
            > 100_000
    {
        return Err("Map tile contains too many features".into());
    }
    tile.layers.retain(|layer| layer.name != "buildings");
    Ok(Bytes::from(tile.encode_to_vec()))
}

struct LocalArchive {
    path: PathBuf,
    sections: [(u64, u64); 4],
    compressed: bool,
    size: u64,
    modified: std::time::SystemTime,
}

impl LocalArchive {
    async fn open(path: PathBuf) -> Result<Self, String> {
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(&path)
            .await
            .map_err(|error| error.to_string())?;
        let metadata = file.metadata().await.map_err(|error| error.to_string())?;
        let mut bytes = [0; 127];
        file.read_exact(&mut bytes)
            .await
            .map_err(|_| "Invalid map archive header")?;
        let header = pmtiles::Header::try_from_bytes(Bytes::copy_from_slice(&bytes))
            .map_err(|error| error.to_string())?;
        if header.spec_version() != 3
            || !matches!(
                header.internal_compression(),
                pmtiles::Compression::None | pmtiles::Compression::Gzip
            )
        {
            return Err("Use a version 3 PMTiles archive with plain or gzip directories".into());
        }
        let sections: [(u64, u64); 4] = std::array::from_fn(|i| {
            let offset = 8 + i * 16;
            (
                u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()),
                u64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap()),
            )
        });
        if sections.iter().any(|(offset, length)| {
            offset
                .checked_add(*length)
                .is_none_or(|end| end > metadata.len())
        }) || sections[0].0 < 127
            || sections[0].0 + sections[0].1 > 16_384
            || sections[1].1 > 1_048_576
        {
            return Err("Map archive sections are invalid or exceed their limits".into());
        }
        Ok(Self {
            path,
            sections,
            compressed: header.internal_compression() == pmtiles::Compression::Gzip,
            size: metadata.len(),
            modified: metadata.modified().map_err(|error| error.to_string())?,
        })
    }

    async fn metadata(&self) -> Result<String, String> {
        use pmtiles::AsyncBackend;
        let (offset, length) = self.sections[1];
        let bytes = self
            .read(offset as usize, length as usize)
            .await
            .map_err(|error| error.to_string())?
            .bytes;
        let decoded = self.internal(&bytes, 1_048_576)?;
        String::from_utf8(decoded).map_err(|_| "Invalid map archive metadata".into())
    }

    fn internal(&self, bytes: &[u8], limit: usize) -> Result<Vec<u8>, String> {
        let decoded = if self.compressed {
            let mut decoded = Vec::new();
            GzDecoder::new(bytes)
                .take(limit as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| "Invalid map archive compression")?;
            decoded
        } else {
            bytes.to_vec()
        };
        if decoded.len() > limit {
            return Err("Expanded map archive section exceeds its limit".into());
        }
        Ok(decoded)
    }

    fn directory(&self, bytes: &[u8]) -> Result<(), String> {
        let decoded = self.internal(bytes, MAX_DECODED_BYTES)?;
        let mut input = decoded.as_slice();
        let count = archive_varint(&mut input)?;
        if count > 100_000 {
            return Err("Map archive directory has too many entries".into());
        }
        let mut id = 0_u64;
        for _ in 0..count {
            id = id
                .checked_add(archive_varint(&mut input)?)
                .filter(|id| *id <= pmtiles::MAX_TILE_ID)
                .ok_or("Invalid map archive tile index")?;
        }
        let runs: Vec<_> = (0..count)
            .map(|_| archive_varint(&mut input))
            .collect::<Result<_, _>>()?;
        let lengths: Vec<_> = (0..count)
            .map(|_| archive_varint(&mut input))
            .collect::<Result<_, _>>()?;
        let mut end = None;
        for (run, length) in runs.into_iter().zip(lengths) {
            let encoded = archive_varint(&mut input)?;
            let offset = if encoded == 0 {
                end.ok_or("Invalid map archive directory offset")?
            } else {
                encoded - 1
            };
            let next = offset
                .checked_add(length)
                .ok_or("Invalid map archive directory range")?;
            if run > u64::from(u32::MAX)
                || length > MAX_TILE_BYTES as u64
                || next > self.sections[if run == 0 { 2 } else { 3 }].1
            {
                return Err("Map archive directory entry exceeds its range".into());
            }
            end = Some(next);
        }
        Ok(())
    }
}

fn archive_varint(input: &mut &[u8]) -> Result<u64, String> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = input
            .split_first()
            .ok_or("Incomplete map archive directory")?;
        *input = rest;
        if shift == 63 && byte > 1 {
            return Err("Map archive directory integer overflow".into());
        }
        value |= u64::from(byte & 127) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    Err("Map archive directory integer overflow".into())
}

impl pmtiles::AsyncBackend for LocalArchive {
    async fn read(
        &self,
        offset: usize,
        length: usize,
    ) -> pmtiles::PmtResult<pmtiles::BackendResponse> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        if length > MAX_TILE_BYTES
            || (offset as u64)
                .checked_add(length as u64)
                .is_none_or(|end| end > self.size)
        {
            return Err(pmtiles::PmtError::Reading(std::io::Error::other(
                "Map archive read exceeds its limit",
            )));
        }
        let mut file = tokio::fs::File::open(&self.path).await?;
        let metadata = file.metadata().await?;
        if metadata.len() != self.size || metadata.modified()? != self.modified {
            return Err(pmtiles::PmtError::SourceModified);
        }
        file.seek(std::io::SeekFrom::Start(offset as u64)).await?;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes).await?;
        let internal = if offset == 0 {
            let (start, length) = self.sections[0];
            Some(&bytes[start as usize..(start + length) as usize])
        } else if offset as u64 >= self.sections[2].0
            && (offset as u64) < self.sections[2].0 + self.sections[2].1
        {
            Some(bytes.as_slice())
        } else {
            None
        };
        if let Some(bytes) = internal {
            self.directory(bytes)
                .map_err(|error| pmtiles::PmtError::Reading(std::io::Error::other(error)))?;
        }
        Ok(pmtiles::BackendResponse::new(Bytes::from(bytes)))
    }
}

pub(crate) fn save_json_atomic(value: &impl serde::Serialize, path: &Path) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() > 65_536 {
        return Err("Map settings exceed 64 KiB".into());
    }
    let parent = path.parent().ok_or("Choose a settings file directory")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let temporary = Temporary(path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary.0)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    std::fs::rename(&temporary.0, path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    Ok(())
}
