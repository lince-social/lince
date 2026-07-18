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

#[derive(Clone)]
struct CellApiState {
    board_state: BoardStateStore,
    engine: Arc<engine::Engine>,
    jwt_secret: Arc<String>,
    lanes: Arc<LaneHub>,
    listening_port: u16,
    local_auth_required: bool,
    packages: PackageCatalogStore,
    store: Store,
}

pub async fn serve_cell_api_only(
    listen_addr: Option<String>,
    jwt_secret: String,
    local_auth_required: bool,
    staged_setup: Option<DesktopInstallSetup>,
    bound_addr_sender: Option<oneshot::Sender<SocketAddr>>,
) -> Result<(), IoError> {
    use axum::{
        Json,
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
        let user = store::auth::user_by_id(&state.store.pool, claims.sub as i64)
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
        Ok(Some(claims.sub.to_string()))
    }

    /// Best-effort viewer resolution for the SSR bootstrap: unlike
    /// `authenticate_headers`, never errors — the page must render whether or
    /// not the visitor is logged in (there is no separate login page to fall
    /// back to). Missing/invalid/stale tokens and no-auth-required Cells all
    /// resolve to `None` (no viewer identity to show), never a hard failure.
    async fn viewer_from_headers(state: &CellApiState, headers: &HeaderMap) -> Option<ViewerBootstrap> {
        if !state.local_auth_required {
            return None;
        }
        let token = bearer_token(headers).ok().flatten()?;
        let claims = utils::auth::decode_jwt(state.jwt_secret.as_str(), &token).ok()?;
        let user = store::auth::user_by_id(&state.store.pool, claims.sub as i64)
            .await
            .ok()
            .flatten()?;
        Some(ViewerBootstrap {
            id: user.id.to_string(),
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
            user.id as u64,
            &user.username,
            user.role_id as u64,
            &user.role,
            &user.permissions,
            CELL_JWT_TTL,
        )
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        let cookie = format!(
            "{CELL_AUTH_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax"
        );
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

    async fn list_empty_authed(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        Ok(Json(Vec::<serde_json::Value>::new()))
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
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "Nome de grupo invalido.".to_string()))?;
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

    async fn get_official_package_content(
        State(state): State<CellApiState>,
        headers: HeaderMap,
        Path((filename, asset_path)): Path<(String, String)>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let packages = crate::sand::official_packages().map_err(|message| {
            (StatusCode::INTERNAL_SERVER_ERROR, message)
        })?;
        let package = packages
            .into_iter()
            .find(|package| package.archive_filename() == filename)
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Esse widget oficial nao existe.".into()))?;
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
            return Err((StatusCode::BAD_REQUEST, "invalid media filename".to_string()));
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

    // ---- the organ↔organ HTTP boundary (blueprint XV; no local JWT — the
    // visibility gate, signatures, and the blocked-organ check do the gating)

    async fn organ_introduction(
        State(state): State<CellApiState>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        state
            .engine
            .introduction()
            .await
            .map(Json)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
    }

    async fn organ_inbox(
        State(state): State<CellApiState>,
        Json(package): Json<engine::sync::Package>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        let applied = state
            .engine
            .import_package(&package)
            .await
            .map_err(|error| (StatusCode::FORBIDDEN, error.to_string()))?;
        Ok(Json(serde_json::json!({ "applied": applied.len() })))
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
    )
    .await?;
    let engine = Arc::new(
        engine::Engine::new(cell_store.clone())
            .await
            .map_err(IoError::other)?,
    );
    // Simplest v1: read each organ's `lince.file_sync` config once at boot and
    // spawn its watch loop if enabled. A toggle from the Organ sand takes
    // effect on the next boot; no live start/stop supervisor yet.
    engine::file_sync::spawn_configured_watchers(engine.clone())
        .await
        .map_err(IoError::other)?;
    let state = CellApiState {
        board_state: BoardStateStore::new().map_err(IoError::other)?,
        engine,
        jwt_secret: Arc::new(jwt_secret),
        lanes: Arc::new(LaneHub::new()),
        listening_port: local_addr.port(),
        local_auth_required,
        packages,
        store: cell_store,
    };

    let static_dir = crate::infrastructure::paths::static_dir();
    let router = axum::Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(static_assets::favicon))
        .route("/board/frame.js", get(static_assets::frame_js))
        .route("/board/editor.js", get(static_assets::editor_js))
        .route("/board/vendor/d3.v7.min.js", get(static_assets::d3_js))
        .route("/board/vendor/d3.LICENSE.txt", get(static_assets::d3_license))
        .route("/api/auth/login", post(login))
        .route("/auth/login", post(login))
        .route("/host/auth/login", post(login))
        .route("/organ", get(list_organs))
        .route("/host/board/state", get(get_board_state).put(put_board_state))
        .route("/host/notifications", get(list_empty_authed))
        .route("/host/packages/local", get(list_local_packages))
        .route("/host/packages/local/group/{filename}", get(get_local_group))
        .route("/host/packages/local/{package_id}", get(get_local_package))
        .route(
            "/host/packages/local/by-filename/{filename}/content/{*asset_path}",
            get(get_official_package_content),
        )
        .route("/sand/{*path}", get(sand_asset))
        .route("/host/media", post(upload_media))
        .route("/host/media/{name}", get(get_media))
        .route("/organ/introduction", get(organ_introduction))
        .route("/organ/inbox", post(organ_inbox))
        .route("/organ/open-promises", get(organ_open_promises))
        .route("/host/transport/ws", get(connect));
    // Only lince-desktop enables `native-picker` (see the Cargo.toml
    // comment) — the plain `lince` CLI never registers this route.
    #[cfg(feature = "native-picker")]
    let router = router.route("/host/media/pick", post(pick_media));
    let router = router.with_state(state);
    let app = if static_dir.exists() {
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
    status(format!("Cell API listening at http://{local_addr}"));
    axum::serve(listener, app).await.map_err(IoError::other)
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
