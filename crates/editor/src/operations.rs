use crate::{Result, files::Scope};
use cap_std::fs::{Dir, Metadata, OpenOptions};
use std::{
    ffi::OsString,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
    parent: Dir,
    name: OsString,
    metadata: Metadata,
}

#[derive(Clone)]
pub struct TrashTicket {
    pub original: PathBuf,
    pub stored: PathBuf,
}

pub fn relocated(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(from).ok()?;
    Some(if relative.as_os_str().is_empty() {
        to.to_path_buf()
    } else {
        to.join(relative)
    })
}

impl Scope {
    fn parent_slot(&self, path: &Path) -> Result<(Dir, OsString)> {
        let relative = path
            .strip_prefix(&self.path)
            .map_err(|_| "Choose a path inside a selected root")?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("Selected roots and parent-directory paths cannot be changed".into());
        }
        let parent = relative
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Ok((
            self.dir.open_dir(parent).map_err(|e| e.to_string())?,
            relative.file_name().unwrap().to_owned(),
        ))
    }

    pub fn entry(&self, path: &Path) -> Result<Entry> {
        let (parent, name) = self.parent_slot(path)?;
        let metadata = parent.symlink_metadata(&name).map_err(|e| e.to_string())?;
        if !metadata.is_file() && !metadata.is_dir() && !metadata.is_symlink() {
            return Err("Only files, directories, and links can be changed".into());
        }
        Ok(Entry {
            path: path.into(),
            directory: metadata.is_dir(),
            parent,
            name,
            metadata,
        })
    }

    pub fn create_directory(&self, path: &Path) -> Result<()> {
        let (parent, name) = self.parent_slot(path)?;
        parent.create_dir(&name).map_err(|e| e.to_string())?;
        sync(&parent)
    }

    pub fn move_entry(&self, entry: &Entry, destination: &Path) -> Result<PathBuf> {
        if destination.starts_with(&entry.path) {
            return Err("An item cannot be moved inside itself".into());
        }
        let (parent, name) = self.parent_slot(destination)?;
        entry.validate()?;
        rename_new(&entry.parent, &entry.name, &parent, &name)?;
        sync(&entry.parent)?;
        sync(&parent)?;
        Ok(destination.into())
    }

    pub fn trash(&self, entry: &Entry) -> Result<TrashTicket> {
        let trash = self.path.join(".lince-trash");
        if entry.path.starts_with(&trash) {
            return Err("Move items out of .lince-trash to restore them".into());
        }
        self.parent_slot(&entry.path)?;
        match self.dir.create_dir(".lince-trash") {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
        if !self
            .dir
            .symlink_metadata(".lince-trash")
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            return Err("The trash directory must not be a link".into());
        }
        let trash_dir = self
            .dir
            .open_dir(".lince-trash")
            .map_err(|e| e.to_string())?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "{}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        trash_dir.create_dir(&name).map_err(|e| e.to_string())?;
        let slot = trash_dir.open_dir(&name).map_err(|e| e.to_string())?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
            use std::os::unix::fs::PermissionsExt;
            slot.set_permissions(
                ".",
                cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(0o700)),
            )
            .map_err(|e| e.to_string())?;
        }
        let mut origin = slot
            .open_with("original-path", &options)
            .map_err(|e| e.to_string())?;
        origin
            .write_all(
                entry
                    .path
                    .to_str()
                    .ok_or("Trash requires a UTF-8 path")?
                    .as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        origin.sync_all().map_err(|e| e.to_string())?;
        entry.validate()?;
        rename_new(&entry.parent, &entry.name, &slot, Path::new("item"))?;
        sync(&entry.parent)?;
        sync(&slot)?;
        sync(&trash_dir)?;
        Ok(TrashTicket {
            original: entry.path.clone(),
            stored: trash.join(name).join("item"),
        })
    }

    pub fn restore_trash(&self, ticket: &TrashTicket) -> Result<PathBuf> {
        if !ticket.stored.starts_with(self.path.join(".lince-trash")) {
            return Err("This trash entry belongs to a different root".into());
        }
        self.move_entry(&self.entry(&ticket.stored)?, &ticket.original)
    }
}

impl Entry {
    fn validate(&self) -> Result<()> {
        let current = self
            .parent
            .symlink_metadata(&self.name)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            if self.metadata.dev() != current.dev() || self.metadata.ino() != current.ino() {
                return Err("The selected item was replaced; select it again".into());
            }
        }
        if current.file_type() != self.metadata.file_type() {
            return Err("The selected item changed; select it again".into());
        }
        Ok(())
    }
}

fn rename_new(
    from: &Dir,
    source: &std::ffi::OsStr,
    to: &Dir,
    target: impl AsRef<Path>,
) -> Result<()> {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))]
    {
        rustix::fs::renameat_with(
            from,
            Path::new(source),
            to,
            target.as_ref(),
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|e| format!("Move failed without overwriting another item: {e}"))?;
        Ok(())
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    )))]
    {
        let _ = (from, source, to, target);
        Err("Atomic moves without replacing files are unavailable on this platform".into())
    }
}

fn sync(dir: &Dir) -> Result<()> {
    #[cfg(unix)]
    dir.open(".")
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

#[cfg(test)]
mod tests;
