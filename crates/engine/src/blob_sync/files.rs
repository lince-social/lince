use super::*;
use nucleus::blob_sync::{MAX_BYTES, MAX_ENTRIES, validate_path};

impl BlobSync {
    pub(super) async fn snapshot(
        &self,
        id: &str,
        paths: Vec<PathBuf>,
    ) -> Result<Manifest, EngineError> {
        let selected = tokio::task::spawn_blocking(move || scan(paths))
            .await
            .map_err(failure)??;
        let mut entries = Vec::new();
        for (index, (path, mut entry)) in selected.into_iter().enumerate() {
            if entry.hash.is_some() {
                let tag = self
                    .blobs
                    .add_path(&path)
                    .with_named_tag(format!("lince-blob/{id}/{index}"))
                    .await
                    .map_err(failure)?;
                let info = self.blobs.observe(tag.hash).await.map_err(failure)?;
                if !info.is_complete() || info.size() != entry.size {
                    return Err(failure(
                        "A selected file changed size while preparing the copy. Send it again",
                    ));
                }
                entry.hash = Some(tag.hash.to_string());
            }
            entries.push(entry);
        }
        let manifest = Manifest {
            id: id.into(),
            entries,
        };
        manifest.validate().map_err(failure)?;
        self.blobs.sync_db().await.map_err(failure)?;
        Ok(manifest)
    }

    pub(super) async fn export(
        &self,
        transfer: &Transfer,
    ) -> Result<tempfile::TempDir, EngineError> {
        let destination = Path::new(
            transfer
                .destination
                .as_deref()
                .ok_or_else(|| failure("Choose a destination first"))?,
        );
        let parent = destination
            .parent()
            .ok_or_else(|| failure("Invalid destination"))?;
        let staging = tempfile::Builder::new()
            .prefix(".lince-blob-")
            .tempdir_in(parent)?;
        for entry in &transfer.manifest.entries {
            if entry.hash.is_none() {
                tokio::fs::create_dir_all(staging.path().join(&entry.path)).await?;
            }
        }
        for entry in &transfer.manifest.entries {
            if let Some(hash) = &entry.hash {
                let hash = hash.parse::<iroh_blobs::Hash>().map_err(failure)?;
                let target = staging.path().join(&entry.path);
                if target.try_exists()? {
                    return Err(failure("These filenames collide on this filesystem"));
                }
                self.blobs.export(hash, target).await.map_err(failure)?;
            }
        }
        Ok(staging)
    }
}

fn scan(paths: Vec<PathBuf>) -> Result<Vec<(PathBuf, Entry)>, EngineError> {
    if paths.is_empty() || paths.len() > MAX_ENTRIES {
        return Err(failure("Choose files or a folder"));
    }
    let mut pending = Vec::new();
    for path in paths {
        let path = std::path::absolute(path)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| failure("Choose a named file or directory"))?
            .to_string();
        pending.push((path, name));
    }
    let mut files = Vec::new();
    let mut bytes = 0u64;
    while let Some((path, name)) = pending.pop() {
        validate_path(&name).map_err(failure)?;
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_symlink() || !(metadata.is_dir() || metadata.is_file()) {
            return Err(failure(
                "Blob Sync accepts ordinary files and directories; remove links and special files",
            ));
        }
        let directory = metadata.is_dir();
        let size = if directory { 0 } else { metadata.len() };
        bytes = bytes
            .checked_add(size)
            .filter(|bytes| *bytes <= MAX_BYTES)
            .ok_or_else(|| failure("A copy cannot exceed 1 TiB"))?;
        if directory {
            for child in std::fs::read_dir(&path)? {
                let child = child?;
                let child_name = child
                    .file_name()
                    .into_string()
                    .map_err(|_| failure("Use UTF-8 filenames"))?;
                pending.push((child.path(), format!("{name}/{child_name}")));
                if files.len() + pending.len() >= MAX_ENTRIES {
                    return Err(failure("Too many files or directories"));
                }
            }
        }
        files.push((
            path,
            Entry {
                path: name,
                hash: (!directory).then(|| "0".repeat(64)),
                size,
            },
        ));
        if files.len() > MAX_ENTRIES {
            return Err(failure("Too many files or directories"));
        }
    }
    files.sort_by(|a, b| a.1.path.cmp(&b.1.path));
    Manifest {
        id: uuid::Uuid::new_v4().to_string(),
        entries: files.iter().map(|(_, entry)| entry.clone()).collect(),
    }
    .validate()
    .map_err(failure)?;
    Ok(files)
}

pub(super) fn verify_copy(destination: &Path, manifest: &Manifest) -> Result<(), EngineError> {
    use std::io::Read;
    if std::fs::symlink_metadata(destination)?.is_symlink() {
        return Err(failure("The destination is a link; it was not changed"));
    }
    let children = std::fs::read_dir(destination)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    let entries = scan(children)?;
    if entries.len() != manifest.entries.len() {
        return Err(failure("The destination already contains a different copy"));
    }
    let expected = manifest
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut buffer = [0u8; 64 * 1024];
    for (path, entry) in entries {
        let expected = expected
            .get(entry.path.as_str())
            .ok_or_else(|| failure("The destination already contains different files"))?;
        if entry.size != expected.size || entry.hash.is_some() != expected.hash.is_some() {
            return Err(failure("The destination already contains different files"));
        }
        if let Some(hash) = &expected.hash {
            let mut file = std::fs::File::open(path)?;
            let mut hasher = bao_tree::blake3::Hasher::new();
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            if hasher.finalize().to_hex().as_str() != hash {
                return Err(failure(
                    "The destination already contains different file contents",
                ));
            }
        }
    }
    Ok(())
}
