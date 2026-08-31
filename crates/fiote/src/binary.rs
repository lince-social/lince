use std::{
    env,
    path::{Path, PathBuf},
};

pub const BINARY_ENV: &str = "LINCE_PI_BIN";
pub const VENDORED_RELATIVE: &str = "vendor/pi/node_modules/.bin/pi";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiBinary {
    pub path: PathBuf,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Environment,
    Vendored,
    Path,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Vendored => "vendored",
            Self::Path => "path",
        }
    }
}

pub fn locate(workspace_root: &Path) -> Result<PiBinary, String> {
    if let Some(raw) = env::var_os(BINARY_ENV) {
        let path = PathBuf::from(raw);
        return if path.is_file() {
            Ok(PiBinary {
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

    let vendored = workspace_root.join(VENDORED_RELATIVE);
    if vendored.is_file() {
        return Ok(PiBinary {
            path: vendored,
            origin: Origin::Vendored,
        });
    }

    if let Some(found) = search_path("pi") {
        return Ok(PiBinary {
            path: found,
            origin: Origin::Path,
        });
    }

    Err(format!(
        "no pi agent binary: set {BINARY_ENV}, install one at `{}`, or put `pi` on PATH",
        vendored.display()
    ))
}

fn search_path(program: &str) -> Option<PathBuf> {
    let raw = env::var_os("PATH")?;
    env::split_paths(&raw)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}
