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
        presentation::http::{media_assets, static_assets},
    },
    std::{
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

/// Whether this process serves the board UI, or only the API a logged-in
/// client talks to.
///
/// `ApiOnly` is the headless-server posture (`lince --server`): a box that
/// holds data and answers authenticated clients, but hands nobody a board.
/// Without it, anyone who can reach the port opens `/`, gets a full working
/// board backed by the server's own store, and drops sands onto it.
///
/// This is an HTTP-surface switch only. The iroh ALPNs (`lince/sync/1`,
/// `lince/thread/1`, `lince/live/1`) authenticate contacts by Organ identity,
/// which is a different system from local users — gating those would break
/// peer sync, the very reason to run a server.
///
/// `ApiOnly` is meaningless unless local auth is on: `authenticate_headers`
/// is a no-op when `local_auth_required` is false, so removing the board
/// while leaving `/host/transport/ws` open would still hand any network peer
/// an unauthenticated `act()` surface — hardening in looks only. The `lince`
/// CLI therefore forces auth on whenever `--server` is passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpServeMode {
    FullUi,
    ApiOnly,
}

/// How often the organism takes a beat.
///
/// This is the delivery resolution for every declared schedule: a cadence can
/// be written in milliseconds, but a polled heartbeat cannot deliver one on
/// time. Sixty seconds is right for habits and bills, which is what rules are
/// for today; finer delivery is the tickless deadline fabric, not a smaller
/// number here.
const HEARTBEAT_PERIOD_SECS: u64 = 60;

#[derive(Clone)]
struct CellApiState {
    board_state: BoardStateStore,
    engine: Arc<engine::Engine>,
    jwt_secret: Arc<String>,
    lanes: Arc<LaneHub>,
    listening_port: u16,
    local_auth_required: bool,
    /// The iroh endpoint (Ontology §11 "Transport: iroh"): peer connectivity
    /// and the LAN nearby list. `None` when binding failed — the Cell still
    /// serves its own board, it just cannot reach or be reached by peers.
    wire: crate::presentation::http::wire_supervisor::WireSlot,
    packages: PackageCatalogStore,
    store: Store,
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

    /// Best-effort viewer resolution for the SSR bootstrap: unlike
    /// `authenticate_headers`, never errors — the page must render whether or
    /// not the visitor is logged in (there is no separate login page to fall
    /// back to). Missing/invalid/stale tokens and no-auth-required Cells all
    /// resolve to `None` (no viewer identity to show), never a hard failure.
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
        }
    }

    async fn local_server_bootstrap(state: &CellApiState) -> Vec<ServerBootstrap> {
        store::organs::local(&state.store.pool)
            .await
            .ok()
            .flatten()
            .map(|organ| server_bootstrap_from_organ(organ, state.local_auth_required))
            .into_iter()
            .collect()
    }

    async fn index(State(state): State<CellApiState>, headers: HeaderMap) -> impl IntoResponse {
        let board_state = state.board_state.snapshot().await;
        let servers = local_server_bootstrap(&state).await;
        let viewer = viewer_from_headers(&state, &headers).await;
        let bootstrap = AppBootstrap::new(
            WidgetBridgeSnapshot::default(),
            board_state,
            servers,
            AppRuntimeInfo {
                port: state.listening_port,
                version: env!("CARGO_PKG_VERSION"),
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
        if !password_valid {
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
        let invites = store::invites::pending(&state.store.pool)
            .await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let notifications = invites
            .into_iter()
            .map(|invite| {
                serde_json::json!({
                    "id": invite.record_uid,
                    "kind": "thread_invite",
                    "title": "Conversation request",
                    "body": format!("{} wants to start an individual synced conversation.", invite.from_organ),
                    "recordId": invite.root,
                    "organId": invite.from_organ,
                })
            })
            .collect::<Vec<_>>();
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
            // Pull the accepted root immediately so the Record sand can open
            // it from this response instead of waiting for the next cycle.
            wire.sync_once()
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        }
        Ok(Json(
            serde_json::json!({ "record_id": root, "accepted": accept }),
        ))
    }

    /// Lists the sands installed under `<lince_data_dir>/web/sand/` for the
    /// "Catálogo de widgets". Mirrors the FullUi `/host/packages/local`
    /// handler so both surfaces return the same catalog; the cell path used to
    /// stub this to an empty list, hiding every on-disk sand.
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
        Ok(Json(local_server_bootstrap(&state).await))
    }

    #[derive(Serialize)]
    struct GroupCardsResponse {
        workspace_name: String,
        cards: Vec<BoardCard>,
    }

    /// Returns a sand GROUP's member cards (with their relative layout, z-order,
    /// group ids, ABI listen topics, and sand HTML) so the client can drop the
    /// whole group onto the board at once (Stage 8b, base task 2: kanban adds as
    /// a group). Reads the installed `.lince` group archive from the sand dir.
    async fn get_local_group(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path(filename): Path<String>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        // Sanitize: only a bare filename inside the sand dir, no path traversal.
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

    /// Returns a single installed sand's preview (its HTML + manifest metadata)
    /// so the client's "add" flow can build a card from it. The cell path had
    /// only `list` + `content`; without this, clicking a sand in the catalog
    /// fetched a missing route and silently failed to add.
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

    // ---- DNA publish/catalog (2026-08-07) ----------------------------------
    //
    // No bucket/object-store backend runs anywhere in this codebase (see
    // media_assets.rs's own doc comment) and `crates/transport` carries no
    // package-fetch frames, so "publish into an organ's bucket" is scoped to
    // THIS Cell's own local organ: `/organ` already only ever returns the
    // local organ (`local_server_bootstrap` wraps `store::organs::local`),
    // never a remote one. Publish writes a Record + `record_extension`
    // (namespace `lince.dna`) plus the package bytes under
    // `paths::dna_dir()`, mirroring `media_assets.rs`'s disk pattern; a
    // paired organ picks the Record up through the ordinary op-log sync
    // (`record_extension` already replicates, see `engine::sync`), so no
    // bespoke cross-organ publish protocol is needed. Cross-organ *search*
    // (browsing another organ's catalog before it has synced in) is out of
    // scope until that protocol exists.
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
        let sand_toml_key = format!("lince/dna/sand/{prefix}/{slug}/{version}/{sand_toml_filename}");

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

    /// Unpublishes a DNA package: drops the `lince.dna` extension so it
    /// leaves the catalog. The underlying Record itself is left alone —
    /// unpublish is "no longer offered as a sand", not record deletion,
    /// which stays the permission-gated `delete-record` Action's job.
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

    /// Serves a sand out of `<lince_data_dir>/web/sand/` — flat files
    /// (`/sand/todo.html`) and bundle-directory files alike
    /// (`/sand/example-bundle/index.html`), all extracted from the embedded
    /// tree in `cell_surface` at boot.
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

    // ---- body images (2026-07-17): the ONLY way a `![](...)` in a record
    // body reaches a local file — upload sniffs bytes against a raster
    // allowlist and stores under an opaque name; nothing serves an arbitrary
    // path (see `presentation::http::media_assets` for why).
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
        let path = media_assets::store_media_bytes(&bytes).await?;
        Ok(Json(serde_json::json!({ "path": path })))
    }

    // Server-side native file picker (2026-07-18): opens the system file
    // dialog via xdg-desktop-portal in THIS process — no WebKitGTK file
    // chooser (crashes on this box, see media_assets::pick_and_store_image's
    // doc comment) and no Tauri IPC/capability wall. Assumes the browser and
    // the Cell are the same machine (the sand only calls this when it thinks
    // it's local — see editor.js). Only compiled into lince-desktop (see the
    // `native-picker` feature comment on lince-web's Cargo.toml) — the plain
    // `lince` CLI doesn't register this route at all.
    #[cfg(feature = "native-picker")]
    async fn pick_media(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let path = media_assets::pick_and_store_image().await?;
        Ok(Json(serde_json::json!({ "path": path })))
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

    /// Organs currently announcing on this LAN — the Organ sand's "nearby"
    /// list. Names are untrusted labels; `known` says whether the announced
    /// organ uid already has a contact row.
    async fn organ_nearby(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let mut peers = Vec::new();
        if let Some(wire) = state.wire.read().await.clone() {
            for peer in wire.nearby().current() {
                // `known` comes from the NodeId, not from anything the peer
                // broadcast — the retired multicast announce carried an
                // organ_uid for this, which meant telling the whole LAN who
                // you were before anyone had authenticated.
                let contact = store::organs::contact_by_node_id(&state.store.pool, &peer.node_id)
                    .await
                    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
                peers.push(serde_json::json!({
                    // The short fingerprint is DISAMBIGUATION among many rows,
                    // never a security check: under iroh the address already
                    // is the key.
                    "fp": peer.fingerprint,
                    "node_id": peer.node_id,
                    // Untrusted self-declared label. The UI must render it as
                    // a claim; a known contact's own name wins where we have
                    // one.
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

    /// Ceiling on one camera frame. A scan loop posts frames continuously, and
    /// a still photograph of a QR code is tens of kilobytes — nothing
    /// legitimate approaches this.
    const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

    /// Read a QR code out of a single camera frame.
    ///
    /// The decode half of the pairing code: rendering already happens here
    /// because a sand's CSP blocks every external script, and reading belongs
    /// on the same side for the same reason plus one more — what comes out is
    /// a code that decides who this Cell trusts, so it is worth having in one
    /// audited place instead of in every sand that scans.
    ///
    /// Nothing is stored and nothing is decided here. The answer goes back to
    /// the chrome, which fills a field a human still has to act on.
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
        // "No code in this frame" is the ordinary answer while a camera is
        // pointed at a wall, so it is a 200 with `text: null` — a scan loop
        // must not have to read failures to know it is still looking.
        Ok(Json(serde_json::json!({ "text": text })))
    }

    #[derive(Deserialize)]
    struct PairRequest {
        /// A NodeId, or a full `lince1|…` pairing code from a QR or a paste.
        node_id: String,
        /// The name the LOCAL user typed. Never the label inside the code:
        /// that is a claim by whoever made it.
        #[serde(default)]
        name: String,
    }

    /// Pair with an organ by NodeId: dial it, fetch its introduction, adopt it
    /// as a contact, and bind the NodeId to that contact so it is reachable
    /// afterwards.
    ///
    /// The NodeId may come from the nearby list, a scanned QR, or a paste —
    /// the route does not care, because under iroh dialing a NodeId reaches
    /// that keypair or nothing. What is at risk is only ACQUIRING the right
    /// NodeId, which is why QR-in-person and paste-over-a-trusted-channel are
    /// the ranked flows and no on-wire verification step is offered here.
    ///
    /// This uses the THREAD alpn, not sync: the peer is by definition not yet
    /// a contact, so the sync door is closed to us and theirs to them.
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
        // Accept either shape. A bare NodeId still works (the nearby list
        // hands one over); a full code additionally carries addresses, which
        // is what makes an in-person scan work where mDNS is blocked.
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

    /// Knock on a nearby Cell's thread door without making either Organ a
    /// known contact. Acceptance grants only the created conversation root.
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

    /// Open a live session against a contact Organ and relay this browser
    /// socket to it over iroh (Ontology §11 "live mode").
    ///
    /// The LOCAL user must be authenticated to use their own Cell as a way
    /// out: otherwise anyone who could reach this box could borrow its
    /// identity to open sessions on someone else's.
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
        ws.on_upgrade(move |socket| async move {
            if let Err(error) =
                crate::presentation::http::live_proxy::relay(wire, organ, socket).await
            {
                tracing::debug!(%error, "live relay ended");
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

    // Sands are built in Rust (each sand is an HTML string; groups are `.lince`
    // workspace archives) and written fresh to `<lince_data_dir>/web/sand/` on
    // every boot; served below at `/sand/*`. `PackageCatalogStore::new` renders
    // the official widgets + groups into that dir and also backs the
    // `/host/packages/local` catalog listing.
    let packages = PackageCatalogStore::new().map_err(IoError::other)?;
    let cell_store = Store::open(&default_lince_db_url())
        .await
        .map_err(IoError::other)?;
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
    let organ_signer = engine::trust::Signer::load_or_create(
        &key_dir.join("keys").join("organ-ed25519-v1.key"),
        &local_organ.uid,
        "ed25519:organ:v1",
    )
    .map_err(IoError::other)?;
    engine
        .set_organ_signer(organ_signer)
        .await
        .map_err(IoError::other)?;
    // The iroh endpoint (Ontology §11 "Transport: iroh"). Its key is per-CELL
    // and separate from the organ signer above: the node key authenticates a
    // live connection, the organ key authenticates durable bytes.
    //
    // A bind failure must not stop the Cell from serving — a machine with no
    // usable network still runs Lince locally. Peers simply stay unreachable.
    let wire = match engine::wire::node_secret(&key_dir.join("keys").join("node-ed25519-v1.key"))
        .map_err(IoError::other)
    {
        Ok(secret) => {
            let reach = if discovery_reaches_internet(&cell_store, &local_organ.uid).await {
                engine::wire::Reach::Internet
            } else {
                engine::wire::Reach::Local
            };
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
    if let Some(wire) = wire.clone() {
        tokio::spawn(async move { wire.serve().await });
    }
    // Held behind a lock because discovery is a builder option: changing it
    // rebinds the endpoint rather than mutating it, and every reader has to
    // pick up the replacement (see `wire_supervisor`).
    let wire_slot: crate::presentation::http::wire_supervisor::WireSlot =
        Arc::new(tokio::sync::RwLock::new(wire.clone()));
    // The pairing code, mirrored for the Profile panel to show. Refreshed only
    // when it actually changes: it lives on the Organ record, which syncs, and
    // rewriting it every boot would be pure noise on every contact's feed.
    //
    // What it contains is exactly what you would hand someone anyway — NodeId,
    // published root key, current addresses — so there is nothing here a
    // contact should not already have.
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
    // The identity floor (Ontology §11). The ROOT key signs only the roster
    // and key successions; it is generated here so the published format is
    // final from the first exchange, and it is meant to be MOVED OFFLINE —
    // a root that stays on a running Cell is the thing the split exists to
    // avoid. The roster starts at one member and grows when devices are
    // enrolled; publishing it now is what stops a contact who pairs today
    // from being stranded by a device added tomorrow.
    if let Err(error) =
        publish_local_roster(&engine, &local_organ.uid, &key_dir, wire.as_deref()).await
    {
        tracing::warn!(%error, "cannot publish the Cell roster");
    }
    // Seeds every enabled organ's File Sync watch loop at boot, then keeps
    // them in sync with the `lince.file_sync` extension via the fact bus —
    // toggling File Sync from the Organ sand takes effect immediately, no
    // reboot required.
    let _file_sync_supervisor = engine::file_sync::spawn_supervisor(engine.clone());
    // The organism's heartbeat. Without this the Cell has a pulse it never
    // takes: promises never expire on their own, timers never fire, and a rule
    // declaring "every week, set this back to -1" waits for someone to press
    // apply — which is the person doing the scheduling the rule was written to
    // take over.
    //
    // Started after the signer, so the first beat can attest what it commits.
    // The period is the delivery resolution: a cadence may be declared in
    // milliseconds, but nothing polled arrives finer than this. Sub-second
    // delivery needs the deadline fabric, not a smaller number here.
    let _heartbeat = engine.clone().run(HEARTBEAT_PERIOD_SECS);
    let state = CellApiState {
        board_state: BoardStateStore::new().map_err(IoError::other)?,
        engine,
        jwt_secret: Arc::new(jwt_secret),
        lanes: Arc::new(LaneHub::new()),
        listening_port: local_addr.port(),
        local_auth_required,
        wire: wire_slot,
        packages,
        store: cell_store,
    };

    let static_dir = crate::infrastructure::paths::static_dir();
    let router = axum::Router::new().route("/api/auth/login", post(login))
        .route("/auth/login", post(login))
        .route("/host/auth/login", post(login))
        .route("/organ", get(list_organs))
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
        .route("/organ/nearby", get(organ_nearby))
        .route("/organ/pair", post(organ_pair))
        .route("/organ/conversation/offer", post(offer_nearby_conversation))
        .route("/organ/qr-decode", post(qr_decode))
        .route("/organ/open-promises", get(organ_open_promises))
        .route(
            "/organ/transfers/envelopes",
            post(crate::presentation::http::transfer_delivery::receive_envelope),
        )
        .route(
            "/organ/transfers/pull",
            post(crate::presentation::http::transfer_delivery::pull_envelope),
        )
        .route(
            "/organ/transfers/receipts",
            post(crate::presentation::http::transfer_delivery::receive_receipt),
        )
        .route(
            "/organ/transfers/commands",
            post(crate::presentation::http::transfer_delivery::receive_command),
        )
        .route(
            "/organ/transfers/policy-events",
            post(crate::presentation::http::transfer_delivery::receive_policy_event),
        )
        .route(
            "/organ/transfers/application-attestations",
            post(crate::presentation::http::transfer_delivery::receive_application_attestation),
        )
        .route("/host/transport/ws", get(connect));
    // Only lince-desktop enables `native-picker` (see the Cargo.toml
    // comment) — the plain `lince` CLI never registers this route.
    #[cfg(feature = "native-picker")]
    let router = router.route("/host/media/pick", post(pick_media));

    // Everything that hands a visitor a working board. Registered as one
    // block so the hardened surface is auditable at a glance — a board route
    // added to the chain above would silently appear on a `--server` box,
    // whereas one added here cannot.
    let serve_ui = mode == HttpServeMode::FullUi;
    let router = if serve_ui {
        router
            .route("/", get(index))
            .route("/favicon.ico", get(static_assets::favicon))
            .route("/board/frame.js", get(static_assets::frame_js))
            .route("/board/editor.js", get(static_assets::editor_js))
            .route("/board/lynx-ui.css", get(static_assets::lynx_ui_css))
            .route("/board/lynx-ui.js", get(static_assets::lynx_ui_js))
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
            // The guest half of live mode: this Cell relays a LOCAL BROWSER
            // socket out to a contact Cell over iroh (Ontology §11). A
            // headless server has no such browser, so the route is dead
            // weight there — the inbound half a client uses to reach THIS
            // Cell is `/host/transport/ws`, which stays in both modes.
            .route("/live/{organ}/connect", get(live_connect))
    } else {
        router
    };

    let router = router.with_state(state.clone());
    // Both arms serve the same tree — skipping only the `nest_service` branch
    // would leave the `route(...)` fallback serving every static asset.
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

    // Force revalidation on every response. The board's JS is served either from
    // disk (ServeDir, which sends only `Last-Modified` — no `Cache-Control`, so
    // the Tauri webview heuristically caches and serves STALE files) or embedded.
    // A stale `store.js` next to a fresh `main.js` surfaces as
    // "store.addImportedGroup is not a function"; no-cache keeps the whole board
    // coherent after a rebuild without needing a manual cache wipe.
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
    crate::presentation::http::transfer_delivery::spawn_worker(state.clone());
    // Organ sync (Ontology §11): reactive deltas + catch-up reconciliation
    // against every synced contact, woken by the fact bus.
    crate::presentation::http::sync_runner::spawn_runner(state.clone());
    // Discovery is a builder option, so toggling internet reachability rebinds
    // the endpoint instead of mutating it — the File Sync supervisor pattern,
    // applied to the one setting that cannot be changed in place.
    crate::presentation::http::wire_supervisor::spawn(state.clone(), key_dir.clone());
    status(match mode {
        HttpServeMode::FullUi => format!("Cell API listening at http://{local_addr}"),
        HttpServeMode::ApiOnly => format!(
            "Cell API listening at http://{local_addr} (server mode: no board UI, login required)"
        ),
    });
    axum::serve(listener, app).await.map_err(IoError::other)
}

/// Generate (once) the Organ root key, publish its public half so it travels
/// in this Organ's Introduction, and sign a roster naming this Cell.
///
/// Publishing the root PUBLIC key through the Introduction is what makes the
/// whole chain rule workable: a contact adopts it at pairing — the one and
/// only trust-on-first-use — and every roster and succession afterwards must
/// chain from it. Without that, a roster arriving later would have nothing to
/// be checked against.
async fn publish_local_roster(
    engine: &engine::Engine,
    organ_uid: &str,
    key_dir: &std::path::Path,
    wire: Option<&engine::wire::Wire>,
) -> Result<(), IoError> {
    let root_path = key_dir.join("keys").join("root-ed25519-v1.key");
    let held = engine.roster_of(organ_uid).await.map_err(IoError::other)?;

    // The root key is CREATED at most once, ever. Creating one whenever the
    // file is missing would mint a brand-new identity the first time the owner
    // does the thing the split exists to encourage — moving the root to
    // offline media — and every contact would see a key that chains from
    // nothing. So: no file and no roster means first boot, create it; no file
    // WITH a roster means the root is deliberately elsewhere, and this Cell
    // simply cannot sign until it comes back.
    engine.set_root_key_path(root_path.clone());
    if !root_path.exists() {
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
    engine
        .publish_root_key(&root)
        .await
        .map_err(IoError::other)?;

    // Without an endpoint there is no node id to name, and a roster listing a
    // Cell nobody can dial is worse than none.
    let Some(wire) = wire else {
        return Ok(());
    };

    // Re-sign only when the roster would actually change. Bumping the version
    // on every boot would burn through versions and, worse, train contacts to
    // accept a stream of rosters they have no reason to inspect.
    let node_id = wire.node_id().to_string();
    if let Some(held) = &held {
        let unchanged = held.roster.root_key == root.public_key_b64()
            && held.roster.cells.iter().any(|cell| cell.node_id == node_id);
        if unchanged {
            return Ok(());
        }
    }
    let operational_key = engine
        .local_organ_public_key()
        .await
        .map_err(IoError::other)?
        .unwrap_or_default();
    // PRESERVE the other members. Republishing with only this Cell would
    // silently evict every enrolled device — the roster is the membership
    // list, so dropping a name from it IS revocation, and that must never be
    // a side effect of a reboot.
    let mut cells: Vec<engine::roster::CellEntry> = held
        .map(|held| held.roster.cells)
        .unwrap_or_default()
        .into_iter()
        .filter(|cell| cell.cell_uid != organ_uid)
        .collect();
    cells.push(engine::roster::CellEntry {
        cell_uid: organ_uid.to_string(),
        node_id,
        label: "this cell".to_string(),
        operational_key,
        // A Cell is a front door only if it publishes addresses publicly,
        // which is exactly what `lince.discovery.internet` controls. Deriving
        // it keeps the roster from claiming a public tier the endpoint is not
        // actually serving.
        front_door: discovery_reaches_internet(&engine.store, organ_uid).await,
    });
    engine
        .publish_roster(&root, cells)
        .await
        .map_err(IoError::other)?;
    Ok(())
}

/// Whether this Cell should be resolvable across the internet (DHT + DNS), from
/// `lince.discovery` `{internet}` on the local Organ.
///
/// DEFAULT ON (Ontology §11): a Cell that is not resolvable across the internet
/// cannot serve the case that motivates the whole design — the always-on Cell
/// telling the phone about a change the laptop made. Turning it OFF is the
/// deliberate choice, and what it costs is that peers see only a relay rather
/// than a direct address, which hides approximate location and online hours.
///
/// Read once at bind because discovery is an Endpoint builder option fixed at
/// construction; changing it must rebind the endpoint, not mutate it.
pub(crate) async fn discovery_reaches_internet(store: &Store, organ_uid: &str) -> bool {
    match store::records::get_extension(&store.pool, organ_uid, "lince.discovery").await {
        Ok(Some(fields)) => fields
            .get("internet")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        _ => true,
    }
}

/// Whether this Cell advertises and listens for nearby Lince Cells over mDNS.
///
/// Default ON preserves the existing LAN behavior. Unlike internet address
/// publication this is room-scoped, so it has its own switch.
pub(crate) async fn discovery_is_local(store: &Store, organ_uid: &str) -> bool {
    match store::records::get_extension(&store.pool, organ_uid, "lince.discovery").await {
        Ok(Some(fields)) => fields
            .get("local")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        _ => true,
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

fn default_lince_db_url() -> String {
    // The new store owns `lince.db`. Resolve the directory the same way the
    // legacy layer does (`utils::config::lince_data_dir`) so both honor
    // `LINCE_DATA_DIR_OVERRIDE` and always land side by side — `lince.db` (new
    // schema) next to `lince-legacy.db` (legacy) — never the same file.
    let dir = utils::config::lince_data_dir().unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    format!("sqlite://{}", dir.join("lince.db").display())
}
