use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use crate::Result;

const RETIRED: &[&str] = &[
    "downloads",
    "host-pairing",
    "apk-original",
    "apk-startup-fixed",
    "java-check",
    "java-pairing-check",
    "platform-35.zip",
    "lince-phone-smoketest.apk",
    "mobile-smoke-arm64.apk",
];

pub fn run(args: &[OsString]) -> Result<()> {
    let mut apply = false;
    let mut builds = false;
    let mut cache =
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("git/.cache-lince-android"));
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--apply") => apply = true,
            Some("--builds") => builds = true,
            Some("--cache") => {
                cache = Some(PathBuf::from(args.next().ok_or("--cache needs a path")?));
            }
            _ => {
                return Err(
                    "usage: cargo xtask android-clean [--cache PATH] [--builds] [--apply]".into(),
                );
            }
        }
    }
    let cache = cache.ok_or("Set HOME or pass --cache PATH")?;
    let candidates = candidates(&cache, builds)?;
    for path in candidates {
        println!(
            "{} {}",
            if apply { "Removing" } else { "Would remove" },
            path.display()
        );
        if apply {
            remove(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    println!("Keeping SDK, Gradle, emulators, profiles, current APKs, evidence and signing keys.");
    if !apply {
        println!("No files changed. Add --apply to remove the listed disposable files.");
    }
    Ok(())
}

fn candidates(cache: &Path, builds: bool) -> Result<Vec<PathBuf>> {
    if cache
        .file_name()
        .is_none_or(|name| name != ".cache-lince-android")
        || fs::symlink_metadata(cache)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        || !cache.join("sdk/platform-tools").is_dir()
    {
        return Err(
            "Choose the .cache-lince-android directory containing sdk/platform-tools".into(),
        );
    }
    let mut paths: Vec<_> = RETIRED.iter().map(|name| cache.join(name)).collect();
    if builds && !cache.join("target").is_symlink() {
        for target in ["", "aarch64-linux-android", "x86_64-linux-android"] {
            let directory = cache.join("target").join(target);
            if directory.is_symlink() {
                continue;
            }
            for profile in ["debug", "release"] {
                paths.push(directory.join(profile));
            }
        }
    }
    paths.retain(|path| fs::symlink_metadata(path).is_ok());
    Ok(paths)
}

fn remove(path: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_preserves_tools_profiles_and_keys() {
        let root = std::env::temp_dir().join(format!("lince-clean-test-{}", std::process::id()));
        let cache = root.join(".cache-lince-android");
        for name in [
            "sdk/platform-tools",
            "downloads",
            "host-pairing",
            "target/debug",
            "target/android-smoke",
            "preview-data",
            "gradle",
        ] {
            fs::create_dir_all(cache.join(name)).unwrap();
            fs::write(cache.join(name).join("keep-or-remove"), "fixture").unwrap();
        }
        let keys = root.join(".lince-android-signing");
        fs::create_dir_all(&keys).unwrap();
        fs::write(keys.join("app-signing.p12"), "fixture").unwrap();
        assert!(candidates(&root, true).is_err());
        let paths = candidates(&cache, false).unwrap();
        assert!(cache.join("downloads").exists());
        for path in paths {
            remove(&path).unwrap();
        }
        for name in [
            "sdk/platform-tools",
            "target/debug",
            "preview-data",
            "gradle",
        ] {
            assert!(cache.join(name).join("keep-or-remove").exists());
        }
        assert!(keys.join("app-signing.p12").exists());
        assert!(!cache.join("downloads").exists());
        assert_eq!(
            candidates(&cache, true).unwrap(),
            vec![cache.join("target/debug")]
        );
        for path in candidates(&cache, true).unwrap() {
            remove(&path).unwrap();
        }
        assert!(cache.join("target/android-smoke/keep-or-remove").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_removes_symlinks_without_following_them() {
        let root = std::env::temp_dir().join(format!("lince-clean-links-{}", std::process::id()));
        let cache = root.join(".cache-lince-android");
        fs::create_dir_all(cache.join("sdk/platform-tools")).unwrap();
        let outside = root.join("private");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("key"), "fixture").unwrap();
        fs::create_dir_all(outside.join("debug")).unwrap();
        fs::write(outside.join("debug/key"), "fixture").unwrap();
        std::os::unix::fs::symlink(&outside, cache.join("downloads")).unwrap();
        std::os::unix::fs::symlink(&outside, cache.join("target")).unwrap();
        for path in candidates(&cache, true).unwrap() {
            remove(&path).unwrap();
        }
        assert!(outside.join("key").exists());
        assert!(outside.join("debug/key").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
