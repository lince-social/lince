use {
    crate::{
        application::state::AppState,
        domain::{
            board::{BoardCard, BoardState, BoardWorkspace},
            lince_package::{LincePackage, slugify},
            workspace_archive::{
                build_workspace_archive, parse_workspace_archive, reconstruct_package_from_card,
            },
        },
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{
        Json,
        body::Body,
        extract::{Multipart, Path, State},
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::IntoResponse,
    },
};

pub async fn get_board_state(State(state): State<AppState>) -> ApiResult<Json<BoardState>> {
    Ok(Json(hydrated_board_state(&state).await))
}

pub async fn put_board_state(
    State(state): State<AppState>,
    Json(payload): Json<BoardState>,
) -> ApiResult<Json<BoardState>> {
    let saved = state
        .board_state
        .replace(payload)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    Ok(Json(saved))
}

pub async fn export_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let board_state = state.board_state.snapshot().await;
    let workspace = board_state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .cloned()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Workspace nao encontrada."))?;
    let packages = collect_workspace_packages(&state, &workspace)
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    let archive = build_workspace_archive(&workspace, &packages)
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let filename = format!("{}.workspace.sand", slugify(&workspace.name));

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha ao exportar workspace.",
            )
        })?,
    );

    Ok((headers, Body::from(archive)))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupExportRequest {
    #[serde(default)]
    pub name: String,
    pub card_ids: Vec<String>,
}

pub async fn export_workspace_group(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<GroupExportRequest>,
) -> ApiResult<impl IntoResponse> {
    let board_state = state.board_state.snapshot().await;
    let workspace = board_state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Workspace nao encontrada."))?;

    let cards: Vec<BoardCard> = workspace
        .cards
        .iter()
        .filter(|card| payload.card_ids.contains(&card.id) && !card.system)
        .cloned()
        .collect();
    if cards.is_empty() {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Nenhum card do grupo foi encontrado no workspace.",
        ));
    }

    let name = if payload.name.trim().is_empty() {
        "Grupo".to_string()
    } else {
        payload.name.trim().to_string()
    };
    let group_workspace = BoardWorkspace {
        id: format!("group-{}", uuid::Uuid::new_v4()),
        name: name.clone(),
        camera: crate::domain::board::default_camera(),
        cards,
    };
    let packages = collect_workspace_packages(&state, &group_workspace)
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let archive = build_workspace_archive(&group_workspace, &packages)
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let filename = format!("{}.group.sand", slugify(&name));

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha ao exportar grupo.",
            )
        })?,
    );

    Ok((headers, Body::from(archive)))
}

pub async fn import_group(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<BoardState>> {
    let mut archive_file: Option<(String, Vec<u8>)> = None;
    let mut center_x: Option<f64> = None;
    let mut center_y: Option<f64> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(crate::presentation::http::api_error::invalid_multipart)?
    {
        match field.name() {
            Some("centerX") => {
                center_x = field.text().await.ok().and_then(|value| value.parse().ok());
            }
            Some("centerY") => {
                center_y = field.text().await.ok().and_then(|value| value.parse().ok());
            }
            _ => {
                let Some(filename) = field.file_name().map(ToOwned::to_owned) else {
                    continue;
                };
                let bytes = field
                    .bytes()
                    .await
                    .map_err(crate::presentation::http::api_error::invalid_multipart)?;
                archive_file = Some((filename, bytes.to_vec()));
            }
        }
    }

    let (filename, bytes) = archive_file.ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "Nenhum arquivo .group.sand foi enviado.",
        )
    })?;
    let imported = parse_workspace_archive(&filename, &bytes)
        .map_err(|message| api_error(StatusCode::UNPROCESSABLE_ENTITY, message))?;
    for package in &imported.packages {
        state
            .packages
            .persist_package(package)
            .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    }

    let mut board_state = state.board_state.snapshot().await;
    let active_id = board_state.active_workspace_id.clone();
    let workspace = board_state
        .workspaces
        .iter_mut()
        .find(|workspace| workspace.id == active_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Workspace ativa nao encontrada."))?;

    let mut cards: Vec<BoardCard> = imported
        .workspace
        .cards
        .into_iter()
        .filter(|card| !card.system && !card.pinned)
        .collect();
    if cards.is_empty() {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "O arquivo de grupo nao tem cards importaveis.",
        ));
    }

    // Recenter the group's bounding box on the caller's viewport center.
    if let (Some(center_x), Some(center_y)) = (center_x, center_y) {
        let min_x = cards.iter().map(|card| card.x).fold(f64::INFINITY, f64::min);
        let min_y = cards.iter().map(|card| card.y).fold(f64::INFINITY, f64::min);
        let max_x = cards
            .iter()
            .map(|card| card.x + card.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = cards
            .iter()
            .map(|card| card.y + card.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let delta_x = center_x - (min_x + max_x) / 2.0;
        let delta_y = center_y - (min_y + max_y) / 2.0;
        for card in &mut cards {
            card.x += delta_x;
            card.y += delta_y;
        }
    }

    // Cards merge into the current workspace as one locked group.
    let group_id = format!("group-{}", uuid::Uuid::new_v4());
    for card in &mut cards {
        card.id = format!("card-{}", uuid::Uuid::new_v4());
        card.group_id = Some(group_id.clone());
    }
    workspace.cards.extend(cards);

    let saved = state
        .board_state
        .replace(board_state)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    Ok(Json(saved))
}

pub async fn import_workspace(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<BoardWorkspace>> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(crate::presentation::http::api_error::invalid_multipart)?
    {
        let Some(filename) = field.file_name().map(ToOwned::to_owned) else {
            continue;
        };
        let bytes = field
            .bytes()
            .await
            .map_err(crate::presentation::http::api_error::invalid_multipart)?;
        let imported = parse_workspace_archive(&filename, &bytes)
            .map_err(|message| api_error(StatusCode::UNPROCESSABLE_ENTITY, message))?;
        for package in &imported.packages {
            state
                .packages
                .persist_package(package)
                .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
        }

        let mut workspace = imported.workspace;
        let mut board_state = state.board_state.snapshot().await;
        workspace.id = format!("space-{}", uuid::Uuid::new_v4());
        workspace.name = next_workspace_name(&board_state, &workspace.name);
        board_state.active_workspace_id = workspace.id.clone();
        board_state.workspaces.push(workspace.clone());
        state
            .board_state
            .replace(board_state)
            .await
            .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
        return Ok(Json(workspace));
    }

    Err(api_error(
        StatusCode::BAD_REQUEST,
        "Nenhum arquivo .workspace.sand foi enviado.",
    ))
}

fn next_workspace_name(board_state: &BoardState, base_name: &str) -> String {
    let trimmed = base_name.trim();
    let fallback = if trimmed.is_empty() {
        "Workspace importada"
    } else {
        trimmed
    };

    if !board_state
        .workspaces
        .iter()
        .any(|workspace| workspace.name == fallback)
    {
        return fallback.to_string();
    }

    for index in 2..1000 {
        let candidate = format!("{fallback} {index}");
        if !board_state
            .workspaces
            .iter()
            .any(|workspace| workspace.name == candidate)
        {
            return candidate;
        }
    }

    format!("{fallback} {}", uuid::Uuid::new_v4())
}

fn collect_workspace_packages(
    state: &AppState,
    workspace: &BoardWorkspace,
) -> Result<Vec<LincePackage>, String> {
    let mut packages = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for card in workspace.cards.iter().filter(|card| card.kind == "package") {
        let package = load_package_for_card(state, card)?;
        let filename = package.archive_filename();
        if seen.insert(filename) {
            packages.push(package);
        }
    }

    Ok(packages)
}

fn load_package_for_card(state: &AppState, card: &BoardCard) -> Result<LincePackage, String> {
    if !card.package_name.trim().is_empty() {
        match state.packages.load_by_filename(card.package_name.trim()) {
            Ok(package) => return Ok(package),
            Err(error) => {
                tracing::warn!(
                    "workspace export could not find {}, reconstructing from card: {}",
                    card.package_name,
                    error
                );
            }
        }
    }

    reconstruct_package_from_card(card)
}

pub(crate) async fn hydrated_board_state(state: &AppState) -> BoardState {
    let mut board_state = state.board_state.snapshot().await;

    for workspace in &mut board_state.workspaces {
        for card in &mut workspace.cards {
            hydrate_package_card_from_catalog(state, card);
        }
    }

    board_state
}

fn hydrate_package_card_from_catalog(state: &AppState, card: &mut BoardCard) {
    if card.kind != "package" || card.package_name.trim().is_empty() {
        return;
    }

    let Ok(package) = state.packages.load_by_filename(card.package_name.trim()) else {
        return;
    };

    let archive_filename = package.archive_filename();
    let manifest = package.manifest;
    card.title = manifest.title;
    card.description = manifest.description;
    card.html = package.html;
    card.author = manifest.author;
    card.requires_server = manifest.requires_server;
    card.permissions = manifest.permissions;
    card.package_name = archive_filename;
}
