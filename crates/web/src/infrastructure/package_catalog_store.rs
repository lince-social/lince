use {
    crate::{
        domain::lince_package::{
            LEGACY_PACKAGE_ARCHIVE_EXTENSION, LEGACY_PACKAGE_EXTENSION, LincePackage,
            PACKAGE_EXTENSION, is_package_filename, normalize_package_filename,
            package_id_from_filename, parse_lince_package,
        },
        infrastructure::paths,
        sand,
    },
    serde::Serialize,
    std::{
        path::{Path, PathBuf},
        sync::Arc,
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackageSummary {
    pub id: String,
    pub filename: String,
    pub icon: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub details: String,
    pub initial_width: u8,
    pub initial_height: u8,
    pub requires_server: bool,
    pub permissions: Vec<String>,
    // Sand GROUPS (e.g. kanban ships as board + record_info) surface as a single
    // catalog entry with `is_group = true`; adding it drops the whole group.
    // `member_count` is how many sub-sands the group carries.
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub member_count: usize,
}

#[derive(Clone)]
pub struct PackageCatalogStore {
    dir: Arc<PathBuf>,
}

impl PackageCatalogStore {
    pub fn new() -> Result<Self, String> {
        let dir = paths::sand_dir();
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!("Nao consegui criar a pasta ~/.config/lince/web/sand: {error}")
        })?;
        sand::render_official_widgets(&dir)?;
        // Sand-as-group archives (e.g. kanban = board + record_info) are emitted
        // alongside the single-sand packages (Stage 8b, Phase 3).
        sand::render_official_groups(&dir)?;

        Ok(Self { dir: Arc::new(dir) })
    }

    pub fn list(&self) -> Result<Vec<InstalledPackageSummary>, String> {
        let mut groups = Vec::new();
        let mut singles = Vec::new();

        for entry in std::fs::read_dir(&*self.dir).map_err(|error| {
            format!("Nao consegui ler a pasta ~/.config/lince/web/sand: {error}")
        })? {
            let entry = entry
                .map_err(|error| format!("Nao consegui ler um item da pasta local: {error}"))?;
            let path = entry.path();
            let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !is_package_filename(filename) {
                continue;
            }
            let filename = filename.to_string();

            let bytes = std::fs::read(&path)
                .map_err(|error| format!("Nao consegui ler um widget local: {error}"))?;
            // Group `.lince` files are workspace archives (a group of sub-sands).
            // Surface them as a single `is_group` catalog entry instead of a
            // single sand (Stage 8b, base task 2: kanban adds as a group).
            if crate::domain::workspace_archive::is_workspace_archive_bytes(&bytes) {
                groups.push(summary_from_group(&filename, &bytes)?);
                continue;
            }
            let package = parse_lince_package(filename, &bytes)?;
            singles.push(summary_from_package(package));
        }

        // A group named the same as a single sand (kanban.lince vs kanban.html,
        // both id "kanban") REPLACES that single sand — so "Kanban" in the
        // catalog is the group, per the add-as-group default. Members that are
        // reusable on their own (e.g. record_info) keep their own single entry.
        let group_ids: std::collections::HashSet<String> =
            groups.iter().map(|group| group.id.clone()).collect();
        singles.retain(|single| !group_ids.contains(&single.id));

        let mut packages = groups;
        packages.extend(singles);
        packages.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
        Ok(packages)
    }

    pub fn load(&self, package_id: &str) -> Result<LincePackage, String> {
        let filename = normalize_package_filename(package_id);
        self.load_by_filename(&filename)
    }

    pub fn load_by_filename(&self, filename: &str) -> Result<LincePackage, String> {
        let filename = Path::new(filename)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Nome de widget local invalido.".to_string())?;
        let normalized = normalize_package_filename(filename);
        let path = resolve_package_path(&self.dir, filename, &normalized);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("Nao consegui ler o sand solicitado: {error}"))?;
        let canonical_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&normalized);
        parse_lince_package(canonical_name, &bytes)
    }
}


pub fn summary_from_package(package: LincePackage) -> InstalledPackageSummary {
    let filename = package.archive_filename();
    let id = package_id_from_filename(&filename);
    let manifest = package.manifest;

    InstalledPackageSummary {
        id,
        filename,
        icon: manifest.icon,
        title: manifest.title,
        author: manifest.author,
        version: manifest.version,
        description: manifest.description,
        details: manifest.details,
        initial_width: manifest.initial_width,
        initial_height: manifest.initial_height,
        requires_server: manifest.requires_server,
        permissions: manifest.permissions,
        is_group: false,
        member_count: 0,
    }
}

// Build a catalog entry for a sand GROUP (a workspace archive). The group's
// display metadata comes from the workspace name and its primary (first, lowest
// z-order) member sand; adding it drops every member as one grouped unit.
fn summary_from_group(filename: &str, bytes: &[u8]) -> Result<InstalledPackageSummary, String> {
    let imported = crate::domain::workspace_archive::parse_workspace_archive(filename, bytes)?;
    let id = package_id_from_filename(filename);
    let member_count = imported.workspace.cards.len();

    // The primary member is the lowest z-order card (the base layer, e.g. the
    // kanban board under its record_info). Fall back to the first package.
    let primary = imported
        .workspace
        .cards
        .iter()
        .min_by_key(|card| card.z_index)
        .and_then(|card| {
            imported
                .packages
                .iter()
                .find(|package| package.archive_filename() == card.package_name)
        })
        .or_else(|| imported.packages.first());

    let manifest = primary.map(|package| package.manifest.clone());
    let title = if imported.workspace.name.trim().is_empty() {
        manifest
            .as_ref()
            .map(|manifest| manifest.title.clone())
            .unwrap_or_else(|| id.clone())
    } else {
        imported.workspace.name.clone()
    };

    Ok(InstalledPackageSummary {
        id,
        filename: filename.to_string(),
        icon: manifest
            .as_ref()
            .map(|manifest| manifest.icon.clone())
            .unwrap_or_else(|| "▤".to_string()),
        title,
        author: manifest
            .as_ref()
            .map(|manifest| manifest.author.clone())
            .unwrap_or_else(|| "Lince".to_string()),
        version: manifest
            .as_ref()
            .map(|manifest| manifest.version.clone())
            .unwrap_or_else(|| "1.0.0".to_string()),
        description: manifest
            .as_ref()
            .map(|manifest| manifest.description.clone())
            .unwrap_or_default(),
        details: manifest
            .as_ref()
            .map(|manifest| manifest.details.clone())
            .unwrap_or_default(),
        initial_width: manifest.as_ref().map(|manifest| manifest.initial_width).unwrap_or(6),
        initial_height: manifest.as_ref().map(|manifest| manifest.initial_height).unwrap_or(5),
        requires_server: manifest
            .as_ref()
            .map(|manifest| manifest.requires_server)
            .unwrap_or(false),
        permissions: manifest
            .map(|manifest| manifest.permissions)
            .unwrap_or_default(),
        is_group: true,
        member_count,
    })
}

fn resolve_package_path(dir: &Path, original: &str, normalized: &str) -> PathBuf {
    let primary = dir.join(normalized);
    if primary.exists() {
        return primary;
    }

    let direct = dir.join(original);
    if direct.exists() {
        return direct;
    }

    if normalized.ends_with(PACKAGE_EXTENSION) {
        let sand_fallback = dir.join(
            normalized.trim_end_matches(PACKAGE_EXTENSION).to_string() + LEGACY_PACKAGE_EXTENSION,
        );
        if sand_fallback.exists() {
            return sand_fallback;
        }

        let lince_fallback = dir.join(
            normalized.trim_end_matches(PACKAGE_EXTENSION).to_string()
                + LEGACY_PACKAGE_ARCHIVE_EXTENSION,
        );
        if lince_fallback.exists() {
            return lince_fallback;
        }
    }

    primary
}
