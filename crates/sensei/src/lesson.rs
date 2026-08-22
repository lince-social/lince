use crate::rules::{Rule, enforced};
use std::path::{Path, PathBuf};

pub fn teach(manifest_dir: impl AsRef<Path>) {
    let rules = enforced();
    let sources = sources_of(manifest_dir.as_ref());

    let mut lessons: Vec<(&dyn Rule, Vec<String>)> = Vec::new();
    for rule in &rules {
        lessons.push((rule.as_ref(), Vec::new()));
    }

    for path in &sources {
        println!("cargo:rerun-if-changed={}", path.display());
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        for (rule, caught) in lessons.iter_mut() {
            if !rule.wants(path) {
                continue;
            }
            for finding in rule.inspect(&source) {
                caught.push(format!(
                    "  {}:{} {}",
                    path.display(),
                    finding.line,
                    finding.complaint
                ));
            }
        }
    }

    let mut spoken = Vec::new();
    for (rule, caught) in &lessons {
        if caught.is_empty() {
            continue;
        }
        spoken.push(format!(
            "{}\n{}\n{}",
            rule.name(),
            caught.join("\n"),
            rule.remedy()
        ));
    }

    if !spoken.is_empty() {
        panic!("\n{}\n", spoken.join("\n\n"));
    }
}

fn sources_of(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for folder in ["src", "tests", "benches", "examples"] {
        let dir = root.join(folder);
        if dir.is_dir() {
            println!("cargo:rerun-if-changed={}", dir.display());
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
