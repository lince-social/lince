use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_ENTRIES: usize = 4096;
pub const MAX_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
pub const MAX_OFFER_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: String,
    pub hash: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub entries: Vec<Entry>,
}

impl Manifest {
    pub fn bytes(&self) -> u64 {
        self.entries.iter().map(|entry| entry.size).sum()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.len() != 36 || uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("Invalid Blob Sync offer identifier".into());
        }
        if self.entries.is_empty() || self.entries.len() > MAX_ENTRIES {
            return Err(format!(
                "Choose between 1 and {MAX_ENTRIES} files and directories"
            ));
        }
        let mut paths = BTreeMap::new();
        let mut total = 0u64;
        for entry in &self.entries {
            validate_path(&entry.path)?;
            if paths
                .insert(entry.path.to_lowercase(), entry.hash.is_some())
                .is_some()
            {
                return Err("Files must have distinct names, including letter case".into());
            }
            match &entry.hash {
                Some(hash) if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {}
                None if entry.size == 0 => {}
                _ => return Err("Invalid file hash or directory size".into()),
            }
            total = total
                .checked_add(entry.size)
                .filter(|bytes| *bytes <= MAX_BYTES)
                .ok_or("A Blob Sync copy cannot exceed 1 TiB")?;
        }
        for path in paths.keys() {
            let mut parent = path.as_str();
            while let Some((next, _)) = parent.rsplit_once('/') {
                if paths.get(next) != Some(&false) {
                    return Err("Every parent must be a directory in the offer".into());
                }
                parent = next;
            }
        }
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_OFFER_BYTES - 128
        {
            return Err("The file list is too large for one offer".into());
        }
        Ok(())
    }
}

pub fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > 1024 || path.split('/').count() > 64 {
        return Err("Invalid file path length".into());
    }
    for part in path.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.len() > 240
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&stem.as_str())
        {
            return Err(
                "Choose portable filenames without special characters or parent paths".into(),
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub node_id: String,
    pub label: String,
    pub nearby: bool,
}
