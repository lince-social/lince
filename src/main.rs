// mod database;
// mod karma;
mod components;

// use database::startup::tidy_database;
// use karma::karma::karma;
use components::*;

use axum::{routing::get, Router};

use std::thread;
use std::time::Duration;
#[tokio::main]
async fn main() {
    // tidy_database();
    // karma();
    thread::spawn(|| loop {
        println!("hello from karma");
        thread::sleep(Duration::from_secs(1));
    });

    let app = Router::new().route("/", get(sections::page::root));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on: {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap()
}
