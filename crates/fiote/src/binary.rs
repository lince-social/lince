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
