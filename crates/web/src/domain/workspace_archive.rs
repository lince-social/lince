use {
    crate::domain::{
        board::{BoardCard, BoardWorkspace},
        lince_package::{
            LEGACY_PACKAGE_ARCHIVE_EXTENSION, LincePackage, PACKAGE_EXTENSION,
            build_lince_archive, parse_lince_package, parse_manifest_from_html, slugify,
        },
    },
    serde::{Deserialize, Serialize},
    std::{
        collections::BTreeSet,
        io::{Cursor, Read, Write},
    },
    zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions},
};

const WORKSPACE_FILE_NAME: &str = "workspace.json";
const PACKAGES_DIR_NAME: &str = "packages";
const WORKSPACE_ARCHIVE_EXTENSION: &str = ".workspace.sand";
const LEGACY_WORKSPACE_ARCHIVE_EXTENSION: &str = ".workspace.lince";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceArchive {
    pub workspace: BoardWorkspace,
    #[serde(default)]
    pub packages: Vec<WorkspaceArchivePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceArchivePackage {
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct ImportedWorkspaceArchive {
    pub workspace: BoardWorkspace,
    pub packages: Vec<LincePackage>,
}

pub fn build_workspace_archive(
    workspace: &BoardWorkspace,
    packages: &[LincePackage],
) -> Result<Vec<u8>, String> {
    let package_entries = unique_packages(packages);
    let archive = WorkspaceArchive {
        workspace: workspace.clone(),
        packages: package_entries
            .iter()
            .map(|package| WorkspaceArchivePackage {
                filename: package.archive_filename(),
            })
            .collect(),
    };
    let raw = serde_json::to_vec_pretty(&archive)
        .map_err(|error| format!("Nao consegui serializar o workspace: {error}"))?;

    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    writer
        .start_file(WORKSPACE_FILE_NAME, options)
        .map_err(|error| format!("Nao consegui montar o archive do workspace: {error}"))?;
    writer
        .write_all(&raw)
        .map_err(|error| format!("Nao consegui escrever o workspace exportado: {error}"))?;

    for package in package_entries {
        let filename = package.archive_filename();
        let entry_name = format!("{PACKAGES_DIR_NAME}/{filename}");
        let raw_archive = build_lince_archive(&package)?;

        writer
            .start_file(&entry_name, options)
            .map_err(|error| format!("Nao consegui adicionar {filename} ao workspace: {error}"))?;
        writer
            .write_all(&raw_archive)
            .map_err(|error| format!("Nao consegui escrever {filename} no workspace: {error}"))?;
    }

    let cursor = writer
        .finish()
        .map_err(|error| format!("Nao consegui finalizar o archive do workspace: {error}"))?;

    Ok(cursor.into_inner())
}

pub fn parse_workspace_archive(
    filename: &str,
    bytes: &[u8],
) -> Result<ImportedWorkspaceArchive, String> {
    if !is_workspace_archive_filename(filename) {
        return Err("O arquivo precisa ter extensao .workspace.sand.".into());
    }

    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|_| "Nao foi possivel abrir o arquivo de workspace.")?;

    let mut workspace_json = String::new();
    archive
        .by_name(WORKSPACE_FILE_NAME)
        .map_err(|_| "workspace.json nao foi encontrado no arquivo exportado.")?
        .read_to_string(&mut workspace_json)
        .map_err(|_| "workspace.json precisa ser UTF-8 valido.".to_string())?;

    let workspace = serde_json::from_str::<WorkspaceArchive>(&workspace_json)
        .map_err(|_| "workspace.json invalido para um archive de workspace.".to_string())?;

    let mut packages = Vec::new();

    for package in &workspace.packages {
        let entry_name = format!("{PACKAGES_DIR_NAME}/{}", package.filename);
        let raw_package = read_archive_entry_bytes(&mut archive, &entry_name)?;
        let parsed = parse_lince_package(&package.filename, &raw_package)?;
        packages.push(parsed);
    }

    Ok(ImportedWorkspaceArchive {
        workspace: workspace.workspace,
        packages,
    })
}

pub fn reconstruct_package_from_card(card: &BoardCard) -> Result<LincePackage, String> {
    let title = fallback_string(&card.title, "Widget importado");
    let filename = if card.package_name.trim().is_empty() {
        format!("{}{}", slugify(&title), PACKAGE_EXTENSION)
    } else {
        let package_name = card.package_name.trim();
        if package_name.contains('.') {
            package_name.to_string()
        } else {
            format!("{}{}", slugify(package_name), PACKAGE_EXTENSION)
        }
    };
    let html = if card.html.trim().is_empty() {
        "<!doctype html><html lang=\"pt-BR\"><body></body></html>".to_string()
    } else {
        card.html.clone()
    };
    let manifest = parse_manifest_from_html(&html).unwrap_or_else(|_| {
        crate::domain::lince_package::PackageManifest {
            icon: "◧".into(),
            title: title.clone(),
            author: fallback_string(&card.author, "Workspace importado"),
            version: "0.1.0".into(),
            description: fallback_string(
                &card.description,
                "Widget reconstruido a partir de um card exportado do workspace.",
            ),
            details:
                    "Widget reconstruido automaticamente a partir do estado exportado de um workspace."
                    .into(),
            initial_width: card.w,
            initial_height: card.h,
            requires_server: card.requires_server,
            permissions: card.permissions.clone(),
        }
    });

    let manifest = crate::domain::lince_package::PackageManifest {
        title,
        author: fallback_string(&card.author, &manifest.author),
        description: fallback_string(&card.description, &manifest.description),
        initial_width: card.w,
        initial_height: card.h,
        requires_server: card.requires_server || manifest.requires_server,
        permissions: if card.permissions.is_empty() {
            manifest.permissions.clone()
        } else {
            card.permissions.clone()
        },
        ..manifest
    };

    if filename
        .to_ascii_lowercase()
        .ends_with(LEGACY_PACKAGE_ARCHIVE_EXTENSION)
    {
        LincePackage::new_archive(Some(filename), manifest, html, "index.html", Default::default())
    } else {
        LincePackage::new(Some(filename), manifest, html)
    }
}

fn is_workspace_archive_filename(filename: &str) -> bool {
    let lowercase = filename.trim().to_ascii_lowercase();
    lowercase.ends_with(WORKSPACE_ARCHIVE_EXTENSION)
        || lowercase.ends_with(LEGACY_WORKSPACE_ARCHIVE_EXTENSION)
}

fn unique_packages(packages: &[LincePackage]) -> Vec<LincePackage> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();

    for package in packages {
        let filename = package.archive_filename();
        if seen.insert(filename) {
            unique.push(package.clone());
        }
    }

    unique
}

fn fallback_string(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn read_archive_entry_bytes(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    expected_name: &str,
) -> Result<Vec<u8>, String> {
    let mut file = archive
        .by_name(expected_name)
        .map_err(|_| format!("{expected_name} nao foi encontrado no arquivo exportado."))?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .map_err(|_| format!("{expected_name} nao pode ser lido do arquivo exportado."))?;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::{BoardCard, BoardWorkspace};
    use serde_json::{Map, Value};

    #[test]
    fn workspace_archive_roundtrip_preserves_workspace_and_packages() {
        let workspace = BoardWorkspace {
            id: "space-1".into(),
            name: "Area 1".into(),
            cards: vec![BoardCard {
                id: "card-1".into(),
                kind: "package".into(),
                title: "Demo".into(),
                description: "Widget demo".into(),
                text: String::new(),
                html: "<!doctype html><html><body>demo</body></html>".into(),
                author: "Lince".into(),
                permissions: vec!["demo".into()],
                package_name: "demo-widget.html".into(),
                requires_server: false,
                server_id: String::new(),
                view_id: None,
                streams_enabled: true,
                widget_state: Value::Object(Map::new()),
                x: 1,
                y: 1,
                w: 4,
                h: 3,
            }],
        };

        let package = LincePackage::new(
            Some("demo-widget.html".into()),
            crate::domain::lince_package::PackageManifest {
                icon: "◧".into(),
                title: "Demo".into(),
                author: "Lince".into(),
                version: "0.1.0".into(),
                description: "Widget demo".into(),
                details: "Detalhes".into(),
                initial_width: 4,
                initial_height: 3,
                requires_server: false,
                permissions: vec!["demo".into()],
            },
            "<!doctype html><html><body>demo</body></html>",
        )
        .expect("package should build");

        let bytes = build_workspace_archive(&workspace, &[package]).expect("archive should build");
        let imported =
            parse_workspace_archive("area-1.workspace.sand", &bytes).expect("archive should parse");

        assert_eq!(imported.workspace.name, "Area 1");
        assert_eq!(imported.workspace.cards.len(), 1);
        assert_eq!(imported.packages.len(), 1);
        assert_eq!(imported.packages[0].archive_filename(), "demo-widget.html");
    }
}
