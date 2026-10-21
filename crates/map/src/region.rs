use crate::{
    Bounds, TileStore,
    pack::{File, Format, Manifest},
    source::normalize,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub dataset: String,
    pub source: String,
    pub date: String,
    pub license: String,
    pub credits: String,
    pub coverage: Bounds,
    pub min_zoom: u8,
    pub max_zoom: u8,
}
impl Definition {
    pub fn tiles(&self) -> Result<Vec<crate::Tile>, String> {
        if self.dataset.trim().is_empty()
            || self.dataset.len() > 256
            || self.min_zoom > self.max_zoom
            || self.max_zoom > crate::geometry::MAX_DATA_ZOOM
        {
            return Err("Choose a regional dataset name and detail range 0–14".into());
        }
        for value in [&self.source, &self.date, &self.license, &self.credits] {
            if value.trim().is_empty() || value.len() > 4096 {
                return Err("Regional packs require source, date, license and credits".into());
            }
        }
        let tiles = self.coverage.pyramid(self.max_zoom)?;
        Ok(tiles
            .into_iter()
            .filter(|tile| tile.z >= self.min_zoom)
            .collect())
    }
}
#[derive(Default)]
pub struct Progress {
    pub wake: Option<Arc<dyn Fn() + Send + Sync>>,
    pub total: AtomicU64,
    pub completed: AtomicU64,
    pub bytes: AtomicU64,
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub async fn prepare(
    store: Arc<TileStore>,
    definition: Definition,
    destination: PathBuf,
    cancelled: Arc<AtomicBool>,
    progress: Arc<Progress>,
) -> Result<Manifest, String> {
    let tiles = definition.tiles()?;
    if destination.exists() {
        return Err("Choose a new regional pack directory; existing packs are preserved".into());
    }
    let parent = destination
        .parent()
        .ok_or("Choose a regional pack directory")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| error.to_string())?;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let staging = Staging(parent.join(format!(
        ".lince-region-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    tokio::fs::create_dir(&staging.0)
        .await
        .map_err(|error| error.to_string())?;
    progress.total.store(tiles.len() as u64, Ordering::Relaxed);
    if let Some(wake) = &progress.wake {
        wake();
    }
    let quota = store
        .settings()
        .storage_megabytes
        .checked_mul(1024 * 1024)
        .ok_or("Regional quota overflows")?;
    let mut files = Vec::new();
    let mut total = 0_u64;
    for tile in tiles {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Regional preparation cancelled".into());
        }
        let bytes = normalize(&store.load(tile).await?)?;
        if bytes.is_empty() {
            progress.completed.fetch_add(1, Ordering::Relaxed);
            if let Some(wake) = &progress.wake {
                wake();
            }
            continue;
        }
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or("Regional size overflows")?;
        if total > quota {
            return Err("Regional pack exceeds the configured map storage quota".into());
        }
        let path = format!("{}/{}/{}.pbf", tile.z, tile.x, tile.y);
        let target = staging.0.join(&path);
        tokio::fs::create_dir_all(target.parent().ok_or("Invalid tile path")?)
            .await
            .map_err(|error| error.to_string())?;
        tokio::fs::write(target, &bytes)
            .await
            .map_err(|error| error.to_string())?;
        files.push(File {
            path,
            bytes: bytes.len() as u64,
            expanded_bytes: bytes.len() as u64,
            sha256: hex(&Sha256::digest(&bytes)),
        });
        progress.bytes.store(total, Ordering::Relaxed);
        progress.completed.fetch_add(1, Ordering::Relaxed);
        if let Some(wake) = &progress.wake {
            wake();
        }
    }
    let manifest = Manifest {
        version: 1,
        format: Format::Directory,
        schema: "shortbread-1.1".into(),
        dataset: definition.dataset,
        source: definition.source,
        date: definition.date,
        coverage: definition.coverage,
        min_zoom: definition.min_zoom,
        max_zoom: definition.max_zoom,
        license: definition.license,
        credits: definition.credits,
        files,
    };
    manifest.validate()?;
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    tokio::fs::write(staging.0.join("manifest.json"), bytes)
        .await
        .map_err(|error| error.to_string())?;
    tokio::fs::write(
        staging.0.join("CREDITS.txt"),
        format!(
            "{}\n{}\n{}\n",
            manifest.source, manifest.credits, manifest.license
        ),
    )
    .await
    .map_err(|error| error.to_string())?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("Regional preparation cancelled".into());
    }
    if destination.exists() {
        return Err("Regional destination was created concurrently".into());
    }
    tokio::fs::rename(&staging.0, &destination)
        .await
        .map_err(|error| error.to_string())?;
    Ok(manifest)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn read_definition(path: &Path) -> Result<Definition, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 65_536 {
        return Err("Regional definition exceeds 64 KiB".into());
    }
    let definition: Definition =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    definition.tiles()?;
    Ok(definition)
}
