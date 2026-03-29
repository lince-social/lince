#![forbid(unsafe_code)]
#![deny(warnings)]

use {
    semver::Version,
    serde::Serialize,
    std::{
        env, fs,
        io::Error,
        path::{Path, PathBuf},
    },
    web::{LincePackage, sand},
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
    println!("  LINCE_SAND_EXPORT_DIR  Override the dna lounge directory");
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
    let output_dir = resolve_arg_path(args, "--output-dir")
        .or_else(|| env::var_os("LINCE_SAND_EXPORT_DIR").map(PathBuf::from))
        .unwrap_or_else(|| sibling_dna_root().join("lounge"));
    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let dna_root = output_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| sibling_dna_root());

    if !dry_run {
        fs::create_dir_all(&output_dir)?;
    }

    for package in sand::official_packages() {
        let export = build_export(&dna_root, package)?;
        if dry_run {
            println!(
                "would export {} {} to {}",
                export.sand.name,
                export.sand.version,
                output_dir.join(&export.sand.name).display()
            );
            continue;
        }

        let bundle_dir = output_dir.join(&export.sand.name);
        if bundle_dir.exists() {
            fs::remove_dir_all(&bundle_dir)?;
        }
        fs::create_dir_all(&bundle_dir)?;
        fs::write(bundle_dir.join("index.html"), export.html)?;
        fs::write(
            bundle_dir.join("sand.toml"),
            toml::to_string_pretty(&export.sand).map_err(Error::other)?,
        )?;
    }

    Ok(())
}

fn build_export(dna_root: &Path, package: LincePackage) -> Result<ExportBundle, Error> {
    let html = strip_manifest_script(&package.html_document());
    let name = package_name_from_filename(&package.archive_filename());
    let version = exported_version(dna_root, &name, &package.manifest.version)?;
    let sand = SandToml {
        name,
        channel: Channel::Official,
        version,
        author: package.manifest.author,
        description: package.manifest.description,
        details: Some(package.manifest.details),
        initial_width: Some(package.manifest.initial_width),
        initial_height: Some(package.manifest.initial_height),
        permissions: package.manifest.permissions,
        tags: Vec::new(),
        class: None,
        title: Some(package.manifest.title),
        icon: Some(package.manifest.icon),
        required_permissions: None,
        required_host_meta: None,
        host_contract_version: None,
        min_lince_version: None,
    };

    Ok(ExportBundle { sand, html })
}

fn exported_version(dna_root: &Path, name: &str, current: &str) -> Result<String, Error> {
    let current_version = Version::parse(current).map_err(|error| {
        Error::other(format!(
            "Official widget {name} has invalid semver version {current:?}: {error}"
        ))
    })?;
    let existing = load_existing_official_version(dna_root, name)?;

    let version = match existing {
        Some(existing) if current_version <= existing => bump_patch(existing),
        _ => current_version,
    };

    Ok(version.to_string())
}

fn load_existing_official_version(dna_root: &Path, name: &str) -> Result<Option<Version>, Error> {
    let sand_toml_path = canonical_sand_toml_path(dna_root, Channel::Official, name);
    if !sand_toml_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&sand_toml_path)?;
    let sand: SandToml = toml::from_str(&raw).map_err(Error::other)?;
    let version = Version::parse(&sand.version).map_err(|error| {
        Error::other(format!(
            "Canonical dna widget {} has invalid semver version {:?}: {error}",
            sand.name, sand.version
        ))
    })?;
    Ok(Some(version))
}

fn canonical_sand_toml_path(dna_root: &Path, channel: Channel, name: &str) -> PathBuf {
    let prefix = package_prefix(name);
    dna_root
        .join("sand")
        .join(channel.as_str())
        .join(prefix)
        .join(name)
        .join("sand.toml")
}

fn package_name_from_filename(filename: &str) -> String {
    let stem = filename
        .strip_suffix(".html")
        .unwrap_or(filename)
        .trim()
        .to_ascii_lowercase();
    let mut out = String::new();
    let mut previous_was_separator = false;

    for ch in stem.chars() {
        let normalized = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            '_' | '-' | ' ' => Some('_'),
            _ => None,
        };

        let Some(normalized) = normalized else {
            continue;
        };

        if normalized == '_' {
            if out.is_empty() || previous_was_separator {
                continue;
            }
            previous_was_separator = true;
        } else {
            previous_was_separator = false;
        }

        out.push(normalized);
    }

    let out = out.trim_matches('_');
    if out.is_empty() {
        "widget".to_string()
    } else {
        out.to_string()
    }
}

fn package_prefix(name: &str) -> String {
    name.chars().take(2).collect::<String>()
}

fn sibling_dna_root() -> PathBuf {
    repo_root_dir()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("dna")
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

fn bump_patch(mut version: Version) -> Version {
    version.patch += 1;
    version.pre = semver::Prerelease::EMPTY;
    version.build = semver::BuildMetadata::EMPTY;
    version
}

fn strip_manifest_script(html: &str) -> String {
    let lowercase = html.to_ascii_lowercase();
    let mut offset = 0;

    while let Some(relative_start) = lowercase[offset..].find("<script") {
        let script_start = offset + relative_start;
        let Some(relative_tag_end) = lowercase[script_start..].find('>') else {
            break;
        };
        let tag_end = script_start + relative_tag_end;
        let open_tag = &lowercase[script_start..=tag_end];
        let has_manifest_id =
            open_tag.contains("id=\"lince-manifest\"") || open_tag.contains("id='lince-manifest'");

        if !has_manifest_id {
            offset = tag_end + 1;
            continue;
        }

        let Some(relative_close_start) = lowercase[tag_end + 1..].find("</script>") else {
            break;
        };
        let close_end = tag_end + 1 + relative_close_start + "</script>".len();
        let mut stripped = String::with_capacity(html.len());
        stripped.push_str(&html[..script_start]);
        stripped.push_str(&html[close_end..]);
        return stripped.trim().to_string();
    }

    html.trim().to_string()
}

#[derive(Debug)]
struct ExportBundle {
    sand: SandToml,
    html: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Channel {
    Official,
    Community,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Community => "community",
        }
    }
}

#[derive(Debug, Serialize)]
struct SandToml {
    name: String,
    channel: Channel,
    version: String,
    author: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_width: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_height: Option<u8>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    permissions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_host_meta: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_contract_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_lince_version: Option<String>,
}
