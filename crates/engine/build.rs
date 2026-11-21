// non-lean-yet
// Scope: fn files, fn main.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   5, 6, 25, 31, 45, 50, 75.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (79).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn files(root: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|value| value.to_str()),
                Some("target" | ".git")
            ) {
                files(&path, output);
            }
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("rs" | "toml" | "sql" | "json")
        ) {
            output.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the Engine manifest directory"),
    );
    sensei::teach(&manifest);
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("the Engine crate lives under the repository's crates directory");
    let mut paths = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
    for name in ["nucleus", "store", "engine", "protein", "lingua", "utils"] {
        let directory = root.join("crates").join(name);
        println!("cargo:rerun-if-changed={}", directory.display());
        files(&directory, &mut paths);
    }
    paths.sort();
    let mut hash = Sha256::new();
    hash.update(b"lince.execution.build.v1\0");
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        hash.update(
            path.strip_prefix(root)
                .expect("every hashed Engine input is under the repository root")
                .to_string_lossy()
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(std::fs::read(path).expect("tested source"));
        hash.update([0]);
    }
    for key in [
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        hash.update(std::env::var(key).unwrap_or_default());
        hash.update([0]);
    }
    let mut features: Vec<_> = std::env::vars()
        .filter(|(key, _)| key.starts_with("CARGO_FEATURE_"))
        .collect();
    features.sort();
    for (key, value) in features {
        hash.update(key);
        hash.update([0]);
        hash.update(value);
        hash.update([0]);
    }
    let version = std::process::Command::new(
        std::env::var_os("RUSTC").expect("Cargo supplies the compiler for the Engine identity"),
    )
    .arg("-vV")
    .output()
    .expect("compiler version");
    assert!(
        version.status.success() && !version.stdout.is_empty(),
        "compiler identity unavailable"
    );
    hash.update(version.stdout);
    let encoded: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo:rustc-env=LINCE_EXECUTION_BUILD_HASH=sha256:{encoded}");
}
