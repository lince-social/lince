use {
    crate::{
        domain::lince_package::{LincePackage, parse_lince_package, validate_package_upload},
        infrastructure::package_catalog_store::InstalledPackageSummary,
        presentation::http::api_error::{ApiResult, invalid_multipart},
    },
    axum::{
        Json,
        extract::{Multipart, Path, State},
        http::StatusCode,
    },
    serde::Serialize,
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

impl From<LincePackage> for PackagePreview {
    fn from(package: LincePackage) -> Self {
        let filename = package.archive_filename();
        let id = filename.trim_end_matches(".lince").to_string();
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
    let packages = state
        .packages
        .list()
        .map_err(|message| {
            crate::presentation::http::api_error::api_error(StatusCode::BAD_GATEWAY, message)
        })?;

    Ok(Json(packages))
}

pub async fn get_local_package(
    State(state): State<crate::application::state::AppState>,
    Path(package_id): Path<String>,
) -> ApiResult<Json<PackagePreview>> {
    let package = state
        .packages
        .load(&package_id)
        .map_err(|message| {
            crate::presentation::http::api_error::api_error(StatusCode::NOT_FOUND, message)
        })?;

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
        "Nenhum arquivo .lince foi enviado.",
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
        "Nenhum arquivo .lince foi enviado.",
    ))
}

fn map_validation_error(
    message: String,
) -> (StatusCode, Json<crate::presentation::http::api_error::ApiError>) {
    crate::presentation::http::api_error::api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
}
