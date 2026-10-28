use crate::{Library, Result, address};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

impl Library {
    pub fn extract_zip(&self, uri: &str, destination: &Path) -> Result<PathBuf> {
        if destination.exists()
            && fs::read_dir(destination)
                .map_err(|error| error.to_string())?
                .next()
                .is_some()
        {
            return Err("Choose an empty extraction folder".into());
        }
        let destination = std::path::absolute(destination).map_err(|error| error.to_string())?;
        let parent = destination.parent().ok_or("Choose an extraction folder")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let url = address(uri)?;
        let remote = if url.scheme() == "file" {
            None
        } else {
            Some(self.http_file(uri, Default::default(), 64 * 1024 * 1024 * 1024, false)?)
        };
        let path = if let Some(file) = &remote {
            file.path.clone()
        } else {
            url.to_file_path()
                .map_err(|_| "Invalid local archive address")?
        };
        let file = fs::File::open(path).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
        if archive.len() > 100_000 {
            return Err("This archive has more than 100,000 entries".into());
        }
        let staging = tempfile::tempdir_in(parent).map_err(|error| error.to_string())?;
        let mut expanded = 0u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            let relative = entry
                .enclosed_name()
                .ok_or("Archive paths must stay inside the extracted folder")?;
            if entry.is_symlink()
                || relative
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
                || entry.name().contains(['\\', ':', '\0'])
                || entry
                    .unix_mode()
                    .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o040000 | 0o100000))
            {
                return Err("Archive entries must be ordinary relative files and folders".into());
            }
            expanded = expanded
                .checked_add(entry.size())
                .ok_or("Archive size overflow")?;
            if expanded > 64 * 1024 * 1024 * 1024 {
                return Err("Choose an archive with at most 64 GiB expanded data".into());
            }
            let path = staging.path().join(relative);
            if entry.is_dir() {
                fs::create_dir_all(path).map_err(|error| error.to_string())?;
                continue;
            }
            fs::create_dir_all(path.parent().ok_or("Invalid archive file")?)
                .map_err(|error| error.to_string())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())?;
            let expected = entry.size();
            let mut bounded = std::io::Read::take(&mut entry, expected.saturating_add(1));
            let copied = io::copy(&mut bounded, &mut output).map_err(|error| error.to_string())?;
            if copied != expected {
                return Err("The archive returned an incomplete file".into());
            }
        }
        fs::rename(staging.path(), &destination).map_err(|error| error.to_string())?;
        Ok(destination)
    }
}
