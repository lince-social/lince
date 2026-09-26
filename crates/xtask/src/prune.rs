use std::env;
use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Report {
    snapshots: usize,
    skipped: usize,
}

fn target_directory(root: &Path) -> PathBuf {
    root.join(env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target".into()))
}

pub(crate) fn with_cleanup<T>(
    root: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    with_cleanup_at(&target_directory(root), operation)
}

fn with_cleanup_at<T>(
    target: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    automatic(target);
    let result = operation();
    automatic(target);
    result
}

fn automatic(target: &Path) {
    let mut report = Report::default();
    match visit(target, 3, false, &mut report) {
        Ok(()) if report.snapshots > 0 => {
            println!(
                "Removed {} superseded incremental snapshots.",
                report.snapshots
            );
        }
        Ok(()) => {}
        Err(error) => eprintln!("Could not prune incremental snapshots: {error}"),
    }
}

fn is_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn lock(path: &Path) -> io::Result<Option<File>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let file = File::options().read(true).write(true).open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn visit(path: &Path, depth: usize, dry_run: bool, report: &mut Report) -> io::Result<()> {
    if !is_directory(path) {
        return Ok(());
    }
    let incremental = path.join("incremental");
    if is_directory(&incremental) {
        let Some(_profile_lock) = lock(&path.join(".cargo-lock"))? else {
            report.skipped += 1;
            return Ok(());
        };
        for entry in fs::read_dir(incremental)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                prune_variant(&entry.path(), dry_run, report)?;
            }
        }
    } else if depth > 0 {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                visit(&entry.path(), depth - 1, dry_run, report)?;
            }
        }
    }
    Ok(())
}

fn session(name: &str) -> Option<(u128, String)> {
    let parts: Vec<_> = name.split('-').collect();
    if parts.len() != 4 || parts[0] != "s" || parts[3] == "working" {
        return None;
    }
    if parts[1..].iter().any(|part| {
        part.is_empty()
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
    }) {
        return None;
    }
    let timestamp = u128::from_str_radix(parts[1], 36).ok()?;
    Some((timestamp, format!("s-{}-{}.lock", parts[1], parts[2])))
}

fn prune_variant(path: &Path, dry_run: bool, report: &mut Report) -> io::Result<()> {
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some((timestamp, lock_name)) = entry.file_name().to_str().and_then(session) else {
            continue;
        };
        if fs::symlink_metadata(entry.path().join("dep-graph.bin"))
            .is_ok_and(|metadata| metadata.is_file())
        {
            snapshots.push((timestamp, entry.path(), path.join(lock_name)));
        }
    }
    let Some(newest) = snapshots.iter().map(|snapshot| snapshot.0).max() else {
        return Ok(());
    };
    for (timestamp, snapshot, lock_path) in snapshots {
        if timestamp >= newest {
            continue;
        }
        let Some(_session_lock) = lock(&lock_path)? else {
            report.skipped += 1;
            continue;
        };
        if !dry_run {
            fs::remove_dir_all(snapshot)?;
        }
        report.snapshots += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = env::temp_dir().join(format!(
                "lince-prune-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn snapshot(&self, profile: &str, variant: &str, name: &str) -> PathBuf {
            let profile = self.0.join(profile);
            let snapshot = profile.join("incremental").join(variant).join(name);
            fs::create_dir_all(&snapshot).unwrap();
            fs::write(profile.join(".cargo-lock"), b"").unwrap();
            fs::write(snapshot.join("dep-graph.bin"), b"cache").unwrap();
            if let Some((_, name)) = session(name) {
                fs::write(snapshot.parent().unwrap().join(name), b"").unwrap();
            }
            snapshot
        }

        fn prune(&self, dry_run: bool) -> Report {
            let mut report = Report::default();
            visit(&self.0, 3, dry_run, &mut report).unwrap();
            report
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn workflow_prunes_before_and_after_success() {
        let fixture = Fixture::new();
        let old = fixture.snapshot("debug", "engine", "s-1-a-b");
        let current = fixture.snapshot("debug", "engine", "s-2-a-b");
        let value = with_cleanup_at(&fixture.0, || {
            assert!(!old.exists());
            assert!(current.exists());
            fixture.snapshot("debug", "engine", "s-3-a-b");
            Ok(42)
        })
        .unwrap();
        assert_eq!(value, 42);
        assert!(!current.exists());
        assert!(current.parent().unwrap().join("s-3-a-b").exists());
    }

    #[test]
    fn workflow_prunes_after_failure_and_preserves_the_error() {
        let fixture = Fixture::new();
        let old = fixture.snapshot("debug", "engine", "s-1-a-b");
        let result: Result<(), String> = with_cleanup_at(&fixture.0, || {
            fixture.snapshot("debug", "engine", "s-2-a-b");
            Err("test command failed".into())
        });
        assert_eq!(result, Err("test command failed".into()));
        assert!(!old.exists());
    }

    #[test]
    fn workflow_still_runs_while_a_profile_is_busy() {
        let fixture = Fixture::new();
        let old = fixture.snapshot("debug", "engine", "s-1-a-b");
        fixture.snapshot("debug", "engine", "s-2-a-b");
        let _profile_lock = lock(&fixture.0.join("debug/.cargo-lock")).unwrap().unwrap();
        let value = with_cleanup_at(&fixture.0, || Ok(42)).unwrap();
        assert_eq!(value, 42);
        assert!(old.exists());
    }

    #[test]
    fn keeps_newest_snapshot_of_each_variant_and_compiled_artifacts() {
        let fixture = Fixture::new();
        let old = fixture.snapshot("debug", "engine-one", "s-z-abc-def");
        let new = fixture.snapshot("debug", "engine-one", "s-10-abc-ghi");
        let other = fixture.snapshot("debug", "engine-two", "s-1-abc-def");
        let deps = fixture.0.join("debug/deps");
        fs::create_dir(&deps).unwrap();
        fs::write(deps.join("libengine.rlib"), b"compiled").unwrap();
        assert_eq!(fixture.prune(false).snapshots, 1);
        assert!(!old.exists());
        assert!(new.exists());
        assert!(other.exists());
        assert_eq!(fs::read(deps.join("libengine.rlib")).unwrap(), b"compiled");
        assert_eq!(fixture.prune(false).snapshots, 0);
    }

    #[test]
    fn preview_preserves_files_and_supports_nested_targets() {
        let fixture = Fixture::new();
        let old = fixture.snapshot("media/x86_64-pc-windows-gnu/debug", "engine", "s-1-a-b");
        let new = fixture.snapshot("media/x86_64-pc-windows-gnu/debug", "engine", "s-2-a-b");
        assert_eq!(fixture.prune(true).snapshots, 1);
        assert!(old.exists());
        assert!(new.exists());
        assert_eq!(fixture.prune(false).snapshots, 1);
        assert!(!old.exists());
    }

    #[test]
    fn skips_busy_profiles_and_snapshots() {
        let fixture = Fixture::new();
        let old = fixture.snapshot("debug", "engine", "s-1-a-b");
        fixture.snapshot("debug", "engine", "s-2-a-b");
        let profile_lock = lock(&fixture.0.join("debug/.cargo-lock")).unwrap().unwrap();
        let report = fixture.prune(false);
        assert_eq!(report.snapshots, 0);
        assert_eq!(report.skipped, 1);
        drop(profile_lock);
        let session_lock = lock(&old.parent().unwrap().join("s-1-a.lock"))
            .unwrap()
            .unwrap();
        let report = fixture.prune(false);
        assert_eq!(report.snapshots, 0);
        assert_eq!(report.skipped, 1);
        assert!(old.exists());
        drop(session_lock);
        assert_eq!(fixture.prune(false).snapshots, 1);
    }

    #[test]
    fn leaves_incomplete_unknown_and_unlocked_snapshots_alone() {
        let fixture = Fixture::new();
        let working = fixture.snapshot("debug", "engine", "s-3-a-working");
        let unknown = fixture.snapshot("debug", "engine", "unexpected");
        let unlocked = fixture.snapshot("debug", "engine", "s-1-a-b");
        let newest = fixture.snapshot("debug", "engine", "s-2-a-b");
        fs::remove_file(unlocked.parent().unwrap().join("s-1-a.lock")).unwrap();
        assert_eq!(fixture.prune(false).snapshots, 0);
        for path in [working, unknown, unlocked, newest] {
            assert!(path.exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn ignores_symlinks_and_preserves_hardlinked_cache_files() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let old = fixture.snapshot("debug", "engine", "s-1-a-b");
        let new = fixture.snapshot("debug", "engine", "s-2-a-b");
        fs::hard_link(old.join("dep-graph.bin"), new.join("shared.bin")).unwrap();
        symlink(&old, old.parent().unwrap().join("s-3-a-b")).unwrap();
        let external = Fixture::new();
        let external_old = external.snapshot("debug", "other", "s-1-a-b");
        external.snapshot("debug", "other", "s-2-a-b");
        symlink(external.0.join("debug"), fixture.0.join("linked")).unwrap();
        assert_eq!(fixture.prune(false).snapshots, 1);
        assert!(!old.exists());
        assert!(external_old.exists());
        assert_eq!(fs::read(new.join("shared.bin")).unwrap(), b"cache");
    }
}
