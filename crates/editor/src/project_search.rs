use crate::{
    Result,
    files::Scope,
    search::{Options, Pattern},
};
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Match {
    pub root: PathBuf,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

pub struct Matches {
    pub rows: Vec<Match>,
    pub limited: bool,
}

pub fn search(
    roots: Vec<PathBuf>,
    needle: &str,
    options: Options,
    ignored: bool,
    cancel: &AtomicBool,
) -> Result<Matches> {
    if needle.contains(['\r', '\n']) {
        return Err("Project search accepts one line of text".into());
    }
    let pattern = Pattern::new(needle, options)?;
    let mut roots = roots
        .into_iter()
        .map(|root| Scope::open(&root))
        .collect::<Result<Vec<_>>>()?;
    roots.sort_by(|a, b| a.path.cmp(&b.path));
    roots.dedup_by(|a, b| a.path == b.path);
    let mut previous = None::<PathBuf>;
    roots.retain(|scope| {
        if previous
            .as_ref()
            .is_some_and(|path| scope.path.starts_with(path))
        {
            false
        } else {
            previous = Some(scope.path.clone());
            true
        }
    });
    let mut output = Matches {
        rows: Vec::new(),
        limited: false,
    };
    let mut visited = 0;
    let mut bytes = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    for scope in roots {
        let mut walk = ignore::WalkBuilder::new(&scope.path);
        walk.hidden(false)
            .follow_links(false)
            .git_ignore(!ignored)
            .git_global(!ignored)
            .git_exclude(!ignored)
            .require_git(false)
            .max_depth(Some(64));
        walk.filter_entry(move |entry| {
            ignored || (entry.file_name() != ".git" && entry.file_name() != ".lince-trash")
        });
        for entry in walk.build() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Search cancelled".into());
            }
            visited += 1;
            if visited > 100_000 || bytes >= 64 * 1024 * 1024 || Instant::now() >= deadline {
                output.limited = true;
                return Ok(output);
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    output.limited = true;
                    continue;
                }
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let file = match scope.bind(entry.path()).and_then(|file| file.reader()) {
                Ok(file) => file,
                Err(_) => {
                    output.limited = true;
                    continue;
                }
            };
            let mut reader = BufReader::new(file);
            let first_match = output.rows.len();
            let mut line = Vec::new();
            let mut number = 0;
            let mut long = false;
            let mut file_bytes = 0;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Search cancelled".into());
                }
                if bytes >= 64 * 1024 * 1024 || Instant::now() >= deadline {
                    output.limited = true;
                    return Ok(output);
                }
                if file_bytes >= crate::MAX_FILE_BYTES {
                    output.limited = true;
                    break;
                }
                let available = match reader.fill_buf() {
                    Ok(buffer) => buffer,
                    Err(_) => {
                        output.limited = true;
                        break;
                    }
                };
                if available.contains(&0) {
                    output.rows.truncate(first_match);
                    break;
                }
                let count = available
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(available.len(), |n| n + 1);
                let finished = count == 0 || available.get(count - 1) == Some(&b'\n');
                if line.len() + count > crate::MAX_WINDOW_BYTES {
                    long = true;
                    output.limited = true;
                }
                if !long {
                    line.extend_from_slice(&available[..count]);
                }
                reader.consume(count);
                bytes += count;
                file_bytes += count;
                if finished {
                    if !long {
                        let text = match std::str::from_utf8(&line) {
                            Ok(text) => text,
                            Err(_) => {
                                output.rows.truncate(first_match);
                                break;
                            }
                        };
                        if let Some(found) = pattern.first(text) {
                            let column = text[..found.start].chars().count();
                            output.rows.push(Match {
                                root: scope.path.clone(),
                                path: entry.path().to_path_buf(),
                                line: number,
                                column,
                                text: text
                                    .chars()
                                    .skip(column.saturating_sub(30))
                                    .take(150)
                                    .collect::<String>()
                                    .trim_end()
                                    .into(),
                            });
                            if output.rows.len() == 200 {
                                output.limited = true;
                                return Ok(output);
                            }
                        }
                    }
                    line.clear();
                    long = false;
                    number += 1;
                }
                if count == 0 {
                    break;
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_unopened_directories_and_respects_ignored_and_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir(root.join("nested")).unwrap();
        std::fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(root.join("nested/visible.txt"), "first\n猫 NEEDLE\n").unwrap();
        std::fs::write(root.join("ignored.txt"), "needle").unwrap();
        std::fs::write(root.join("binary.bin"), b"needle\0").unwrap();
        let mut late_binary = "needle\n".to_string();
        late_binary.push_str(&"x".repeat(10_000));
        late_binary.push('\0');
        std::fs::write(root.join("late-binary.bin"), late_binary).unwrap();
        let options = Options {
            case_sensitive: false,
            whole_word: true,
        };
        let result = search(
            vec![root.clone(), root.join("nested")],
            "needle",
            options,
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!((result.rows[0].line, result.rows[0].column), (1, 2));
        assert!(!result.limited);
        let result = search(vec![root], "needle", options, true, &AtomicBool::new(false)).unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn long_lines_and_match_limits_are_reported_and_cancellation_stops_the_search() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("long.txt"),
            format!("{}\nneedle\n", "x".repeat(crate::MAX_WINDOW_BYTES + 1)),
        )
        .unwrap();
        let result = search(
            vec![dir.path().into()],
            "needle",
            Options::default(),
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(result.limited);
        assert_eq!(result.rows[0].line, 1);
        std::fs::write(dir.path().join("many.txt"), "needle\n".repeat(300)).unwrap();
        let result = search(
            vec![dir.path().into()],
            "needle",
            Options::default(),
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(result.limited);
        assert_eq!(result.rows.len(), 200);
        assert!(
            search(
                vec![dir.path().into()],
                "needle",
                Options::default(),
                false,
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }
}
