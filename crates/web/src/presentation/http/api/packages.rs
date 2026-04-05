use {
    crate::{
        application::state::AppState,
        domain::lince_package::{
            LincePackage, PackageTransport, normalize_asset_path, package_id_from_filename,
            parse_lince_package, validate_package_upload,
        },
        infrastructure::{
            auth::{parse_cookie_header, session_cookie_name},
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
    chrono::{Duration, NaiveDateTime, Utc},
    semver::Version,
    serde::{Deserialize, Serialize},
    serde_json::{Value, json},
    std::{
        collections::BTreeMap,
        io::{Error, ErrorKind},
    },
};

const DATASTAR_BOOTSTRAP_SCRIPT: &str =
    r#"<script type="module" src="/host/static/vendored/datastar.js"></script>"#;
const WIDGET_BOOTSTRAP_SCRIPT: &str =
    r#"<script src="/host/static/presentation/board/widget-frame-bootstrap.js"></script>"#;
const DNA_BUCKET_PREFIX: &str = "lince/dna/sand";
const DNA_RESOURCE_NAMESPACE: &str = "lince.dna";
const DNA_CATEGORY_NAMESPACE: &str = "record.categories";
const DNA_BASE_CATEGORY: &str = "sand";
const DNA_CHANNEL_OFFICIAL: &str = "official";
const DNA_CHANNEL_COMMUNITY: &str = "community";
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
    pub requires_server: bool,
    pub permissions: Vec<String>,
    pub html: String,
    pub frame_src: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaCatalogStatus {
    pub package_count: usize,
    pub origins: Vec<DnaCatalogOrigin>,
    pub packages: Vec<DnaPackageSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaCatalogOrigin {
    pub organ_id: String,
    pub origin_name: String,
    pub package_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaPackageSummary {
    pub id: String,
    pub record_id: i64,
    pub organ_id: String,
    pub origin_name: String,
    pub head: String,
    pub body: String,
    pub slug: String,
    pub channel: String,
    pub version: String,
    pub bucket_key: String,
    pub package_format: String,
    pub categories: Vec<String>,
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
    pub version: String,
    pub package_prefix: String,
    pub bucket_key: String,
    pub sand_toml_key: String,
    pub transport_filename: String,
    pub package_format: String,
    pub categories: Vec<String>,
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
    categories: Vec<String>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaDeleteResponse {
    pub ok: bool,
    pub organ_id: String,
    pub record_id: i64,
    pub deleted_bucket_keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DnaRecordRow {
    id: i64,
    #[serde(default)]
    quantity: f64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    head: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    body: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DnaRecordExtensionRow {
    id: i64,
    record_id: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    namespace: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    freestyle_data_structure: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DnaResourceRefRow {
    id: i64,
    record_id: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    provider: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    resource_kind: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    resource_path: String,
    #[serde(default)]
    freestyle_data_structure: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    created_at: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DnaExtensionMeta {
    #[serde(default)]
    published: bool,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    slug: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    channel: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    version: String,
    #[serde(default)]
    canonical_resource_ref_id: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
struct DnaResourceMeta {
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    slug: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    channel: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    version: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    package_prefix: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    transport_filename: String,
    #[serde(default)]
    package_format: Option<String>,
    #[serde(default)]
    sand_toml_key: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    role: String,
}

#[derive(Debug, Clone)]
struct DnaPublication {
    summary: DnaPackageSummary,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct DnaPublicationState {
    record_id: i64,
    record_extension_id: i64,
    categories_extension_id: Option<i64>,
    version: String,
    canonical_resource_ref: Option<DnaResourceRefRow>,
    resource_refs: Vec<DnaResourceRefRow>,
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

pub async fn get_dna_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<DnaCatalogStatus>> {
    let snapshot = load_dna_catalog_snapshot(&state, &headers).await?;
    Ok(Json(snapshot))
}

pub async fn search_dna_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DnaPackageSearchQuery>,
) -> ApiResult<Json<Vec<DnaPackageSummary>>> {
    let packages = load_dna_catalog_packages(&state, &headers, query.q.as_deref()).await?;
    Ok(Json(packages))
}

pub async fn preview_dna_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((organ_id, record_id)): Path<(String, i64)>,
) -> ApiResult<Json<PackagePreview>> {
    let publication = load_dna_publication(&state, &headers, &organ_id, record_id).await?;
    let package = load_publication_package(&state, &headers, &publication).await?;
    Ok(Json(PackagePreview::from_ephemeral(&state, package).await))
}

pub async fn install_dna_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((organ_id, record_id)): Path<(String, i64)>,
) -> ApiResult<Json<PackagePreview>> {
    let publication = load_dna_publication(&state, &headers, &organ_id, record_id).await?;
    let package = load_publication_package(&state, &headers, &publication).await?;
    state
        .packages
        .persist_package(&package)
        .map_err(map_validation_error)?;
    Ok(Json(PackagePreview::from_local(package)))
}

pub async fn delete_dna_publication(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((organ_id, record_id)): Path<(String, i64)>,
) -> ApiResult<Json<DnaDeleteResponse>> {
    let _publication = load_dna_publication(&state, &headers, &organ_id, record_id).await?;
    let organ = load_publish_organ(&state, &organ_id).await?;
    let mut deleted_bucket_keys = std::collections::BTreeSet::new();

    for resource_ref in list_sand_resource_refs_for_record(&state, &headers, &organ, record_id).await? {
        delete_bucket_object_if_exists(&state, &headers, &organ, &resource_ref.resource_path).await?;
        deleted_bucket_keys.insert(resource_ref.resource_path.clone());
        let resource_meta = parse_resource_meta(resource_ref.freestyle_data_structure.as_deref());
        if let Some(sand_toml_key) = resource_meta.sand_toml_key.as_deref() {
            delete_bucket_object_if_exists(&state, &headers, &organ, sand_toml_key).await?;
            deleted_bucket_keys.insert(sand_toml_key.to_string());
        }
    }

    delete_table_row(&state, &headers, &organ, "record", record_id).await?;

    Ok(Json(DnaDeleteResponse {
        ok: true,
        organ_id,
        record_id,
        deleted_bucket_keys: deleted_bucket_keys.into_iter().collect(),
    }))
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

async fn load_dna_catalog_packages(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> ApiResult<Vec<DnaPackageSummary>> {
    let mut packages = load_dna_catalog_snapshot(state, headers).await?.packages;
    let normalized_query = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    if let Some(query) = normalized_query.as_deref() {
        packages.retain(|package| matches_dna_query(package, query));
    }

    packages.sort_by(|left, right| {
        channel_sort_rank(&left.channel)
            .cmp(&channel_sort_rank(&right.channel))
            .then_with(|| {
                left.head
                    .to_ascii_lowercase()
                    .cmp(&right.head.to_ascii_lowercase())
            })
            .then_with(|| left.origin_name.to_ascii_lowercase().cmp(&right.origin_name.to_ascii_lowercase()))
            .then_with(|| left.record_id.cmp(&right.record_id))
    });

    Ok(packages)
}

async fn load_dna_catalog_snapshot(
    state: &AppState,
    headers: &HeaderMap,
) -> ApiResult<DnaCatalogStatus> {
    let mut packages = Vec::new();
    let mut origins = Vec::new();

    for organ in accessible_dna_organs(state, headers).await? {
        let mut organ_publications = collect_organ_publications(state, headers, &organ).await?;
        let package_count = organ_publications.len();
        origins.push(DnaCatalogOrigin {
            organ_id: organ.id.clone(),
            origin_name: organ.name.clone(),
            package_count,
        });
        packages.append(&mut organ_publications);
    }

    packages.sort_by(|left, right| {
        channel_sort_rank(&left.summary.channel)
            .cmp(&channel_sort_rank(&right.summary.channel))
            .then_with(|| {
                left.summary
                    .head
                    .to_ascii_lowercase()
                    .cmp(&right.summary.head.to_ascii_lowercase())
            })
            .then_with(|| {
                left.summary
                    .origin_name
                    .to_ascii_lowercase()
                    .cmp(&right.summary.origin_name.to_ascii_lowercase())
            })
            .then_with(|| left.summary.record_id.cmp(&right.summary.record_id))
    });
    origins.sort_by(|left, right| {
        left.origin_name
            .to_ascii_lowercase()
            .cmp(&right.origin_name.to_ascii_lowercase())
            .then_with(|| left.organ_id.cmp(&right.organ_id))
    });

    Ok(DnaCatalogStatus {
        package_count: packages.len(),
        origins,
        packages: packages.into_iter().map(|publication| publication.summary).collect(),
    })
}

async fn accessible_dna_organs(state: &AppState, headers: &HeaderMap) -> ApiResult<Vec<Organ>> {
    let mut organs = Vec::new();
    for organ in state
        .organs
        .list()
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
    {
        if organ_requires_auth(&organ, state.local_auth_required)
            && state
                .auth
                .server_session(current_session_token(headers).as_deref(), &organ.id)
                .await
                .is_none()
        {
            continue;
        }

        organs.push(organ);
    }

    Ok(organs)
}

async fn load_dna_publication(
    state: &AppState,
    headers: &HeaderMap,
    organ_id: &str,
    record_id: i64,
) -> ApiResult<DnaPublication> {
    let organ = load_publish_organ(state, organ_id).await?;
    if organ_requires_auth(&organ, state.local_auth_required)
        && state
            .auth
            .server_session(current_session_token(headers).as_deref(), &organ.id)
            .await
            .is_none()
    {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Esse organ exige autenticacao para listar ou instalar sand.",
        ));
    }

    collect_organ_publications(state, headers, &organ)
        .await?
        .into_iter()
        .find(|publication| publication.summary.record_id == record_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Sand publicado nao encontrado."))
}

async fn collect_organ_publications(
    state: &AppState,
    headers: &HeaderMap,
    organ: &Organ,
) -> ApiResult<Vec<DnaPublication>> {
    let records = list_table_rows::<DnaRecordRow>(state, headers, organ, "record").await?;
    let extensions =
        list_table_rows::<DnaRecordExtensionRow>(state, headers, organ, "record_extension").await?;
    let resource_refs =
        list_table_rows::<DnaResourceRefRow>(state, headers, organ, "record_resource_ref").await?;

    let record_by_id = records
        .into_iter()
        .filter(|record| record.quantity > 0.0)
        .map(|record| (record.id, record))
        .collect::<BTreeMap<_, _>>();
    let resource_ref_by_id = resource_refs
        .iter()
        .cloned()
        .map(|row| (row.id, row))
        .collect::<BTreeMap<_, _>>();
    let refs_by_record = resource_refs.into_iter().fold(
        BTreeMap::<i64, Vec<DnaResourceRefRow>>::new(),
        |mut acc, row| {
            acc.entry(row.record_id).or_default().push(row);
            acc
        },
    );
    let categories_by_record = extensions
        .iter()
        .filter(|row| row.namespace == DNA_CATEGORY_NAMESPACE)
        .filter_map(|row| {
            extract_categories(&row.freestyle_data_structure)
                .map(|categories| (row.record_id, normalize_categories(categories)))
        })
        .collect::<BTreeMap<_, _>>();

    let mut publications = Vec::new();

    for extension in extensions
        .into_iter()
        .filter(|row| row.namespace == DNA_RESOURCE_NAMESPACE)
    {
        let meta = parse_extension_meta(&extension.freestyle_data_structure);
        if !meta.published {
            continue;
        }
        let Some(record) = record_by_id.get(&extension.record_id) else {
            continue;
        };

        let canonical_ref = meta
            .canonical_resource_ref_id
            .and_then(|resource_id| resource_ref_by_id.get(&resource_id).cloned())
            .or_else(|| {
                refs_by_record.get(&record.id).and_then(|rows| {
                    rows.iter()
                        .find(|row| row.provider == "bucket" && row.resource_kind == "sand")
                        .cloned()
                })
            });

        let Some(resource_ref) = canonical_ref else {
            continue;
        };
        let resource_meta = parse_resource_meta(resource_ref.freestyle_data_structure.as_deref());
        let slug = first_non_empty(&[
            meta.slug.as_str(),
            resource_meta.slug.as_str(),
            record.head.as_str(),
        ])
        .map(normalize_package_slug)
        .unwrap_or_else(|| "lince_sand".to_string());
        let package_format = resource_meta
            .package_format
            .clone()
            .unwrap_or_else(|| infer_package_format_from_path(&resource_ref.resource_path).to_string());
        let channel = normalize_publication_channel_or_default(
            first_non_empty(&[meta.channel.as_str(), resource_meta.channel.as_str()])
                .or_else(|| infer_channel_from_path(&resource_ref.resource_path)),
        );
        let version = first_non_empty(&[meta.version.as_str(), resource_meta.version.as_str()])
            .unwrap_or("0.1.0")
            .trim()
            .to_string();
        let categories = categories_by_record
            .get(&record.id)
            .cloned()
            .filter(|entries| !entries.is_empty())
            .unwrap_or_else(|| publication_categories(Vec::new()));

        publications.push(DnaPublication {
            summary: DnaPackageSummary {
                id: format!("{}:{}", organ.id, record.id),
                record_id: record.id,
                organ_id: organ.id.clone(),
                origin_name: organ.name.clone(),
                head: record.head.clone(),
                body: record.body.clone(),
                slug,
                channel,
                version,
                bucket_key: resource_ref.resource_path.clone(),
                package_format,
                categories,
            },
        });
    }

    Ok(publications)
}

async fn list_table_rows<T: serde::de::DeserializeOwned>(
    state: &AppState,
    headers: &HeaderMap,
    organ: &Organ,
    table_name: &str,
) -> ApiResult<Vec<T>> {
    if !organ_requires_auth(organ, state.local_auth_required) {
        let payload = state
            .backend
            .list_table_rows(&local_host_subject(), table_name)
            .await
            .map_err(map_backend_error)?;
        return serde_json::from_value(payload)
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()));
    }

    let session_token = current_session_token(headers);
    let bearer_token = extract_remote_token(state, headers, &organ.id).await?;
    let response = state
        .manas
        .send_table_request(
            &organ.base_url,
            &bearer_token,
            reqwest::Method::GET,
            table_name,
            None,
            None,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    proxy_remote_json_response(state, session_token.as_deref(), &organ.id, response).await
}

fn parse_extension_meta(raw: &str) -> DnaExtensionMeta {
    serde_json::from_str(raw).unwrap_or_default()
}

fn parse_resource_meta(raw: Option<&str>) -> DnaResourceMeta {
    raw.and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

fn first_non_empty<'a>(values: &'a [&'a str]) -> Option<&'a str> {
    values
        .iter()
        .copied()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn infer_package_format_from_path(path: &str) -> &'static str {
    if path.to_ascii_lowercase().ends_with(".lince") {
        "lince"
    } else {
        "html"
    }
}

fn matches_dna_query(summary: &DnaPackageSummary, query: &str) -> bool {
    [
        summary.id.as_str(),
        summary.organ_id.as_str(),
        summary.origin_name.as_str(),
        summary.head.as_str(),
        summary.body.as_str(),
        summary.slug.as_str(),
        summary.channel.as_str(),
        summary.version.as_str(),
        summary.bucket_key.as_str(),
    ]
    .into_iter()
    .chain(summary.categories.iter().map(String::as_str))
    .any(|value| value.to_ascii_lowercase().contains(query))
}

async fn load_publication_package(
    state: &AppState,
    headers: &HeaderMap,
    publication: &DnaPublication,
) -> ApiResult<LincePackage> {
    let organ = load_publish_organ(state, &publication.summary.organ_id).await?;
    let bytes = download_bucket_object(state, headers, &organ, &publication.summary.bucket_key).await?;
    parse_lince_package(
        publication
            .summary
            .bucket_key
            .rsplit('/')
            .next()
            .unwrap_or("sand.html"),
        &bytes,
    )
    .map_err(map_validation_error)
}

async fn parse_publish_multipart(multipart: &mut Multipart) -> ApiResult<PublishMultipartPayload> {
    let mut server_id = String::new();
    let mut channel = DNA_CHANNEL_OFFICIAL.to_string();
    let mut head = String::new();
    let mut body = String::new();
    let mut categories = Vec::new();
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
            "channel" => channel = normalize_publication_channel(&value).map_err(map_validation_error)?,
            "head" => head = value.trim().to_string(),
            "body" => body = value.trim().to_string(),
            "categories" => categories = parse_categories_field(&value),
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
        categories,
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

fn build_remote_sand_toml(slug: &str, version: &str, channel: &str) -> String {
    format!("name = {slug:?}\nversion = {version:?}\nchannel = {channel:?}\n")
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

async fn upsert_dna_publication(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    head: &str,
    body: &str,
    slug: &str,
    channel: &str,
    categories: &[String],
    package_prefix: &str,
    bucket_key: &str,
    sand_toml_key: &str,
    transport_filename: &str,
    package: &LincePackage,
    package_bytes: &[u8],
    sand_toml_bytes: &[u8],
) -> ApiResult<(i64, i64, i64)> {
    let desired_version = parse_release_version(&package.manifest.version)?;
    let existing = find_existing_publication_state(state, headers, server, slug, channel).await?;
    let resource_meta = build_resource_meta_json(
        "canonical_transport",
        slug,
        channel,
        &package.manifest.version,
        package_prefix,
        transport_filename,
        sand_toml_key,
        package,
    );

    if let Some(existing) = existing {
        let current_ref = existing.canonical_resource_ref.clone();
        let current_version = parse_release_version(&existing.version)?;
        if desired_version < current_version {
            return Err(api_error(
                StatusCode::CONFLICT,
                format!(
                    "Esse sand ja foi publicado com a versao {}. Downgrade para {} foi bloqueado.",
                    existing.version, package.manifest.version
                ),
            ));
        }

        let current_bytes = if let Some(current_ref) = current_ref.as_ref() {
            match download_bucket_object(state, headers, server, &current_ref.resource_path).await {
                Ok(bytes) => Some(bytes),
                Err((StatusCode::NOT_FOUND, _)) => None,
                Err(other) => return Err(other),
            }
        } else {
            None
        };

        if desired_version == current_version
            && current_bytes
                .as_deref()
                .is_some_and(|bytes| bytes != package_bytes)
        {
            return Err(api_error(
                StatusCode::CONFLICT,
                format!(
                    "A versao {} ja existe com bytes diferentes. Publique uma nova versao antes de trocar o artefato.",
                    package.manifest.version
                ),
            ));
        }

        update_table_row(
            state,
            headers,
            server,
            "record",
            existing.record_id,
            json!({
                "quantity": 1,
                "head": head,
                "body": body,
            }),
        )
        .await?;

        let reuse_current_canonical = current_ref.as_ref().is_some_and(|row| {
            row.resource_path == bucket_key && desired_version == current_version
        });

        if reuse_current_canonical && current_bytes.is_none() {
            upload_package_artifacts(
                state,
                headers,
                server,
                bucket_key,
                package_bytes.to_vec(),
                package_content_type(package.transport()),
                sand_toml_key,
                sand_toml_bytes.to_vec(),
            )
            .await?;
        }

        if reuse_current_canonical {
            let canonical_resource_ref_id = current_ref.as_ref().map(|row| row.id).ok_or_else(|| {
                api_error(
                    StatusCode::BAD_GATEWAY,
                    "Nao encontrei a referencia canonica desse sand publicado.",
                )
            })?;
            update_table_row(
                state,
                headers,
                server,
                "record_resource_ref",
                canonical_resource_ref_id,
                json!({
                    "record_id": existing.record_id,
                    "provider": "bucket",
                    "resource_kind": "sand",
                    "resource_path": bucket_key,
                    "title": package.manifest.title.clone(),
                    "position": 1,
                    "freestyle_data_structure": resource_meta,
                }),
            )
            .await?;
            update_table_row(
                state,
                headers,
                server,
                "record_extension",
                existing.record_extension_id,
                json!({
                    "record_id": existing.record_id,
                    "namespace": DNA_RESOURCE_NAMESPACE,
                    "version": 1,
                    "freestyle_data_structure": build_extension_meta_json(
                        slug,
                        channel,
                        &package.manifest.version,
                        canonical_resource_ref_id,
                        package_prefix,
                        package,
                    ),
                }),
            )
            .await?;
            upsert_categories_extension(
                state,
                headers,
                server,
                existing.record_id,
                existing.categories_extension_id,
                categories,
            )
            .await?;
            return Ok((
                existing.record_id,
                existing.record_extension_id,
                canonical_resource_ref_id,
            ));
        }

        upload_package_artifacts(
            state,
            headers,
            server,
            bucket_key,
            package_bytes.to_vec(),
            package_content_type(package.transport()),
            sand_toml_key,
            sand_toml_bytes.to_vec(),
        )
        .await?;

        let new_resource_ref_id = match create_table_row(
            state,
            headers,
            server,
            "record_resource_ref",
            json!({
                "record_id": existing.record_id,
                "provider": "bucket",
                "resource_kind": "sand",
                "resource_path": bucket_key,
                "title": package.manifest.title.clone(),
                "position": 1,
                "freestyle_data_structure": resource_meta,
            }),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                let _ = delete_bucket_object(state, headers, server, bucket_key).await;
                let _ = delete_bucket_object(state, headers, server, sand_toml_key).await;
                return Err(error);
            }
        };

        if let Err(error) = update_table_row(
            state,
            headers,
            server,
            "record_extension",
            existing.record_extension_id,
            json!({
                "record_id": existing.record_id,
                "namespace": DNA_RESOURCE_NAMESPACE,
                "version": 1,
                "freestyle_data_structure": build_extension_meta_json(
                    slug,
                    channel,
                    &package.manifest.version,
                    new_resource_ref_id,
                    package_prefix,
                    package,
                ),
            }),
        )
        .await
        {
            let _ = delete_table_row(
                state,
                headers,
                server,
                "record_resource_ref",
                new_resource_ref_id,
            )
            .await;
            let _ = delete_bucket_object(state, headers, server, bucket_key).await;
            let _ = delete_bucket_object(state, headers, server, sand_toml_key).await;
            return Err(error);
        }

        if let Some(previous_ref) = current_ref.as_ref() {
            let mut previous_meta_value = previous_ref
                .freestyle_data_structure
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or_else(|| json!({}));
            if let Some(object) = previous_meta_value.as_object_mut() {
                object.insert("role".into(), Value::String("archived_release".into()));
            }
            update_table_row(
                state,
                headers,
                server,
                "record_resource_ref",
                previous_ref.id,
                json!({
                    "record_id": existing.record_id,
                    "provider": "bucket",
                    "resource_kind": "sand",
                    "resource_path": previous_ref.resource_path.clone(),
                    "title": package.manifest.title.clone(),
                    "position": 2,
                    "freestyle_data_structure": serde_json::to_string(&previous_meta_value)
                        .unwrap_or_else(|_| "{}".to_string()),
                }),
            )
            .await?;
        }

        upsert_categories_extension(
            state,
            headers,
            server,
            existing.record_id,
            existing.categories_extension_id,
            categories,
        )
        .await?;
        prune_old_releases(state, headers, server, existing.record_id, new_resource_ref_id).await?;

        return Ok((
            existing.record_id,
            existing.record_extension_id,
            new_resource_ref_id,
        ));
    }

    upload_package_artifacts(
        state,
        headers,
        server,
        bucket_key,
        package_bytes.to_vec(),
        package_content_type(package.transport()),
        sand_toml_key,
        sand_toml_bytes.to_vec(),
    )
    .await?;

    let record_id = match create_table_row(
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
    .await
    {
        Ok(value) => value,
        Err(error) => {
            let _ = delete_bucket_object(state, headers, server, bucket_key).await;
            let _ = delete_bucket_object(state, headers, server, sand_toml_key).await;
            return Err(error);
        }
    };

    let resource_ref_id = match create_table_row(
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
            "freestyle_data_structure": build_resource_meta_json(
                "canonical_transport",
                slug,
                channel,
                &package.manifest.version,
                package_prefix,
                transport_filename,
                sand_toml_key,
                package,
            ),
        }),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            let _ = delete_table_row(state, headers, server, "record", record_id).await;
            let _ = delete_bucket_object(state, headers, server, bucket_key).await;
            let _ = delete_bucket_object(state, headers, server, sand_toml_key).await;
            return Err(error);
        }
    };

    let record_extension_id = match create_table_row(
        state,
        headers,
        server,
        "record_extension",
        json!({
            "record_id": record_id,
            "namespace": DNA_RESOURCE_NAMESPACE,
            "version": 1,
            "freestyle_data_structure": build_extension_meta_json(
                slug,
                channel,
                &package.manifest.version,
                resource_ref_id,
                package_prefix,
                package,
            ),
        }),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            let _ = delete_table_row(state, headers, server, "record", record_id).await;
            let _ = delete_bucket_object(state, headers, server, bucket_key).await;
            let _ = delete_bucket_object(state, headers, server, sand_toml_key).await;
            return Err(error);
        }
    };

    upsert_categories_extension(
        state,
        headers,
        server,
        record_id,
        None,
        categories,
    )
    .await?;

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

async fn update_table_row(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    table_name: &str,
    id: i64,
    payload: Value,
) -> ApiResult<()> {
    if !organ_requires_auth(server, state.local_auth_required) {
        state
            .backend
            .update_table_row(&local_host_subject(), table_name, id, payload_object(&payload)?)
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
            reqwest::Method::PATCH,
            table_name,
            Some(id),
            Some(payload),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let _: MutationPayload =
        proxy_remote_json_response(state, session_token.as_deref(), &server.id, response).await?;
    Ok(())
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

async fn download_bucket_object(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    key: &str,
) -> ApiResult<Vec<u8>> {
    if !organ_requires_auth(server, state.local_auth_required) {
        let downloaded = state.backend.download_file(key).await.map_err(map_backend_error)?;
        return download_object_bytes(downloaded)
            .await
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()));
    }

    let session_token = current_session_token(headers);
    let bearer_token = extract_remote_token(state, headers, &server.id).await?;
    let link_response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::POST,
            "/api/files/download-link",
            Some(json!({ "key": key })),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let link = extract_remote_link(state, session_token.as_deref(), &server.id, link_response).await?;
    let response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            reqwest::Method::GET,
            &link.url,
            None,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    let response = ensure_remote_response_success(
        state,
        session_token.as_deref(),
        &server.id,
        response,
        "Nao foi possivel baixar o sand publicado nesse organ.",
    )
    .await?;
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))
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

async fn delete_bucket_object_if_exists(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    key: &str,
) -> ApiResult<()> {
    match delete_bucket_object(state, headers, server, key).await {
        Ok(()) => Ok(()),
        Err((StatusCode::NOT_FOUND, _)) => Ok(()),
        Err(other) => Err(other),
    }
}

async fn find_existing_publication_state(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    slug: &str,
    channel: &str,
) -> ApiResult<Option<DnaPublicationState>> {
    let records = list_table_rows::<DnaRecordRow>(state, headers, server, "record").await?;
    let extensions =
        list_table_rows::<DnaRecordExtensionRow>(state, headers, server, "record_extension").await?;
    let resource_refs =
        list_table_rows::<DnaResourceRefRow>(state, headers, server, "record_resource_ref").await?;

    let record_by_id = records
        .into_iter()
        .filter(|record| record.quantity > 0.0)
        .map(|record| (record.id, record))
        .collect::<BTreeMap<_, _>>();
    let refs_by_record =
        resource_refs
            .into_iter()
            .fold(BTreeMap::<i64, Vec<DnaResourceRefRow>>::new(), |mut acc, row| {
                acc.entry(row.record_id).or_default().push(row);
                acc
            });
    let categories_extension_by_record = extensions
        .iter()
        .filter(|row| row.namespace == DNA_CATEGORY_NAMESPACE)
        .fold(BTreeMap::<i64, i64>::new(), |mut acc, row| {
            acc.entry(row.record_id).or_insert(row.id);
            acc
        });

    for extension in extensions
        .into_iter()
        .filter(|row| row.namespace == DNA_RESOURCE_NAMESPACE)
    {
        let meta = parse_extension_meta(&extension.freestyle_data_structure);
        if !meta.published {
            continue;
        }

        let Some(record) = record_by_id.get(&extension.record_id) else {
            continue;
        };
        let resource_refs = refs_by_record
            .get(&record.id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.provider == "bucket" && row.resource_kind == "sand")
            .collect::<Vec<_>>();
        let canonical_resource_ref = meta
            .canonical_resource_ref_id
            .and_then(|resource_id| resource_refs.iter().find(|row| row.id == resource_id).cloned())
            .or_else(|| {
                resource_refs
                    .iter()
                    .find(|row| parse_resource_meta(row.freestyle_data_structure.as_deref()).role == "canonical_transport")
                    .cloned()
            })
            .or_else(|| resource_refs.first().cloned());
        let canonical_meta =
            parse_resource_meta(canonical_resource_ref.as_ref().and_then(|row| row.freestyle_data_structure.as_deref()));
        let publication_slug = first_non_empty(&[
            meta.slug.as_str(),
            canonical_meta.slug.as_str(),
            record.head.as_str(),
        ])
        .map(normalize_package_slug)
        .unwrap_or_else(|| "lince_sand".to_string());
        let publication_channel = normalize_publication_channel_or_default(
            first_non_empty(&[meta.channel.as_str(), canonical_meta.channel.as_str()]).or_else(
                || {
                    canonical_resource_ref
                        .as_ref()
                        .and_then(|row| infer_channel_from_path(&row.resource_path))
                },
            ),
        );

        if publication_slug != slug || publication_channel != channel {
            continue;
        }

        return Ok(Some(DnaPublicationState {
            record_id: record.id,
            record_extension_id: extension.id,
            categories_extension_id: categories_extension_by_record.get(&record.id).copied(),
            version: first_non_empty(&[meta.version.as_str(), canonical_meta.version.as_str()])
                .unwrap_or("0.1.0")
                .trim()
                .to_string(),
            canonical_resource_ref,
            resource_refs,
        }));
    }

    Ok(None)
}

async fn list_sand_resource_refs_for_record(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    record_id: i64,
) -> ApiResult<Vec<DnaResourceRefRow>> {
    Ok(list_table_rows::<DnaResourceRefRow>(state, headers, server, "record_resource_ref")
        .await?
        .into_iter()
        .filter(|row| {
            row.record_id == record_id && row.provider == "bucket" && row.resource_kind == "sand"
        })
        .collect())
}

fn build_resource_meta_json(
    role: &str,
    slug: &str,
    channel: &str,
    version: &str,
    package_prefix: &str,
    transport_filename: &str,
    sand_toml_key: &str,
    package: &LincePackage,
) -> String {
    serde_json::to_string(&json!({
        "role": role,
        "slug": slug,
        "channel": channel,
        "version": version,
        "package_prefix": package_prefix,
        "transport_filename": transport_filename,
        "package_format": package_format_label(package.transport()),
        "mime_type": package_content_type(package.transport()),
        "entry_path": package.entry_path(),
        "sand_toml_key": sand_toml_key,
        "available_files": [transport_filename, SAND_TOML_FILENAME],
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn build_extension_meta_json(
    slug: &str,
    channel: &str,
    version: &str,
    canonical_resource_ref_id: i64,
    package_prefix: &str,
    package: &LincePackage,
) -> String {
    serde_json::to_string(&json!({
        "published": true,
        "slug": slug,
        "channel": channel,
        "version": version,
        "canonical_resource_ref_id": canonical_resource_ref_id,
        "package_prefix": package_prefix,
        "default_transport": package_format_label(package.transport()),
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn parse_release_version(version: &str) -> ApiResult<Version> {
    Version::parse(version.trim()).map_err(|error| {
        api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("A versao do sand precisa ser SemVer valida: {error}"),
        )
    })
}

fn normalize_publication_channel(raw: &str) -> Result<String, String> {
    let channel = raw.trim().to_ascii_lowercase();
    match channel.as_str() {
        DNA_CHANNEL_OFFICIAL | DNA_CHANNEL_COMMUNITY => Ok(channel),
        _ => Err("O canal do sand precisa ser official ou community.".into()),
    }
}

fn normalize_publication_channel_or_default(raw: Option<&str>) -> String {
    raw.and_then(|value| normalize_publication_channel(value).ok())
        .unwrap_or_else(|| DNA_CHANNEL_COMMUNITY.to_string())
}

fn infer_channel_from_path(path: &str) -> Option<&str> {
    let prefix = format!("{DNA_BUCKET_PREFIX}/");
    path.strip_prefix(&prefix)
        .and_then(|value| value.split('/').next())
        .filter(|value| matches!(*value, DNA_CHANNEL_OFFICIAL | DNA_CHANNEL_COMMUNITY))
}

fn channel_sort_rank(channel: &str) -> u8 {
    match channel.trim().to_ascii_lowercase().as_str() {
        DNA_CHANNEL_OFFICIAL => 0,
        DNA_CHANNEL_COMMUNITY => 1,
        _ => 2,
    }
}

fn resource_created_at(row: &DnaResourceRefRow) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(row.created_at.trim(), "%Y-%m-%d %H:%M:%S").ok()
}

fn should_prune_release(row: &DnaResourceRefRow) -> bool {
    resource_created_at(row)
        .map(|created_at| created_at <= Utc::now().naive_utc() - Duration::days(30))
        .unwrap_or(false)
}

async fn upsert_categories_extension(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    record_id: i64,
    categories_extension_id: Option<i64>,
    categories: &[String],
) -> ApiResult<()> {
    let payload = json!({
        "record_id": record_id,
        "namespace": DNA_CATEGORY_NAMESPACE,
        "version": 1,
        "freestyle_data_structure": serde_json::to_string(&json!({
            "categories": categories,
        }))
        .unwrap_or_else(|_| "{}".to_string()),
    });

    if let Some(extension_id) = categories_extension_id {
        update_table_row(state, headers, server, "record_extension", extension_id, payload).await
    } else {
        let _ = create_table_row(state, headers, server, "record_extension", payload).await?;
        Ok(())
    }
}

async fn prune_old_releases(
    state: &AppState,
    headers: &HeaderMap,
    server: &Organ,
    record_id: i64,
    canonical_resource_ref_id: i64,
) -> ApiResult<()> {
    let mut archived = list_sand_resource_refs_for_record(state, headers, server, record_id)
        .await?
        .into_iter()
        .filter(|row| row.id != canonical_resource_ref_id)
        .collect::<Vec<_>>();
    archived.sort_by(|left, right| {
        let left_meta = parse_resource_meta(left.freestyle_data_structure.as_deref());
        let right_meta = parse_resource_meta(right.freestyle_data_structure.as_deref());
        let left_version = Version::parse(left_meta.version.trim()).ok();
        let right_version = Version::parse(right_meta.version.trim()).ok();
        right_version
            .cmp(&left_version)
            .then_with(|| resource_created_at(right).cmp(&resource_created_at(left)))
            .then_with(|| right.id.cmp(&left.id))
    });

    for row in archived.into_iter().skip(2) {
        if !should_prune_release(&row) {
            continue;
        }
        let meta = parse_resource_meta(row.freestyle_data_structure.as_deref());
        delete_bucket_object_if_exists(state, headers, server, &row.resource_path).await?;
        if let Some(sand_toml_key) = meta.sand_toml_key.as_deref() {
            delete_bucket_object_if_exists(state, headers, server, sand_toml_key).await?;
        }
        delete_table_row(state, headers, server, "record_resource_ref", row.id).await?;
    }

    Ok(())
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

async fn ensure_remote_response_success(
    state: &AppState,
    session_token: Option<&str>,
    server_id: &str,
    response: reqwest::Response,
    default_message: &str,
) -> ApiResult<reqwest::Response> {
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

    Ok(response)
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

async fn download_object_bytes(
    downloaded: persistence::storage::DownloadedObject,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    let mut reader = downloaded.body.into_async_read();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut bytes).await?;
    Ok(bytes)
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
    let channel = payload.channel.clone();
    let package = parse_lince_package(&payload.upload.filename, &payload.upload.bytes)
        .map_err(map_validation_error)?;
    let head = fallback_text(&payload.head, &package.manifest.title);
    let body = fallback_text(&payload.body, &package.manifest.description);
    let slug = normalize_package_slug(&package_name_seed(&payload.upload.filename, &package));
    let categories = publication_categories(payload.categories);
    let package_prefix = format!(
        "{DNA_BUCKET_PREFIX}/{channel}/{}/{slug}/{}",
        package_prefix_letters(&slug),
        package.manifest.version
    );
    let transport_filename = canonical_transport_filename(&slug, package.transport());
    let bucket_key = format!("{package_prefix}/{transport_filename}");
    let sand_toml_key = format!("{package_prefix}/{SAND_TOML_FILENAME}");
    let package_format = package_format_label(package.transport()).to_string();
    let package_bytes = build_lince_transport_bytes(&package).map_err(map_validation_error)?;
    let sand_toml = build_remote_sand_toml(&slug, &package.manifest.version, &channel);
    let sand_toml_bytes = sand_toml.into_bytes();

    let record_result = upsert_dna_publication(
        &state,
        &headers,
        &server,
        &head,
        &body,
        &slug,
        &channel,
        &categories,
        &package_prefix,
        &bucket_key,
        &sand_toml_key,
        &transport_filename,
        &package,
        &package_bytes,
        &sand_toml_bytes,
    )
    .await;

    let (record_id, record_extension_id, resource_ref_id) = match record_result {
        Ok(value) => value,
        Err(error) => return Err(error),
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
        version: package.manifest.version.clone(),
        package_prefix,
        bucket_key,
        sand_toml_key,
        transport_filename,
        package_format,
        categories,
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
        requires_server: manifest.requires_server,
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

    if html.contains("</head>") {
        return html.replace("</head>", &format!("{injections}\n</head>"));
    }

    if html.contains("<body") {
        return html.replacen("<body", &format!("{injections}\n<body"), 1);
    }

    if html.contains("<script") {
        return html.replacen("<script", &format!("{injections}\n<script"), 1);
    }

    if html.contains("</html>") {
        return html.replace("</html>", &format!("{injections}\n</html>"));
    }

    format!("{injections}\n{html}")
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

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn parse_categories_field(raw: &str) -> Vec<String> {
    raw.split([',', '\n', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_categories(categories: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for category in categories {
        let trimmed = category.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

fn extract_categories(freestyle_data_structure: &str) -> Option<Vec<String>> {
    let value = serde_json::from_str::<Value>(freestyle_data_structure).ok()?;
    let categories = value
        .get("categories")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(categories)
}

fn publication_categories(categories: Vec<String>) -> Vec<String> {
    normalize_categories(
        std::iter::once(DNA_BASE_CATEGORY.to_string())
            .chain(categories)
            .collect(),
    )
}

fn map_validation_error(
    message: String,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
}
