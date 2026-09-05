use crate::rules::{Rule, enforced};
use std::path::{Path, PathBuf};

const SHOWN_PER_RULE: usize = 40;

pub fn teach(manifest_dir: impl AsRef<Path>) {
    let sources = sources_of(manifest_dir.as_ref());
    for path in &sources {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    if let Some(complaint) = examine(&sources) {
        panic!("\n{complaint}\n");
    }
}

pub fn teach_workspace(root: impl AsRef<Path>) {
    let root = root.as_ref();
    let sources = workspace_sources(root);
    for path in &sources {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    if let Some(complaint) = examine(&sources) {
        panic!("\n{complaint}\n");
    }
}

pub fn examine(sources: &[PathBuf]) -> Option<String> {
    let rules = enforced();
    let mut lessons: Vec<(&dyn Rule, Vec<String>, usize)> = Vec::new();
    for rule in &rules {
        lessons.push((rule.as_ref(), Vec::new(), 0));
    }

    for path in sources {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        for (rule, caught, total) in lessons.iter_mut() {
            if !rule.wants(path) {
                continue;
            }
            for finding in rule.inspect(&source) {
                *total += 1;
                if caught.len() < SHOWN_PER_RULE {
                    caught.push(format!(
                        "  {}:{} {}",
                        path.display(),
                        finding.line,
                        finding.complaint
                    ));
                }
            }
        }
    }

    let mut spoken = Vec::new();
    for (rule, caught, total) in &lessons {
        if caught.is_empty() {
            continue;
        }
        let mut body = caught.join("\n");
        if *total > caught.len() {
            body.push_str(&format!(
                "\n  ...and {} more, {total} in all",
                total - caught.len()
            ));
        }
        spoken.push(format!("{}\n{}\n{}", rule.name(), body, rule.remedy()));
    }

    (!spoken.is_empty()).then(|| spoken.join("\n\n"))
}

pub struct Mended {
    pub path: PathBuf,
    pub rule: &'static str,
}

pub fn mend(sources: &[PathBuf]) -> std::io::Result<Vec<Mended>> {
    let rules = enforced();
    let mut mended = Vec::new();
    for path in sources {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut current = source.clone();
        let mut by = None;
        for rule in &rules {
            if !rule.wants(path) {
                continue;
            }
            if let Some(next) = rule.fix(&current) {
                current = next;
                by = Some(rule.name());
            }
        }
        if current != source {
            std::fs::write(path, &current)?;
            mended.push(Mended {
                path: path.clone(),
                rule: by.unwrap_or("a lesson"),
            });
        }
    }
    Ok(mended)
}

pub fn workspace_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let crates = root.join("crates");
    let Ok(entries) = std::fs::read_dir(&crates) else {
        return sources;
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    for folder in folders {
        sources.extend(sources_of(&folder));
    }
    sources
}

pub fn workspace_root(manifest_dir: &Path) -> PathBuf {
    let mut at = manifest_dir;
    loop {
        if at.join("Cargo.toml").is_file() && at.join("crates").is_dir() {
            return at.to_path_buf();
        }
        match at.parent() {
            Some(parent) => at = parent,
            None => return manifest_dir.to_path_buf(),
        }
    }
}

fn sources_of(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for folder in ["src", "tests", "benches", "examples"] {
        let dir = root.join(folder);
        if dir.is_dir() {
            collect(&dir, &mut sources);
        }
    }
    let build = root.join("build.rs");
    if build.is_file() {
        sources.push(build);
    }
    sources.sort();
    sources
}

fn collect(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.is_file() {
            into.push(path);
        }
    }
}
