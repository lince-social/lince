use {
    crate::{
        domain::lince_package::{
            LincePackage, MAX_PACKAGE_BYTES, build_lince_archive, parse_lince_package, slugify,
            validate_package_upload,
        },
        infrastructure::paths,
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
    pub permissions: Vec<String>,
}

#[derive(Clone)]
pub struct PackageCatalogStore {
    dir: Arc<PathBuf>,
}

impl PackageCatalogStore {
    pub fn new() -> Result<Self, String> {
        let dir = paths::package_dir();
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!("Nao consegui criar a pasta ~/.config/lince/web/widgets: {error}")
        })?;
        seed_from_view_examples(&paths::package_examples_dir(), &dir)?;

        Ok(Self { dir: Arc::new(dir) })
    }

    pub fn list(&self) -> Result<Vec<InstalledPackageSummary>, String> {
        let mut packages = Vec::new();

        for entry in std::fs::read_dir(&*self.dir).map_err(|error| {
            format!("Nao consegui ler a pasta ~/.config/lince/web/widgets: {error}")
        })? {
            let entry = entry
                .map_err(|error| format!("Nao consegui ler um item da pasta local: {error}"))?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("lince") {
                continue;
            }

            let bytes = std::fs::read(&path)
                .map_err(|error| format!("Nao consegui ler um package local: {error}"))?;
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("package.lince");
            let package = parse_lince_package(filename, &bytes)?;
            packages.push(summary_from_package(package));
        }

        packages.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
        Ok(packages)
    }

    pub fn load(&self, package_id: &str) -> Result<LincePackage, String> {
        let filename = format!("{}.lince", slugify(package_id));
        self.load_by_filename(&filename)
    }

    pub fn load_by_filename(&self, filename: &str) -> Result<LincePackage, String> {
        let filename = Path::new(filename)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Nome de package local invalido.".to_string())?;
        let path = self.dir.join(filename);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("Nao consegui ler o package local solicitado: {error}"))?;
        parse_lince_package(filename, &bytes)
    }

    pub fn install_upload(&self, filename: &str, bytes: &[u8]) -> Result<LincePackage, String> {
        validate_package_upload(filename, bytes)?;
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("O package .lince excede o limite de tamanho aceito.".into());
        }

        let package = parse_lince_package(filename, bytes)?;
        self.persist_package(&package)?;
        Ok(package)
    }

    pub fn persist_package(&self, package: &LincePackage) -> Result<(), String> {
        let raw_archive = build_lince_archive(package)?;
        let path = self.dir.join(package.archive_filename());
        std::fs::write(&path, raw_archive).map_err(|error| {
            format!("Nao consegui salvar o package em ~/.config/lince/web/widgets: {error}")
        })?;
        Ok(())
    }
}

pub fn summary_from_package(package: LincePackage) -> InstalledPackageSummary {
    let filename = package.archive_filename();
    let id = filename.trim_end_matches(".lince").to_string();
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
        permissions: manifest.permissions,
    }
}

fn seed_from_view_examples(examples_dir: &Path, target_dir: &Path) -> Result<(), String> {
    if !examples_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&examples_dir)
        .map_err(|error| format!("Nao consegui ler view-examples: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Nao consegui ler um item de view-examples: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("lince") {
            continue;
        }

        let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        let target_path = target_dir.join(filename);
        if target_path.exists() {
            continue;
        }

        std::fs::copy(&path, &target_path).map_err(|error| {
            format!(
                "Nao consegui copiar um example package para ~/.config/lince/web/widgets: {error}"
            )
        })?;
    }

    Ok(())
}
