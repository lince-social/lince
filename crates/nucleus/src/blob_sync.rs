use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::{Uuid, fmt::Hyphenated};

use crate::json::encoded_len;

/// Maximum number of files and directories in an offer.
pub const MAX_ENTRIES: usize = 4096;
/// Maximum total size of the offered files, one tebibyte.
pub const MAX_BYTES: u64 = 1 << 40;
/// Maximum encoded size of the complete offer envelope.
pub const MAX_OFFER_BYTES: usize = 2 * 1024 * 1024;

const OFFER_ENVELOPE_BYTES: usize = 128;
const HASH_HEX_BYTES: usize = 64;
const MAX_PATH_BYTES: usize = 1024;
const MAX_PATH_COMPONENTS: usize = 64;
const MAX_COMPONENT_BYTES: usize = 240;

/// A file or directory in an offered copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Portable path relative to the offer's root.
    pub path: String,
    /// Hexadecimal blob hash for a file, absent for a directory.
    pub hash: Option<String>,
    /// File size in bytes, zero for a directory.
    pub size: u64,
}

/// Identifies an offered copy and lists its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Hyphenated UUID of the offer.
    pub id: String,
    /// All files and directories, including parents of nested entries.
    pub entries: Vec<Entry>,
}

impl Manifest {
    /// Returns the total listed size in bytes. Validate untrusted manifests first.
    ///
    /// # Panics
    ///
    /// May panic with overflow checks enabled if an unvalidated manifest exceeds [`u64::MAX`].
    pub fn bytes(&self) -> u64 {
        self.entries.iter().map(|entry| entry.size).sum()
    }

    /// Checks portability and the file, directory, and envelope limits of this offer.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for an invalid identifier, entry, directory tree, or envelope size.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.len() != Hyphenated::LENGTH || Uuid::parse_str(&self.id).is_err() {
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
                .insert(entry.path.to_lowercase(), entry.hash.is_none())
                .is_some()
            {
                return Err("Files must have distinct names, including letter case".into());
            }
            match &entry.hash {
                Some(hash)
                    if hash.len() == HASH_HEX_BYTES
                        && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {}
                None if entry.size == 0 => {}
                _ => return Err("Invalid file hash or directory size".into()),
            }
            total = total
                .checked_add(entry.size)
                .filter(|bytes| *bytes <= MAX_BYTES)
                .ok_or("A Blob Sync copy cannot exceed 1 TiB")?;
        }
        // Each parent is checked as an entry, so direct edges cover every ancestor.
        if paths
            .keys()
            .filter_map(|path| path.rsplit_once('/'))
            .any(|(parent, _)| paths.get(parent) != Some(&true))
        {
            return Err("Every parent must be a directory in the offer".into());
        }
        if encoded_len(self).map_err(|error| error.to_string())?
            > MAX_OFFER_BYTES - OFFER_ENVELOPE_BYTES
        {
            return Err("The file list is too large for one offer".into());
        }
        Ok(())
    }
}

/// Checks that a relative path is portable across supported filesystems.
///
/// # Errors
///
/// Returns a diagnostic for an invalid length, depth, or component name.
pub fn validate_path(path: &str) -> Result<(), String> {
    let parts = path.split('/');
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || parts.clone().nth(MAX_PATH_COMPONENTS).is_some()
    {
        return Err("Invalid file path length".into());
    }
    for part in parts {
        if part.is_empty()
            || part.len() > MAX_COMPONENT_BYTES
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
            || is_reserved_name(part)
        {
            return Err(
                "Choose portable filenames without special characters or parent paths".into(),
            );
        }
    }
    Ok(())
}

fn is_reserved_name(part: &str) -> bool {
    let stem = part
        .split_once('.')
        .map_or(part, |(stem, _)| stem)
        .as_bytes();
    [b"CON", b"PRN", b"AUX", b"NUL"]
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(*reserved))
        || matches!(
            stem.split_last(),
            Some((b'1'..=b'9', prefix))
                if prefix.eq_ignore_ascii_case(b"COM") || prefix.eq_ignore_ascii_case(b"LPT")
        )
}

/// A peer that can receive an offered copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// Transport node identifier.
    pub node_id: String,
    /// Display label for the peer.
    pub label: String,
    /// Whether the peer was discovered nearby.
    pub nearby: bool,
}
