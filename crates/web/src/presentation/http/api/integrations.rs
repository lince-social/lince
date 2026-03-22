use {
    crate::{
        application::state::AppState,
        infrastructure::{
            auth::{parse_cookie_header, session_cookie_name},
            server_profile_store::ServerProfile,
        },
        presentation::http::api_error::{ApiResult, api_error},
    },
    async_stream::stream,
    axum::{
        Json,
        body::to_bytes,
        extract::Request,
        extract::{Path, State},
        http::{HeaderMap, HeaderValue, Method, StatusCode, header},
        response::IntoResponse,
    },
    serde_json::Value,
};

const MAX_PROXY_BODY_BYTES: usize = 1024 * 1024;

pub async fn proxy_manas_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((server_id, view_id)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    let server = load_server_profile(&state, &server_id)?;
    println!(
        "[web host] view stream request: server_id={} base_url={} view_id={}",
        server_id, server.base_url, view_id
    );
    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let response = state
        .manas
        .open_view_stream(&server.base_url, &bearer_token, view_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    println!(
        "[web host] view stream upstream opened: server_id={} view_id={} status={}",
        server_id,
        view_id,
        response.status()
    );

    let stream = stream! {
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Result::<_, std::io::Error>::Ok(chunk),
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!("manas proxy stream read failed: {error}");
                    eprintln!(
                        "[web host] view stream read failed: server_id={} view_id={} error={}",
                        server_id, view_id, error
                    );
                    yield Err(std::io::Error::other("Nao foi possivel ler o stream remoto da view."));
                    break;
                }
            }
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));

    Ok((headers, axum::body::Body::from_stream(stream)))
}

pub async fn proxy_manas_table_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Path((server_id, table_name)): Path<(String, String)>,
    request: Request,
) -> ApiResult<impl IntoResponse> {
    let server = load_server_profile(&state, &server_id)?;
    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let body = request_json_body(request).await?;
    let method_name = method.to_string();
    println!(
        "[web host] table collection request: server_id={} table={} method={} body={}",
        server_id,
        table_name,
        method_name,
        body.as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "null".into())
    );
    let response = state
        .manas
        .send_table_request(
            &server.base_url,
            &bearer_token,
            method,
            &table_name,
            None,
            body,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    println!(
        "[web host] table collection upstream response: server_id={} table={} method={} status={}",
        server_id,
        table_name,
        method_name,
        response.status()
    );

    proxy_json_response(response).await
}

pub async fn proxy_manas_table_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Path((server_id, table_name, id)): Path<(String, String, i64)>,
    request: Request,
) -> ApiResult<impl IntoResponse> {
    let server = load_server_profile(&state, &server_id)?;
    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let body = request_json_body(request).await?;
    let method_name = method.to_string();
    println!(
        "[web host] table item request: server_id={} table={} id={} method={} body={}",
        server_id,
        table_name,
        id,
        method_name,
        body.as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "null".into())
    );
    let response = state
        .manas
        .send_table_request(
            &server.base_url,
            &bearer_token,
            method,
            &table_name,
            Some(id),
            body,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    println!(
        "[web host] table item upstream response: server_id={} table={} id={} method={} status={}",
        server_id,
        table_name,
        id,
        method_name,
        response.status()
    );

    proxy_json_response(response).await
}

async fn extract_manas_token(
    state: &AppState,
    headers: &HeaderMap,
    server_id: &str,
) -> ApiResult<String> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let Some(session) = state
        .auth
        .server_session(session_token.as_deref(), server_id)
        .await
    else {
        eprintln!(
            "[web host] missing server session: server_id={} local_session_present={}",
            server_id,
            session_token.is_some()
        );
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Essa sessao local nao esta conectada a esse servidor.",
        ));
    };

    Ok(session.bearer_token)
}

async fn request_json_body(request: Request) -> ApiResult<Option<Value>> {
    match *request.method() {
        Method::GET | Method::DELETE => Ok(None),
        _ => {
            let bytes = to_bytes(request.into_body(), MAX_PROXY_BODY_BYTES)
                .await
                .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
            if bytes.is_empty() {
                return Ok(None);
            }

            serde_json::from_slice::<Value>(&bytes)
                .map(Some)
                .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))
        }
    }
}

async fn proxy_json_response(response: reqwest::Response) -> ApiResult<(StatusCode, Json<Value>)> {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    Ok((status, Json(payload)))
}

fn load_server_profile(state: &AppState, server_id: &str) -> ApiResult<ServerProfile> {
    state
        .servers
        .get(server_id)
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Servidor nao encontrado."))
}
