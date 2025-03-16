mod application;
mod domain;
mod infrastructure;
mod presentation;

use application::use_cases::karma::karma::karma;
use infrastructure::database::startup::tidy_database;

use axum::{Router, routing::get};
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tidy_database();
    thread::spawn(|| {
        loop {
            karma();
            thread::sleep(Duration::from_secs(1));
        }
    });

    let app = Router::new().route("/", get("Helo wordi"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on: {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap()
}
