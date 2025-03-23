mod controller;
mod model;
mod view;

use axum::{Router, routing::get};
use controller::{configuration::configuration_router, section::section_router};
use model::{database::management::startup::tidy_database, karma::karma::karma};
use std::{thread, time::Duration};
use view::section::page::page;

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
        .route("/", get(page))
        .nest("/section", section_router().await)
        .nest("/configuration", configuration_router().await);
    // .nest("/table")

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on: {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap()
}
