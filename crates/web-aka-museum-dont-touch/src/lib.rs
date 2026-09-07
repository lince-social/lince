pub mod cell_surface;

#[cfg(test)]
mod board_js_tests;
mod cell_bootstrap;
mod domain;
mod infrastructure;
mod presentation;
pub mod sand;

pub use crate::domain::lince_package::{LincePackage, slugify};

use {
    crate::{
        domain::{
            board::{
                AppBootstrap, AppRuntimeInfo, BoardCard, BoardState, ServerBootstrap,
                ViewerBootstrap,
            },
            widget_bridge::WidgetBridgeSnapshot,
        },
        infrastructure::{
            board_state_store::BoardStateStore, package_catalog_store::PackageCatalogStore,
        },
        presentation::http::{live_proxy, media_assets, static_assets},
    },
    std::{
        collections::HashMap,
        io::{Error as IoError, ErrorKind},
        net::SocketAddr,
        path::PathBuf,
        sync::Arc,
    },
    store::Store,
    tokio::sync::oneshot,
    transport::LaneHub,
    utils::desktop_setup::DesktopInstallSetup,
    utils::logging::status,
};

const DEFAULT_WEB_LISTEN_ADDR: &str = "127.0.0.1:6174";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpServeMode {
    FullUi,
    ApiOnly,
}

const HEARTBEAT_PERIOD_SECS: u64 = 60;

#[derive(Clone)]
struct CellApiState {
    board_state: BoardStateStore,
    engine: Arc<engine::Engine>,
    jwt_secret: Arc<String>,
    lanes: Arc<LaneHub>,
    listening_port: u16,
    local_auth_required: bool,
    wire: crate::presentation::http::wire_supervisor::WireSlot,
    packages: PackageCatalogStore,
    store: Store,
    remote_logins: Arc<tokio::sync::RwLock<HashMap<String, live_proxy::RemoteLogin>>>,
}

pub async fn serve_cell_api_only(
    listen_addr: Option<String>,
    jwt_secret: String,
    local_auth_required: bool,
    staged_setup: Option<DesktopInstallSetup>,
    bound_addr_sender: Option<oneshot::Sender<SocketAddr>>,
    mode: HttpServeMode,
) -> Result<(), IoError> {
    use axum::{
        Json,
        body::Body,
        extract::{Multipart, Path, State, WebSocketUpgrade, ws::WebSocket},
        http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
        response::{Html, IntoResponse, Response},
        routing::{get, post},
    };
    use serde::{Deserialize, Serialize};
    use std::time::Duration;
    use tower_http::services::ServeDir;

    const CELL_JWT_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7);
    const CELL_AUTH_COOKIE: &str = "lince_auth";

    #[derive(Deserialize)]
    struct LoginRequest {
        username: String,
        password: String,
    }

    #[derive(Serialize)]
    struct LoginResponse {
        token: String,
        token_type: &'static str,
    }

    fn auth_header(headers: &HeaderMap) -> Option<&str> {
        headers.get(header::AUTHORIZATION)?.to_str().ok()
    }

    fn auth_cookie(headers: &HeaderMap) -> Option<String> {
        let raw = headers.get(header::COOKIE)?.to_str().ok()?;
        raw.split(';').find_map(|entry| {
            let (name, value) = entry.trim().split_once('=')?;
            (name == CELL_AUTH_COOKIE).then(|| value.to_string())
        })
    }

    fn bearer_token(headers: &HeaderMap) -> Result<Option<String>, (StatusCode, String)> {
        if let Some(authorization) = auth_header(headers) {
            return authorization
                .strip_prefix("Bearer ")
                .map(str::to_string)
                .map(Some)
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Expected Bearer token".into()));
        }
        Ok(auth_cookie(headers))
    }

    async fn authenticate_headers(
        state: &CellApiState,
        headers: &HeaderMap,
    ) -> Result<Option<String>, (StatusCode, String)> {
        if !state.local_auth_required {
            return Ok(None);
        }
        let token = bearer_token(headers)?
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Missing JWT".into()))?;
        let claims = utils::auth::decode_jwt(state.jwt_secret.as_str(), &token)
            .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;
        let user = store::auth::user_by_uid(&state.store.pool, &claims.sub)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    "User from token no longer exists".into(),
                )
            })?;
        if !store::people::is_active(&state.store.pool, &user.uid)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        {
            return Err((StatusCode::UNAUTHORIZED, "Token auth data is stale".into()));
        }
        let current_permissions =
            utils::auth::normalized_permission_strings(user.permissions.clone());
        let claim_permissions =
            utils::auth::normalized_permission_strings(claims.permissions.clone());
        if user.username != claims.username
            || user.role_id as u64 != claims.role_id
            || user.role != claims.role
            || current_permissions != claim_permissions
        {
            return Err((StatusCode::UNAUTHORIZED, "Token auth data is stale".into()));
        }
        Ok(Some(claims.sub))
    }

    async fn viewer_from_headers(
        state: &CellApiState,
        headers: &HeaderMap,
    ) -> Option<ViewerBootstrap> {
        if !state.local_auth_required {
            return None;
        }
        let token = bearer_token(headers).ok().flatten()?;
        let claims = utils::auth::decode_jwt(state.jwt_secret.as_str(), &token).ok()?;
        let user = store::auth::user_by_uid(&state.store.pool, &claims.sub)
            .await
            .ok()
            .flatten()?;
        if !store::people::is_active(&state.store.pool, &user.uid)
            .await
            .ok()?
        {
            return None;
        }
        Some(ViewerBootstrap {
            id: user.uid.clone(),
            username: user.username,
            name: user.name,
            role: user.role,
            permissions: utils::auth::normalized_permission_strings(user.permissions),
        })
    }

    fn server_bootstrap_from_organ(
        organ: store::organs::OrganRecord,
        local_auth_required: bool,
    ) -> ServerBootstrap {
        ServerBootstrap {
            id: organ.uid,
            name: organ.head,
            base_url: organ.base_url,
            requires_auth: local_auth_required,
            authenticated: !local_auth_required,
            session_state: None,
            username_hint: String::new(),
            connected_at_unix: None,
            last_error: String::new(),
            local: true,
        }
    }

    async fn local_server_bootstrap(
        state: &CellApiState,
        viewer_present: bool,
    ) -> Vec<ServerBootstrap> {
        organ_list(state, viewer_present).await
    }

    async fn organ_list(state: &CellApiState, viewer_present: bool) -> Vec<ServerBootstrap> {
        let mut servers = Vec::new();
        if let Ok(Some(local)) = store::organs::local(&state.store.pool).await {
            let mut row = server_bootstrap_from_organ(local, state.local_auth_required);
            row.name = format!("{} (esta Lince)", row.name);
            row.authenticated = !state.local_auth_required || viewer_present;
            servers.push(row);
        }
        let held = state.remote_logins.read().await;
        if let Ok(contacts) = store::organs::contacts(&state.store.pool).await {
            for contact in contacts {
                if contact.trust == "blocked" {
                    continue;
                }
                let login = held.get(&contact.record_uid);
                servers.push(ServerBootstrap {
                    id: contact.record_uid.clone(),
                    name: contact.head.clone(),
                    base_url: contact.base_url.clone(),
                    requires_auth: true,
                    authenticated: login.is_some(),
                    session_state: Some(
                        if login.is_some() {
                            "logged_in"
                        } else {
                            "logged_out"
                        }
                        .to_string(),
                    ),
                    username_hint: login.map(|l| l.username.clone()).unwrap_or_default(),
                    connected_at_unix: None,
                    last_error: String::new(),
                    local: false,
                });
            }
        }
        servers
    }

    async fn index(State(state): State<CellApiState>, headers: HeaderMap) -> impl IntoResponse {
        let board_state = state.board_state.snapshot().await;
        let viewer = viewer_from_headers(&state, &headers).await;
        let servers = local_server_bootstrap(&state, viewer.is_some()).await;
        let bootstrap = AppBootstrap::new(
            WidgetBridgeSnapshot::default(),
            board_state,
            servers,
            AppRuntimeInfo {
                port: state.listening_port,
                version: env!("CARGO_PKG_VERSION"),
                revision: utils::build_info::revision(),
            },
            viewer,
        );
        Html(crate::presentation::pages::render_app(&bootstrap))
    }

    async fn login(
        State(state): State<CellApiState>,
        Json(request): Json<LoginRequest>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        let user = store::auth::user_by_username(&state.store.pool, request.username.trim())
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Invalid username or password".into(),
                )
            })?;
        let password_valid = utils::auth::verify_password(&request.password, &user.password_hash)
            .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;
        let active = store::people::is_active(&state.store.pool, &user.uid)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        if !password_valid || !active {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Invalid username or password".into(),
            ));
        }
        let token = utils::auth::issue_jwt(
            state.jwt_secret.as_str(),
            &user.uid,
            &user.username,
            user.role_id as u64,
            &user.role,
            &user.permissions,
            CELL_JWT_TTL,
        )
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let cookie = format!("{CELL_AUTH_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax");
        let mut response = Json(LoginResponse {
            token: token.clone(),
            token_type: "Bearer",
        })
        .into_response();
        response.headers_mut().insert(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie)
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?,
        );
        Ok(response)
    }

    async fn get_board_state(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        Ok(Json(state.board_state.snapshot().await))
    }

    async fn put_board_state(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(next_state): Json<BoardState>,
    ) -> Result<impl IntoResponse, (axum::http::StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        state
            .board_state
            .replace(next_state)
            .await
            .map(Json)
            .map_err(|error| (axum::http::StatusCode::BAD_REQUEST, error))
    }

    async fn export_workspace(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(workspace_id): Path<String>,
    ) -> Result<Response, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let board_state = state.board_state.snapshot().await;
        let workspace = board_state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Workspace nao encontrada.".into()))?;
        let mut packages = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for card in workspace.cards.iter().filter(|card| card.kind == "package") {
            let package = if card.package_name.trim().is_empty() {
                crate::domain::workspace_archive::reconstruct_package_from_card(card)
            } else {
                state
                    .packages
                    .load_by_filename(card.package_name.trim())
                    .or_else(|_| {
                        crate::domain::workspace_archive::reconstruct_package_from_card(card)
                    })
            }
            .map_err(|message| (StatusCode::BAD_GATEWAY, message))?;
            if seen.insert(package.archive_filename()) {
                packages.push(package);
            }
        }
        let archive =
            crate::domain::workspace_archive::build_workspace_archive(&workspace, &packages)
                .map_err(|message| (StatusCode::BAD_GATEWAY, message))?;
        let filename = format!(
            "{}.workspace.sand",
            crate::domain::lince_package::slugify(&workspace.name)
        );

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/zip")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            )
            .body(Body::from(archive))
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
    }

    async fn list_notifications(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let notifications = state
            .engine
            .notifications()
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        Ok(Json(serde_json::json!({ "notifications": notifications })))
    }

    async fn answer_thread_notification(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path((notification_id, answer)): Path<(String, String)>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let accept = match answer.as_str() {
            "accept" => true,
            "decline" => false,
            _ => return Err((StatusCode::NOT_FOUND, "unknown notification action".into())),
        };
        let wire = state.wire.read().await.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "no iroh endpoint".to_string(),
            )
        })?;
        let root = wire
            .answer_conversation_invite(&notification_id, accept)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        if accept {
            wire.sync_once()
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        }
        Ok(Json(
            serde_json::json!({ "record_id": root, "accepted": accept }),
        ))
    }

    async fn list_local_packages(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let packages = state
            .packages
            .list()
            .map_err(|message| (StatusCode::BAD_GATEWAY, message))?;
        Ok(Json(packages))
    }

    async fn list_organs(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let viewer = viewer_from_headers(&state, &headers).await.is_some();
        Ok(Json(organ_list(&state, viewer).await))
    }

    #[derive(Serialize)]
    struct GroupCardsResponse {
        workspace_name: String,
        cards: Vec<BoardCard>,
    }

    async fn get_local_group(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(filename): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let safe = std::path::Path::new(&filename)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Nome de grupo invalido.".to_string(),
                )
            })?;
        let path = crate::infrastructure::paths::sand_dir().join(safe);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|_| (StatusCode::NOT_FOUND, "Grupo nao encontrado.".to_string()))?;
        let imported = crate::domain::workspace_archive::parse_workspace_archive(safe, &bytes)
            .map_err(|message| (StatusCode::UNPROCESSABLE_ENTITY, message))?;
        Ok(Json(GroupCardsResponse {
            workspace_name: imported.workspace.name,
            cards: imported.workspace.cards,
        }))
    }

    #[derive(Serialize)]
    struct LocalPackagePreview {
        id: String,
        filename: String,
        icon: String,
        title: String,
        author: String,
        version: String,
        description: String,
        details: String,
        initial_width: u8,
        initial_height: u8,
        requires_server: bool,
        permissions: Vec<String>,
        html: String,
    }

    async fn get_local_package(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(package_id): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let package = state
            .packages
            .load(&package_id)
            .map_err(|message| (StatusCode::NOT_FOUND, message))?;
        let filename = package.archive_filename();
        let id = crate::domain::lince_package::package_id_from_filename(&filename);
        let manifest = package.manifest.clone();
        Ok(Json(LocalPackagePreview {
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
            html: package.html,
        }))
    }

    const DNA_EXTENSION_NAMESPACE: &str = "lince.dna";

    #[derive(Serialize)]
    struct DnaPreviewResponse {
        filename: String,
        title: String,
        version: String,
        author: String,
        description: String,
    }

    async fn multipart_file_field(
        multipart: &mut Multipart,
        field_name: &str,
    ) -> Result<(String, axum::body::Bytes), (StatusCode, String)> {
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?
        {
            if field.name() == Some(field_name) {
                let filename = field.file_name().unwrap_or("sand.html").to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
                return Ok((filename, bytes));
            }
        }
        Err((
            StatusCode::BAD_REQUEST,
            format!("missing `{field_name}` field"),
        ))
    }

    async fn preview_dna_package(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        mut multipart: Multipart,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let (filename, bytes) = multipart_file_field(&mut multipart, "file").await?;
        let package = crate::domain::lince_package::parse_lince_package(filename.clone(), &bytes)
            .map_err(|message| (StatusCode::UNPROCESSABLE_ENTITY, message))?;
        Ok(Json(DnaPreviewResponse {
            filename,
            title: package.manifest.title,
            version: package.manifest.version,
            author: package.manifest.author,
            description: package.manifest.description,
        }))
    }

    #[derive(Serialize)]
    struct DnaCatalogEntry {
        #[serde(rename = "organId")]
        organ_id: String,
        #[serde(rename = "originName")]
        origin_name: String,
        #[serde(rename = "recordId")]
        record_id: String,
        head: String,
        body: String,
        slug: Option<String>,
        version: String,
        #[serde(rename = "packageFormat")]
        package_format: String,
        categories: Vec<String>,
        #[serde(rename = "bucketKey")]
        bucket_key: String,
    }

    #[derive(Serialize)]
    struct DnaCatalogResponse {
        packages: Vec<DnaCatalogEntry>,
    }

    async fn dna_catalog(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let organ = store::organs::local(&state.store.pool)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let (organ_id, origin_name) = organ
            .map(|o| (o.uid, o.head))
            .unwrap_or_else(|| ("local".to_string(), "This organ".to_string()));
        let extensions = store::records::all_extensions(&state.store.pool, DNA_EXTENSION_NAMESPACE)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let mut packages = Vec::with_capacity(extensions.len());
        for (record_uid, fds) in extensions {
            let Some(record) = store::records::get(&state.store.pool, &record_uid)
                .await
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            else {
                continue;
            };
            packages.push(DnaCatalogEntry {
                organ_id: organ_id.clone(),
                origin_name: origin_name.clone(),
                record_id: record.uid,
                head: record.head,
                body: record.body,
                slug: record.slug,
                version: fds
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.1.0")
                    .to_string(),
                package_format: fds
                    .get("package_format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("html")
                    .to_string(),
                categories: fds
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                bucket_key: fds
                    .get("bucket_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
        packages.sort_by(|a, b| a.head.cmp(&b.head));
        Ok(Json(DnaCatalogResponse { packages }))
    }

    #[derive(Serialize)]
    struct DnaPublishResponse {
        #[serde(rename = "organId")]
        organ_id: String,
        version: String,
        #[serde(rename = "recordId")]
        record_id: String,
        slug: String,
        categories: Vec<String>,
        #[serde(rename = "bucketKey")]
        bucket_key: String,
        #[serde(rename = "sandTomlKey")]
        sand_toml_key: String,
    }

    async fn publish_dna_package(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        mut multipart: Multipart,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let mut head = String::new();
        let mut body = String::new();
        let mut categories_raw = String::new();
        let mut upload: Option<(String, axum::body::Bytes)> = None;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?
        {
            match field.name() {
                Some("head") => {
                    head = field
                        .text()
                        .await
                        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
                }
                Some("body") => {
                    body = field
                        .text()
                        .await
                        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
                }
                Some("categories") => {
                    categories_raw = field
                        .text()
                        .await
                        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
                }
                Some("file") => {
                    let filename = field.file_name().unwrap_or("sand.html").to_string();
                    let bytes = field
                        .bytes()
                        .await
                        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
                    upload = Some((filename, bytes));
                }
                _ => {}
            }
        }
        let head = head.trim().to_string();
        if head.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "record.head is required".into()));
        }
        let (filename, bytes) =
            upload.ok_or_else(|| (StatusCode::BAD_REQUEST, "missing `file` field".to_string()))?;
        let package = crate::domain::lince_package::parse_lince_package(filename, &bytes)
            .map_err(|message| (StatusCode::UNPROCESSABLE_ENTITY, message))?;

        let mut categories: Vec<String> = categories_raw
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect();
        if !categories.iter().any(|c| c == "sand") {
            categories.push("sand".to_string());
        }

        let slug = crate::slugify(&head);
        let version = package.manifest.version.clone();
        let prefix: String = {
            let compact: String = slug.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let mut chars = compact.chars();
            let first = chars.next().unwrap_or('x');
            let second = chars.next().unwrap_or(first);
            [first, second].into_iter().collect()
        };
        let package_format = if matches!(
            package.transport(),
            crate::domain::lince_package::PackageTransport::Archive
        ) {
            "lince"
        } else {
            "html"
        };
        let transport_filename = if package_format == "lince" {
            format!("{slug}.lince")
        } else {
            format!("{slug}_metadata.html")
        };
        let dir = crate::infrastructure::paths::dna_dir()
            .join(&prefix)
            .join(&slug)
            .join(&version);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let package_bytes = crate::domain::lince_package::build_lince_archive(&package)
            .map_err(|message| (StatusCode::UNPROCESSABLE_ENTITY, message))?;
        tokio::fs::write(dir.join(&transport_filename), &package_bytes)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let sand_toml_filename = "sand.toml".to_string();
        let manifest_toml = package
            .manifest_toml()
            .map_err(|message| (StatusCode::INTERNAL_SERVER_ERROR, message))?;
        tokio::fs::write(dir.join(&sand_toml_filename), manifest_toml.as_bytes())
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let bucket_key = format!("lince/dna/sand/{prefix}/{slug}/{version}/{transport_filename}");
        let sand_toml_key =
            format!("lince/dna/sand/{prefix}/{slug}/{version}/{sand_toml_filename}");

        let record = store::records::create(
            &state.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Sand,
                head: &head,
                body: &body,
                quantity: store::exact::zero(),
            },
        )
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        store::records::set_extension(
            &state.store.pool,
            &record.uid,
            DNA_EXTENSION_NAMESPACE,
            &serde_json::json!({
                "version": version,
                "categories": categories,
                "package_format": package_format,
                "bucket_key": bucket_key,
                "sand_toml_key": sand_toml_key,
                "slug": slug,
            }),
        )
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

        let organ = store::organs::local(&state.store.pool)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let organ_id = organ.map(|o| o.uid).unwrap_or_else(|| "local".to_string());

        Ok(Json(DnaPublishResponse {
            organ_id,
            version,
            record_id: record.uid,
            slug,
            categories,
            bucket_key,
            sand_toml_key,
        }))
    }

    async fn delete_dna_publication(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path((_organ_id, record_id)): Path<(String, String)>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        store::records::delete_extension(&state.store.pool, &record_id, DNA_EXTENSION_NAMESPACE)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        Ok(Json(serde_json::json!({ "deleted": true })))
    }

    async fn get_official_package_content(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path((filename, asset_path)): Path<(String, String)>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let packages = crate::sand::official_packages()
            .map_err(|message| (StatusCode::INTERNAL_SERVER_ERROR, message))?;
        let package = packages
            .into_iter()
            .find(|package| package.archive_filename() == filename)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    "Esse widget oficial nao existe.".into(),
                )
            })?;
        let content_root_url = format!(
            "/host/packages/local/by-filename/{}/content",
            urlencoding::encode(&filename)
        );
        crate::presentation::http::package_assets::serve_package_asset(
            &package,
            &asset_path,
            &content_root_url,
        )
        .map_err(|(status, Json(payload))| (status, payload.error))
    }

    async fn sand_asset(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(path): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let full = crate::infrastructure::paths::sand_dir().join(&path);
        let bytes = tokio::fs::read(&full)
            .await
            .map_err(|_| (StatusCode::NOT_FOUND, "sand asset not found".to_string()))?;
        let content_type = crate::cell_surface::guess_content_type(&path);
        Ok(([(header::CONTENT_TYPE, content_type)], bytes))
    }

    async fn upload_media(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        mut multipart: Multipart,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let mut bytes = None;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?
        {
            if field.name() == Some("file") {
                bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?,
                );
                break;
            }
        }
        let bytes =
            bytes.ok_or_else(|| (StatusCode::BAD_REQUEST, "missing `file` field".to_string()))?;
        let path = media_assets::store_media_bytes(&state.store.pool, &bytes).await?;
        Ok(Json(serde_json::json!({ "path": path })))
    }

    #[cfg(feature = "native-picker")]
    async fn pick_media(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let path = media_assets::pick_and_store_image(&state.store.pool).await?;
        Ok(Json(serde_json::json!({ "path": path })))
    }

    async fn get_storage(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let media = directory_bytes(crate::infrastructure::paths::media_dir()).await;
        let dna = directory_bytes(crate::infrastructure::paths::dna_dir()).await;
        let usage = store::budget::usage(&state.store.pool, media, dna, None)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        Ok(Json(serde_json::json!({
            "total_bytes": usage.total_bytes,
            "on_disk_bytes": usage.on_disk_bytes(),
            "unbudgeted_bytes": usage.unbudgeted_bytes,
            "areas": usage.areas.iter().map(|area| serde_json::json!({
                "name": area.area.name(),
                "used_bytes": area.used_bytes,
                "limit_bytes": area.limit_bytes,
                "live": area.live,
            })).collect::<Vec<_>>(),
        })))
    }

    async fn vault_body(
        state: &CellApiState,
        body: &serde_json::Value,
    ) -> Result<(String, String, String), (StatusCode, String)> {
        let record_uid = body
            .get("record_uid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let password = body
            .get("password")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if record_uid.is_empty() || password.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "a Record and a password are both needed".to_string(),
            ));
        }
        let record = store::records::get(&state.store.pool, &record_uid)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "no such Record".to_string()))?;
        Ok((record_uid, password, record.body))
    }

    async fn lock_vault(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        let actor = authenticate_headers(&state, &headers).await?;
        let (record_uid, password, stored) = vault_body(&state, &body).await?;
        let description = match body.get("description").and_then(serde_json::Value::as_str) {
            Some(text) => text.to_string(),
            None => {
                if utils::vault::is_locked(&stored) {
                    return Err((
                        StatusCode::CONFLICT,
                        "this description is already locked".to_string(),
                    ));
                }
                stored
            }
        };
        let locked = utils::vault::lock(&record_uid, &password, &description)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        state
            .engine
            .act(
                engine::actions::Action::EditRecordText {
                    target: record_uid.clone(),
                    head: None,
                    body: Some(locked),
                },
                actor,
            )
            .await
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
        Ok(Json(serde_json::json!({
            "record_uid": record_uid,
            "locked": true,
            "older_revisions_keep_their_old_password": true,
        })))
    }

    async fn unlock_vault(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let (record_uid, password, stored) = vault_body(&state, &body).await?;
        let description =
            utils::vault::unlock(&record_uid, &password, &stored).map_err(|error| match error {
                utils::vault::VaultError::NotAVault => (StatusCode::CONFLICT, error.to_string()),
                utils::vault::VaultError::Unopenable => (StatusCode::FORBIDDEN, error.to_string()),
            })?;
        Ok(Json(serde_json::json!({
            "record_uid": record_uid,
            "description": description,
        })))
    }
    async fn get_record_changes(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(record_uid): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let changes = store::record_changes::recent(
            &state.store.pool,
            &record_uid,
            store::record_changes::MAX_PER_RECORD,
        )
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        Ok(Json(serde_json::json!({
            "record_uid": record_uid,
            "retention_days": store::record_changes::RETENTION_DAYS,
            "changes": changes.iter().map(|change| serde_json::json!({
                "field": change.field,
                "cause": change.cause.as_str(),
                "winner_organ": change.winner_organ,
                "displaced": change.displaced,
                "mine": change.displaced_local,
                "at": change.at,
            })).collect::<Vec<_>>(),
        })))
    }

    async fn set_storage_budget(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let bytes = body
            .get("bytes")
            .and_then(serde_json::Value::as_i64)
            .ok_or((
                StatusCode::BAD_REQUEST,
                "bytes must be a whole number".to_string(),
            ))?;
        store::budget::set_total(&state.store.pool, bytes)
            .await
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
        media_assets::enforce_budget(&state.store.pool).await?;
        Ok(Json(serde_json::json!({ "total_bytes": bytes })))
    }

    async fn directory_bytes(dir: std::path::PathBuf) -> i64 {
        tokio::task::spawn_blocking(move || {
            fn walk(dir: &std::path::Path) -> i64 {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    return 0;
                };
                entries
                    .flatten()
                    .map(|entry| match entry.file_type() {
                        Ok(kind) if kind.is_dir() => walk(&entry.path()),
                        Ok(_) => entry
                            .metadata()
                            .map(|meta| i64::try_from(meta.len()).unwrap_or(i64::MAX))
                            .unwrap_or(0),
                        Err(_) => 0,
                    })
                    .sum()
            }
            walk(&dir)
        })
        .await
        .unwrap_or(0)
    }

    async fn get_media(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(name): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        if !media_assets::valid_media_filename(&name) {
            return Err((
                StatusCode::BAD_REQUEST,
                "invalid media filename".to_string(),
            ));
        }
        let path = crate::infrastructure::paths::media_dir().join(&name);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|_| (StatusCode::NOT_FOUND, "image not found".to_string()))?;
        media_assets::touch(path).await;
        let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(media_assets::content_type_for_ext(ext)),
        );
        response_headers.insert(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        );
        Ok((response_headers, bytes))
    }

    async fn organ_nearby(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let mut peers = Vec::new();
        if let Some(wire) = state.wire.read().await.clone() {
            for peer in wire.nearby().current() {
                let contact = store::organs::contact_by_node_id(&state.store.pool, &peer.node_id)
                    .await
                    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
                peers.push(serde_json::json!({
                    "fp": peer.fingerprint,
                    "node_id": peer.node_id,
                    "name": contact
                        .as_ref()
                        .map(|c| c.head.clone())
                        .unwrap_or_else(|| peer.name.clone()),
                    "claimed_name": peer.name,
                    "organ_uid": contact.as_ref().map(|c| c.record_uid.clone()),
                    "known": contact.is_some(),
                }));
            }
        }
        Ok(Json(serde_json::json!({ "peers": peers })))
    }

    async fn sign_key_succession(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let new_key = body
            .get("new_key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if new_key.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Informe a chave publica nova.".to_string(),
            ));
        }
        let root = state
            .engine
            .root_signer()
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .ok_or((
                StatusCode::CONFLICT,
                "A chave raiz nao esta nesta Cell. Traga-a de volta para assinar a sucessao."
                    .to_string(),
            ))?;
        if new_key == root.public_key_b64() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Essa ja e a chave atual.".to_string(),
            ));
        }
        state
            .engine
            .sign_succession(&root, &new_key)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        Ok(Json(serde_json::json!({
            "old_key": root.public_key_b64(),
            "new_key": new_key,
        })))
    }

    const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

    async fn qr_decode(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        frame: axum::body::Bytes,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        if frame.len() > MAX_FRAME_BYTES {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "frame too large".into()));
        }
        let text = engine::pairing::decode_qr(&frame)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
        Ok(Json(serde_json::json!({ "text": text })))
    }

    #[derive(Deserialize)]
    struct PairRequest {
        node_id: String,
        #[serde(default)]
        name: String,
    }

    async fn organ_pair(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(request): Json<PairRequest>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let wire = state.wire.read().await.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "no iroh endpoint".to_string(),
            )
        })?;
        let invite = engine::pairing::PairingInvite::decode(&request.node_id).unwrap_or(
            engine::pairing::PairingInvite {
                node_id: request.node_id.trim().to_string(),
                root_key: None,
                label: None,
                addrs: Vec::new(),
            },
        );
        let organ_uid = wire
            .pair_with(&invite, &request.name)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        let intro_head = store::records::get(&state.store.pool, &organ_uid)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .map(|record| record.head)
            .unwrap_or_default();
        let code = state
            .engine
            .pairing_code(&organ_uid)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .unwrap_or_default();
        Ok(Json(serde_json::json!({
            "organ_uid": organ_uid,
            "head": intro_head,
            "code": code,
        })))
    }

    #[derive(Deserialize)]
    struct ConversationOfferRequest {
        node_id: String,
        title: String,
    }

    async fn offer_nearby_conversation(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(request): Json<ConversationOfferRequest>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let title = request.title.trim();
        if title.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "conversation title is required".into(),
            ));
        }
        let wire = state.wire.read().await.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "no iroh endpoint".to_string(),
            )
        })?;
        let (conversation, thread) = wire
            .offer_conversation_to_node(request.node_id.trim(), title)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        Ok(Json(serde_json::json!({
            "conversation": conversation,
            "thread": thread,
        })))
    }

    #[derive(Deserialize)]
    struct ReferenceReadRequest {
        owner: String,
        root: String,
        record: String,
    }

    async fn read_reference(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(request): Json<ReferenceReadRequest>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let wire = state.wire.read().await.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "no iroh endpoint".to_string(),
            )
        })?;
        match wire
            .fetch_reference(
                request.owner.trim(),
                request.root.trim(),
                request.record.trim(),
            )
            .await
        {
            Ok(row) => Ok(Json(serde_json::json!({ "row": row, "live": true }))),
            Err(error) => {
                let message = error.to_string();
                let refused =
                    message.contains("no longer shared") || message.contains("no accepted grant");
                Err((
                    if refused {
                        StatusCode::FORBIDDEN
                    } else {
                        StatusCode::BAD_GATEWAY
                    },
                    message,
                ))
            }
        }
    }

    async fn organ_open_promises(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        let subject = headers
            .get("x-lince-organ")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "missing X-Lince-Organ".into()))?;
        state
            .engine
            .open_promise_export(subject)
            .await
            .map(Json)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
    }

    #[derive(serde::Deserialize)]
    struct RemoteSessionRequest {
        username: String,
        password: String,
    }

    #[derive(serde::Deserialize)]
    struct InviteSessionRequest {
        invite: String,
        username: String,
        password: String,
        #[serde(default)]
        name: String,
    }

    async fn open_invite_session(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Json(request): Json<InviteSessionRequest>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let wire = state.wire.read().await.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "no iroh endpoint".to_string(),
            )
        })?;
        let raw = request.invite.trim();
        if raw.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Paste the code that Lince gave you.".to_string(),
            ));
        }
        let invite =
            engine::pairing::PairingInvite::decode(raw).unwrap_or(engine::pairing::PairingInvite {
                node_id: raw.to_string(),
                root_key: None,
                label: None,
                addrs: Vec::new(),
            });
        let login = live_proxy::RemoteLogin {
            username: request.username.trim().to_string(),
            password: request.password.clone(),
        };
        let organ = live_proxy::login_with_invite(wire, &invite, &login)
            .await
            .map_err(|message| (StatusCode::UNAUTHORIZED, message))?;

        let name = if !request.name.trim().is_empty() {
            request.name.trim().to_string()
        } else {
            invite
                .label
                .clone()
                .unwrap_or_else(|| "Lince remota".to_string())
        };
        store::organs::add_contact(&state.store.pool, &organ, None, &name, "", 0)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        store::organs::set_node_id(&state.store.pool, &organ, Some(&invite.node_id))
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        state
            .remote_logins
            .write()
            .await
            .insert(organ.clone(), login.clone());
        Ok(Json(serde_json::json!({
            "organ": organ,
            "name": name,
            "username": login.username,
            "authenticated": true,
        })))
    }

    async fn open_remote_session(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(organ): Path<String>,
        Json(request): Json<RemoteSessionRequest>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let wire = state.wire.read().await.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "no iroh endpoint".to_string(),
            )
        })?;
        let login = live_proxy::RemoteLogin {
            username: request.username.trim().to_string(),
            password: request.password.clone(),
        };
        live_proxy::verify_login(wire, &organ, &login)
            .await
            .map_err(|message| (StatusCode::UNAUTHORIZED, message))?;
        state
            .remote_logins
            .write()
            .await
            .insert(organ.clone(), login.clone());
        Ok(Json(serde_json::json!({
            "organ": organ,
            "username": login.username,
            "authenticated": true,
        })))
    }

    async fn close_remote_session(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(organ): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        state.remote_logins.write().await.remove(&organ);
        Ok(Json(
            serde_json::json!({ "organ": organ, "authenticated": false }),
        ))
    }

    async fn live_connect(
        ws: WebSocketUpgrade,
        State(state): State<CellApiState>,
        Path(organ): Path<String>,
        headers: HeaderMap,
    ) -> Response {
        if let Err((status, message)) = authenticate_headers(&state, &headers).await {
            return (status, message).into_response();
        }
        let Some(wire) = state.wire.read().await.clone() else {
            return (StatusCode::SERVICE_UNAVAILABLE, "no iroh endpoint").into_response();
        };
        let login = state.remote_logins.read().await.get(&organ).cloned();
        ws.on_upgrade(move |socket| async move {
            if let Err(failure) =
                crate::presentation::http::live_proxy::relay(wire, organ, login, socket).await
            {
                tracing::warn!(%failure, "live relay ended");
            }
        })
    }

    async fn connect(
        ws: WebSocketUpgrade,
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Response {
        match authenticate_headers(&state, &headers).await {
            Ok(subject) => ws.on_upgrade(move |socket| async move {
                handle_cell_api_socket(state, subject, socket).await;
            }),
            Err((status, message)) => (status, message).into_response(),
        }
    }

    async fn handle_cell_api_socket(
        state: CellApiState,
        subject: Option<String>,
        socket: WebSocket,
    ) {
        let connection_id = nucleus::new_uid("conn");
        transport::ws::serve(state.engine, state.lanes, connection_id, subject, socket).await;
    }

    let listen_addr = listen_addr
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_WEB_LISTEN_ADDR.to_string());
    let address = listen_addr.parse::<SocketAddr>().map_err(|error| {
        IoError::other(format!("Invalid listen address `{listen_addr}`: {error}"))
    })?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| {
            if error.kind() == ErrorKind::AddrInUse {
                IoError::new(
                    ErrorKind::AddrInUse,
                    format!("Address {address} is already in use"),
                )
            } else {
                IoError::new(error.kind(), format!("Failed to bind {address}: {error}"))
            }
        })?;
    let local_addr = listener.local_addr().map_err(IoError::other)?;

    let packages = PackageCatalogStore::new().map_err(IoError::other)?;
    let cell_store = Store::open(&default_lince_db_url())
        .await
        .map_err(IoError::other)?;
    media_assets::enforce_budget(&cell_store.pool)
        .await
        .map_err(|(_, message)| IoError::other(message))?;
    let local_base_url = local_base_url_from_socket_addr(local_addr);
    crate::cell_bootstrap::bootstrap_cell(
        &cell_store,
        local_auth_required,
        &local_base_url,
        staged_setup.as_ref(),
        mode == HttpServeMode::ApiOnly,
    )
    .await?;
    let engine = Arc::new(
        engine::Engine::new(cell_store.clone())
            .await
            .map_err(IoError::other)?,
    );
    let local_organ = store::organs::local(&cell_store.pool)
        .await
        .map_err(IoError::other)?
        .ok_or_else(|| IoError::other("local Organ was not initialized"))?;
    let key_dir = utils::config::lince_data_dir().unwrap_or_else(|| PathBuf::from("."));
    let this_cell = store::cells::local(&cell_store.pool)
        .await
        .map_err(IoError::other)?
        .ok_or_else(|| IoError::other("this Cell has no Cell Record"))?;
    let _karma_director = match start_karma(&engine, &this_cell.uid) {
        Ok(handle) => Some(handle),
        Err(error) => {
            tracing::warn!(%error, "Karma schedules will not fire on this Cell");
            None
        }
    };
    let organ_signer = engine::trust::Signer::load_or_create(
        &key_dir.join("keys").join("organ-ed25519-v1.key"),
        &local_organ.uid,
        &engine::roster::cell_key_id(&this_cell.uid),
    )
    .map_err(IoError::other)?;
    engine
        .set_organ_signer(organ_signer)
        .await
        .map_err(IoError::other)?;
    let wire = match engine::wire::node_secret(&key_dir.join("keys").join("node-ed25519-v1.key"))
        .map_err(IoError::other)
    {
        Ok(secret) => {
            let reach = discovery_reach(&cell_store, &local_organ.uid).await;
            match engine::wire::Wire::bind_with_discovery(
                engine.clone(),
                secret,
                reach,
                Some(local_organ.head.as_str()),
                discovery_is_local(&cell_store, &local_organ.uid).await,
            )
            .await
            {
                Ok(wire) => Some(Arc::new(wire)),
                Err(error) => {
                    tracing::warn!(%error, "iroh endpoint unavailable; peers unreachable");
                    None
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, "no node key; peers unreachable");
            None
        }
    };
    let lanes = Arc::new(LaneHub::new());
    if let Some(wire) = wire.clone() {
        wire.set_live_handler(transport::live::LiveHost::new(
            engine.clone(),
            lanes.clone(),
        ));
        wire.serve_enrolment();
        tokio::spawn(async move { wire.serve().await });
    }
    let wire_slot: crate::presentation::http::wire_supervisor::WireSlot =
        Arc::new(tokio::sync::RwLock::new(wire.clone()));
    if let Some(wire) = wire.clone() {
        if let Ok(invite) = wire.pairing_invite().await {
            let encoded = invite.encode();
            let existing =
                store::records::get_extension(&cell_store.pool, &local_organ.uid, "lince.pairing")
                    .await
                    .ok()
                    .flatten();
            let unchanged = existing
                .as_ref()
                .and_then(|fields| fields.get("invite").and_then(serde_json::Value::as_str))
                == Some(encoded.as_str());
            if !unchanged {
                let svg = invite.qr_svg().unwrap_or_default();
                let _ = store::records::set_extension(
                    &cell_store.pool,
                    &local_organ.uid,
                    "lince.pairing",
                    &serde_json::json!({ "invite": encoded, "qr_svg": svg }),
                )
                .await;
            }
        }
    }
    if let Err(error) =
        publish_local_roster(&engine, &local_organ.uid, &key_dir, wire.as_deref()).await
    {
        tracing::warn!(%error, "cannot publish the Cell roster");
    }
    let _file_sync_supervisor = engine::file_sync::spawn_supervisor(engine.clone());
    let _heartbeat = engine.clone().run(HEARTBEAT_PERIOD_SECS);
    let state = CellApiState {
        board_state: BoardStateStore::new().map_err(IoError::other)?,
        engine,
        jwt_secret: Arc::new(jwt_secret),
        lanes: lanes.clone(),
        listening_port: local_addr.port(),
        local_auth_required,
        wire: wire_slot,
        packages,
        store: cell_store,
        remote_logins: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    };

    let static_dir = crate::infrastructure::paths::static_dir();
    let router = axum::Router::new()
        .route("/api/auth/login", post(login))
        .route("/auth/login", post(login))
        .route("/host/auth/login", post(login))
        .route("/organ", get(list_organs))
        .route("/organ/session", post(open_invite_session))
        .route(
            "/organ/{organ}/session",
            post(open_remote_session).delete(close_remote_session),
        )
        .route(
            "/host/board/state",
            get(get_board_state).put(put_board_state),
        )
        .route(
            "/host/board/workspaces/{workspace_id}/export",
            get(export_workspace),
        )
        .route("/host/notifications", get(list_notifications))
        .route(
            "/host/notifications/{notification_id}/{answer}",
            post(answer_thread_notification),
        )
        .route("/host/packages/local", get(list_local_packages))
        .route(
            "/host/packages/local/group/{filename}",
            get(get_local_group),
        )
        .route("/host/packages/local/{package_id}", get(get_local_package))
        .route(
            "/host/packages/local/by-filename/{filename}/content/{*asset_path}",
            get(get_official_package_content),
        )
        .route("/host/packages/preview", post(preview_dna_package))
        .route("/host/packages/dna/catalog", get(dna_catalog))
        .route("/host/packages/dna/publish", post(publish_dna_package))
        .route(
            "/host/packages/dna/publications/{organ_id}/{record_id}",
            axum::routing::delete(delete_dna_publication),
        )
        .route("/host/media", post(upload_media))
        .route("/host/media/{name}", get(get_media))
        .route(
            "/host/records/{record_uid}/changes",
            get(get_record_changes),
        )
        .route("/host/storage", get(get_storage))
        .route("/host/vault/lock", post(lock_vault))
        .route("/host/vault/unlock", post(unlock_vault))
        .route("/host/storage/budget", post(set_storage_budget))
        .route("/organ/nearby", get(organ_nearby))
        .route("/organ/pair", post(organ_pair))
        .route("/organ/conversation/offer", post(offer_nearby_conversation))
        .route("/organ/reference/read", post(read_reference))
        .route("/organ/qr-decode", post(qr_decode))
        .route("/organ/identity/succession", post(sign_key_succession))
        .route("/organ/open-promises", get(organ_open_promises))
        .route("/host/transport/ws", get(connect));
    #[cfg(feature = "native-picker")]
    let router = router.route("/host/media/pick", post(pick_media));

    let serve_ui = mode == HttpServeMode::FullUi;
    let router = if serve_ui {
        router
            .route("/", get(index))
            .route("/favicon.ico", get(static_assets::favicon))
            .route("/board/frame.js", get(static_assets::frame_js))
            .route("/board/editor.js", get(static_assets::editor_js))
            .route("/board/lynx-ui.css", get(static_assets::lynx_ui_css))
            .route("/board/lynx-ui.js", get(static_assets::lynx_ui_js))
            .route("/board/vault.js", get(static_assets::vault_js))
            .route(
                "/board/collab-editor.js",
                get(static_assets::collab_editor_js),
            )
            .route("/board/vendor/d3.v7.min.js", get(static_assets::d3_js))
            .route(
                "/board/vendor/d3.LICENSE.txt",
                get(static_assets::d3_license),
            )
            .route(
                "/board/vendor/mermaid.min.js",
                get(static_assets::mermaid_js),
            )
            .route(
                "/board/vendor/mermaid.LICENSE.txt",
                get(static_assets::mermaid_license),
            )
            .route(
                "/board/vendor/loro-index.js",
                get(static_assets::loro_index_js),
            )
            .route(
                "/board/vendor/loro_wasm.js",
                get(static_assets::loro_wasm_js),
            )
            .route(
                "/board/vendor/loro_wasm_bg.wasm",
                get(static_assets::loro_wasm_bg),
            )
            .route(
                "/board/vendor/loro.LICENSE.txt",
                get(static_assets::loro_license),
            )
            .route("/sand/{*path}", get(sand_asset))
            .route("/live/{organ}/connect", get(live_connect))
    } else {
        router
    };

    let router = router.with_state(state.clone());
    let app = if !serve_ui {
        router
    } else if static_dir.exists() {
        router
            .nest_service("/static", ServeDir::new(&static_dir))
            .nest_service("/host/static", ServeDir::new(&static_dir))
    } else {
        router
            .route("/static/{*path}", get(static_assets::serve))
            .route("/host/static/{*path}", get(static_assets::serve))
    };

    let app = app.layer(axum::middleware::map_response(
        |mut response: Response| async move {
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, must-revalidate"),
            );
            response
        },
    ));

    if let Some(sender) = bound_addr_sender {
        let _ = sender.send(local_addr);
    }
    if let Some(wire) = state.wire.read().await.clone() {
        wire.set_transfer_handler(Arc::new(
            crate::presentation::http::transfer_delivery::TransferPeerHandler::new(state.clone()),
        ));
    }
    crate::presentation::http::transfer_delivery::spawn_worker(state.clone());
    crate::presentation::http::sync_runner::spawn_runner(state.clone());
    crate::presentation::http::wire_supervisor::spawn(state.clone(), key_dir.clone());
    status(match mode {
        HttpServeMode::FullUi => format!("Cell API listening at http://{local_addr}"),
        HttpServeMode::ApiOnly => format!(
            "Cell API listening at http://{local_addr} (server mode: no board UI, login required)"
        ),
    });
    axum::serve(listener, app).await.map_err(IoError::other)
}

fn start_karma(
    engine: &Arc<engine::Engine>,
    cell_uid: &str,
) -> Result<tokio::task::JoinHandle<()>, engine::EngineError> {
    let config = engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(format!(
        "{cell_uid}:{}",
        std::process::id()
    ))?;
    engine.install_karma_runtime_config(config.clone())?;
    let director = engine.clone().start_karma_deadline_director(config);
    Ok(tokio::spawn(async move {
        match director.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "Karma deadline director stopped"),
            Err(error) => tracing::warn!(%error, "Karma deadline director panicked"),
        }
    }))
}

async fn publish_local_roster(
    engine: &engine::Engine,
    organ_uid: &str,
    key_dir: &std::path::Path,
    wire: Option<&engine::wire::Wire>,
) -> Result<(), IoError> {
    let root_path = key_dir.join("keys").join("root-ed25519-v1.key");
    let held = engine.roster_of(organ_uid).await.map_err(IoError::other)?;

    engine.set_root_key_path(root_path.clone());
    engine.set_sealing_keyring_path(key_dir.join("keys").join("cell-x25519-keyring-v1.json"));
    let creating = !root_path.exists();
    if creating {
        if held.is_some() {
            tracing::info!(
                "root key is not on this Cell; the published roster stays valid \
                 until it expires, and enrolling or revoking a device needs it back"
            );
            return Ok(());
        }
    }
    let root =
        engine::trust::Signer::load_or_create(&root_path, organ_uid, engine::roster::ROOT_KEY_ID)
            .map_err(IoError::other)?;
    if creating {
        let (revoked_key, signature) = engine.revocation_certificate(&root);
        let certificate = serde_json::json!({
            "organ_uid": organ_uid,
            "revoked_key": revoked_key,
            "signature": signature,
        });
        let certificate_path = root_path.with_extension("revocation.json");
        if let Err(error) = std::fs::write(
            &certificate_path,
            serde_json::to_vec_pretty(&certificate).map_err(IoError::other)?,
        ) {
            tracing::warn!(%error, "could not write the pre-signed revocation certificate");
        } else {
            tracing::info!(
                path = %certificate_path.display(),
                "wrote the pre-signed revocation certificate; keep it with the root key, offline"
            );
        }
    }
    engine
        .publish_root_key(&root)
        .await
        .map_err(IoError::other)?;

    let Some(wire) = wire else {
        return Ok(());
    };

    let node_id = wire.node_id().to_string();
    let cell = store::cells::local(&engine.store.pool)
        .await
        .map_err(IoError::other)?
        .ok_or_else(|| IoError::other("this Cell has no Cell Record"))?;
    let operational_key = engine
        .local_organ_public_key()
        .await
        .map_err(IoError::other)?
        .unwrap_or_default();
    let mut cells: Vec<engine::roster::CellEntry> = held
        .as_ref()
        .map(|held| held.roster.cells.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|held_cell| held_cell.cell_uid != cell.uid)
        .collect();
    let sealing_key = engine
        .published_sealing_key()
        .await
        .map_err(IoError::other)?;
    cells.push(engine::roster::CellEntry {
        cell_uid: cell.uid.clone(),
        node_id,
        label: cell.label.clone(),
        operational_key,
        sealing_key,
        capabilities: engine::roster::full_capabilities(),
        front_door: wire.reach() != engine::wire::Reach::Local,
    });
    if engine::roster::needs_publishing(held.as_ref(), &root.public_key_b64(), &cells) {
        engine
            .publish_roster(&root, cells)
            .await
            .map_err(IoError::other)?;
    }
    if let Err(error) = engine.sign_public_record(&root).await {
        tracing::warn!(%error, "could not sign the public directory record");
    }
    republish_public_record(engine, organ_uid).await;
    Ok(())
}

async fn republish_public_record(engine: &engine::Engine, organ_uid: &str) {
    match engine.republish_public_record(organ_uid).await {
        Ok(true) => tracing::info!("published this Organ's front door under its identity key"),
        Ok(false) => {}
        Err(error) => tracing::warn!(%error, "could not publish the directory record"),
    }
}

pub(crate) async fn discovery_reach(store: &Store, organ_uid: &str) -> engine::wire::Reach {
    if !internet_default() {
        return engine::wire::Reach::Local;
    }
    let fields = discovery_config(store, organ_uid).await;
    let reachable = fields
        .as_ref()
        .and_then(|fields| fields.get("internet").and_then(serde_json::Value::as_bool))
        .unwrap_or(true);
    if !reachable {
        return engine::wire::Reach::Local;
    }
    let direct = fields
        .as_ref()
        .and_then(|fields| fields.get("direct").and_then(serde_json::Value::as_bool))
        .unwrap_or(false);
    if direct {
        engine::wire::Reach::Internet
    } else {
        engine::wire::Reach::Relay
    }
}

pub(crate) async fn discovery_config(store: &Store, organ_uid: &str) -> Option<serde_json::Value> {
    if let Ok(Some(fields)) = store::cells::config(&store.pool, "lince.discovery").await {
        return Some(fields);
    }
    store::records::get_extension(&store.pool, organ_uid, "lince.discovery")
        .await
        .ok()
        .flatten()
}

fn internet_default() -> bool {
    !matches!(
        std::env::var("LINCE_DISCOVERY_INTERNET").as_deref(),
        Ok("0") | Ok("false") | Ok("no")
    )
}

pub(crate) async fn discovery_is_local(store: &Store, organ_uid: &str) -> bool {
    let Some(fields) = discovery_config(store, organ_uid).await else {
        return false;
    };
    let on = fields
        .get("local")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !on {
        return false;
    }
    match fields
        .get("local_until")
        .and_then(serde_json::Value::as_str)
    {
        Some(until) => chrono::DateTime::parse_from_rfc3339(until)
            .map(|when| when.with_timezone(&chrono::Utc) > chrono::Utc::now())
            .unwrap_or(false),
        None => true,
    }
}

fn local_base_url_from_socket_addr(address: SocketAddr) -> String {
    let host = if address.ip().is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        address.ip().to_string()
    };
    format!("http://{host}:{}", address.port())
}

pub fn default_lince_db_url() -> String {
    let dir = utils::config::lince_data_dir().unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    format!("sqlite://{}", dir.join("lince.db").display())
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    async fn cell() -> (Store, String) {
        let store = Store::open_memory().await.expect("store");
        let organ = store::organs::ensure_local(&store.pool, "http://d.test")
            .await
            .expect("organ")
            .uid;
        (store, organ)
    }

    #[tokio::test]
    async fn defaults_are_relay_only_and_lan_silent() {
        let (store, organ) = cell().await;
        assert_eq!(
            discovery_reach(&store, &organ).await,
            engine::wire::Reach::Relay,
            "reachable, but never publishing this machine's own address"
        );
        assert!(
            !discovery_is_local(&store, &organ).await,
            "announcing yourself to a room is a disclosure, not a default"
        );
    }

    #[tokio::test]
    async fn direct_connections_are_opt_in() {
        let (store, organ) = cell().await;
        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "internet": true, "direct": true }),
        )
        .await
        .expect("config");
        assert_eq!(
            discovery_reach(&store, &organ).await,
            engine::wire::Reach::Internet
        );

        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "internet": false }),
        )
        .await
        .expect("config");
        assert_eq!(
            discovery_reach(&store, &organ).await,
            engine::wire::Reach::Local,
            "switching internet off publishes nothing at all"
        );
    }

    #[tokio::test]
    async fn lan_presence_lapses_on_its_own() {
        let (store, organ) = cell().await;
        let future = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "local": true, "local_until": future }),
        )
        .await
        .expect("config");
        assert!(
            discovery_is_local(&store, &organ).await,
            "on while it lasts"
        );

        let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "local": true, "local_until": past }),
        )
        .await
        .expect("config");
        assert!(
            !discovery_is_local(&store, &organ).await,
            "and off afterwards, whether or not anyone remembered"
        );

        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "local": true, "local_until": "soon-ish" }),
        )
        .await
        .expect("config");
        assert!(!discovery_is_local(&store, &organ).await);
    }

    #[tokio::test]
    async fn cell_config_wins_over_the_organ_record() {
        let (store, organ) = cell().await;
        store::records::set_extension(
            &store.pool,
            &organ,
            "lince.discovery",
            &serde_json::json!({ "internet": true, "direct": true }),
        )
        .await
        .expect("organ extension");
        assert_eq!(
            discovery_reach(&store, &organ).await,
            engine::wire::Reach::Internet,
            "the old location still answers when nothing newer exists"
        );

        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "internet": true, "direct": false }),
        )
        .await
        .expect("cell config");
        assert_eq!(
            discovery_reach(&store, &organ).await,
            engine::wire::Reach::Relay,
            "and this device's own answer wins once it has one"
        );
    }
}
