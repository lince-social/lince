use {
    crate::{
        domain::lince_package::{
            LincePackage, package_id_from_filename, parse_lince_package, validate_package_upload,
        },
        infrastructure::{
            dna_hub_store::DnaSandSearchMatch, package_catalog_store::InstalledPackageSummary,
        },
        presentation::http::api_error::{ApiResult, api_error, invalid_multipart},
    },
    axum::{
        Json,
        extract::{Multipart, Path, Query, State},
        http::StatusCode,
    },
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize)]
pub struct PackagePreview {
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
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaCatalogStatus {
    pub package_count: usize,
    pub packages: Vec<DnaPackageSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaPackageSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub path: String,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
pub struct DnaPackageSearchQuery {
    pub q: Option<String>,
}

impl From<LincePackage> for PackagePreview {
    fn from(package: LincePackage) -> Self {
        let filename = package.archive_filename();
        let id = package_id_from_filename(&filename);
        let LincePackage { manifest, html, .. } = package;
        Self {
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
            html,
        }
    }
}

pub async fn list_local_packages(
    State(state): State<crate::application::state::AppState>,
) -> ApiResult<Json<Vec<InstalledPackageSummary>>> {
    let packages = state.packages.list().map_err(|message| {
        crate::presentation::http::api_error::api_error(StatusCode::BAD_GATEWAY, message)
    })?;

    Ok(Json(packages))
}

pub async fn get_local_package(
    State(state): State<crate::application::state::AppState>,
    Path(package_id): Path<String>,
) -> ApiResult<Json<PackagePreview>> {
    let package = state.packages.load(&package_id).map_err(|message| {
        crate::presentation::http::api_error::api_error(StatusCode::NOT_FOUND, message)
    })?;

    Ok(Json(PackagePreview::from(package)))
}

pub async fn get_dna_catalog(
    State(state): State<crate::application::state::AppState>,
) -> ApiResult<Json<DnaCatalogStatus>> {
    let catalog = state.dna_hub.catalog().await.map_err(map_hub_error)?;
    let package_count = catalog.packages.len();
    let mut packages = catalog
        .packages
        .into_iter()
        .map(|(id, entry)| {
            let channel = channel_from_catalog_path(&entry.path)?;
            Ok(DnaPackageSummary {
                id,
                title: entry.title,
                description: entry.description,
                path: entry.path,
                channel,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(map_hub_error)?;
    packages.sort_by(|left, right| {
        left.title
            .to_ascii_lowercase()
            .cmp(&right.title.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(Json(DnaCatalogStatus {
        package_count,
        packages,
    }))
}

pub async fn search_dna_packages(
    State(state): State<crate::application::state::AppState>,
    Query(query): Query<DnaPackageSearchQuery>,
) -> ApiResult<Json<Vec<DnaPackageSummary>>> {
    let matches = state
        .dna_hub
        .search(query.q.as_deref().unwrap_or_default())
        .await
        .map_err(map_hub_error)?;
    Ok(Json(
        matches
            .into_iter()
            .map(DnaPackageSummary::from_search_match)
            .collect(),
    ))
}

pub async fn preview_dna_package(
    State(state): State<crate::application::state::AppState>,
    Path((channel, package_name)): Path<(String, String)>,
) -> ApiResult<Json<PackagePreview>> {
    let package = state
        .dna_hub
        .preview_package(&channel, &package_name)
        .await
        .map_err(map_hub_error)?;
    Ok(Json(PackagePreview::from(package)))
}

pub async fn install_dna_package(
    State(state): State<crate::application::state::AppState>,
    Path((channel, package_name)): Path<(String, String)>,
) -> ApiResult<Json<PackagePreview>> {
    let package = state
        .dna_hub
        .preview_package(&channel, &package_name)
        .await
        .map_err(map_hub_error)?;
    state
        .packages
        .persist_package(&package)
        .map_err(map_validation_error)?;
    Ok(Json(PackagePreview::from(package)))
}

pub async fn preview_package(mut multipart: Multipart) -> ApiResult<Json<PackagePreview>> {
    while let Some(field) = multipart.next_field().await.map_err(invalid_multipart)? {
        let Some(filename) = field.file_name().map(ToOwned::to_owned) else {
            continue;
        };

        let bytes = field.bytes().await.map_err(invalid_multipart)?;
        validate_package_upload(&filename, &bytes).map_err(map_validation_error)?;
        let preview = parse_lince_package(&filename, &bytes)
            .map(PackagePreview::from)
            .map_err(map_validation_error)?;
        return Ok(Json(preview));
    }

    Err(crate::presentation::http::api_error::api_error(
        StatusCode::BAD_REQUEST,
        "Nenhum widget HTML foi enviado.",
    ))
}

pub async fn install_package(
    State(state): State<crate::application::state::AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<PackagePreview>> {
    while let Some(field) = multipart.next_field().await.map_err(invalid_multipart)? {
        let Some(filename) = field.file_name().map(ToOwned::to_owned) else {
            continue;
        };

        let bytes = field.bytes().await.map_err(invalid_multipart)?;
        validate_package_upload(&filename, &bytes).map_err(map_validation_error)?;
        let package = state
            .packages
            .install_upload(&filename, &bytes)
            .map_err(map_validation_error)?;
        return Ok(Json(PackagePreview::from(package)));
    }

    Err(crate::presentation::http::api_error::api_error(
        StatusCode::BAD_REQUEST,
        "Nenhum widget HTML foi enviado.",
    ))
}

fn map_validation_error(
    message: String,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn map_hub_error(
    message: String,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    let status = if message.contains("nao encontrado") {
        StatusCode::NOT_FOUND
    } else if message.contains("invalido") {
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::BAD_GATEWAY
    };
    api_error(status, message)
}

impl DnaPackageSummary {
    fn from_search_match(value: DnaSandSearchMatch) -> Self {
        Self {
            id: value.package_name,
            title: value.title,
            description: value.description,
            path: value.path,
            channel: value.channel,
        }
    }
}

fn channel_from_catalog_path(path: &str) -> Result<String, String> {
    let mut parts = path.split('/');
    let channel = parts
        .next()
        .ok_or_else(|| "O catalogo remoto de widgets e invalido.".to_string())?;
    let _prefix = parts
        .next()
        .ok_or_else(|| "O catalogo remoto de widgets e invalido.".to_string())?;
    let _package_name = parts
        .next()
        .ok_or_else(|| "O catalogo remoto de widgets e invalido.".to_string())?;
    if parts.next().is_some() {
        return Err("O catalogo remoto de widgets e invalido.".to_string());
    }

    match channel {
        "official" => Ok("official".to_string()),
        "community" => Ok("community".to_string()),
        _ => Err("O catalogo remoto de widgets e invalido.".to_string()),
    }
}
