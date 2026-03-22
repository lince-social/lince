use {
    serde::{Deserialize, Serialize},
    std::{
        io::{Cursor, Read},
        path::Path,
    },
    zip::ZipArchive,
};

pub const MAX_PACKAGE_BYTES: usize = 768 * 1024;
pub const PACKAGE_EXTENSION: &str = ".html";
pub const LEGACY_PACKAGE_EXTENSION: &str = ".sand";
pub const LEGACY_PACKAGE_ARCHIVE_EXTENSION: &str = ".lince";
const MANIFEST_SCRIPT_ID: &str = "lince-manifest";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let html = upsert_manifest_script(&html, &manifest);

        Ok(Self {
            filename,
            manifest,
            html,
        })
    }

    pub fn archive_filename(&self) -> String {
        self.filename
            .as_deref()
            .filter(|filename| is_package_filename(filename))
            .map(normalize_package_filename)
            .unwrap_or_else(|| format!("{}{}", slugify(&self.manifest.title), PACKAGE_EXTENSION))
    }

    pub fn manifest_json(&self) -> String {
        serde_json::to_string_pretty(&self.manifest).expect("package manifest should serialize")
    }

    pub fn html_document(&self) -> String {
        upsert_manifest_script(&self.html, &self.manifest)
    }
}

pub fn validate_package_upload(filename: &str, bytes: &[u8]) -> Result<(), String> {
    if !is_package_filename(filename) {
        return Err("O arquivo precisa ter extensao .html, .sand ou .lince.".into());
    }

    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("O widget excede o limite de tamanho aceito.".into());
    }

    Ok(())
}

pub fn parse_lince_package(
    filename: impl Into<String>,
    bytes: &[u8],
) -> Result<LincePackage, String> {
    let filename = normalize_package_filename(&filename.into());

    if looks_like_zip_archive(bytes) {
        return parse_archive_package(filename, bytes);
    }

    parse_html_package(filename, bytes)
}

pub fn build_lince_archive(package: &LincePackage) -> Result<Vec<u8>, String> {
    Ok(package.html_document().into_bytes())
}

pub fn is_package_filename(filename: &str) -> bool {
    let lowercase = filename.trim().to_ascii_lowercase();
    lowercase.ends_with(PACKAGE_EXTENSION)
        || lowercase.ends_with(LEGACY_PACKAGE_EXTENSION)
        || lowercase.ends_with(LEGACY_PACKAGE_ARCHIVE_EXTENSION)
}

pub fn package_id_from_filename(filename: &str) -> String {
    strip_package_extension(filename).to_string()
}

pub fn normalize_package_filename(filename: &str) -> String {
    format!("{}{}", strip_package_extension(filename), PACKAGE_EXTENSION)
}

fn strip_package_extension(filename: &str) -> &str {
    let trimmed = filename.trim();
    let lowercase = trimmed.to_ascii_lowercase();

    if lowercase.ends_with(PACKAGE_EXTENSION) {
        &trimmed[..trimmed.len() - PACKAGE_EXTENSION.len()]
    } else if lowercase.ends_with(LEGACY_PACKAGE_EXTENSION) {
        &trimmed[..trimmed.len() - LEGACY_PACKAGE_EXTENSION.len()]
    } else if lowercase.ends_with(LEGACY_PACKAGE_ARCHIVE_EXTENSION) {
        &trimmed[..trimmed.len() - LEGACY_PACKAGE_ARCHIVE_EXTENSION.len()]
    } else {
        trimmed
    }
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
        return Err("O HTML do widget nao pode estar vazio.".into());
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

pub fn parse_manifest_from_html(html: &str) -> Result<PackageManifest, String> {
    let Some(raw_manifest) = extract_manifest_json(html) else {
        return Err(
            "O widget HTML precisa conter <script type=\"application/json\" id=\"lince-manifest\">.</script>"
                .into(),
        );
    };

    if let Ok(manifest) = serde_json::from_str::<PackageManifest>(&raw_manifest) {
        return normalize_manifest(manifest);
    }

    let raw_manifest = serde_json::from_str::<RawPackageManifest>(&raw_manifest).map_err(|_| {
        "O manifesto embutido em lince-manifest nao eh um JSON valido para este widget.".to_string()
    })?;

    manifest_from_raw(raw_manifest)
}

fn manifest_from_raw(raw: RawPackageManifest) -> Result<PackageManifest, String> {
    normalize_manifest(PackageManifest {
        icon: raw.icon.unwrap_or_else(|| "◧".into()),
        title: raw.title,
        author: raw.author,
        version: raw.version.unwrap_or_else(|| "0.1.0".into()),
        description: raw
            .description
            .unwrap_or_else(|| "Widget HTML importado.".into()),
        details: raw
            .details
            .unwrap_or_else(|| "Widget autonomo pronto para virar um card independente.".into()),
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
    manifest.description = fallback_string(&manifest.description, "Widget HTML importado.");
    manifest.details = fallback_string(
        &manifest.details,
        "Widget autonomo pronto para virar um card independente.",
    );
    manifest.initial_width = clamp_dimension(manifest.initial_width, 1, 6);
    manifest.initial_height = clamp_dimension(manifest.initial_height, 1, 6);
    manifest.permissions = normalize_permissions(manifest.permissions);

    if manifest.title.is_empty() {
        return Err("O widget precisa definir um title no manifesto embutido.".into());
    }

    if manifest.author.is_empty() {
        return Err("O widget precisa definir um author no manifesto embutido.".into());
    }

    Ok(manifest)
}

fn parse_archive_package(filename: String, bytes: &[u8]) -> Result<LincePackage, String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|_| "Nao foi possivel abrir o arquivo .sand.")?;

    let html = read_archive_entry(&mut archive, "index.html")?;
    let manifest =
        if let Some(config_raw) = read_optional_archive_entry(&mut archive, "config.toml")? {
            let raw_manifest: RawPackageManifest = toml::from_str(&config_raw)
                .map_err(|_| "config.toml invalido para um package .sand.".to_string())?;
            manifest_from_raw(raw_manifest)?
        } else {
            parse_manifest_from_html(&html)?
        };

    LincePackage::new(Some(filename), manifest, html)
}

fn parse_html_package(filename: String, bytes: &[u8]) -> Result<LincePackage, String> {
    let html = std::str::from_utf8(bytes)
        .map_err(|_| "O widget HTML precisa ser UTF-8 valido.".to_string())?;
    let manifest = parse_manifest_from_html(html)?;
    LincePackage::new(Some(filename), manifest, html)
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
        "O package .sand precisa conter um arquivo {expected_name}."
    ))
}

fn read_optional_archive_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    expected_name: &str,
) -> Result<Option<String>, String> {
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

        return Ok(Some(content));
    }

    Ok(None)
}

fn looks_like_zip_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn extract_manifest_json(html: &str) -> Option<String> {
    let lowercase = html.to_ascii_lowercase();
    let mut offset = 0;

    while let Some(relative_start) = lowercase[offset..].find("<script") {
        let script_start = offset + relative_start;
        let tag_end = lowercase[script_start..].find('>')? + script_start;
        let open_tag = &lowercase[script_start..=tag_end];
        let has_manifest_id =
            open_tag.contains("id=\"lince-manifest\"") || open_tag.contains("id='lince-manifest'");

        if !has_manifest_id {
            offset = tag_end + 1;
            continue;
        }

        let close_start = lowercase[tag_end + 1..].find("</script>")? + tag_end + 1;
        return Some(html[tag_end + 1..close_start].trim().to_string());
    }

    None
}

fn upsert_manifest_script(html: &str, manifest: &PackageManifest) -> String {
    let script = format!(
        "<script type=\"application/json\" id=\"{MANIFEST_SCRIPT_ID}\">\n{}\n</script>",
        serde_json::to_string_pretty(manifest).expect("package manifest should serialize to JSON")
    );
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

        if has_manifest_id {
            let Some(relative_close_start) = lowercase[tag_end + 1..].find("</script>") else {
                break;
            };
            let close_end = tag_end + 1 + relative_close_start + "</script>".len();
            let mut updated = String::with_capacity(html.len() + script.len());
            updated.push_str(&html[..script_start]);
            updated.push_str(&script);
            updated.push_str(&html[close_end..]);
            return updated;
        }

        offset = tag_end + 1;
    }

    if let Some(head_open_start) = lowercase.find("<head") {
        if let Some(relative_head_end) = lowercase[head_open_start..].find('>') {
            let insert_at = head_open_start + relative_head_end + 1;
            let mut updated = String::with_capacity(html.len() + script.len() + 1);
            updated.push_str(&html[..insert_at]);
            updated.push('\n');
            updated.push_str(&script);
            updated.push_str(&html[insert_at..]);
            return updated;
        }
    }

    if lowercase.starts_with("<!doctype") {
        if let Some(doctype_end) = html.find('>') {
            let insert_at = doctype_end + 1;
            let mut updated = String::with_capacity(html.len() + script.len() + 1);
            updated.push_str(&html[..insert_at]);
            updated.push('\n');
            updated.push_str(&script);
            updated.push_str(&html[insert_at..]);
            return updated;
        }
    }

    format!("{script}\n{html}")
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
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    #[test]
    fn html_roundtrip_preserves_manifest_and_html() {
        let package = LincePackage::new(
            Some("demo.html".into()),
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

        let bytes = build_lince_archive(&package).expect("html should build");
        let parsed = parse_lince_package("demo.html", &bytes).expect("html should parse");

        assert_eq!(parsed.archive_filename(), "demo.html");
        assert_eq!(parsed.manifest, package.manifest);
        assert!(parsed.html.starts_with("<!doctype html>"));
        assert!(parsed.html.contains("id=\"lince-manifest\""));
    }

    #[test]
    fn legacy_archive_without_config_can_use_embedded_manifest() {
        let package = LincePackage::new(
            Some("demo.html".into()),
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

        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        writer
            .start_file("index.html", options)
            .expect("index entry should start");
        writer
            .write_all(package.html_document().as_bytes())
            .expect("index entry should write");

        let archive = writer.finish().expect("archive should finish").into_inner();
        let parsed = parse_lince_package("demo.sand", &archive).expect("archive should parse");

        assert_eq!(parsed.archive_filename(), "demo.html");
        assert_eq!(parsed.manifest, package.manifest);
    }

    #[test]
    fn slugify_generates_safe_filename() {
        assert_eq!(slugify("Weather + Tasks"), "weather-tasks");
        assert_eq!(slugify("   "), "lince-widget");
    }
}
