//! Locates the external Fiote executable.

use std::{
    env::{split_paths, var_os},
    path::{Path, PathBuf},
};

/// Environment variable that overrides the executable path.
pub const BINARY_ENV: &str = "LINCE_FIOTE_BIN";

/// A discovered executable and how its path was selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FioteBinary {
    /// Candidate file path, as specified by the environment or found in `PATH`.
    pub path: PathBuf,
    /// The source of the path.
    pub origin: Origin,
}

/// Where the executable path was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The explicit environment override.
    Environment,
    /// The first matching entry in `PATH`.
    Path,
}

impl Origin {
    /// Returns the stable source label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Path => "path",
        }
    }
}

/// Resolves the explicit override first, otherwise searches `PATH` in order.
/// The workspace root is accepted for API compatibility.
///
/// # Errors
/// Returns a diagnostic for an invalid override or a failed `PATH` search.
pub fn locate(_workspace_root: &Path) -> Result<FioteBinary, String> {
    if let Some(raw) = var_os(BINARY_ENV) {
        let path = PathBuf::from(raw);
        if !path.is_file() {
            return Err(format!(
                "{BINARY_ENV} points at `{}`, which is not a file",
                path.display()
            ));
        }
        return Ok(FioteBinary {
            path,
            origin: Origin::Environment,
        });
    }
    let path = var_os("PATH")
        .and_then(|raw| {
            split_paths(&raw)
                .map(|directory| directory.join("fiote"))
                .find(|candidate| candidate.is_file())
        })
        .ok_or_else(|| format!("no Fiote agent binary: set {BINARY_ENV} or put `fiote` on PATH"))?;
    Ok(FioteBinary {
        path,
        origin: Origin::Path,
    })
}
