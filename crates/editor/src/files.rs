use crate::{MAX_FILE_BYTES, Result};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone)]
pub struct Scope {
    pub path: PathBuf,
    pub(crate) dir: Arc<Dir>,
}

impl Scope {
    pub fn open(path: &Path) -> Result<Self> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let dir = Dir::open_ambient_dir(&path, ambient_authority()).map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            dir: Arc::new(dir),
        })
    }

    pub fn relative(&self, path: &Path) -> Result<PathBuf> {
        let path = path
            .strip_prefix(&self.path)
            .map_err(|_| "The path is outside this root")?;
        let relative = self
            .dir
            .canonicalize(if path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                path
            })
            .map_err(|e| e.to_string())?;
        Ok(if relative == Path::new(".") {
            PathBuf::new()
        } else {
            relative
        })
    }

    pub fn bind(&self, path: &Path) -> Result<FileBinding> {
        let relative = self.relative(path)?;
        let name = relative.file_name().ok_or("Select a file")?.to_owned();
        let directory = relative
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = self.dir.open_dir(directory).map_err(|e| e.to_string())?;
        Ok(FileBinding {
            path: self.path.join(relative),
            parent,
            name,
        })
    }

    pub fn create(&self, path: &Path) -> Result<FileBinding> {
        let relative = path
            .strip_prefix(&self.path)
            .map_err(|_| "Create files inside a selected root")?;
        let name = relative.file_name().ok_or("Enter a file name")?;
        let directory = relative
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = self.dir.open_dir(directory).map_err(|e| e.to_string())?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        parent
            .open_with(name, &options)
            .map_err(|e| e.to_string())?
            .sync_all()
            .map_err(|e| e.to_string())?;
        self.bind(path)
    }
}

pub struct FileBinding {
    pub path: PathBuf,
    parent: Dir,
    name: OsString,
}

pub struct SaveDestination {
    pub path: PathBuf,
    scope: Scope,
    expected: Option<(Arc<FileBinding>, Snapshot)>,
}

impl SaveDestination {
    pub fn inspect(path: &Path) -> Result<Self> {
        let scope = Scope::open(path.parent().ok_or("Choose a directory")?)?;
        let name = path.file_name().ok_or("Choose a file name")?;
        let path = scope.path.join(name);
        let expected = match scope.dir.symlink_metadata(name) {
            Ok(metadata) if metadata.is_file() => {
                let file = Arc::new(scope.bind(&path)?);
                let snapshot = file.read()?;
                Some((file, snapshot))
            }
            Ok(_) => {
                return Err(
                    "Choose a regular file; directories and links cannot be replaced".into(),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            path,
            scope,
            expected,
        })
    }

    pub fn exists(&self) -> bool {
        self.expected.is_some()
    }

    pub fn save(self, text: &str, bom: bool) -> Result<(Arc<FileBinding>, Snapshot)> {
        let (file, mut snapshot) = match self.expected {
            Some(expected) => expected,
            None => {
                let file = Arc::new(self.scope.create(&self.path)?);
                let snapshot = file.read()?;
                (file, snapshot)
            }
        };
        snapshot.bom = bom;
        let saved = file.save(&snapshot, text)?;
        Ok((file, saved))
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub text: Arc<str>,
    pub bom: bool,
    digest: [u8; 32],
}

impl Snapshot {
    pub fn recovered(text: String, bom: bool) -> Self {
        let mut hash = Sha256::new();
        if bom {
            hash.update([0xef, 0xbb, 0xbf]);
        }
        hash.update(text.as_bytes());
        Self {
            text: text.into(),
            bom,
            digest: hash.finalize().into(),
        }
    }
}

impl FileBinding {
    pub fn reader(&self) -> Result<cap_std::fs::File> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = self
            .parent
            .open_with(&self.name, &options)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Only regular files can be read".into());
        }
        Ok(file)
    }

    pub fn preview(&self) -> Result<Snapshot> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = self
            .parent
            .open_with(&self.name, &options)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Only regular files can be previewed".into());
        }
        let mut bytes = Vec::new();
        file.take(crate::MAX_WINDOW_BYTES as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let content = if bom { &bytes[3..] } else { &bytes };
        let text = if content.contains(&0) {
            String::new()
        } else {
            let value = match std::str::from_utf8(content) {
                Ok(text) => text,
                Err(error) if error.error_len().is_none() => {
                    std::str::from_utf8(&content[..error.valid_up_to()])
                        .map_err(|e| e.to_string())?
                }
                Err(_) => {
                    return Ok(Snapshot::recovered(
                        "This file is not UTF-8 text.".into(),
                        false,
                    ));
                }
            };
            value.into()
        };
        Ok(Snapshot::recovered(text, bom))
    }

    pub fn read(&self) -> Result<Snapshot> {
        let metadata = self
            .parent
            .symlink_metadata(&self.name)
            .map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("Only regular files can be edited; reopen changed links".into());
        }
        if metadata.len() > MAX_FILE_BYTES as u64 + 3 {
            return Err("This file exceeds the 16 MiB editing limit".into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = self
            .parent
            .open_with(&self.name, &options)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Only regular files can be edited".into());
        }
        let mut bytes = Vec::new();
        file.take((MAX_FILE_BYTES + 4) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let content = if bom { &bytes[3..] } else { &bytes };
        if content.len() > MAX_FILE_BYTES {
            return Err("This file exceeds the 16 MiB editing limit".into());
        }
        if content.contains(&0) {
            return Err("Binary files cannot be edited as text".into());
        }
        let text = std::str::from_utf8(content)
            .map_err(|_| "Only UTF-8 text is supported; the original file was left intact")?
            .into();
        Ok(Snapshot {
            text,
            bom,
            digest: Sha256::digest(&bytes).into(),
        })
    }

    pub fn save(&self, expected: &Snapshot, text: &str) -> Result<Snapshot> {
        if text.len() > MAX_FILE_BYTES || text.contains('\0') {
            return Err("The text cannot be saved in this editor".into());
        }
        let current = self.read()?;
        if current.digest != expected.digest {
            return Err("The file changed on disk. Refresh and review it before saving".into());
        }
        let metadata = self
            .parent
            .metadata(&self.name)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            if metadata.nlink() > 1 {
                return Err(
                    "Saving a hard-linked file would separate its links; copy it first".into(),
                );
            }
        }
        if metadata.permissions().readonly() {
            return Err("This file is read-only".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let name = format!(
            ".lince-save-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = self
            .parent
            .open_with(&name, &options)
            .map_err(|e| e.to_string())?;
        let result = (|| -> Result<()> {
            if expected.bom {
                file.write_all(&[0xef, 0xbb, 0xbf])
                    .map_err(|e| e.to_string())?;
            }
            file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            file.set_permissions(metadata.permissions())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            if self.read()?.digest != expected.digest {
                return Err("The file changed while saving; refresh before trying again".into());
            }
            self.parent
                .rename(&name, &self.parent, &self.name)
                .map_err(|e| e.to_string())?;
            #[cfg(unix)]
            self.parent
                .open(".")
                .map_err(|e| e.to_string())?
                .sync_all()
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.parent.remove_file(&name);
        }
        result?;
        let mut hash = Sha256::new();
        if expected.bom {
            hash.update([0xef, 0xbb, 0xbf]);
        }
        hash.update(text.as_bytes());
        Ok(Snapshot {
            text: text.into(),
            bom: expected.bom,
            digest: hash.finalize().into(),
        })
    }
}

#[cfg(test)]
mod tests;
