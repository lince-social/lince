use {
    crate::{
        application::state::AppState,
        domain::lince_package::{
            LincePackage, PackageTransport, normalize_asset_path, package_id_from_filename,
            parse_lince_package, validate_package_upload,
        },
        infrastructure::{
            auth::{parse_cookie_header, session_cookie_name},
            dna_hub_store::DnaSandSearchMatch,
            organ_store::{Organ, organ_requires_auth},
            package_catalog_store::InstalledPackageSummary,
        },
        presentation::http::api_error::{ApiResult, api_error, invalid_multipart},
    },
    ::application::auth::AuthSubject,
    axum::{
        Json,
        body::Body,
        extract::{Multipart, Path, Query, State},
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::{IntoResponse, Response},
    },
    serde::{Deserialize, Serialize},
    serde_json::{Value, json},
    std::io::{Error, ErrorKind},
};

const DATASTAR_BOOTSTRAP_SCRIPT: &str =
    r#"<script type="module" src="/host/static/vendored/datastar.js"></script>"#;
const WIDGET_BOOTSTRAP_SCRIPT: &str =
    r#"<script type="module" src="/host/static/presentation/board/widget-frame-bootstrap.js"></script>"#;
const DNA_BUCKET_PREFIX: &str = "lince/dna/sand";
const DNA_RESOURCE_NAMESPACE: &str = "lince.dna";
const HTML_TRANSPORT_FILENAME_SUFFIX: &str = "_metadata.html";
const SAND_TOML_FILENAME: &str = "sand.toml";

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
    pub frame_src: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaPublishResponse {
    pub ok: bool,
    pub organ_id: String,
    pub record_id: i64,
    pub record_extension_id: i64,
    pub resource_ref_id: i64,
    pub head: String,
    pub body: String,
    pub slug: String,
    pub channel: String,
    pub package_prefix: String,
    pub bucket_key: String,
    pub sand_toml_key: String,
    pub transport_filename: String,
    pub package_format: String,
}

#[derive(Debug)]
struct PublishUpload {
    filename: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct PublishMultipartPayload {
    server_id: String,
    channel: String,
    head: String,
    body: String,
    upload: PublishUpload,
}

#[derive(Debug, Deserialize)]
struct LinkPayload {
    url: String,
}

#[derive(Debug, Deserialize)]
struct MutationPayload {
    last_insert_rowid: Option<i64>,
}

impl PackagePreview {
    fn from_local(package: LincePackage) -> Self {
        let filename = package.archive_filename();
        let frame_src = local_package_frame_src(&filename);
        package_preview(package, frame_src)
    }

    async fn from_ephemeral(state: &AppState, package: LincePackage) -> Self {
        let preview_id = state.package_previews.store(package.clone()).await;
        let frame_src = preview_package_frame_src(&preview_id);
        package_preview(package, frame_src)
    }
}

pub async fn list_local_packages(State(state): State<AppState>) -> ApiResult<Json<Vec<InstalledPackageSummary>>> {
    let packages = state.packages.list().map_err(|message| {
        crate::presentation::http::api_error::api_error(StatusCode::BAD_GATEWAY, message)
    })?;

    Ok(Json(packages))
}

pub async fn get_local_package(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
) -> ApiResult<Json<PackagePreview>> {
    let package = state.packages.load(&package_id).map_err(|message| {
        crate::presentation::http::api_error::api_error(StatusCode::NOT_FOUND, message)
    })?;

    Ok(Json(PackagePreview::from_local(package)))
}

pub async fn get_local_package_content(
    State(state): State<AppState>,
    Path((filename, asset_path)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let package = state.packages.load_by_filename(&filename).map_err(|message| {
        crate::presentation::http::api_error::api_error(StatusCode::NOT_FOUND, message)
    })?;
    let content_root_url = format!(
        "/host/packages/local/by-filename/{}/content",
        urlencoding::encode(&filename)
    );
    serve_package_asset(&package, &asset_path, &content_root_url)
}

pub async fn get_preview_package_content(
    State(state): State<AppState>,
    Path((preview_id, asset_path)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let package = state.package_previews.get(&preview_id).await.ok_or_else(|| {
        crate::presentation::http::api_error::api_error(
            StatusCode::NOT_FOUND,
            "Esse preview de widget expirou.",
        )
    })?;
    let content_root_url = format!(
        "/host/packages/previews/{}/content",
        urlencoding::encode(&preview_id)
    );
    serve_package_asset(&package, &asset_path, &content_root_url)
}

pub async fn get_dna_catalog(State(state): State<AppState>) -> ApiResult<Json<DnaCatalogStatus>> {
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    Path((channel, package_name)): Path<(String, String)>,
) -> ApiResult<Json<PackagePreview>> {
    let package = state
        .dna_hub
        .preview_package(&channel, &package_name)
        .await
        .map_err(map_hub_error)?;
    Ok(Json(PackagePreview::from_ephemeral(&state, package).await))
}

pub async fn install_dna_package(
    State(state): State<AppState>,
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
    Ok(Json(PackagePreview::from_local(package)))
}

pub async fn preview_package(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<PackagePreview>> {
    while let Some(field) = multipart.next_field().await.map_err(invalid_multipart)? {
        let Some(filename) = field.file_name().map(ToOwned::to_owned) else {
            continue;
        };

        let bytes = field.bytes().await.map_err(invalid_multipart)?;
        validate_package_upload(&filename, &bytes).map_err(map_validation_error)?;
        let package = parse_lince_package(&filename, &bytes).map_err(map_validation_error)?;
        return Ok(Json(PackagePreview::from_ephemeral(&state, package).await));
    }

    Err(crate::presentation::http::api_error::api_error(
        StatusCode::BAD_REQUEST,
        "Nenhum widget foi enviado.",
    ))
}

pub async fn install_package(
    State(state): State<AppState>,
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
        return Ok(Json(PackagePreview::from_local(package)));
    }

    Err(crate::presentation::http::api_error::api_error(
        StatusCode::BAD_REQUEST,
        "Nenhum widget foi enviado.",
    ))
}

async fn parse_publish_multipart(multipart: &mut Multipart) -> ApiResult<PublishMultipartPayload> {
    let mut server_id = String::new();
    let mut channel = "official".to_string();
    let mut head = String::new();
    let mut body = String::new();
    let mut upload = None;

    while let Some(field) = multipart.next_field().await.map_err(invalid_multipart)? {
        let field_name = field.name().unwrap_or_default().trim().to_string();
        if field_name == "file" {
            let Some(filename) = field.file_name().map(ToOwned::to_owned) else {
                continue;
            };
            let bytes = field.bytes().await.map_err(invalid_multipart)?;
            upload = Some(PublishUpload {
                filename,
                bytes: bytes.to_vec(),
            });
            continue;
        }

        let value = field.text().await.map_err(invalid_multipart)?;
        match field_name.as_str() {
            "serverId" => server_id = value.trim().to_string(),
            "channel" => channel = value.trim().to_string(),
            "head" => head = value.trim().to_string(),
            "body" => body = value.trim().to_string(),
            _ => {}
        }
    }

    let upload = upload.ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "Escolha um sand .html, .sand ou .lince para publicar.",
        )
    })?;
    validate_package_upload(&upload.filename, &upload.bytes).map_err(map_validation_error)?;

    if server_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Escolha um organo no card antes de publicar.",
        ));
    }

    Ok(PublishMultipartPayload {
        server_id,
        channel,
        head,
        body,
        upload,
    })
}

async fn load_publish_organ(state: &AppState, server_id: &str) -> ApiResult<Organ> {
    state
        .organs
        .get(server_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Organ nao encontrado."))
}

fn fallback_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.trim().to_string()
    } else {
        value.to_string()
    }
}

fn package_name_seed(filename: &str, package: &LincePackage) -> String {
    let base = package_id_from_filename(filename);
    if base.trim().is_empty() || base == "index" || base == "widget" {
        package.manifest.title.clone()
    } else {
        base
    }
}

fn normalize_package_slug(raw: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in raw.trim().chars() {
        let normalized = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            _ if ch.is_ascii_whitespace() || ch == '-' || ch == '_' => Some('_'),
            _ => None,
        };
        let Some(normalized) = normalized else {
            continue;
        };

        if normalized == '_' {
            if slug.is_empty() || last_was_separator {
                continue;
            }
            last_was_separator = true;
        } else {
            last_was_separator = false;
        }

        slug.push(normalized);
    }

    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "lince_sand".to_string()
    } else {
        slug.to_string()
    }
}

fn package_prefix_letters(slug: &str) -> String {
    let mut chars = slug.chars().filter(|ch| ch.is_ascii_alphanumeric());
    let first = chars.next().unwrap_or('x');
    let second = chars.next().unwrap_or(first);
    format!("{first}{second}")
}

fn validate_publish_channel(raw: &str) -> ApiResult<String> {
    match raw.trim() {
        "official" => Ok("official".to_string()),
        "community" => Ok("community".to_string()),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "Channel invalido. Use official ou community.",
        )),
    }
}

fn canonical_transport_filename(slug: &str, transport: PackageTransport) -> String {
    match transport {
        PackageTransport::Archive => format!("{slug}.lince"),
        PackageTransport::Html => format!("{slug}{HTML_TRANSPORT_FILENAME_SUFFIX}"),
    }
}

fn package_format_label(transport: PackageTransport) -> &'static str {
    match transport {
        PackageTransport::Archive => "lince",
        PackageTransport::Html => "html",
    }
}

fn package_content_type(transport: PackageTransport) -> &'static str {
    match transport {
        PackageTransport::Archive => "application/zip",
        PackageTransport::Html => "text/html; charset=utf-8",
    }
}

fn build_remote_sand_toml(slug: &str, channel: &str) -> String {
    format!("name = {slug:?}\nchannel = {channel:?}\n")
}

fn build_lince_transport_bytes(package: &LincePackage) -> Result<Vec<u8>, String> {
    crate::domain::lince_package::build_lince_archive(package)
}

async fn upload_package_artifacts(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    bucket_key: &str,
    bucket_bytes: Vec<u8>,
    bucket_content_type: &str,
    sand_toml_key: &str,
    sand_toml_bytes: Vec<u8>,
) -> ApiResult<()> {
    upload_bucket_object(
        state,
        headers,
        server,
        bucket_key,
        bucket_bytes,
        bucket_content_type,
    )
    .await?;
    if let Err(error) = upload_bucket_object(
        state,
        headers,
        server,
        sand_toml_key,
        sand_toml_bytes,
        "application/toml; charset=utf-8",
    )
    .await
    {
        let _ = delete_bucket_object(state, headers, server, bucket_key).await;
        return Err(error);
    }

    Ok(())
}

async fn persist_dna_publication(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    head: &str,
    body: &str,
    channel: &str,
    slug: &str,
    package_prefix: &str,
    bucket_key: &str,
    sand_toml_key: &str,
    transport_filename: &str,
    package: &LincePackage,
) -> ApiResult<(i64, i64, i64)> {
    let record_id = create_table_row(
        state,
        headers,
        server,
        "record",
        json!({
            "quantity": 1,
            "head": head,
            "body": body,
        }),
    )
    .await?;

    let resource_ref_result = create_table_row(
        state,
        headers,
        server,
        "record_resource_ref",
        json!({
            "record_id": record_id,
            "provider": "bucket",
            "resource_kind": "sand",
            "resource_path": bucket_key,
            "title": package.manifest.title.clone(),
            "position": 1,
            "freestyle_data_structure": serde_json::to_string(&json!({
                "role": "canonical_transport",
                "slug": slug,
                "channel": channel,
                "package_prefix": package_prefix,
                "transport_filename": transport_filename,
                "package_format": package_format_label(package.transport()),
                "mime_type": package_content_type(package.transport()),
                "entry_path": package.entry_path(),
                "sand_toml_key": sand_toml_key,
                "available_files": [transport_filename, SAND_TOML_FILENAME],
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        }),
    )
    .await;

    let resource_ref_id = match resource_ref_result {
        Ok(value) => value,
        Err(error) => {
            let _ = delete_table_row(state, headers, server, "record", record_id).await;
            return Err(error);
        }
    };

    let extension_result = create_table_row(
        state,
        headers,
        server,
        "record_extension",
        json!({
            "record_id": record_id,
            "namespace": DNA_RESOURCE_NAMESPACE,
            "version": 1,
            "freestyle_data_structure": serde_json::to_string(&json!({
                "published": true,
                "channel": channel,
                "slug": slug,
                "version": package.manifest.version.clone(),
                "canonical_resource_ref_id": resource_ref_id,
                "package_prefix": package_prefix,
                "default_transport": package_format_label(package.transport()),
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        }),
    )
    .await;

    let record_extension_id = match extension_result {
        Ok(value) => value,
        Err(error) => {
            let _ = delete_table_row(state, headers, server, "record", record_id).await;
            return Err(error);
        }
    };

    Ok((record_id, record_extension_id, resource_ref_id))
}

async fn create_table_row(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    table_name: &str,
    payload: Value,
) -> ApiResult<i64> {
    if !organ_requires_auth(server, state.local_auth_required) {
        let outcome = state
            .backend
            .create_table_row(&local_host_subject(), table_name, payload_object(&payload)?)
            .await
            .map_err(map_backend_error)?;
        return outcome.last_insert_rowid.ok_or_else(|| {
            api_error(
                StatusCode::BAD_GATEWAY,
                "O backend nao retornou o id da linha criada.",
            )
        });
    }

    let session_token = current_session_token(headers);
    let bearer_token = extract_remote_token(state, headers, &server.id).await?;
    let response = state
        .manas
        .send_table_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::POST,
            table_name,
            None,
            Some(payload),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let payload: MutationPayload =
        proxy_remote_json_response(state, session_token.as_deref(), &server.id, response).await?;
    payload.last_insert_rowid.ok_or_else(|| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "O organ remoto nao retornou o id da linha criada.",
        )
    })
}

async fn delete_table_row(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    table_name: &str,
    id: i64,
) -> ApiResult<()> {
    if !organ_requires_auth(server, state.local_auth_required) {
        state
            .backend
            .delete_table_row(&local_host_subject(), table_name, id)
            .await
            .map_err(map_backend_error)?;
        return Ok(());
    }

    let session_token = current_session_token(headers);
    let bearer_token = extract_remote_token(state, headers, &server.id).await?;
    let response = state
        .manas
        .send_table_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::DELETE,
            table_name,
            Some(id),
            None,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let _: MutationPayload =
        proxy_remote_json_response(state, session_token.as_deref(), &server.id, response).await?;
    Ok(())
}

async fn upload_bucket_object(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> ApiResult<()> {
    if !organ_requires_auth(server, state.local_auth_required) {
        state
            .backend
            .upload_file(key, body, Some(content_type))
            .await
            .map_err(map_backend_error)?;
        return Ok(());
    }

    let session_token = current_session_token(headers);
    let bearer_token = extract_remote_token(state, headers, &server.id).await?;
    let link_response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::POST,
            "/api/files/upload-link",
            Some(json!({ "key": key })),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let link = extract_remote_link(state, session_token.as_deref(), &server.id, link_response).await?;
    let upload_response = state
        .manas
        .send_backend_bytes_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::PUT,
            &link.url,
            body,
            Some(content_type),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    ensure_empty_remote_success(state, session_token.as_deref(), &server.id, upload_response, "Nao foi possivel enviar o sand para o bucket remoto.").await
}

async fn delete_bucket_object(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    key: &str,
) -> ApiResult<()> {
    if !organ_requires_auth(server, state.local_auth_required) {
        state.backend.delete_file(key).await.map_err(map_backend_error)?;
        return Ok(());
    }

    let session_token = current_session_token(headers);
    let bearer_token = extract_remote_token(state, headers, &server.id).await?;
    let link_response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::POST,
            "/api/files/delete-link",
            Some(json!({ "key": key })),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let link = extract_remote_link(state, session_token.as_deref(), &server.id, link_response).await?;
    let delete_response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::DELETE,
            &link.url,
            None,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    ensure_empty_remote_success(state, session_token.as_deref(), &server.id, delete_response, "Nao foi possivel limpar os arquivos publicados no bucket remoto.").await
}

fn current_session_token(headers: &HeaderMap) -> Option<String> {
    parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    )
}

async fn extract_remote_token(
    state: &AppState,
    headers: &HeaderMap,
    server_id: &str,
) -> ApiResult<String> {
    let session_token = current_session_token(headers);
    let Some(session) = state
        .auth
        .server_session(session_token.as_deref(), server_id)
        .await
    else {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Esse organ exige autenticacao. Conecte a sessao primeiro.",
        ));
    };

    Ok(session.bearer_token)
}

async fn extract_remote_link(
    state: &AppState,
    session_token: Option<&str>,
    server_id: &str,
    response: reqwest::Response,
) -> ApiResult<LinkPayload> {
    let payload: LinkPayload =
        proxy_remote_json_response(state, session_token, server_id, response).await?;
    if payload.url.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            "O organ remoto nao retornou um link de acesso ao bucket.",
        ));
    }
    Ok(payload)
}

async fn ensure_empty_remote_success(
    state: &AppState,
    session_token: Option<&str>,
    server_id: &str,
    response: reqwest::Response,
    default_message: &str,
) -> ApiResult<()> {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if status == StatusCode::UNAUTHORIZED {
        state
            .auth
            .expire_server_session(
                session_token,
                server_id,
                "Sessao remota expirada. Conecte esse servidor novamente.",
            )
            .await;
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Sessao remota expirada. Conecte esse servidor novamente.",
        ));
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(api_error(
            status,
            if body.trim().is_empty() {
                default_message.to_string()
            } else {
                body
            },
        ));
    }

    Ok(())
}

async fn proxy_remote_json_response<T: serde::de::DeserializeOwned>(
    state: &AppState,
    session_token: Option<&str>,
    server_id: &str,
    response: reqwest::Response,
) -> ApiResult<T> {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if status == StatusCode::UNAUTHORIZED {
        state
            .auth
            .expire_server_session(
                session_token,
                server_id,
                "Sessao remota expirada. Conecte esse servidor novamente.",
            )
            .await;
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Sessao remota expirada. Conecte esse servidor novamente.",
        ));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(api_error(
            status,
            if body.trim().is_empty() {
                "O organ remoto recusou a operacao.".to_string()
            } else {
                body
            },
        ));
    }
    response
        .json::<T>()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))
}

fn payload_object(payload: &Value) -> ApiResult<&serde_json::Map<String, Value>> {
    payload
        .as_object()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON object payload"))
}

fn local_host_subject() -> AuthSubject {
    AuthSubject {
        user_id: 0,
        username: "local-host".into(),
        role_id: 0,
        role: "admin".into(),
    }
}

fn map_backend_error(
    error: Error,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    let status = match error.kind() {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    api_error(status, error.to_string())
}

pub async fn publish_dna_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<Json<DnaPublishResponse>> {
    let payload = parse_publish_multipart(&mut multipart).await?;
    let server = load_publish_organ(&state, &payload.server_id).await?;
    let package = parse_lince_package(&payload.upload.filename, &payload.upload.bytes)
        .map_err(map_validation_error)?;
    let head = fallback_text(&payload.head, &package.manifest.title);
    let body = fallback_text(&payload.body, &package.manifest.description);
    let slug = normalize_package_slug(&package_name_seed(&payload.upload.filename, &package));
    let channel = validate_publish_channel(&payload.channel)?;
    let package_prefix = format!(
        "{DNA_BUCKET_PREFIX}/{channel}/{}/{slug}",
        package_prefix_letters(&slug)
    );
    let transport_filename = canonical_transport_filename(&slug, package.transport());
    let bucket_key = format!("{package_prefix}/{transport_filename}");
    let sand_toml_key = format!("{package_prefix}/{SAND_TOML_FILENAME}");
    let package_format = package_format_label(package.transport()).to_string();
    let package_bytes = build_lince_transport_bytes(&package).map_err(map_validation_error)?;
    let transport_content_type = package_content_type(package.transport());
    let sand_toml = build_remote_sand_toml(&slug, &channel);

    upload_package_artifacts(
        &state,
        &headers,
        &server,
        &bucket_key,
        package_bytes,
        transport_content_type,
        &sand_toml_key,
        sand_toml.into_bytes(),
    )
    .await?;

    let record_result = persist_dna_publication(
        &state,
        &headers,
        &server,
        &head,
        &body,
        &channel,
        &slug,
        &package_prefix,
        &bucket_key,
        &sand_toml_key,
        &transport_filename,
        &package,
    )
    .await;

    let (record_id, record_extension_id, resource_ref_id) = match record_result {
        Ok(value) => value,
        Err(error) => {
            let _ = delete_bucket_object(&state, &headers, &server, &bucket_key).await;
            let _ = delete_bucket_object(&state, &headers, &server, &sand_toml_key).await;
            return Err(error);
        }
    };

    Ok(Json(DnaPublishResponse {
        ok: true,
        organ_id: server.id,
        record_id,
        record_extension_id,
        resource_ref_id,
        head,
        body,
        slug,
        channel,
        package_prefix,
        bucket_key,
        sand_toml_key,
        transport_filename,
        package_format,
    }))
}

fn package_preview(package: LincePackage, frame_src: String) -> PackagePreview {
    let filename = package.archive_filename();
    let id = package_id_from_filename(&filename);
    let LincePackage { manifest, html, .. } = package;
    PackagePreview {
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
        frame_src,
    }
}

fn serve_package_asset(
    package: &LincePackage,
    asset_path: &str,
    content_root_url: &str,
) -> ApiResult<Response> {
    let asset_path = normalize_asset_path(asset_path)
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;

    let (bytes, content_type) = if asset_path == "index.html" || asset_path == package.entry_path()
    {
        (
            inject_package_html(&package.html_document(), package.entry_path(), content_root_url)
                .into_bytes(),
            "text/html; charset=utf-8",
        )
    } else if asset_path == "config.toml" {
        (
            package
                .manifest_toml()
                .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
                .into_bytes(),
            "application/toml; charset=utf-8",
        )
    } else if let Some(bytes) = package.asset_bytes(&asset_path) {
        (bytes.to_vec(), content_type_for_path(&asset_path))
    } else {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Esse arquivo interno do widget nao existe.",
        ));
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    Ok((headers, Body::from(bytes)).into_response())
}

fn inject_package_html(raw_html: &str, entry_path: &str, content_root_url: &str) -> String {
    if raw_html.contains("window.__LINCE_WIDGET_HOST__")
        || raw_html.contains("widget-frame-bootstrap.js")
    {
        return ensure_base_href(raw_html, entry_path, content_root_url);
    }

    let datastar_script = if raw_html.contains("datastar.js") {
        ""
    } else {
        DATASTAR_BOOTSTRAP_SCRIPT
    };
    let injections = [datastar_script, WIDGET_BOOTSTRAP_SCRIPT]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let html = ensure_base_href(raw_html, entry_path, content_root_url);

    if html.contains("</body>") {
        return html.replace("</body>", &format!("{injections}\n</body>"));
    }

    if html.contains("</html>") {
        return html.replace("</html>", &format!("{injections}\n</html>"));
    }

    format!("{html}\n{injections}")
}

fn local_package_frame_src(filename: &str) -> String {
    local_package_content_url(filename, "index.html")
}

fn local_package_content_url(filename: &str, asset_path: &str) -> String {
    format!(
        "/host/packages/local/by-filename/{}/content/{}",
        urlencoding::encode(filename),
        encode_asset_path(asset_path)
    )
}

fn preview_package_frame_src(preview_id: &str) -> String {
    preview_package_content_url(preview_id, "index.html")
}

fn preview_package_content_url(preview_id: &str, asset_path: &str) -> String {
    format!(
        "/host/packages/previews/{}/content/{}",
        urlencoding::encode(preview_id),
        encode_asset_path(asset_path)
    )
}

fn encode_asset_path(asset_path: &str) -> String {
    asset_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(urlencoding::encode)
        .map(|segment| segment.into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn content_type_for_path(asset_path: &str) -> &'static str {
    match path_extension_lower(asset_path).as_deref() {
        Some("css") => "text/css; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("gif") => "image/gif",
        Some("htm") | Some("html") => "text/html; charset=utf-8",
        Some("jpeg") | Some("jpg") => "image/jpeg",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("md") | Some("txt") => "text/plain; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("toml") => "application/toml; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn path_extension_lower(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn ensure_base_href(raw_html: &str, entry_path: &str, content_root_url: &str) -> String {
    let parent_path = std::path::Path::new(entry_path)
        .parent()
        .and_then(|value| value.to_str())
        .map(|value| value.trim_matches('/'))
        .filter(|value| !value.is_empty());

    let Some(parent_path) = parent_path else {
        return raw_html.to_string();
    };

    if raw_html.to_ascii_lowercase().contains("<base ") {
        return raw_html.to_string();
    }

    let href = format!("{content_root_url}/{}/", encode_asset_path(parent_path));
    let base_tag = format!("<base href=\"{href}\">");

    if let Some(head_open_start) = raw_html.to_ascii_lowercase().find("<head")
        && let Some(relative_head_end) = raw_html[head_open_start..].find('>')
    {
        let insert_at = head_open_start + relative_head_end + 1;
        let mut updated = String::with_capacity(raw_html.len() + base_tag.len() + 1);
        updated.push_str(&raw_html[..insert_at]);
        updated.push('\n');
        updated.push_str(&base_tag);
        updated.push_str(&raw_html[insert_at..]);
        return updated;
    }

    format!("{base_tag}\n{raw_html}")
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
