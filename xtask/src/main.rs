#![forbid(unsafe_code)]
#![deny(warnings)]

use {
    serde::Serialize,
    std::{
        env,
        fs,
        io::Error,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    },
    web::{LincePackage, sand, slugify},
};

fn main() -> Result<(), Error> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    match args.first().map(String::as_str) {
        Some("sand") => sand_command(&args[1..]),
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!("Usage:");
    println!("  cargo run -p xtask -- sand export [--output-dir PATH]");
    println!();
    println!("Environment:");
    println!("  LINCE_SAND_EXPORT_DIR  Override the handoff directory");
}

fn sand_command(args: &[String]) -> Result<(), Error> {
    match args.first().map(String::as_str) {
        Some("export") => export_sand(&args[1..]),
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn export_sand(args: &[String]) -> Result<(), Error> {
    let staging_dir = resolve_arg_path(args, "--output-dir")
        .or_else(|| env::var_os("LINCE_SAND_EXPORT_DIR").map(PathBuf::from))
        .unwrap_or_else(|| {
            repo_root_dir()
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("dna")
                .join(".lounge")
                .join("incoming")
        });

    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let packages = sand::official_packages();
    let bundles = packages
        .into_iter()
        .map(build_bundle)
        .collect::<Result<Vec<_>, _>>()?;

    if !dry_run {
        fs::create_dir_all(&staging_dir)?;
    }

    let source_commit = git_commit_hash(repo_root_dir().as_path()).unwrap_or_else(|_| "unknown".to_string());
    let manifest = HandoffManifest {
        schema: 1,
        source_commit,
        generated_at_unix: now_unix(),
        artifacts: bundles.iter().map(|bundle| bundle.summary.clone()).collect(),
    };

    for bundle in bundles {
        if dry_run {
            println!(
                "would export {} as {}",
                bundle.summary.package_id, bundle.summary.filename
            );
            continue;
        }

        let bundle_dir = staging_dir.join(&bundle.summary.package_id);
        fs::create_dir_all(&bundle_dir)?;
        fs::write(bundle_dir.join("index.html"), &bundle.html)?;
        fs::write(
            bundle_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&bundle.package.manifest).map_err(Error::other)?,
        )?;
        fs::write(
            bundle_dir.join("source.json"),
            serde_json::to_vec_pretty(&bundle.summary).map_err(Error::other)?,
        )?;
    }

    if !dry_run {
        fs::write(
            staging_dir.join("index.json"),
            serde_json::to_vec_pretty(&manifest).map_err(Error::other)?,
        )?;
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct HandoffManifest {
    schema: u8,
    source_commit: String,
    generated_at_unix: u64,
    artifacts: Vec<ArtifactSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct ArtifactSummary {
    package_id: String,
    filename: String,
    bytes: usize,
    title: String,
    version: String,
    permissions: Vec<String>,
}

#[derive(Debug)]
struct Bundle {
    package: LincePackage,
    html: Vec<u8>,
    summary: ArtifactSummary,
}

fn build_bundle(package: LincePackage) -> Result<Bundle, Error> {
    let html = package.html_document().into_bytes();
    let package_id = slugify(&package.manifest.title);
    let summary = ArtifactSummary {
        package_id,
        filename: package.archive_filename(),
        bytes: html.len(),
        title: package.manifest.title.clone(),
        version: package.manifest.version.clone(),
        permissions: package.manifest.permissions.clone(),
    };

    Ok(Bundle {
        package,
        html,
        summary,
    })
}

fn resolve_arg_path(args: &[String], name: &str) -> Option<PathBuf> {
    args.windows(2)
        .find_map(|window| (window[0] == name).then(|| PathBuf::from(&window[1])))
}

fn repo_root_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn git_commit_hash(repo_root: &Path) -> Result<String, Error> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()?;

    if !output.status.success() {
        return Err(Error::other("Failed to resolve git commit hash"));
    }

    let hash = String::from_utf8(output.stdout).map_err(Error::other)?;
    Ok(hash.trim().to_string())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
