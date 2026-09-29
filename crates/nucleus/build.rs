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
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    sensei::teach(&manifest);
    let root = manifest.parent().unwrap().parent().unwrap();
    let mut paths = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
    for name in ["nucleus", "store", "engine", "protein", "anicca", "utils"] {
        files(&root.join("crates").join(name), &mut paths);
    }
    paths.sort();
    let mut hash = Sha256::new();
    hash.update(b"lince.execution.build.v1\0");
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        hash.update(
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(std::fs::read(path).expect("tested source"));
        hash.update([0]);
    }
    for key in ["TARGET", "PROFILE", "CARGO_ENCODED_RUSTFLAGS"] {
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
    let version = std::process::Command::new(std::env::var_os("RUSTC").unwrap())
        .arg("--version")
        .output()
        .expect("compiler version");
    hash.update(version.stdout);
    let encoded: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo:rustc-env=LINCE_EXECUTION_BUILD_HASH=sha256:{encoded}");
}
