use crate::{git_dirty, git_revision, source_fingerprint, unix_millis};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const CRITICAL_PACKAGES: &[&str] = &[
    "accesskit",
    "avian2d",
    "bevy",
    "bevy_render",
    "naga",
    "raw-window-handle",
    "wgpu",
    "wgpu-core",
    "wgpu-hal",
    "wgpu-types",
    "winit",
];

const SOURCE_SURFACE_PACKAGES: &[&str] = &["avian2d", "bevy_render"];

const PLATFORM_MARKERS: &[(&str, &str)] = &[
    ("android", "android"),
    ("ios", "ios"),
    ("linux", "linux"),
    ("macos", "macos"),
    ("wasm32", "wasm32"),
    ("windows", "windows"),
];

#[derive(Clone, Debug, Serialize)]
pub struct DependencyAudit {
    pub schema_version: u32,
    pub gate: String,
    pub status: String,
    pub created_unix_millis: u128,
    pub git_revision: String,
    pub git_dirty: bool,
    pub source_fingerprint_sha256: String,
    pub target: String,
    pub profiles: Vec<DependencyProfile>,
    pub source_surfaces: Vec<SourceSurface>,
    pub assertions: Vec<AuditAssertion>,
    pub source_findings: Vec<SourceFinding>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DependencyProfile {
    pub name: String,
    pub enabled_features: Vec<String>,
    pub package_count: usize,
    pub git_source_families: Vec<String>,
    pub critical_packages: Vec<ResolvedPackage>,
    pub duplicate_critical_packages: Vec<PackageDuplicate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub source: String,
    pub license: String,
    pub activated_features: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageDuplicate {
    pub name: String,
    pub versions: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditAssertion {
    pub name: String,
    pub state: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceFinding {
    pub subject: String,
    pub state: String,
    pub detail: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceSurface {
    pub name: String,
    pub version: String,
    pub source: String,
    pub owner: String,
    pub retention_boundary: String,
    pub declared_license: String,
    pub license_files: Vec<String>,
    pub rust_file_count: usize,
    pub lexical_unsafe_token_count: usize,
    pub rust_files_with_unsafe_tokens: usize,
    pub platform_markers: Vec<String>,
    pub state: String,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: Option<CargoResolve>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    license: Option<String>,
    manifest_path: String,
}

#[derive(Debug, Deserialize)]
struct CargoResolve {
    root: Option<String>,
    nodes: Vec<CargoNode>,
}

#[derive(Debug, Deserialize)]
struct CargoNode {
    id: String,
    dependencies: Vec<String>,
    features: Vec<String>,
}

impl DependencyAudit {
    pub fn collect() -> Result<Self, String> {
        let target = host_target();
        let specifications = [
            ("base", Vec::new()),
            ("bevy", vec!["bevy-preflight"]),
            ("physics", vec!["physics-preflight"]),
        ];
        let profiles = specifications
            .into_iter()
            .map(|(name, features)| collect_profile(name, features, &target))
            .collect::<Result<Vec<_>, _>>()?;
        let source_surfaces = collect_source_surfaces(&target)?;
        let assertions = audit_assertions(&profiles, &source_surfaces);
        let status = if assertions.iter().any(|assertion| assertion.state == "fail") {
            "failed"
        } else {
            "passed"
        };

        Ok(Self {
            schema_version: 2,
            gate: "dependency and ownership preflight".into(),
            status: status.into(),
            created_unix_millis: unix_millis(),
            git_revision: git_revision(),
            git_dirty: git_dirty(),
            source_fingerprint_sha256: source_fingerprint(),
            target,
            profiles,
            source_surfaces,
            assertions,
            source_findings: source_findings(),
            limitations: vec![
                "Metadata proves source and feature resolution, not runtime interoperability.".into(),
                "Lexical unsafe-token counts locate review surfaces; they are not a semantic Rust unsafe-code audit.".into(),
                "Platform-marker counts show source branches, not whether every branch compiles or runs on its named platform.".into(),
                "Bevy manual rendering, shared-device construction and frame ordering are not exercised by this audit.".into(),
            ],
        })
    }

    pub fn write_default(&self) -> Result<PathBuf, String> {
        let path = PathBuf::from("target/interface/reports")
            .join(format!("dependencies-{}.json", unix_millis()));
        self.write_pretty(&path)?;
        Ok(path)
    }

    pub fn write_pretty(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }

    pub fn human_summary(&self) -> String {
        let mut lines = vec![
            "LINCE INTERFACE DEPENDENCY AUDIT".to_owned(),
            format!("STATUS  {}", self.status.to_uppercase()),
            format!("TARGET  {}", self.target),
            String::new(),
        ];
        for profile in &self.profiles {
            let gpu = profile
                .critical_packages
                .iter()
                .filter(|package| {
                    matches!(package.name.as_str(), "wgpu" | "wgpu-core" | "wgpu-hal")
                })
                .map(|package| format!("{} {}", package.name, package.version))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{:<16} {:>4} packages  {:>2} git families  {}",
                profile.name.to_uppercase(),
                profile.package_count,
                profile.git_source_families.len(),
                gpu
            ));
        }
        lines.push(String::new());
        for surface in &self.source_surfaces {
            lines.push(format!(
                "SOURCE {:<15} {:>4} Rust files  {:>5} unsafe tokens  {:>2} platform markers  {}",
                surface.name,
                surface.rust_file_count,
                surface.lexical_unsafe_token_count,
                surface.platform_markers.len(),
                surface.state
            ));
        }
        lines.push(String::new());
        for assertion in &self.assertions {
            lines.push(format!(
                "{:<10} {} — {}",
                assertion.state.to_uppercase(),
                assertion.name,
                assertion.detail
            ));
        }
        lines.join("\n")
    }
}

fn cargo_metadata(features: &[&str], target: &str) -> Result<CargoMetadata, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let mut command = Command::new("cargo");
    command.args([
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--no-default-features",
        "--filter-platform",
        target,
        "--manifest-path",
    ]);
    command.arg(&manifest);
    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    serde_json::from_slice::<CargoMetadata>(&output.stdout).map_err(|error| error.to_string())
}

fn collect_profile(
    name: &str,
    features: Vec<&str>,
    target: &str,
) -> Result<DependencyProfile, String> {
    let metadata = cargo_metadata(&features, target)?;
    profile_from_metadata(name, features, metadata)
}

fn collect_source_surfaces(target: &str) -> Result<Vec<SourceSurface>, String> {
    let metadata = cargo_metadata(&["candidate-audit"], target)?;
    let resolve = metadata
        .resolve
        .ok_or_else(|| "cargo metadata returned no dependency resolution".to_owned())?;
    let root = resolve
        .root
        .ok_or_else(|| "cargo metadata returned no root package".to_owned())?;
    let nodes = resolve
        .nodes
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let reachable = reachable_package_ids(&root, &nodes);
    let mut surfaces = metadata
        .packages
        .into_iter()
        .filter(|package| reachable.contains(&package.id))
        .filter(|package| SOURCE_SURFACE_PACKAGES.contains(&package.name.as_str()))
        .map(source_surface)
        .collect::<Result<Vec<_>, _>>()?;
    surfaces.sort_by(|left, right| {
        (&left.name, &left.version, &left.source).cmp(&(&right.name, &right.version, &right.source))
    });
    Ok(surfaces)
}

fn source_surface(package: CargoPackage) -> Result<SourceSurface, String> {
    let source_root = Path::new(&package.manifest_path)
        .parent()
        .ok_or_else(|| format!("{} has no source root", package.name))?;
    let rust_files = rust_files(source_root)?;
    let mut lexical_unsafe_token_count = 0;
    let mut rust_files_with_unsafe_tokens = 0;
    let mut platform_markers = BTreeSet::new();
    for path in &rust_files {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let tokens = source
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .collect::<Vec<_>>();
        let count = tokens.iter().filter(|token| **token == "unsafe").count();
        lexical_unsafe_token_count += count;
        if count > 0 {
            rust_files_with_unsafe_tokens += 1;
        }
        for (label, marker) in PLATFORM_MARKERS {
            if tokens.contains(marker) {
                platform_markers.insert((*label).to_owned());
            }
        }
    }
    let license_files = license_files(source_root)?;
    let state = if rust_files.is_empty() {
        "source unavailable".into()
    } else if lexical_unsafe_token_count == 0 {
        "no lexical unsafe tokens found".into()
    } else {
        "unsafe review surface located".into()
    };
    Ok(SourceSurface {
        owner: source_owner(&package.name).into(),
        retention_boundary: retention_boundary(&package.name).into(),
        name: package.name,
        version: package.version,
        source: package.source.unwrap_or_else(|| "workspace".into()),
        declared_license: package.license.unwrap_or_else(|| "undeclared".into()),
        license_files,
        rust_file_count: rust_files.len(),
        lexical_unsafe_token_count,
        rust_files_with_unsafe_tokens,
        platform_markers: platform_markers.into_iter().collect(),
        state,
    })
}

fn source_owner(name: &str) -> &'static str {
    match name {
        "avian2d" => "Lince physics adapter",
        "bevy_render" => "Lince native-world adapter",
        _ => "unassigned",
    }
}

fn retention_boundary(name: &str) -> &'static str {
    match name {
        "avian2d" => {
            "Retain behind PhysicsAdapter only if representative force and collision evidence wins."
        }
        "bevy_render" => {
            "Retain only manual rendering on host resources; no engine-owned window or final compositor."
        }
        _ => "Do not retain without an assigned adapter boundary.",
    }
}

fn rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("failed to read {}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("rs")
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn license_files(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let mut directory = root;
    for distance in 0..=4 {
        let prefix = "../".repeat(distance);
        let mut found = fs::read_dir(directory)
            .map_err(|error| format!("failed to read {}: {error}", directory.display()))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| {
                let upper = name.to_ascii_uppercase();
                upper.starts_with("LICENSE")
                    || upper.starts_with("COPYING")
                    || upper.starts_with("NOTICE")
            })
            .map(|name| format!("{prefix}{name}"))
            .collect::<Vec<_>>();
        if !found.is_empty() {
            files.append(&mut found);
            break;
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent;
    }
    files.sort();
    Ok(files)
}

fn profile_from_metadata(
    name: &str,
    features: Vec<&str>,
    metadata: CargoMetadata,
) -> Result<DependencyProfile, String> {
    let resolve = metadata
        .resolve
        .ok_or_else(|| "cargo metadata returned no dependency resolution".to_owned())?;
    let root = resolve
        .root
        .ok_or_else(|| "cargo metadata returned no root package".to_owned())?;
    let nodes = resolve
        .nodes
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let reachable = reachable_package_ids(&root, &nodes);
    let packages = metadata
        .packages
        .into_iter()
        .filter(|package| reachable.contains(&package.id))
        .collect::<Vec<_>>();
    let mut git_source_families = packages
        .iter()
        .filter_map(|package| package.source.as_deref())
        .filter_map(git_source_family)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    git_source_families.sort();
    let mut critical_packages = packages
        .iter()
        .filter(|package| CRITICAL_PACKAGES.contains(&package.name.as_str()))
        .map(|package| ResolvedPackage {
            name: package.name.clone(),
            version: package.version.clone(),
            source: package.source.clone().unwrap_or_else(|| "workspace".into()),
            license: package
                .license
                .clone()
                .unwrap_or_else(|| "undeclared".into()),
            activated_features: nodes
                .get(&package.id)
                .map(|node| node.features.clone())
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    critical_packages.sort_by(|left, right| {
        (&left.name, &left.version, &left.source).cmp(&(&right.name, &right.version, &right.source))
    });
    let duplicate_critical_packages = duplicate_packages(&critical_packages);

    Ok(DependencyProfile {
        name: name.into(),
        enabled_features: features.into_iter().map(str::to_owned).collect(),
        package_count: reachable.len(),
        git_source_families,
        critical_packages,
        duplicate_critical_packages,
    })
}

fn reachable_package_ids(root: &str, nodes: &BTreeMap<String, CargoNode>) -> BTreeSet<String> {
    let mut reachable = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id.clone()) {
            continue;
        }
        if let Some(node) = nodes.get(&id) {
            pending.extend(node.dependencies.iter().cloned());
        }
    }
    reachable
}

fn duplicate_packages(packages: &[ResolvedPackage]) -> Vec<PackageDuplicate> {
    let mut versions = BTreeMap::<String, BTreeSet<String>>::new();
    for package in packages {
        versions
            .entry(package.name.clone())
            .or_default()
            .insert(package.version.clone());
    }
    versions
        .into_iter()
        .filter(|(_, versions)| versions.len() > 1)
        .map(|(name, versions)| PackageDuplicate {
            name,
            versions: versions.into_iter().collect(),
        })
        .collect()
}

fn git_source_family(source: &str) -> Option<String> {
    source
        .strip_prefix("git+")
        .map(|source| source.split('#').next().unwrap_or(source).to_owned())
}

fn audit_assertions(
    profiles: &[DependencyProfile],
    source_surfaces: &[SourceSurface],
) -> Vec<AuditAssertion> {
    let mut assertions = Vec::new();
    for name in ["base", "bevy", "physics"] {
        assertions.push(single_wgpu_29_assertion(profile(profiles, name), name));
    }
    let expected_sources = SOURCE_SURFACE_PACKAGES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let found_sources = source_surfaces
        .iter()
        .map(|surface| surface.name.as_str())
        .collect::<BTreeSet<_>>();
    assertions.push(AuditAssertion {
        name: "pinned source review coverage".into(),
        state: if found_sources == expected_sources
            && source_surfaces
                .iter()
                .all(|surface| surface.rust_file_count > 0 && surface.owner != "unassigned")
        {
            "pass"
        } else {
            "fail"
        }
        .into(),
        detail: format!(
            "resolved, scanned and assigned {} of {} selected source packages",
            found_sources.len(),
            expected_sources.len()
        ),
    });
    let undeclared_licenses = source_surfaces
        .iter()
        .filter(|surface| surface.declared_license == "undeclared")
        .map(|surface| surface.name.as_str())
        .collect::<Vec<_>>();
    assertions.push(AuditAssertion {
        name: "selected source license declarations".into(),
        state: if undeclared_licenses.is_empty() {
            "pass"
        } else {
            "fail"
        }
        .into(),
        detail: if undeclared_licenses.is_empty() {
            "every selected package declares its license in Cargo metadata".into()
        } else {
            format!("missing declarations: {}", undeclared_licenses.join(", "))
        },
    });
    let missing_license_files = source_surfaces
        .iter()
        .filter(|surface| surface.license_files.is_empty())
        .map(|surface| surface.name.as_str())
        .collect::<Vec<_>>();
    assertions.push(AuditAssertion {
        name: "selected source packaged license files".into(),
        state: if missing_license_files.is_empty() {
            "pass"
        } else {
            "attention"
        }
        .into(),
        detail: if missing_license_files.is_empty() {
            "a license or notice file is present at or above every selected package root".into()
        } else {
            format!(
                "declared licenses lack a nearby packaged file for: {}",
                missing_license_files.join(", ")
            )
        },
    });
    let unsafe_packages = source_surfaces
        .iter()
        .filter(|surface| surface.lexical_unsafe_token_count > 0)
        .map(|surface| {
            format!(
                "{} {}:{}",
                surface.name, surface.version, surface.lexical_unsafe_token_count
            )
        })
        .collect::<Vec<_>>();
    assertions.push(AuditAssertion {
        name: "unsafe ownership inventory recorded".into(),
        state: "pass".into(),
        detail: if unsafe_packages.is_empty() {
            "no lexical unsafe token was found in the selected source packages".into()
        } else {
            format!(
                "third-party lexical unsafe surfaces are pinned and assigned to bounded Lince adapters: {}",
                unsafe_packages.join(", ")
            )
        },
    });
    assertions
}

fn profile<'a>(profiles: &'a [DependencyProfile], name: &str) -> &'a DependencyProfile {
    profiles
        .iter()
        .find(|profile| profile.name == name)
        .unwrap_or_else(|| panic!("missing dependency profile {name}"))
}

fn single_wgpu_29_assertion(profile: &DependencyProfile, label: &str) -> AuditAssertion {
    let versions = package_versions(profile, "wgpu");
    AuditAssertion {
        name: format!("{label} WGPU family"),
        state: if versions == BTreeSet::from(["29.0.4"]) {
            "pass"
        } else {
            "fail"
        }
        .into(),
        detail: format!("resolved WGPU versions {}", join_versions(&versions)),
    }
}

fn package_versions<'a>(profile: &'a DependencyProfile, name: &str) -> BTreeSet<&'a str> {
    profile
        .critical_packages
        .iter()
        .filter(|package| package.name == name)
        .map(|package| package.version.as_str())
        .collect()
}

fn join_versions(versions: &BTreeSet<&str>) -> String {
    if versions.is_empty() {
        "none".into()
    } else {
        versions.iter().copied().collect::<Vec<_>>().join(", ")
    }
}

fn source_findings() -> Vec<SourceFinding> {
    vec![
        SourceFinding {
            subject: "Bevy manual render ownership".into(),
            state: "passed in joined runtime".into(),
            detail: "Bevy 0.19.1 runs without its window runner and hands command buffers from caller-supplied render resources to the Lince-owned WGPU queue and final compositor.".into(),
            evidence: "target/interface-laboratory/joined/report.json ownership and command-buffer assertions".into(),
        },
        SourceFinding {
            subject: "GPUI external composition".into(),
            state: "rejected as production owner".into(),
            detail: "The exact-source diagnostic proved GPUI's compositor path but also retained GPUI ownership of the event loop and final presentation. Lince keeps only its visual and behavioral reference, not the dependency, in the joined runtime.".into(),
            evidence: "target/interface/reports/gpui-diagnostic-v7.json and the GPUI findings in anicca/interface/architecture.md".into(),
        },
        SourceFinding {
            subject: "Avian comparison".into(),
            state: "retained as collision adapter".into(),
            detail: "Avian remains behind PhysicsAdapter for the mature collision island while Lince's structure-of-arrays field solver owns Protein filters, Areas, immunity, sorting and mutation previews.".into(),
            evidence: "target/interface-laboratory/spatial/report.json phase and fixed-step comparison".into(),
        },
    ]
}

fn host_target() -> String {
    Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|output| {
            output
                .lines()
                .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        })
        .unwrap_or_else(|| match std::env::consts::OS {
            "linux" => format!("{}-unknown-linux-gnu", std::env::consts::ARCH),
            "macos" => format!("{}-apple-darwin", std::env::consts::ARCH),
            "windows" => format!("{}-pc-windows-msvc", std::env::consts::ARCH),
            operating_system => format!("{}-unknown-{operating_system}", std::env::consts::ARCH),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_family_keeps_the_exact_reference_and_drops_only_the_commit_fragment() {
        assert_eq!(
            git_source_family("git+https://example.test/repo?rev=precise#0123456789abcdef"),
            Some("https://example.test/repo?rev=precise".into())
        );
        assert_eq!(git_source_family("registry+https://example.test"), None);
    }

    #[test]
    fn duplicate_detection_ignores_repeated_identical_versions() {
        let package = |version: &str| ResolvedPackage {
            name: "wgpu".into(),
            version: version.into(),
            source: "registry".into(),
            license: "MIT".into(),
            activated_features: Vec::new(),
        };
        let duplicates =
            duplicate_packages(&[package("29.0.4"), package("29.0.4"), package("30.0.2")]);

        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].versions, ["29.0.4", "30.0.2"]);
    }
}
