// non-lean-yet
// Scope: const MAX_ENTRIES, struct Entry, struct Listing, struct Batch.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 67.
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   list:ignored (28), list_batches:ignored (53), search:ignored (129).
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 28, 50, 129.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 22.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 10,
//   11, 12, 13, 17, 18, 19, 22, 23, 24, 25, 28, 42, 50, 129.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{Result, files::Scope};

pub const MAX_ENTRIES: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
    pub link: bool,
}

#[derive(Clone, Debug)]
pub struct Listing {
    pub entries: Vec<Entry>,
    pub truncated: bool,
}

pub struct Batch {
    pub entries: Vec<Entry>,
    pub finished: bool,
    pub truncated: bool,
}

pub fn list(scope: &Scope, path: &Path, ignored: bool) -> Result<Listing> {
    let mut listing = Listing {
        entries: Vec::new(),
        truncated: false,
    };
    list_batches(scope, path, ignored, &AtomicBool::new(false), |batch| {
        listing.entries.extend(batch.entries);
        listing.truncated = batch.truncated;
        true
    })?;
    sort(&mut listing.entries);
    Ok(listing)
}

pub fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
}

pub fn list_batches(
    scope: &Scope,
    path: &Path,
    ignored: bool,
    cancel: &AtomicBool,
    mut emit: impl FnMut(Batch) -> bool,
) -> Result<()> {
    let relative = scope.relative(path)?;
    let directory = scope
        .dir
        .open_dir(if relative.as_os_str().is_empty() {
            Path::new(".")
        } else {
            &relative
        })
        .map_err(|e| e.to_string())?;
    let mut filter = ignore::gitignore::GitignoreBuilder::new(&scope.path);
    let mut ancestor = scope.path.clone();
    let _ = filter.add(ancestor.join(".gitignore"));
    for component in relative.components() {
        ancestor.push(component);
        let _ = filter.add(ancestor.join(".gitignore"));
    }
    let filter = filter.build().map_err(|e| e.to_string())?;
    let mut entries = Vec::with_capacity(256);
    let mut count = 0;
    let mut truncated = false;
    for entry in directory.entries().map_err(|e| e.to_string())? {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let path = scope.path.join(&relative).join(entry.file_name());
        let directory = kind.is_dir()
            || kind.is_symlink()
                && scope
                    .dir
                    .metadata(relative.join(entry.file_name()))
                    .is_ok_and(|m| m.is_dir());
        if !ignored
            && (entry.file_name() == ".git"
                || entry.file_name() == ".lince-trash"
                || filter
                    .matched_path_or_any_parents(&path, directory)
                    .is_ignore())
        {
            continue;
        }
        if count == MAX_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(Entry {
            path,
            directory,
            link: kind.is_symlink(),
        });
        count += 1;
        if entries.len() == 256 {
            sort(&mut entries);
            if !emit(Batch {
                entries: std::mem::replace(&mut entries, Vec::with_capacity(256)),
                finished: false,
                truncated: false,
            }) {
                return Ok(());
            }
        }
    }
    sort(&mut entries);
    emit(Batch {
        entries,
        finished: true,
        truncated,
    });
    Ok(())
}

pub fn search(scope: &Scope, query: &str, ignored: bool, cancel: &AtomicBool) -> Result<Listing> {
    let needle = query.to_lowercase();
    let mut builder = ignore::WalkBuilder::new(&scope.path);
    builder
        .hidden(false)
        .follow_links(false)
        .git_ignore(!ignored)
        .git_global(!ignored)
        .git_exclude(!ignored)
        .require_git(false)
        .max_depth(Some(64));
    builder.filter_entry(move |entry| {
        ignored || (entry.file_name() != ".git" && entry.file_name() != ".lince-trash")
    });
    let mut entries = Vec::new();
    let mut visited = 0;
    let mut truncated = false;
    for entry in builder.build() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Search cancelled".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        visited += 1;
        if entries.len() >= 1_000 || visited >= 100_000 {
            truncated = true;
            break;
        }
        if entry.depth() == 0 {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&scope.path)
            .map_err(|e| e.to_string())?;
        if !ignored && relative.components().any(|part| part.as_os_str() == ".git") {
            continue;
        }
        if !relative.to_string_lossy().to_lowercase().contains(&needle) {
            continue;
        }
        let kind = entry.file_type();
        entries.push(Entry {
            path: entry.into_path(),
            directory: kind.is_some_and(|k| k.is_dir()),
            link: kind.is_some_and(|k| k.is_symlink()),
        });
    }
    Ok(Listing { entries, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_arrive_in_bounded_batches_and_can_be_cancelled_after_the_first() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..700 {
            std::fs::write(dir.path().join(format!("file-{index:04}")), []).unwrap();
        }
        let scope = Scope::open(dir.path()).unwrap();
        let cancel = AtomicBool::new(false);
        let mut sizes = Vec::new();
        list_batches(&scope, dir.path(), false, &cancel, |batch| {
            sizes.push(batch.entries.len());
            assert!(batch.entries.len() <= 256);
            true
        })
        .unwrap();
        assert_eq!(sizes, [256, 256, 188]);
        let mut count = 0;
        list_batches(&scope, dir.path(), false, &cancel, |batch| {
            count += batch.entries.len();
            cancel.store(true, Ordering::Relaxed);
            true
        })
        .unwrap();
        assert_eq!(count, 256);
    }
}
