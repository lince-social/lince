use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Path, Request, State as Extract},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use engine::signal_endpoints::{Reading, Submission};
use serde_json::json;
use tokio::sync::{Mutex, Semaphore};

use crate::{State, auth::Failure};

pub(crate) struct Ingress {
    window: Mutex<(Instant, u32)>,
    concurrent: Arc<Semaphore>,
}

impl Default for Ingress {
    fn default() -> Self {
        Self {
            window: Mutex::new((Instant::now(), 0)),
            concurrent: Arc::new(Semaphore::new(8)),
        }
    }
}

impl Ingress {
    async fn admit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, Failure> {
        let mut window = self.window.lock().await;
        if window.0.elapsed() >= Duration::from_secs(60) {
            *window = (Instant::now(), 0);
        }
        if window.1 >= 4096 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "Signal endpoint request limit reached".into(),
            ));
        }
        window.1 += 1;
        self.concurrent.clone().try_acquire_owned().map_err(|_| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                "Signal endpoint ingress is busy".into(),
            )
        })
    }
}

pub(crate) async fn admit(
    Extract(state): Extract<State>,
    request: Request,
    next: Next,
) -> Response {
    if !state.security.public || state.security.request(request.headers()).is_err() {
        return (
            StatusCode::FORBIDDEN,
            "Signal endpoints require the configured public HTTPS origin",
        )
            .into_response();
    }
    let _permit = match state.signal_endpoint_ingress.admit().await {
        Ok(permit) => permit,
        Err(error) => return ([("retry-after", "60")], error).into_response(),
    };
    next.run(request).await
}

pub(crate) async fn reading(
    Extract(state): Extract<State>,
    Path(signal): Path<String>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut authorization = headers.get_all(header::AUTHORIZATION).iter();
    let result = if let Some(header) = authorization.next() {
        let token = header
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "));
        if authorization.next().is_some() || token.is_none() {
            return StatusCode::FORBIDDEN.into_response();
        }
        let submission = match serde_json::from_value::<Submission>(body) {
            Ok(submission) => submission,
            Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
        };
        state
            .cell
            .engine
            .accept_signal_token(&signal, token.unwrap(), submission)
            .await
    } else {
        let reading = match serde_json::from_value::<Reading>(body) {
            Ok(reading) => reading,
            Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
        };
        if reading.signal != signal {
            return StatusCode::FORBIDDEN.into_response();
        }
        state
            .cell
            .engine
            .accept_signal_endpoint_reading(reading)
            .await
    };
    match result {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => {
            let code = error.code().unwrap_or("signal_endpoint_refused");
            let status = match code {
                "signal_endpoint_cooldown" => StatusCode::TOO_MANY_REQUESTS,
                "signal_endpoint_replay" => StatusCode::CONFLICT,
                "signal_endpoint_invalid" => StatusCode::BAD_REQUEST,
                _ if matches!(error, engine::EngineError::Forbidden(_)) => StatusCode::FORBIDDEN,
                _ => return crate::auth::internal(error).into_response(),
            };
            (
                status,
                Json(json!({"code": code, "message": error.to_string()})),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests;
