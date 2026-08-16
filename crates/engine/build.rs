//! Embed `docs/records/*.lingua` into the binary.
//!
//! Lince's own documentation is Records, and `docs/records/` is where they are
//! authored. Both halves that use them — the import Action and the Instinct
//! sand that displays them — read this one embedded copy, so the sand can
//! never show a chapter the import would not create.
//!
//! A build script rather than a list of `include_str!` calls because the list
//! changes every time the documentation is re-split, and a file added to the
//! folder but forgotten in a list is a chapter that silently disappears.

use std::path::Path;

/// The concept every Record in `docs/records/` carries, and the one File Sync
/// SELECTS on.
const MARKER: &str = "instinct";

/// Put `@instinct` on a file that is missing it, and say which.
///
/// **Added rather than merely checked, on purpose.** Being in this folder is
/// what makes a Record part of Lince's documentation, so the marker is
/// derivable and asking a person — or an agent — to remember it is asking them
/// to restate something already true. A missing marker is also the quietest
/// possible failure: the file is not rejected, it just stops being mirrored,
/// and nobody notices until the folder and the store have silently diverged.
///
/// It has to land on DISK and not only in the embedded bundle below, because
/// File Sync selects by reading these files, not by reading the binary.
///
/// Rewriting a source file from a build script is unusual and worth the
/// unusualness here: it is idempotent, it is a data folder rather than code,
/// and the only cost is one extra rebuild the first time — the rewrite touches
/// an mtime cargo is watching, the next run finds nothing to do, and it
/// settles.
fn ensure_marker(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let (prelude, body) = rest.split_at(end);

    let mut lines: Vec<&str> = prelude.lines().collect();
    if lines.iter().any(|line| line.trim() == format!("@{MARKER}")) {
        return None;
    }
    // Under the identity, so the file reads "what it is, then that it is part
    // of the documentation, then the rest". Falls back to the top when there
    // is no identity line, which is a malformed file the parser will reject
    // anyway — better to leave that error legible than to add a second one.
    let at = lines
        .iter()
        .position(|line| line.trim_start().starts_with("@@"))
        .map(|index| index + 1)
        .unwrap_or(lines.len());
    let marker = format!("@{MARKER}");
    lines.insert(at, &marker);

    std::fs::write(path, format!("---\n{}{body}", lines.join("\n"))).ok()?;
    Some(path.file_name()?.to_string_lossy().into_owned())
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/records");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("docs/records must exist ({}): {err}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("lingua"))
        .collect();
    // Sorted, because the order of a directory listing is the filesystem's
    // business and this ends up in a binary that has to be reproducible.
    files.sort();

    let mut fixed = Vec::new();
    for path in &files {
        if let Some(name) = ensure_marker(path) {
            fixed.push(name);
        }
    }
    if !fixed.is_empty() {
        println!(
            "cargo:warning=added @{MARKER} to {} file(s) in docs/records: {}",
            fixed.len(),
            fixed.join(", ")
        );
    }

    let mut out = String::from("pub static BUNDLE: &[(&str, &str)] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let stem = path.file_stem().and_then(|s| s.to_str()).expect("utf-8 filename");
        out.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            stem,
            path.canonicalize().expect("readable").display().to_string()
        ));
    }
    out.push_str("];\n");

    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("bundle.rs");
    std::fs::write(dest, out).expect("write bundle.rs");
}
