use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{ErrorKind, Read},
    path::Path,
    process::Stdio,
    time::Duration,
};

use cap_std::fs::Dir;
use similar::TextDiff;
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

const DIFF_BYTES: usize = 12 * 1024;
const FILE_BYTES: usize = 128 * 1024;
const FILES: usize = 1024;
const SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const SNAPSHOT_TIME: Duration = Duration::from_secs(3);

#[derive(Default)]
struct Patch(String);

impl fmt::Write for Patch {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let remaining = DIFF_BYTES.saturating_sub(self.0.len());
        if text.len() > remaining {
            self.0
                .push_str(&text[..text.floor_char_boundary(remaining)]);
            return Err(fmt::Error);
        }
        self.0.push_str(text);
        Ok(())
    }
}

/// Returns a bounded unified patch. An absent side represents a missing file.
#[must_use]
pub fn file_diff(path: &str, before: Option<&str>, after: Option<&str>) -> Option<String> {
    if before == after {
        return None;
    }
    let old = before.map_or_else(|| "/dev/null".into(), |_| format!("a/{path}"));
    let new = after.map_or_else(|| "/dev/null".into(), |_| format!("b/{path}"));
    let mut patch = Patch::default();
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(50))
        .diff_lines(before.unwrap_or_default(), after.unwrap_or_default());
    let result = fmt::write(
        &mut patch,
        format_args!("{}", diff.unified_diff().header(&old, &new)),
    );
    if result.is_err() {
        patch.0.push_str("\n[Diff truncated]\n");
    } else if patch.0.is_empty() {
        patch.0 = format!("--- {old}\n+++ {new}\n");
    }
    Some(patch.0)
}

pub(super) struct GitSnapshot {
    head: String,
    dirty: BTreeMap<String, FileState>,
}

#[derive(PartialEq)]
enum FileState {
    Missing,
    Text(String),
    Unsupported,
}

impl FileState {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

impl GitSnapshot {
    pub(super) async fn capture(directory: &Path) -> Option<Self> {
        timeout(SNAPSHOT_TIME, async {
            let head =
                String::from_utf8(git(directory, &["rev-parse", "--verify", "HEAD"], 100).await??)
                    .ok()?;
            let head = head.trim().to_owned();
            if head.len() < 40 || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let paths = dirty_paths(directory, &head).await?;
            let dir = Dir::open_ambient_dir(directory, cap_std::ambient_authority()).ok()?;
            let mut retained = 0;
            let dirty = paths
                .into_iter()
                .map(|path| {
                    let mut state = read_file(&dir, &path);
                    if let FileState::Text(text) = &state {
                        if retained + text.len() > SNAPSHOT_BYTES {
                            state = FileState::Unsupported;
                        } else {
                            retained += text.len();
                        }
                    }
                    (path, state)
                })
                .collect();
            Some(Self { head, dirty })
        })
        .await
        .ok()
        .flatten()
    }

    pub(super) async fn changes(self, directory: &Path) -> Option<String> {
        timeout(SNAPSHOT_TIME, async {
            let mut paths = dirty_paths(directory, &self.head).await?;
            paths.extend(self.dirty.keys().cloned());
            let dir = Dir::open_ambient_dir(directory, cap_std::ambient_authority()).ok()?;
            let mut patch = Patch::default();
            for path in paths {
                let before = if let Some(state) = self.dirty.get(&path) {
                    state
                } else {
                    let revision = format!("{}:./{path}", self.head);
                    let state = match git(directory, &["show", &revision], FILE_BYTES).await {
                        Some(Some(bytes)) => text_state(bytes),
                        Some(None) => FileState::Missing,
                        None => FileState::Unsupported,
                    };
                    if append_change(&mut patch, &path, &state, &read_file(&dir, &path)).is_err() {
                        break;
                    }
                    continue;
                };
                if append_change(&mut patch, &path, before, &read_file(&dir, &path)).is_err() {
                    break;
                }
            }
            (!patch.0.is_empty()).then(|| {
                format!(
                    "Working tree changes observed during this command\n\n{}",
                    patch.0
                )
            })
        })
        .await
        .ok()
        .flatten()
    }
}

fn append_change(
    patch: &mut Patch,
    path: &str,
    before: &FileState,
    after: &FileState,
) -> fmt::Result {
    use fmt::Write as _;
    if before == after {
        return Ok(());
    }
    let result = if *before == FileState::Unsupported || *after == FileState::Unsupported {
        writeln!(patch, "{path}: text diff unavailable")
    } else if let Some(change) = file_diff(path, before.text(), after.text()) {
        patch.write_str(&change)
    } else {
        Ok(())
    };
    if result.is_err() {
        patch.0.push_str("\n[Diff truncated]\n");
    }
    result
}

async fn dirty_paths(directory: &Path, head: &str) -> Option<BTreeSet<String>> {
    let tracked = git(
        directory,
        &[
            "diff",
            "--name-only",
            "--no-ext-diff",
            "--no-textconv",
            "--relative",
            "--no-renames",
            "-z",
            head,
            "--",
            ".",
        ],
        256 * 1024,
    )
    .await??;
    let untracked = git(
        directory,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            ".",
        ],
        256 * 1024,
    )
    .await??;
    let paths: BTreeSet<_> = tracked
        .split(|&b| b == 0)
        .chain(untracked.split(|&b| b == 0))
        .filter(|path| !path.is_empty())
        .filter_map(|path| std::str::from_utf8(path).ok().map(str::to_owned))
        .collect();
    (paths.len() <= FILES).then_some(paths)
}

fn read_file(directory: &Dir, path: &str) -> FileState {
    match directory.symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= FILE_BYTES as u64 => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return FileState::Missing,
        _ => return FileState::Unsupported,
    }
    let mut bytes = Vec::new();
    match directory
        .open(path)
        .and_then(|file| file.take((FILE_BYTES + 1) as u64).read_to_end(&mut bytes))
    {
        Ok(_) if bytes.len() <= FILE_BYTES => text_state(bytes),
        _ => FileState::Unsupported,
    }
}

fn text_state(bytes: Vec<u8>) -> FileState {
    if bytes.contains(&0) {
        return FileState::Unsupported;
    }
    String::from_utf8(bytes).map_or(FileState::Unsupported, FileState::Text)
}

async fn git(directory: &Path, args: &[&str], limit: usize) -> Option<Option<Vec<u8>>> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut bytes = Vec::new();
    child
        .stdout
        .take()?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .ok()?;
    if bytes.len() > limit {
        return None;
    }
    Some(child.wait().await.ok()?.success().then_some(bytes))
}
