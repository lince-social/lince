use std::collections::BTreeMap;
use std::path::Path;

use crate::scenario::LinguaFile;
use crate::{Result, artifacts};

pub fn package(directory: &Path) -> Result<Vec<LinguaFile>> {
    fn scan(
        root: &Path,
        directory: &Path,
        files: &mut Vec<LinguaFile>,
        bytes: &mut u64,
        entries: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth > 32 {
            return Err("Lingua package exceeds 32 directory levels".into());
        }
        for entry in std::fs::read_dir(directory)? {
            *entries += 1;
            if *entries > 4096 {
                return Err("Lingua package exceeds 4096 directory entries".into());
            }
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err("Lingua packages do not follow symlinks".into());
            }
            if kind.is_dir() {
                scan(root, &entry.path(), files, bytes, entries, depth + 1)?;
            } else if kind.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|value| value == "lingua")
            {
                *bytes = bytes.saturating_add(entry.metadata()?.len());
                if files.len() >= 256 || *bytes > 64 * 1024 * 1024 {
                    return Err("Lingua package exceeds 256 files or 64 MiB".into());
                }
                files.push(LinguaFile {
                    name: entry
                        .path()
                        .strip_prefix(root)?
                        .to_str()
                        .ok_or("Lingua path is not UTF-8")?
                        .into(),
                    file: entry
                        .path()
                        .strip_prefix(root)?
                        .to_str()
                        .ok_or("Lingua path is not UTF-8")?
                        .into(),
                    hash: artifacts::file_hash(&entry.path())?,
                });
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    scan(directory, directory, &mut files, &mut 0, &mut 0, 0)?;
    if files.is_empty() {
        return Err("the seed package contains no .lingua files".into());
    }
    files.sort_by(|left, right| left.file.cmp(&right.file));
    Ok(files)
}

pub(crate) async fn import(
    engine: &engine::Engine,
    files: &[LinguaFile],
    root: &Path,
    working: &Path,
) -> Result<nucleus::simulation::Observation> {
    std::fs::create_dir(working)?;
    let mut authored = BTreeMap::new();
    for (index, source) in files.iter().enumerate() {
        let path = root.join(&source.file);
        let copy = working.join(format!("{index:04}.lingua"));
        std::fs::copy(path, &copy)?;
        if artifacts::file_hash(&copy)? != source.hash {
            return Err("Lingua seed hash does not match".into());
        }
        authored.insert(source.name.clone(), source.hash.clone());
    }
    let imported = engine.import_lingua_directory(working).await?;
    if !imported.conflicts.is_empty() {
        return Err(format!(
            "Lingua seed refused: {}",
            imported
                .conflicts
                .iter()
                .map(|conflict| {
                    let file = Path::new(&conflict.path)
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .and_then(|name| name.parse::<usize>().ok())
                        .and_then(|index| files.get(index));
                    format!(
                        "{}: {}",
                        file.map_or("package", |file| file.name.as_str()),
                        conflict.reason
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        )
        .into());
    }
    Ok(nucleus::simulation::Observation::LinguaImported {
        files: authored,
        created: imported.created,
        updated: imported.updated_from_disk,
    })
}
