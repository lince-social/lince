use super::*;
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Clone)]
struct Server {
    directory: PathBuf,
    font: PathBuf,
    port: u16,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeQuery {
    scheme: Option<String>,
    kind: Option<String>,
}

pub(super) fn serve(root: &Path, directory: &Path, port: u16) -> Result<()> {
    validate(directory, &load(directory)?)?;
    let directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|error| error.to_string())?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        let state = Arc::new(Server {
            directory,
            font: root.join("institute/assets/fonts/Lato/Lato-Regular.ttf"),
            port,
        });
        let app = Router::new()
            .route("/tokens.css", get(tokens))
            .route("/theme.json", get(document))
            .fallback(get(file))
            .with_state(state);
        println!("Design preview: http://127.0.0.1:{port}");
        println!(
            "Local synthetic prototypes. Saves refresh automatically; Ctrl+C stops the server."
        );
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .map_err(|error| error.to_string())
    })
}

fn allowed(headers: &HeaderMap, state: &Server) -> bool {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| {
            host == format!("127.0.0.1:{}", state.port)
                || host == format!("localhost:{}", state.port)
        })
}

fn response(content_type: &'static str, result: Result<Vec<u8>>) -> Response {
    let mut response = match result {
        Ok(bytes) => (StatusCode::OK, bytes).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    };
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("content-security-policy", "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'".parse().unwrap());
    response
}

fn request_theme(state: &Server, query: &ThemeQuery) -> Result<ThemeSettings> {
    match query.scheme.as_deref().unwrap_or("Dark") {
        "Study" => theme(Some(&local_file(&state.directory, "theme.json")?), "Dark"),
        scheme => theme(None, scheme),
    }
}

async fn tokens(
    State(state): State<Arc<Server>>,
    headers: HeaderMap,
    Query(query): Query<ThemeQuery>,
) -> Response {
    if !allowed(&headers, &state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    response(
        "text/css; charset=utf-8",
        (|| {
            let theme = request_theme(&state, &query)?;
            let kind = kind(query.kind.as_deref().unwrap_or("Square"))?;
            css::export(&theme, Some(kind), &TokenOverrides::default()).map(String::into_bytes)
        })(),
    )
}

async fn document(
    State(state): State<Arc<Server>>,
    headers: HeaderMap,
    Query(query): Query<ThemeQuery>,
) -> Response {
    if !allowed(&headers, &state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    response(
        "application/json",
        request_theme(&state, &query)
            .and_then(|theme| ThemeDocument::export(&theme))
            .map(String::into_bytes),
    )
}

async fn file(State(state): State<Arc<Server>>, headers: HeaderMap, uri: Uri) -> Response {
    if !allowed(&headers, &state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = uri.path();
    match path {
        "/" => response("text/html; charset=utf-8", Ok(VIEWER.as_bytes().to_vec())),
        "/assets/prototype.css" => {
            response("text/css; charset=utf-8", Ok(STYLE.as_bytes().to_vec()))
        }
        "/assets/prototype.js" => response(
            "text/javascript; charset=utf-8",
            Ok(SCRIPT.as_bytes().to_vec()),
        ),
        "/assets/Lato-Regular.ttf" => response(
            "font/ttf",
            fs::read(&state.font).map_err(|error| error.to_string()),
        ),
        "/assets/Lato-OFL.txt" => response(
            "text/plain; charset=utf-8",
            fs::read(state.font.with_file_name("OFL.txt")).map_err(|error| error.to_string()),
        ),
        "/catalogue.json" => response(
            "application/json",
            serde_json::to_vec(&serde_json::json!({
                "schemes": ColorScheme::ALL.map(|scheme| serde_json::json!({"id": scheme, "name": scheme.name()})),
                "kinds": SandStyleKind::ALL.map(|kind| serde_json::json!({"id": kind, "name": kind.name()})),
                "study_theme": local_file(&state.directory, "theme.json").is_ok(),
            })).map_err(|error| error.to_string()),
        ),
        "/revision" => response(
            "text/plain",
            fingerprint(&state.directory, false).map(|revision| revision.to_string().into_bytes()),
        ),
        _ => {
            let relative = path.strip_prefix('/').unwrap_or(path);
            let content_type = match Path::new(relative)
                .extension()
                .and_then(|value| value.to_str())
            {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") => "text/javascript; charset=utf-8",
                Some("json") => "application/json",
                Some("svg") => "image/svg+xml",
                Some("png") => "image/png",
                Some("jpg" | "jpeg") => "image/jpeg",
                _ => return StatusCode::NOT_FOUND.into_response(),
            };
            response(
                content_type,
                local_file(&state.directory, relative)
                    .and_then(|path| fs::read(path).map_err(|error| error.to_string())),
            )
        }
    }
}

pub(super) fn fingerprint(directory: &Path, sources_only: bool) -> Result<u64> {
    fn walk(directory: &Path, sources_only: bool, hasher: &mut impl Hasher) -> Result<()> {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| error.to_string())?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with('.')
                    && !matches!(name.to_str(), Some("target" | "web" | "node_modules"))
                {
                    walk(&path, sources_only, hasher)?;
                }
            } else if file_type.is_file()
                && (!sources_only
                    || matches!(
                        path.extension().and_then(|value| value.to_str()),
                        Some("rs" | "toml" | "lock")
                    ))
            {
                path.hash(hasher);
                if sources_only {
                    let metadata = entry.metadata().map_err(|error| error.to_string())?;
                    metadata.len().hash(hasher);
                    metadata
                        .modified()
                        .map_err(|error| error.to_string())?
                        .hash(hasher);
                } else {
                    fs::read(&path)
                        .map_err(|error| error.to_string())?
                        .hash(hasher);
                }
            }
        }
        Ok(())
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    walk(directory, sources_only, &mut hasher)?;
    Ok(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn preview_rejects_foreign_hosts_and_refresh_detects_content_changes() {
        let directory = super::super::tests::Directory::new();
        let server = Server {
            directory: directory.0.clone(),
            font: PathBuf::new(),
            port: 6180,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("attacker.example:6180"),
        );
        assert!(!allowed(&headers, &server));
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:6180"));
        assert!(allowed(&headers, &server));
        fs::write(directory.0.join("fixture.json"), "one").unwrap();
        let before = fingerprint(&directory.0, false).unwrap();
        fs::write(directory.0.join("fixture.json"), "two").unwrap();
        assert_ne!(before, fingerprint(&directory.0, false).unwrap());
    }
}
