use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File, Metadata, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const LIMIT: u64 = 300_000_000_000;

#[cfg(unix)]
type Identity = (u64, u64);
#[cfg(not(unix))]
type Identity = PathBuf;

fn identity(path: &Path, metadata: &Metadata) -> Identity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        (metadata.dev(), metadata.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        path.to_path_buf()
    }
}

fn allocated(metadata: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        metadata.len()
    }
}

fn walk(path: &Path, visit: &mut impl FnMut(&Path, &Metadata)) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    visit(path, &metadata);
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            walk(&entry?.path(), visit)?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Usage {
    files: HashMap<Identity, (u64, usize)>,
    bytes: u64,
}

impl Usage {
    fn read(root: &Path) -> io::Result<Self> {
        let mut value = Self::default();
        walk(root, &mut |path, metadata| {
            let entry = value
                .files
                .entry(identity(path, metadata))
                .or_insert_with(|| {
                    let size = allocated(metadata);
                    value.bytes = value.bytes.saturating_add(size);
                    (size, 0)
                });
            entry.1 += 1;
        })?;
        Ok(value)
    }

    fn forget(&mut self, nodes: Vec<Identity>) {
        for id in nodes {
            if let Some((size, references)) = self.files.get_mut(&id) {
                *references -= 1;
                if *references == 0 {
                    self.bytes = self.bytes.saturating_sub(*size);
                }
            }
        }
    }
}

fn lock(path: &Path, create: bool) -> io::Result<Option<File>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !create => return Ok(None),
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn profiles(path: &Path, depth: usize, found: &mut Vec<PathBuf>) -> io::Result<()> {
    if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
        return Ok(());
    }
    if path.join(".cargo-lock").is_file() {
        found.push(path.to_path_buf());
    } else if depth > 0 {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && !matches!(
                    entry.file_name().to_str(),
                    Some("sccache" | "park-validation" | "build-iteration" | "doc" | "package")
                )
            {
                profiles(&entry.path(), depth - 1, found)?;
            }
        }
    }
    Ok(())
}

fn artifact_hash(name: &str) -> Option<&str> {
    let stem = name.split('.').next()?;
    let (_, hash) = stem.rsplit_once('-')?;
    (hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hash)
}

#[derive(Default)]
struct Candidate {
    paths: Vec<PathBuf>,
    newest: Option<SystemTime>,
    running: bool,
}

impl Candidate {
    fn add(&mut self, path: PathBuf, running: &HashSet<Identity>) -> io::Result<()> {
        walk(&path, &mut |file, metadata| {
            if let Ok(modified) = metadata.modified() {
                self.newest = Some(self.newest.map_or(modified, |old| old.max(modified)));
            }
            self.running |= running.contains(&identity(file, metadata));
        })?;
        self.paths.push(path);
        Ok(())
    }
}

fn running_executables() -> HashSet<Identity> {
    #[cfg(not(target_os = "linux"))]
    return HashSet::new();
    #[cfg(target_os = "linux")]
    {
        let mut identities = HashSet::new();
        if let Ok(processes) = fs::read_dir("/proc") {
            for process in processes.flatten() {
                if !process
                    .file_name()
                    .to_string_lossy()
                    .bytes()
                    .all(|byte| byte.is_ascii_digit())
                {
                    continue;
                }
                let path = process.path().join("exe");
                if let Ok(metadata) = fs::metadata(&path) {
                    identities.insert(identity(&path, &metadata));
                }
            }
        }
        identities
    }
}

fn candidates(profile: &Path, running: &HashSet<Identity>) -> io::Result<Vec<Candidate>> {
    let mut groups: BTreeMap<String, Candidate> = BTreeMap::new();
    for directory in ["deps", ".fingerprint", "build"] {
        let entries = match fs::read_dir(profile.join(directory)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                continue;
            }
            if let Some(hash) = entry.file_name().to_str().and_then(artifact_hash) {
                groups
                    .entry(hash.into())
                    .or_default()
                    .add(entry.path(), running)?;
            }
        }
    }
    let mut aliases = HashMap::new();
    for (hash, group) in &groups {
        for path in &group.paths {
            if let Ok(metadata) = fs::symlink_metadata(path)
                && metadata.is_file()
            {
                aliases.insert(identity(path, &metadata), hash.clone());
            }
        }
    }
    for entry in fs::read_dir(profile)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && !entry.file_name().to_string_lossy().starts_with('.') {
            let metadata = entry.metadata()?;
            if let Some(hash) = aliases.get(&identity(&entry.path(), &metadata)) {
                groups.get_mut(hash).unwrap().add(entry.path(), running)?;
            }
        }
    }
    let mut result: Vec<_> = groups.into_values().collect();
    match fs::read_dir(profile.join("incremental")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let mut candidate = Candidate::default();
                    candidate.add(entry.path(), running)?;
                    result.push(candidate);
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(result)
}

#[derive(Debug, Default)]
struct Report {
    measured: bool,
    before: u64,
    after: u64,
    removed: usize,
    busy: usize,
}

fn enforce(target: &Path, limit: u64, dry_run: bool) -> io::Result<Report> {
    if !fs::symlink_metadata(target).is_ok_and(|metadata| metadata.is_dir()) {
        return Ok(Report::default());
    }
    let _guard = if dry_run {
        None
    } else {
        let Some(guard) = lock(&target.join(".lince-cache-prune.lock"), true)? else {
            return Ok(Report {
                busy: 1,
                ..Default::default()
            });
        };
        Some(guard)
    };
    let mut usage = Usage::read(target)?;
    let mut report = Report {
        measured: true,
        before: usage.bytes,
        after: usage.bytes,
        ..Default::default()
    };
    if usage.bytes <= limit {
        return Ok(report);
    }
    let mut roots = Vec::new();
    profiles(target, 4, &mut roots)?;
    roots.sort();
    let running = running_executables();
    let mut guards = Vec::new();
    let mut eligible = Vec::new();
    for profile in roots {
        if let Some(guard) = lock(&profile.join(".cargo-lock"), false)? {
            guards.push(guard);
            eligible.extend(candidates(&profile, &running)?);
        } else {
            report.busy += 1;
        }
    }
    usage = Usage::read(target)?;
    report.before = usage.bytes;
    eligible.sort_by(|left, right| {
        left.newest
            .cmp(&right.newest)
            .then_with(|| left.paths.cmp(&right.paths))
    });
    for candidate in eligible {
        if usage.bytes <= limit {
            break;
        }
        if candidate.running {
            report.busy += 1;
            continue;
        }
        let mut nodes = Vec::new();
        for path in &candidate.paths {
            walk(path, &mut |file, metadata| {
                nodes.push(identity(file, metadata))
            })?;
        }
        if !dry_run {
            for path in candidate.paths {
                match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
                    Ok(_) => fs::remove_file(path)?,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
        }
        usage.forget(nodes);
        report.removed += 1;
    }
    report.after = if dry_run {
        usage.bytes
    } else {
        Usage::read(target)?.bytes
    };
    Ok(report)
}

pub(crate) fn command(target: &Path, arguments: &[OsString]) -> Result<(), String> {
    let mut limit = LIMIT;
    let mut dry_run = false;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--dry-run") => dry_run = true,
            Some("--max-bytes") => {
                limit = arguments
                    .next()
                    .and_then(|value| value.to_str())
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .ok_or("--max-bytes requires a positive byte count")?
            }
            _ => return Err("usage: prune-target [--dry-run] [--max-bytes BYTES]".into()),
        }
    }
    let report = enforce(target, limit, dry_run)
        .map_err(|error| format!("Target-cache cleanup failed: {error}"))?;
    if !report.measured && report.busy > 0 {
        println!("Target cache cleanup deferred: another cleanup is active.");
        return Ok(());
    }
    if report.removed > 0 || report.after > limit || report.busy > 0 {
        println!(
            "Target cache{}: {} → {} bytes (limit {}); {} artifact groups{}, {} busy profiles/executables deferred.",
            if dry_run { " preview" } else { "" },
            report.before,
            report.after,
            limit,
            report.removed,
            if dry_run { " selected" } else { " removed" },
            report.busy
        );
        if report.after > limit {
            println!(
                "Remaining data is busy or outside compiled artifact groups; cleanup will retry later."
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::FileTimes;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "lince-budget-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("debug/deps")).unwrap();
            fs::write(root.join("debug/.cargo-lock"), []).unwrap();
            Self(root)
        }
        fn file(&self, name: &str, age: u64) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, [42; 8192]).unwrap();
            self.age(&path, age);
            path
        }
        fn age(&self, path: &Path, age: u64) {
            let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(age);
            walk(path, &mut |path, metadata| {
                if !metadata.is_symlink() {
                    File::open(path)
                        .unwrap()
                        .set_times(FileTimes::new().set_modified(stamp))
                        .unwrap();
                }
            })
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn command_defaults_to_300_gb_and_validates_explicit_limits() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        command(&fixture.0, &[]).unwrap();
        assert!(old.exists());
        assert!(command(&fixture.0, &["--max-bytes".into(), "0".into()]).is_err());
        assert!(command(&fixture.0, &["--max-bytes".into()]).is_err());
        assert!(command(&fixture.0, &["--unknown".into()]).is_err());
        command(
            &fixture.0,
            &["--max-bytes".into(), "1".into(), "--dry-run".into()],
        )
        .unwrap();
        assert!(old.exists());
        command(&fixture.0, &["--max-bytes".into(), "1".into()]).unwrap();
        assert!(!old.exists());
    }

    #[test]
    fn evicts_oldest_dependency_group_with_fingerprint_and_binary_alias() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/program-1111111111111111", 1);
        let fingerprint = fixture.file(
            "debug/.fingerprint/program-1111111111111111/test-bin-program.json",
            1,
        );
        fixture.age(fingerprint.parent().unwrap(), 1);
        let new = fixture.file("debug/deps/librecent-2222222222222222.rlib", 20);
        let alias = fixture.0.join("debug/program");
        fs::hard_link(&old, &alias).unwrap();
        let before = Usage::read(&fixture.0).unwrap().bytes;
        let report = enforce(&fixture.0, before - 1, false).unwrap();
        assert_eq!(report.removed, 1);
        assert!(!old.exists());
        assert!(!alias.exists());
        assert!(!fingerprint.exists());
        assert!(new.exists());
        assert_eq!(report.after, Usage::read(&fixture.0).unwrap().bytes);
        assert!(report.after < before);
    }

    #[test]
    fn oldest_groups_are_ordered_across_nested_targets_and_profiles() {
        let fixture = Fixture::new();
        let old = fixture.file(
            "android/aarch64-linux-android/release/deps/libold-1111111111111111.rlib",
            1,
        );
        fs::write(
            fixture
                .0
                .join("android/aarch64-linux-android/release/.cargo-lock"),
            [],
        )
        .unwrap();
        let new = fixture.file("debug/deps/libnew-2222222222222222.rlib", 20);
        let before = Usage::read(&fixture.0).unwrap().bytes;
        let report = enforce(&fixture.0, before - 1, false).unwrap();
        assert_eq!(report.removed, 1);
        assert!(!old.exists());
        assert!(new.exists());
    }

    #[test]
    fn concurrent_cleanup_defers_without_inventing_size_measurements() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        let _guard = lock(&fixture.0.join(".lince-cache-prune.lock"), true)
            .unwrap()
            .unwrap();
        let report = enforce(&fixture.0, 1, false).unwrap();
        assert!(!report.measured);
        assert_eq!(report.busy, 1);
        assert!(old.exists());
    }

    #[test]
    fn incremental_variants_are_evicted_whole_by_latest_content_timestamp() {
        let fixture = Fixture::new();
        let old = fixture.file(
            "debug/incremental/old-variant/s-1-session-working/query-cache.bin",
            1,
        );
        fixture.age(fixture.0.join("debug/incremental/old-variant").as_path(), 1);
        let new = fixture.file(
            "debug/incremental/new-variant/s-2-session-working/query-cache.bin",
            20,
        );
        let before = Usage::read(&fixture.0).unwrap().bytes;
        let report = enforce(&fixture.0, before - 1, false).unwrap();
        assert_eq!(report.removed, 1);
        assert!(!old.parent().unwrap().parent().unwrap().exists());
        assert!(new.exists());
    }

    #[test]
    fn active_cargo_profile_is_deferred_and_its_lock_file_is_preserved() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        let guard = lock(&fixture.0.join("debug/.cargo-lock"), false)
            .unwrap()
            .unwrap();
        let report = enforce(&fixture.0, 1, false).unwrap();
        assert_eq!(report.removed, 0);
        assert_eq!(report.busy, 1);
        assert!(old.exists());
        drop(guard);
        let report = enforce(&fixture.0, 1, false).unwrap();
        assert_eq!(report.removed, 1);
        assert!(fixture.0.join("debug/.cargo-lock").exists());
    }

    #[test]
    fn hardlinks_across_artifact_groups_are_not_counted_as_freed_twice() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        let second = fixture.0.join("debug/deps/libother-2222222222222222.rlib");
        fs::hard_link(&old, &second).unwrap();
        let before = Usage::read(&fixture.0).unwrap().bytes;
        let report = enforce(&fixture.0, before - 1, false).unwrap();
        assert_eq!(report.removed, 2);
        assert!(!old.exists() && !second.exists());
        assert_eq!(report.after, Usage::read(&fixture.0).unwrap().bytes);
    }

    #[test]
    fn preview_selects_the_same_old_group_without_mutating_any_files() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        let new = fixture.file("debug/deps/libnew-2222222222222222.rlib", 20);
        let before = Usage::read(&fixture.0).unwrap().bytes;
        let report = enforce(&fixture.0, before - 1, true).unwrap();
        assert_eq!(report.removed, 1);
        assert!(old.exists() && new.exists());
        assert_eq!(Usage::read(&fixture.0).unwrap().bytes, before);
        assert!(!fixture.0.join(".lince-cache-prune.lock").exists());
    }

    #[test]
    fn under_budget_and_missing_directories_do_not_evict_or_create_artifacts() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        assert_eq!(enforce(&fixture.0, LIMIT, false).unwrap().removed, 0);
        assert!(old.exists());
        let missing = fixture.0.join("not-created");
        assert_eq!(enforce(&missing, LIMIT, false).unwrap().removed, 0);
        assert!(!missing.exists());
    }

    #[test]
    fn unrelated_receipts_compiler_cache_and_unknown_files_survive_over_budget_cleanup() {
        let fixture = Fixture::new();
        let old = fixture.file("debug/deps/libold-1111111111111111.rlib", 1);
        let receipts = fixture.file("park-validation/report.json", 1);
        let cache = fixture.file("sccache/a/b/compiler-object", 1);
        let unknown = fixture.file("debug/deps/user-notes.txt", 1);
        let report = enforce(&fixture.0, 1, false).unwrap();
        assert!(!old.exists());
        for file in [receipts, cache, unknown] {
            assert!(file.exists());
        }
        assert!(report.after > 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_never_extend_cleanup_outside_the_target_tree() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let external = Fixture::new();
        let retained = external.file("debug/deps/liboutside-1111111111111111.rlib", 1);
        symlink(external.0.join("debug"), fixture.0.join("linked-profile")).unwrap();
        symlink(
            &retained,
            fixture.0.join("debug/deps/liblink-2222222222222222.rlib"),
        )
        .unwrap();
        let incremental = fixture.0.join("debug/incremental/old-variant");
        fs::create_dir_all(&incremental).unwrap();
        symlink(&external.0, incremental.join("outside")).unwrap();
        enforce(&fixture.0, 1, false).unwrap();
        assert!(retained.exists());
        assert!(
            fixture
                .0
                .join("debug/deps/liblink-2222222222222222.rlib")
                .symlink_metadata()
                .is_ok()
        );
    }
}
