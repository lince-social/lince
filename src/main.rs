mod application;
mod domain;
mod infrastructure;
mod presentation;

use application::use_cases::karma::karma::karma;
use axum::routing::get;
use infrastructure::http::handlers::section::page::page_handler;
use infrastructure::http::routers::configuration::configuration_router;
use infrastructure::{database::startup::tidy_database, http::routers::section::section_router};

use axum::Router;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tidy_database().await;

    thread::spawn(|| {
        loop {
            karma();
            thread::sleep(Duration::from_secs(60));
        }
    });

    let app = Router::new()
        .route("/", get(page_handler))
        .nest("/section", section_router().await)
        .nest("/table")
        .nest("/configuration", configuration_router().await);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on: {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap()
}
