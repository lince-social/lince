use {
    crate::domain::{
        board::{BoardCard, BoardWorkspace},
        lince_package::{
            LincePackage, PackageManifest, build_lince_archive, parse_lince_package, slugify,
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
    if !filename.ends_with(".workspace.lince") {
        return Err("O arquivo precisa ter extensao .workspace.lince.".into());
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
        format!("{}.lince", slugify(&title))
    } else if card.package_name.trim().ends_with(".lince") {
        card.package_name.trim().to_string()
    } else {
        format!("{}.lince", slugify(card.package_name.trim()))
    };
    let html = if card.html.trim().is_empty() {
        "<!doctype html><html lang=\"pt-BR\"><body></body></html>".to_string()
    } else {
        card.html.clone()
    };

    LincePackage::new(
        Some(filename),
        PackageManifest {
            icon: "◧".into(),
            title,
            author: fallback_string(&card.author, "Workspace importado"),
            version: "0.1.0".into(),
            description: fallback_string(
                &card.description,
                "Package reconstruido a partir de um card exportado do workspace.",
            ),
            details:
                "Package reconstruido automaticamente a partir do estado exportado de um workspace."
                    .into(),
            initial_width: card.w,
            initial_height: card.h,
            permissions: card.permissions.clone(),
        },
        html,
    )
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
                package_name: "demo-widget.lince".into(),
                server_id: String::new(),
                view_id: None,
                x: 1,
                y: 1,
                w: 4,
                h: 3,
            }],
        };

        let package = LincePackage::new(
            Some("demo-widget.lince".into()),
            PackageManifest {
                icon: "◧".into(),
                title: "Demo".into(),
                author: "Lince".into(),
                version: "0.1.0".into(),
                description: "Widget demo".into(),
                details: "Detalhes".into(),
                initial_width: 4,
                initial_height: 3,
                permissions: vec!["demo".into()],
            },
            "<!doctype html><html><body>demo</body></html>",
        )
        .expect("package should build");

        let bytes = build_workspace_archive(&workspace, &[package]).expect("archive should build");
        let imported = parse_workspace_archive("area-1.workspace.lince", &bytes)
            .expect("archive should parse");

        assert_eq!(imported.workspace.name, "Area 1");
        assert_eq!(imported.workspace.cards.len(), 1);
        assert_eq!(imported.packages.len(), 1);
        assert_eq!(imported.packages[0].archive_filename(), "demo-widget.lince");
    }
}
