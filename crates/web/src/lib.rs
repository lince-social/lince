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
    /// Organ uid -> the credential this Cell holds for it, in memory only.
    ///
    /// This is what makes a remote host stay logged in across a reconnect
    /// instead of asking again every time the link blips. It never reaches
    /// disk, never enters the store, and never syncs: restarting the process
    /// logs every remote host out, which is the honest tradeoff for not
    /// persisting someone's password to another machine.
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
        // Re-read on every request, which is what closes an OPEN session. A
        // deactivation that only blocked new logins would leave whoever was
        // already signed in acting indefinitely — and the session that matters
        // most is exactly the one already running when you decided to end it.
        // The JWT stays valid by its own terms; standing is checked against the
        // store, so this is not something a held token can outlive.
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
        // Same answer as an expired token: no viewer. Rendering the page as
        // them would show a name and a role that no longer authorise anything,
        // and every request behind it would fail — a chrome that lies about who
        // you are is worse than a logged-out one.
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

    /// Every Cell a sand on this board may be pointed at: our own, first, then
    /// every contact Organ we know.
    ///
    /// This used to return the local Cell alone, which is why the host picker
    /// looked empty — it was listing exactly one thing and hiding itself
    /// whenever a sand did not declare a write permission. A host binding is
    /// per sand, so this list is what makes "this card reads MY Lince, that one
    /// reads theirs" expressible at all.
    ///
    /// `authenticated` is answered honestly per row and means different things
    /// by design: for our own Cell it is whether this browser has a session
    /// (always true when the Cell has no auth at all — there is nothing to log
    /// into); for a contact it is whether this Cell currently holds a way in,
    /// either a login we typed or a device binding they granted us.
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
            // Nothing to log into when auth is off — say so rather than
            // showing a login box that would reject every password.
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
                    // A contact's Cell always wants to know who you are. Either
                    // they granted this device a binding, or you type a
                    // password — but you never simply arrive.
                    requires_auth: true,
                    // Whether WE hold a way into THEIR Cell — nothing else.
                    //
                    // This used to also count `organ_login`, which is the other
                    // direction entirely: a login we granted THEM into OURS.
                    // Reading it here answered "they can get into me" to the
                    // question "can I get into them", so a sand bound to a Lince
                    // we had never logged into rendered unlocked, dialled, was
                    // asked for a password we did not hold, and was dropped —
                    // once a second, for as long as the board stayed open.
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
        // The REAL viewer, not an assumption. This bootstrap is what the board
        // renders from before any fetch returns, so claiming a session nobody
        // has would draw every sand unlocked for as long as that takes — rows
        // on screen that the lock exists to prevent.
        let viewer = viewer_from_headers(&state, &headers).await;
        let servers = local_server_bootstrap(&state, viewer.is_some()).await;
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
        // Standing is checked AFTER the password and answered with the SAME
        // sentence, word for word. "This account is deactivated" would be
        // username enumeration wearing a helpful tone: it tells anyone with a
        // guessed name that the name is real. The person who was deactivated
        // already knows why, from whoever deactivated them; the login screen is
        // not where that conversation happens.
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
        // The board no longer calls this — it takes the same list over the
        // websocket, pushed. Kept for API clients, and delegating so the two
        // cannot describe the same invite differently.
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
        let viewer = viewer_from_headers(&state, &headers).await.is_some();
        Ok(Json(organ_list(&state, viewer).await))
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
        let path = media_assets::store_media_bytes(&state.store.pool, &bytes).await?;
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
        let path = media_assets::pick_and_store_image(&state.store.pool).await?;
        Ok(Json(serde_json::json!({ "path": path })))
    }

    /// "How big is this Lince, and what is using it" (Ontology C2c).
    ///
    /// A host route rather than a Protein subscription because none of this is
    /// a Record: it is bytes on this machine, and this Cell's bytes at that —
    /// a phone and a VPS have no reason to report the same number, and putting
    /// it in the Ledger would sync one device's disk usage to every other.
    ///
    /// Directory sizes are measured HERE and passed down, because where the
    /// media and DNA folders live is this crate's layout; `store::budget` owns
    /// the policy and never learns a path.
    async fn get_storage(
        State(state): State<CellApiState>,
        headers: HeaderMap,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        authenticate_headers(&state, &headers).await?;
        let media = directory_bytes(crate::infrastructure::paths::media_dir()).await;
        let dna = directory_bytes(crate::infrastructure::paths::dna_dir()).await;
        // `None`, not `Some(0)`: the Facade cache arrives with C9 and does not
        // exist yet, and a confident zero would read as "nothing cached"
        // instead of "not built".
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

    /// What recently happened to one Record's fields, and why.
    ///
    /// The merge is already correct; this is about it being LEGIBLE. A
    /// field-level edit that lost a last-write-wins race left no trace a
    /// person could find, so `displaced` carries the text that was replaced —
    /// recovering the lost edit is then retyping what is on screen rather than
    /// reading an op log.
    ///
    /// `mine` is the field that decides how a surface should treat an entry.
    /// A remote op overwriting a value this Cell never authored is an ordinary
    /// update; only one that displaced OUR OWN writing is something that
    /// happened TO the person, and marking the rest that way would cry wolf on
    /// every sync.
    ///
    /// Not a history tab: entries age out, so this answers "what just
    /// happened" and nothing longer. The Ledger is the permanent record.
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

    /// Set the Cell's stated total. `0` is unlimited and is a legitimate
    /// answer — the point of a budget is that the owner decides, and "no
    /// ceiling" is one of the decisions.
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

    /// Bytes under a directory, missing directory counting as zero.
    ///
    /// A missing folder is the ordinary state of a Cell that has never stored
    /// anything of that kind, so it is not an error — and refusing to report
    /// would make the panel fail rather than say "nothing here yet".
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

    /// Endorse a NEW identity key with the current root: "this old root says
    /// this new one is also me."
    ///
    /// This is the whole of rotation that can happen on a Cell. The new key
    /// itself is generated wherever the owner keeps key material — offline, if
    /// they took the advice — and only its PUBLIC half comes here. Contacts
    /// pull the endorsement on their next sync pass (`FetchSuccessions`) and
    /// accept a roster signed by the new key from then on, with nobody
    /// re-pairing.
    ///
    /// Needs the root, so it fails on a Cell that has deliberately moved the
    /// root offline — which is the same trade as enrolling or revoking a
    /// device, and the reason the split exists.
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
        // Endorsing the key you are already using would write an edge from a
        // key to itself, which chains nothing and only makes the chain harder
        // to read later.
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

    #[derive(Deserialize)]
    struct ReferenceReadRequest {
        owner: String,
        root: String,
        record: String,
    }

    /// Read a Record a message REFERENCES, live from its owner's Cell
    /// (Ontology §11, C6).
    ///
    /// No cache, here or anywhere: a reference resolves live or it resolves to
    /// nothing. That is what makes revocation real in this one place — the
    /// reader never held a copy — and a cache would trade it away for a
    /// convenience nobody asked for.
    ///
    /// The two failures are told APART for the surface, because they mean
    /// opposite things to the person reading. Unreachable is temporary and
    /// worth retrying; refused is an answer. Collapsing them into one error
    /// would show "no longer shared" to somebody whose friend simply closed
    /// their laptop, which is a false accusation the interface has no business
    /// making.
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
                // The owner's own words for a permission answer. Matched on
                // the message rather than a typed error because the refusal
                // crosses the wire as text — worth replacing with a typed
                // refusal when another caller needs the same distinction.
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
        /// The public value: a pairing code, or a bare NodeId.
        invite: String,
        username: String,
        password: String,
        #[serde(default)]
        name: String,
    }

    /// Log into an Organ from a pasted public value, with no prior contact.
    ///
    /// This is the one that answers "a fresh Lince, from anywhere". The Organ
    /// lands in the contact list as `unknown` — enough to name it, bind sands
    /// to it and reconnect to it, and nothing more. Being able to log into
    /// someone's Cell is not a decision to trust their Organ with our data, and
    /// `unknown` is exactly the tier that keeps sync shut while live mode
    /// works.
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
        // Accept either shape, exactly as pairing does: a full code carries
        // addresses (what makes this work where discovery is blocked), a bare
        // NodeId does not.
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

        // The name the LOCAL user typed wins over anything the code claimed —
        // a self-declared label is how "Eduardo's laptop" ends up on a
        // stranger's row.
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

    /// Log THIS Cell into a contact's Cell with a username and password.
    ///
    /// The credential is verified by actually using it — one live connection is
    /// opened and the handshake either succeeds or does not — rather than
    /// stored on the strength of the user having typed something. A password
    /// that is wrong must fail here, at the moment it is entered, and not later
    /// as an unexplained blank sand.
    ///
    /// This is the device-independent way in: nothing about our keys is
    /// consulted by the far side, so a Lince installed a minute ago works
    /// exactly as well as one they have known for a year.
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

    /// Forget the credential held for a contact's Cell.
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
        let login = state.remote_logins.read().await.get(&organ).cloned();
        ws.on_upgrade(move |socket| async move {
            if let Err(failure) =
                crate::presentation::http::live_proxy::relay(wire, organ, login, socket).await
            {
                // WARN, not debug: this is the one line that says why a sand
                // bound to another Lince shows nothing, and at debug level
                // nobody running a normal build ever sees it.
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

    // Sands are built in Rust (each sand is an HTML string; groups are `.lince`
    // workspace archives) and written fresh to `<lince_data_dir>/web/sand/` on
    // every boot; served below at `/sand/*`. `PackageCatalogStore::new` renders
    // the official widgets + groups into that dir and also backs the
    // `/host/packages/local` catalog listing.
    let packages = PackageCatalogStore::new().map_err(IoError::other)?;
    let cell_store = Store::open(&default_lince_db_url())
        .await
        .map_err(IoError::other)?;
    // A Cell may have been stopped while over budget, or its budget may have
    // been changed by another process. Boot closes that gap before serving a
    // single old media path.
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
    // Filed under THIS CELL's key id. The secret file is unchanged and still
    // per-device; what moved is the id it is published under, so two Cells of
    // one Organ no longer collide on a single `identity_key` row.
    let this_cell = store::cells::local(&cell_store.pool)
        .await
        .map_err(IoError::other)?
        .ok_or_else(|| IoError::other("this Cell has no Cell Record"))?;
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
    // Hoisted above the endpoint: the live handler needs it, and a SECOND hub
    // would put live guests in different lane rooms from the local board — the
    // cursors would simply never meet.
    let lanes = Arc::new(LaneHub::new());
    // Live sessions, installed at the INITIAL bind and not only on a rebind.
    //
    // `wire_supervisor` sets this every time it rebinds for a discovery change,
    // which meant a Cell that simply booted and was never reconfigured had no
    // handler at all: every `lince/live/1` connection was closed with "no live
    // session for this organ", whoever was asking and however they had
    // authenticated. The handler belongs to the endpoint, so every path that
    // makes an endpoint has to install one.
    if let Some(wire) = wire.clone() {
        wire.set_live_handler(transport::live::LiveHost::new(
            engine.clone(),
            lanes.clone(),
        ));
        // Makes "join an Organ from a code" reachable as an Action, which is
        // what turns the enrolment client into something a person can use
        // rather than something only a test can call.
        wire.serve_enrolment();
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
        .route("/host/storage/budget", post(set_storage_budget))
        .route("/organ/nearby", get(organ_nearby))
        .route("/organ/pair", post(organ_pair))
        .route("/organ/conversation/offer", post(offer_nearby_conversation))
        // A reference is READ, never fetched-and-kept: this route proxies one
        // live read to the owner and returns what their gate allows.
        .route("/organ/reference/read", post(read_reference))
        .route("/organ/qr-decode", post(qr_decode))
        .route("/organ/identity/succession", post(sign_key_succession))
        .route("/organ/open-promises", get(organ_open_promises))
        // The six `/organ/transfers/*` peer routes are gone (2026-08-08).
        // Transfer was the last subsystem speaking HTTP peer-to-peer; it now
        // rides `lince/sync/1` as `TransferPost`, so a delivery is dialed by
        // identity like everything else and no contact needs a reachable URL.
        // The paths survive as SIGNING DOMAINS inside `transfer_delivery`.
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
    // Serve Transfer over iroh. Installed here rather than beside the live
    // handler above because it needs the whole `CellApiState`, which does not
    // exist yet at bind time — and on every rebind too (`wire_supervisor`),
    // since the handler belongs to the endpoint.
    if let Some(wire) = state.wire.read().await.clone() {
        wire.set_transfer_handler(Arc::new(
            crate::presentation::http::transfer_delivery::TransferPeerHandler::new(state.clone()),
        ));
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
    // Beside the other key material, and unlike the root it is meant to STAY
    // here: a sealing private key is useless anywhere but on the Cell that has
    // to open mail with it, and a copy of it kept elsewhere would undo the
    // forward secrecy that made it a separate key in the first place.
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
    // The pre-signed revocation certificate is made at key CREATION and never
    // again, and it lives beside the root so the drawer trip that fetches the
    // root to re-establish identity also yields the thing that kills the old
    // key. Generating it later would need the root anyway — which is exactly
    // the situation where you may no longer have it.
    //
    // It does not prove a replacement key is genuine. It is damage limitation
    // that still works when identity cannot yet be re-established at all.
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
            // Not fatal: an Organ with no revocation certificate is the state
            // every Organ was in until now, and refusing to boot over it would
            // be a worse trade than starting without one.
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

    // Without an endpoint there is no node id to name, and a roster listing a
    // Cell nobody can dial is worse than none.
    let Some(wire) = wire else {
        return Ok(());
    };

    // Re-sign only when the roster would actually change. Bumping the version
    // on every boot would burn through versions and, worse, train contacts to
    // accept a stream of rosters they have no reason to inspect.
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
    // PRESERVE the other members. Republishing with only this Cell would
    // silently evict every enrolled device — the roster is the membership
    // list, so dropping a name from it IS revocation, and that must never be
    // a side effect of a reboot.
    let mut cells: Vec<engine::roster::CellEntry> = held
        .as_ref()
        .map(|held| held.roster.cells.clone())
        .unwrap_or_default()
        .into_iter()
        // By CELL uid. Before the split this compared the Organ uid, because
        // one row was both — which is exactly the confusion the split ends.
        .filter(|held_cell| held_cell.cell_uid != cell.uid)
        .collect();
    // Rotates and prunes as a side effect of being read, so a Cell that was
    // off across its own rotation point catches up on the boot that follows.
    // A rotation changes this entry, which is what makes `needs_publishing`
    // re-sign the roster — rotation never has to know about publishing.
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
        // The Cell that holds the root and serves the owner is an ordinary
        // full member. Relay Cells get `relay_capabilities()`.
        capabilities: engine::roster::full_capabilities(),
        // A Cell is a front door only if it publishes addresses publicly.
        // Taken from the ENDPOINT rather than from `lince.discovery.internet`
        // directly: reach is fixed when the endpoint is built, so reading the
        // config again here could claim a public tier the endpoint is not
        // actually serving — which is precisely what a test Cell, bound
        // `Local` while the config default says internet, would have done.
        front_door: wire.reach() != engine::wire::Reach::Local,
    });
    // The re-sign decision — the whole member set, not membership of self —
    // lives in `engine::roster` where it can be tested against a revocation.
    if engine::roster::needs_publishing(held.as_ref(), &root.public_key_b64(), &cells) {
        engine
            .publish_roster(&root, cells)
            .await
            .map_err(IoError::other)?;
    }
    // Sign the directory record on EVERY boot that holds the root, not only
    // when the roster changed. An Organ whose roster is already correct would
    // otherwise never get one — the early return skipped it — and would stay
    // unpublished until the next enrolment. Cheap and idempotent in effect:
    // the record is cut from the roster, so an unchanged roster produces the
    // same front doors.
    //
    // Failing here must not fail the boot. An Organ with no published record
    // is still reachable by every contact holding a roster, which is everyone
    // it has ever paired with.
    if let Err(error) = engine.sign_public_record(&root).await {
        tracing::warn!(%error, "could not sign the public directory record");
    }
    // Broadcast on every boot: the DHT entry expires in hours, so a boot that
    // changes nothing is exactly when re-broadcasting matters.
    republish_public_record(engine, organ_uid).await;
    Ok(())
}

/// Broadcast the stored public record, if there is one.
///
/// Needs no key — it re-sends already-signed bytes — and every failure is a
/// warning rather than an error, because the network being unreachable at boot
/// says nothing about whether this Cell should run.
async fn republish_public_record(engine: &engine::Engine, organ_uid: &str) {
    match engine.republish_public_record(organ_uid).await {
        Ok(true) => tracing::info!("published this Organ's front door under its identity key"),
        Ok(false) => {}
        Err(error) => tracing::warn!(%error, "could not publish the directory record"),
    }
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
/// How reachable this Cell should be (Ontology §11, C4).
///
/// THREE states, not two, and the middle one is the default. `internet`
/// says whether this Cell is reachable at all; `direct` says whether it also
/// publishes this machine's own addresses. Relay-only is the default because
/// a default describes a fresh install on a café network, not an Organ that
/// has already decided to be reachable — and publishing direct addresses
/// tells anyone holding the published key roughly where you are and when you
/// are awake.
///
/// The defaults were REVERSED here on 2026-08-13: `internet` used to mean
/// direct publication and defaulted on.
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
        // OFF unless asked for. This is the reversal.
        .unwrap_or(false);
    if direct {
        engine::wire::Reach::Internet
    } else {
        engine::wire::Reach::Relay
    }
}

/// This Cell's discovery settings.
///
/// Read from the CELL Record first, and only then from the Organ Record where
/// they used to live. Discovery is per-DEVICE — which LAN this machine is on,
/// whether this machine publishes an address — so it never belonged on the
/// shared Organ Record, and keeping it there had a concrete cost: a relay Cell
/// may not write, an Organ-Record extension IS a logged write, so a relay could
/// not configure itself at all. Cell config is written raw and logs no op.
///
/// The fallback stays because an Organ that set these before the move still
/// means them, and one extra row read costs nothing.
pub(crate) async fn discovery_config(store: &Store, organ_uid: &str) -> Option<serde_json::Value> {
    if let Ok(Some(fields)) = store::cells::config(&store.pool, "lince.discovery").await {
        return Some(fields);
    }
    store::records::get_extension(&store.pool, organ_uid, "lince.discovery")
        .await
        .ok()
        .flatten()
}

/// The FIRST-BOOT answer, before anyone has set `lince.discovery.internet`.
///
/// `LINCE_DISCOVERY_INTERNET=0` is the only way to say "never reach the
/// internet" for a Cell that has not booted yet, because the setting lives on
/// an Organ Record that boot itself creates. Two real users: a headless or
/// air-gapped install that must not publish an address before a human can
/// switch it off, and every test that boots a real Cell — without it they
/// publish node addresses AND this Organ's directory record to public
/// infrastructure, under a throwaway identity key, on every run.
fn internet_default() -> bool {
    !matches!(
        std::env::var("LINCE_DISCOVERY_INTERNET").as_deref(),
        Ok("0") | Ok("false") | Ok("no")
    )
}

/// Whether this Cell advertises and listens for nearby Lince Cells over mDNS.
///
/// Default ON preserves the existing LAN behavior. Unlike internet address
/// publication this is room-scoped, so it has its own switch.
/// Whether this Cell advertises and listens for nearby Lince Cells over mDNS.
///
/// OFF by default, and TIME-BOUNDED when switched on (Ontology §11, C4).
/// Announcing yourself to a room is a disclosure, and the thing about a room
/// is that you leave it — a laptop that announced itself in a café three
/// months ago is still announcing itself in every café since. So enabling it
/// writes an expiry, and this reads it: past `local_until`, LAN presence is
/// off again whether or not anyone remembered.
///
/// A missing `local_until` with `local: true` stays on, deliberately — that
/// is a Cell configured before the bound existed, and silently switching off
/// someone's working LAN discovery would be worse than leaving it.
pub(crate) async fn discovery_is_local(store: &Store, organ_uid: &str) -> bool {
    let Some(fields) = discovery_config(store, organ_uid).await else {
        return false;
    };
    let on = fields
        .get("local")
        .and_then(serde_json::Value::as_bool)
        // OFF by default (reversed 2026-08-13). A default describes a café,
        // not a living room.
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
            // An unparseable expiry is treated as EXPIRED. Failing closed on
            // a disclosure setting is the only safe reading of a value we do
            // not understand.
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

/// Public so the `lince` binary's admin subcommands open the SAME file the
/// server would. A headless box has no board to configure itself from, and the
/// one thing worse than no admin surface is a second one pointed elsewhere.
pub fn default_lince_db_url() -> String {
    // The new store owns `lince.db`. Resolve the directory the same way the
    // legacy layer does (`utils::config::lince_data_dir`) so both honor
    // `LINCE_DATA_DIR_OVERRIDE` and always land side by side — `lince.db` (new
    // schema) next to `lince-legacy.db` (legacy) — never the same file.
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

    /// The reversal: a Cell nobody has configured is reachable through a
    /// RELAY and announces itself to no room. A default describes a fresh
    /// install on a café network.
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

    /// Direct addresses are an explicit act, and only then.
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

    /// LAN presence EXPIRES. A laptop that announced itself in a café three
    /// months ago must not still be announcing itself in every café since.
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

        // An expiry we cannot read is treated as expired: failing closed is
        // the only safe reading of a disclosure setting we do not understand.
        store::cells::set_config(
            &store.pool,
            "lince.discovery",
            &serde_json::json!({ "local": true, "local_until": "soon-ish" }),
        )
        .await
        .expect("config");
        assert!(!discovery_is_local(&store, &organ).await);
    }

    /// Per-DEVICE: the Cell Record answers, and the Organ Record is only a
    /// fallback for Cells configured before the move.
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
