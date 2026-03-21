use {
    crate::{
        application::state::AppState,
        infrastructure::auth::{parse_cookie_header, session_cookie_name},
        presentation::http::api_error::{ApiResult, api_error},
    },
    async_stream::stream,
    axum::{
        Json,
        extract::{Path, State},
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::IntoResponse,
    },
    serde::Deserialize,
};

#[derive(Debug, Deserialize)]
pub struct SqlProxyRequest {
    pub sql: String,
}

pub async fn proxy_manas_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(view_id): Path<u64>,
) -> ApiResult<impl IntoResponse> {
    let bearer_token = extract_manas_token(&state, &headers).await?;
    let response = state
        .manas
        .open_view_stream(&bearer_token, view_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    let stream = stream! {
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Result::<_, std::io::Error>::Ok(chunk),
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!("manas proxy stream read failed: {error}");
                    yield Err(std::io::Error::other("Nao foi possivel ler o stream remoto da view."));
                    break;
                }
            }
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));

    Ok((headers, axum::body::Body::from_stream(stream)))
}

pub async fn proxy_manas_sql(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SqlProxyRequest>,
) -> ApiResult<impl IntoResponse> {
    let sql = payload.sql.trim();
    if sql.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "O campo sql nao pode estar vazio.",
        ));
    }

    let bearer_token = extract_manas_token(&state, &headers).await?;
    let (status, body_json) = state
        .manas
        .execute_sql(&bearer_token, sql)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    Ok((status, Json(body_json)))
}

async fn extract_manas_token(state: &AppState, headers: &HeaderMap) -> ApiResult<String> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let session = state.auth.session(session_token.as_deref()).await;
    let Some(bearer_token) = session.and_then(|record| record.manas_token) else {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Essa sessao nao esta conectada ao servidor externo.",
        ));
    };

    Ok(bearer_token)
}
