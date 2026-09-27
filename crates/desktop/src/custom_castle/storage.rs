use super::CustomCastle;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

const MAX_FILE: u64 = 4 * 1024 * 1024;
const MAX_LIBRARY: u64 = 32 * 1024 * 1024;

fn error(message: &str) -> io::Error {
    io::Error::other(message)
}

pub(super) fn directory(world: &bevy::prelude::World) -> io::Result<PathBuf> {
    world
        .get_resource::<crate::workspace::WorkspaceFile>()
        .map(|file| file.directory().join("castles"))
        .ok_or_else(|| error("Workspace storage is unavailable."))
}

fn check_directory(directory: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(directory)?.file_type().is_dir() {
        return Err(error(
            "Custom Castles must be stored in a regular directory.",
        ));
    }
    Ok(())
}

pub(super) fn save(directory: &Path, castle: &CustomCastle) -> io::Result<PathBuf> {
    if !castle.valid() {
        return Err(error("Invalid custom Castle."));
    }
    let bytes = serde_json::to_vec_pretty(castle)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(error("This custom Castle exceeds 4 MiB."));
    }
    fs::create_dir_all(directory)?;
    check_directory(directory)?;
    let mut count = 0;
    let mut total = bytes.len() as u64;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        count += 1;
        if entry.path().extension().is_some_and(|e| e == "json") {
            total = total.saturating_add(entry.metadata()?.len());
        }
        if count >= 256 || total > MAX_LIBRARY {
            return Err(error(
                "The Custom library is full. Move some files out of this folder first.",
            ));
        }
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    let path = directory.join(format!("{}.json", super::file_id()));
    temporary.persist_noclobber(&path).map_err(|e| e.error)?;
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    Ok(path)
}

fn open(path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(error("A custom Castle must be a regular JSON file."));
    }
    #[cfg(unix)]
    {
        use rustix::fs::{Mode, OFlags};
        Ok(File::from(rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?))
    }
    #[cfg(not(unix))]
    File::open(path)
}

pub(super) fn load(directory: &Path, filename: &str) -> io::Result<CustomCastle> {
    check_directory(directory)?;
    let path = Path::new(filename);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || path.extension().is_none_or(|extension| extension != "json")
    {
        return Err(error("Choose a JSON file in the Custom Castles directory."));
    }
    let file = open(&directory.join(path))?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_FILE {
        return Err(error("A custom Castle must be a file of at most 4 MiB."));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(error("This custom Castle exceeds 4 MiB."));
    }
    let castle: CustomCastle = serde_json::from_slice(&bytes)?;
    if !castle.valid() {
        return Err(error("Invalid custom Castle contents or connections."));
    }
    Ok(castle)
}

pub(super) fn entries(directory: &Path) -> (Vec<(String, String, usize)>, Vec<String>) {
    if !directory.exists() {
        return (Vec::new(), Vec::new());
    }
    let paths = check_directory(directory).and_then(|()| fs::read_dir(directory));
    let paths = match paths {
        Ok(paths) => paths,
        Err(error) => return (Vec::new(), vec![error.to_string()]),
    };
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut bytes = 0;
    for (index, entry) in paths.enumerate() {
        if index >= 256 {
            errors.push("Only the first 256 custom files are shown.".into());
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(error.to_string());
                continue;
            }
        };
        let filename = entry.file_name().to_string_lossy().into_owned();
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        bytes += entry.metadata().map_or(0, |m| m.len().min(MAX_FILE));
        if bytes > MAX_LIBRARY {
            errors.push("Custom files exceed the 32 MiB library limit.".into());
            break;
        }
        match load(directory, &filename) {
            Ok(castle) => entries.push((filename, castle.name, castle.parts.len())),
            Err(error) => errors.push(format!("{filename}: {error}")),
        }
    }
    entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    (entries, errors)
}
