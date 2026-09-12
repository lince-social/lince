#![forbid(unsafe_code)]
#![recursion_limit = "256"]

mod auth;
mod live;
mod records;
mod settings;
mod users;

#[cfg(test)]
mod tests;

use std::{io, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::header,
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
}

impl Facade {
    pub async fn start(cell: CellRuntime, address: &str) -> Result<Self, io::Error> {
        let listener = TcpListener::bind(address).await?;
        let address = listener.local_addr()?;
        let (stop, stopping) = watch::channel(false);
        let state = State {
            cell,
            auth: Arc::new(auth::Auth::default()),
            connections: Arc::new(Semaphore::new(128)),
            stop: stopping.clone(),
        };
        auth::required(&state)
            .await
            .map_err(|(_, message)| io::Error::other(message))?;
        settings::ensure(&state)
            .await
            .map_err(|(_, message)| io::Error::other(message))?;
        let router = Router::new()
            .route("/", get(page))
            .route("/records/{uid}", get(page))
            .route(
                "/facade.css",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/css")],
                        include_str!("facade.css"),
                    )
                }),
            )
            .route(
                "/facade.js",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/javascript")],
                        include_str!("facade.js"),
                    )
                }),
            )
            .route(
                "/read-rules.js",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/javascript")],
                        include_str!("read-rules.js"),
                    )
                }),
            )
            .route(
                "/i18n.js",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/javascript")],
                        include_str!("i18n.js"),
                    )
                }),
            )
            .route(
                "/datastar.js",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/javascript")],
                        include_str!("vendor/datastar.js"),
                    )
                }),
            )
            .route(
                "/licenses/datastar",
                get(|| async { include_str!("vendor/LICENSE.txt") }),
            )
            .route(
                "/credits",
                get(|| async { include_str!("vendor/CREDITS.txt") }),
            )
            .route("/session", get(auth::session))
            .route("/users", post(users::change))
            .route("/login", post(auth::login))
            .route("/logout", post(auth::logout))
            .route("/live", get(live::upgrade))
            .layer(DefaultBodyLimit::max(8192))
            .layer(axum::middleware::from_fn(security_headers))
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

async fn page() -> Html<&'static str> {
    Html(include_str!("facade.html"))
}

async fn security_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "same-origin"),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
    }
    response
}
