// non-lean-yet
// Scope: struct ExportReport, struct State, fn regular_or_missing, fn replace.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 103, 113.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 72, 76.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites write_graph:
//   String (51).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   59.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (105, 116).

use crate::graph::{Graph, digest};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct ExportReport {
    pub path: PathBuf,
    pub records: usize,
    pub changed: bool,
    pub digest: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    last_digest: Option<String>,
    pending_digest: Option<String>,
}

fn regular_or_missing(path: &Path, max: u64) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= max => {
            fs::read(path).map(Some).map_err(|e| e.to_string())
        }
        Ok(_) => Err(format!(
            "{} must be a regular file within its size budget",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let directory = path.parent().ok_or("Output has no directory")?;
    let mut staging = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    staging.write_all(bytes).map_err(|e| e.to_string())?;
    staging.as_file().sync_all().map_err(|e| e.to_string())?;
    staging.persist(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn write_graph(graph: &Graph, directory: &Path) -> Result<ExportReport, String> {
    if !directory.is_absolute() {
        return Err("Choose an absolute export directory".into());
    }
    // Finish projection and validation before touching any previous export.
    let content = graph.lingua()?;
    let hash = digest(content.as_bytes());
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let stem = format!("nomad-{}", &graph.root[2..]);
    let path = directory.join(format!("{stem}.lingua"));
    let state_path = directory.join(format!(".{stem}.state.json"));
    let lock_path = directory.join(format!(".{stem}.lock"));
    regular_or_missing(&lock_path, 0)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|_| "Another NOMAD export is writing this source")?;
    let previous = regular_or_missing(&path, 16 * 1024 * 1024)?;
    let state: State = match regular_or_missing(&state_path, 4096)? {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "NOMAD export state is invalid; previous files were preserved")?,
        None => State::default(),
    };
    let previous_hash = previous.as_deref().map(digest);
    if let Some(current) = previous_hash.as_deref() {
        if current != hash
            && state.last_digest.as_deref() != Some(current)
            && state.pending_digest.as_deref() != Some(current)
        {
            return Err(format!(
                "{} was edited outside NOMAD; reconcile those edits before refreshing",
                path.display()
            ));
        }
    } else if state.last_digest.is_some() {
        return Err(
            "The previous NOMAD export was removed; review its state before recreating it".into(),
        );
    }
    let changed = previous_hash.as_deref() != Some(hash.as_str());
    if changed {
        // Journal both accepted hashes before replacing the document. A crash
        // between either atomic rename is recoverable without accepting edits.
        replace(
            &state_path,
            &serde_json::to_vec(&State {
                last_digest: previous_hash,
                pending_digest: Some(hash.clone()),
            })
            .unwrap(),
        )?;
        replace(&path, content.as_bytes())?;
    }
    if changed || state.pending_digest.is_some() || state.last_digest.as_deref() != Some(&hash) {
        replace(
            &state_path,
            &serde_json::to_vec(&State {
                last_digest: Some(hash.clone()),
                pending_digest: None,
            })
            .unwrap(),
        )?;
    }
    Ok(ExportReport {
        path,
        records: graph.nodes.len(),
        changed,
        digest: hash,
    })
}
