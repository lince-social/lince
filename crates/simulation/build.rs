// non-lean-yet
// Scope: fn files, fn main.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   6, 7, 30, 36, 60, 65, 90.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (94).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn files(root: &Path, output: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .expect("source directory")
        .map(|entry| entry.expect("source entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("target" | ".git" | "node_modules")
            ) {
                files(&path, output);
            }
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("rs" | "toml" | "sql" | "lingua" | "lock")
        ) {
            output.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("Cargo sets the simulation manifest directory"),
    );
    sensei::teach(&manifest);
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("the simulation crate lives under the repository's crates directory");
    let mut sources = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
    for name in [
        "simulation",
        "cell",
        "engine",
        "nucleus",
        "store",
        "protein",
        "utils",
        "transport",
        "lingua",
    ] {
        let directory = root.join("crates").join(name);
        println!("cargo:rerun-if-changed={}", directory.display());
        files(&directory, &mut sources);
    }
    sources.sort();
    let mut hash = Sha256::new();
    hash.update(b"lince.simulation.build.v1\0");
    for path in sources {
        println!("cargo:rerun-if-changed={}", path.display());
        hash.update(
            path.strip_prefix(root)
                .expect("every hashed simulation input is under the repository root")
                .to_string_lossy()
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(std::fs::read(path).expect("read tested source"));
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
    let rustc = std::process::Command::new(
        std::env::var_os("RUSTC").expect("Cargo supplies the compiler for the simulation identity"),
    )
    .arg("-vV")
    .output()
    .expect("compiler version");
    assert!(
        rustc.status.success() && !rustc.stdout.is_empty(),
        "compiler identity unavailable"
    );
    hash.update(rustc.stdout);
    let encoded: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo:rustc-env=LINCE_SIMULATION_BUILD_HASH=sha256:{encoded}");
}
