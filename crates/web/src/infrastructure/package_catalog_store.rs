use {
    crate::{
        domain::lince_package::{
            LEGACY_PACKAGE_ARCHIVE_EXTENSION, LEGACY_PACKAGE_EXTENSION, LincePackage,
            MAX_PACKAGE_BYTES, PACKAGE_EXTENSION, build_lince_archive, is_package_filename,
            normalize_package_filename, package_id_from_filename, parse_lince_package,
            validate_package_upload,
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

        Ok(Self { dir: Arc::new(dir) })
    }

    pub fn list(&self) -> Result<Vec<InstalledPackageSummary>, String> {
        let mut packages = Vec::new();

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

            let bytes = std::fs::read(&path)
                .map_err(|error| format!("Nao consegui ler um widget local: {error}"))?;
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("widget.html");
            let package = parse_lince_package(filename, &bytes)?;
            let summary = summary_from_package(package);
            packages.push(summary);
        }

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

    pub fn install_upload(&self, filename: &str, bytes: &[u8]) -> Result<LincePackage, String> {
        validate_package_upload(filename, bytes)?;
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("O widget excede o limite de tamanho aceito.".into());
        }

        let package = parse_lince_package(filename, bytes)?;
        self.persist_package(&package)?;
        Ok(package)
    }

    pub fn persist_package(&self, package: &LincePackage) -> Result<(), String> {
        let raw_archive = build_lince_archive(package)?;
        let path = self.dir.join(package.archive_filename());
        std::fs::write(&path, raw_archive).map_err(|error| {
            format!("Nao consegui salvar o sand em ~/.config/lince/web/sand: {error}")
        })?;
        Ok(())
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
    }
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
