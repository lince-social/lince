use {
    serde::{Deserialize, Serialize},
    std::{
        io::{Cursor, Read, Write},
        path::Path,
    },
    zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions},
};

pub const MAX_PACKAGE_BYTES: usize = 768 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageManifest {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LincePackage {
    pub filename: Option<String>,
    pub manifest: PackageManifest,
    pub html: String,
}

#[derive(Debug, Deserialize)]
struct RawPackageManifest {
    icon: Option<String>,
    title: String,
    author: String,
    version: Option<String>,
    description: Option<String>,
    details: Option<String>,
    initial_width: Option<u8>,
    initial_height: Option<u8>,
    permissions: Option<Vec<String>>,
}

impl LincePackage {
    pub fn new(
        filename: Option<String>,
        manifest: PackageManifest,
        html: impl Into<String>,
    ) -> Result<Self, String> {
        let manifest = normalize_manifest(manifest)?;
        let html = normalize_html(&html.into())?;

        Ok(Self {
            filename,
            manifest,
            html,
        })
    }

    pub fn archive_filename(&self) -> String {
        self.filename
            .as_deref()
            .filter(|filename| filename.ends_with(".lince"))
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{}.lince", slugify(&self.manifest.title)))
    }

    pub fn config_toml(&self) -> String {
        toml::to_string_pretty(&self.manifest).expect("package manifest should serialize")
    }
}

pub fn validate_package_upload(filename: &str, bytes: &[u8]) -> Result<(), String> {
    if !filename.ends_with(".lince") {
        return Err("O arquivo precisa ter extensao .lince.".into());
    }

    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("O package .lince excede o limite de tamanho aceito.".into());
    }

    Ok(())
}

pub fn parse_lince_package(
    filename: impl Into<String>,
    bytes: &[u8],
) -> Result<LincePackage, String> {
    let filename = filename.into();
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|_| "Nao foi possivel abrir o arquivo .lince.")?;

    let config_raw = read_archive_entry(&mut archive, "config.toml")?;
    let html = read_archive_entry(&mut archive, "index.html")?;
    let raw_manifest: RawPackageManifest =
        toml::from_str(&config_raw).map_err(|_| "config.toml invalido para um package .lince.")?;

    LincePackage::new(Some(filename), manifest_from_raw(raw_manifest)?, html)
}

pub fn build_lince_archive(package: &LincePackage) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    writer
        .start_file("config.toml", options)
        .map_err(|error| format!("Nao consegui montar config.toml: {error}"))?;
    writer
        .write_all(package.config_toml().as_bytes())
        .map_err(|error| format!("Nao consegui escrever config.toml: {error}"))?;

    writer
        .start_file("index.html", options)
        .map_err(|error| format!("Nao consegui montar index.html: {error}"))?;
    writer
        .write_all(package.html.as_bytes())
        .map_err(|error| format!("Nao consegui escrever index.html: {error}"))?;

    let cursor = writer
        .finish()
        .map_err(|error| format!("Nao consegui finalizar o arquivo .lince: {error}"))?;

    Ok(cursor.into_inner())
}

pub fn normalize_permissions(permissions: Vec<String>) -> Vec<String> {
    let mut permissions = permissions
        .into_iter()
        .map(|permission| permission.trim().replace(' ', "_"))
        .filter(|permission| !permission.is_empty())
        .collect::<Vec<_>>();

    permissions.sort();
    permissions.dedup();
    permissions.truncate(8);
    permissions
}

pub fn normalize_html(html: &str) -> Result<String, String> {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Err("index.html nao pode estar vazio.".into());
    }

    let lowercase = trimmed.to_ascii_lowercase();
    if lowercase.starts_with("<!doctype") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("<!doctype html>\n{trimmed}"))
    }
}

pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in input.chars() {
        let normalized = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            _ if ch.is_ascii_whitespace() || ch == '-' || ch == '_' => Some('-'),
            _ => None,
        };

        let Some(normalized) = normalized else {
            continue;
        };

        if normalized == '-' {
            if slug.is_empty() || last_was_dash {
                continue;
            }
            last_was_dash = true;
        } else {
            last_was_dash = false;
        }

        slug.push(normalized);
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "lince-widget".to_string()
    } else {
        slug.to_string()
    }
}

fn manifest_from_raw(raw: RawPackageManifest) -> Result<PackageManifest, String> {
    normalize_manifest(PackageManifest {
        icon: raw.icon.unwrap_or_else(|| "◧".into()),
        title: raw.title,
        author: raw.author,
        version: raw.version.unwrap_or_else(|| "0.1.0".into()),
        description: raw
            .description
            .unwrap_or_else(|| "Card importado de um package .lince.".into()),
        details: raw.details.unwrap_or_else(|| {
            "Package modular pronto para ser instalado como um card independente.".into()
        }),
        initial_width: raw.initial_width.unwrap_or(3),
        initial_height: raw.initial_height.unwrap_or(2),
        permissions: raw.permissions.unwrap_or_default(),
    })
}

fn normalize_manifest(mut manifest: PackageManifest) -> Result<PackageManifest, String> {
    manifest.icon = fallback_string(&manifest.icon, "◧");
    manifest.title = manifest.title.trim().to_string();
    manifest.author = manifest.author.trim().to_string();
    manifest.version = fallback_string(&manifest.version, "0.1.0");
    manifest.description = fallback_string(
        &manifest.description,
        "Card importado de um package .lince.",
    );
    manifest.details = fallback_string(
        &manifest.details,
        "Package modular pronto para ser instalado como um card independente.",
    );
    manifest.initial_width = clamp_dimension(manifest.initial_width, 1, 6);
    manifest.initial_height = clamp_dimension(manifest.initial_height, 1, 6);
    manifest.permissions = normalize_permissions(manifest.permissions);

    if manifest.title.is_empty() {
        return Err("config.toml precisa definir um title.".into());
    }

    if manifest.author.is_empty() {
        return Err("config.toml precisa definir um author.".into());
    }

    Ok(manifest)
}

fn read_archive_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    expected_name: &str,
) -> Result<String, String> {
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| "Nao foi possivel ler os arquivos internos do package.")?;

        let file_name = Path::new(file.name())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if file_name != expected_name {
            continue;
        }

        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|_| format!("{expected_name} precisa ser texto UTF-8 valido."))?;

        return Ok(content);
    }

    Err(format!(
        "O package .lince precisa conter um arquivo {expected_name}."
    ))
}

fn clamp_dimension(value: u8, min: u8, max: u8) -> u8 {
    value.clamp(min, max)
}

fn fallback_string(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_roundtrip_preserves_manifest_and_html() {
        let package = LincePackage::new(
            Some("demo.lince".into()),
            PackageManifest {
                icon: "◧".into(),
                title: "Weather demo".into(),
                author: "Lince Labs".into(),
                version: "0.1.0".into(),
                description: "Preview".into(),
                details: "Roundtrip test".into(),
                initial_width: 3,
                initial_height: 2,
                permissions: vec!["read_weather".into()],
            },
            "<html><body>ok</body></html>",
        )
        .expect("package should build");

        let archive = build_lince_archive(&package).expect("archive should build");
        let parsed = parse_lince_package("demo.lince", &archive).expect("archive should parse");

        assert_eq!(parsed.archive_filename(), "demo.lince");
        assert_eq!(parsed.manifest, package.manifest);
        assert!(parsed.html.starts_with("<!doctype html>"));
    }

    #[test]
    fn slugify_generates_safe_filename() {
        assert_eq!(slugify("Weather + Tasks"), "weather-tasks");
        assert_eq!(slugify("   "), "lince-widget");
    }
}
