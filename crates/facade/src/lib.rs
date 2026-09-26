#![forbid(unsafe_code)]
#![recursion_limit = "256"]

mod auth;
mod live;
mod records;
mod security;
mod settings;
mod users;

#[cfg(test)]
mod tests;

use std::{io, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    response::Html,
    routing::{get, post},
};
use cell::CellRuntime;
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
};

pub struct Facade {
    pub address: SocketAddr,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct State {
    cell: CellRuntime,
    auth: Arc<auth::Auth>,
    connections: Arc<Semaphore>,
    stop: watch::Receiver<bool>,
    security: security::Security,
}

impl Facade {
    pub async fn start(cell: CellRuntime, address: &str) -> Result<Self, io::Error> {
        Self::start_with_origin(cell, address, None).await
    }

    pub async fn start_public(
        cell: CellRuntime,
        address: &str,
        origin: &str,
    ) -> Result<Self, io::Error> {
        Self::start_with_origin(cell, address, Some(origin)).await
    }

    async fn start_with_origin(
        cell: CellRuntime,
        address: &str,
        origin: Option<&str>,
    ) -> Result<Self, io::Error> {
        let listen: SocketAddr = address.parse().map_err(io::Error::other)?;
        if !listen.ip().is_loopback() {
            return Err(io::Error::other(
                "Facade listens on loopback; publish its HTTPS origin through a reverse proxy on this server",
            ));
        }
        if !store::auth::admin_exists(&cell.store.pool)
            .await
            .map_err(io::Error::other)?
        {
            return Err(io::Error::other(
                "Create an administrator before starting Facade",
            ));
        }
        let listener = TcpListener::bind(address).await?;
        let address = listener.local_addr()?;
        let security = security::Security::new(
            origin.unwrap_or(&format!("http://{address}")),
            origin.is_some(),
        )?;
        let (stop, stopping) = watch::channel(false);
        let state = State {
            cell,
            auth: Arc::new(auth::Auth::default()),
            connections: Arc::new(Semaphore::new(128)),
            stop: stopping.clone(),
            security,
        };
        settings::ensure(&state)
            .await
            .map_err(|(_, message)| io::Error::other(message))?;
        let router = Router::new()
            .route("/", get(page))
            .route("/records/{uid}", get(page))
            .route(
                "/licenses/datastar",
                get(|| async { include_str!("vendor/LICENSE.txt") }),
            )
            .route(
                "/credits",
                get(|| async { include_str!("vendor/CREDITS.txt") }),
            )
            .route("/session", get(auth::session))
            .route("/login", post(auth::login))
            .route("/logout", post(auth::logout))
            .route("/live", get(live::upgrade))
            .layer(DefaultBodyLimit::max(8192))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                security_headers,
            ))
            .with_state(state);
        let task = tokio::spawn(async move {
            let mut stopping = stopping;
            if let Err(error) = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = stopping.changed().await;
                })
                .await
            {
                tracing::error!(%error, "Facade stopped");
            }
        });
        Ok(Self {
            address,
            stop,
            task,
        })
    }
}

impl Drop for Facade {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.task.abort();
    }
}

async fn page() -> Html<String> {
    Html(include_str!("facade.html").to_owned())
}

async fn security_headers(
    axum::extract::State(state): axum::extract::State<State>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if let Err(error) = state.security.request(request.headers()) {
        return error.into_response();
    }
    let mut response =
        match tokio::time::timeout(std::time::Duration::from_secs(30), next.run(request)).await {
            Ok(response) => response,
            Err(_) => axum::http::StatusCode::REQUEST_TIMEOUT.into_response(),
        };
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "same-origin"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        ),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self' 'unsafe-eval' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
    }
    if state.security.public {
        response.headers_mut().insert(
            "strict-transport-security",
            axum::http::HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}
