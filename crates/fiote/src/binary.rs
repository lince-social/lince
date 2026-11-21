// non-lean-yet
// Scope: const BINARY_ENV, struct FioteBinary, enum Origin, fn locate.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 15.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 29.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 9,
//   10, 11, 15, 16, 17, 21, 29.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites locate: String
//   (29).

use std::{
    env,
    path::{Path, PathBuf},
};

pub const BINARY_ENV: &str = "LINCE_FIOTE_BIN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FioteBinary {
    pub path: PathBuf,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Environment,
    Path,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Path => "path",
        }
    }
}

pub fn locate(_workspace_root: &Path) -> Result<FioteBinary, String> {
    if let Some(raw) = env::var_os(BINARY_ENV) {
        let path = PathBuf::from(raw);
        return if path.is_file() {
            Ok(FioteBinary {
                path,
                origin: Origin::Environment,
            })
        } else {
            Err(format!(
                "{BINARY_ENV} points at `{}`, which is not a file",
                path.display()
            ))
        };
    }

    if let Some(found) = search_path("fiote") {
        return Ok(FioteBinary {
            path: found,
            origin: Origin::Path,
        });
    }

    Err(format!(
        "no Fiote agent binary: set {BINARY_ENV} or put `fiote` on PATH"
    ))
}

fn search_path(program: &str) -> Option<PathBuf> {
    let raw = env::var_os("PATH")?;
    env::split_paths(&raw)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}
