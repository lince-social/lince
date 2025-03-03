// use axum::{
//     http::StatusCode,
//     routing::{get, post},
//     Json, Router,
// };

// use serde::{Deserialize, Serialize};

// use database::startup::startup_database;

// mod database;

// #[tokio::main]
// async fn main() {
//     startup_database();

//     tracing_subscriber::fmt::init();

//     let app = Router::new()
//         .route("/", get(root))
//         .route("/users", post(create_user));

//     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//     axum::serve(listener, app).await.unwrap();
// }

// async fn root() -> &'static str {
//     "Hello, world! Axum Rocks!!!!!!"
// }

// async fn create_user(Json(payload): Json<CreateUser>) -> (StatusCode, Json<User>) {
//     let user = User {
//         id: 1337,
//         username: payload.username,
//     };

//     (StatusCode::CREATED, Json(user))
// }

// #[derive(Deserialize)]
// struct CreateUser {
//     username: String,
// }

// #[derive(Serialize)]
// struct User {
//     id: u64,
//     username: String,
// }

use axum::{routing::get, Router};
use database::startup::tidy_database;
mod components;
mod database;
// use components::configurations;
use components::sections;

#[tokio::main]
async fn main() {
    tidy_database();

    let app = Router::new().route("/", get(sections::page::root));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on: {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap()
}

// async fn root() -> Html<&'static str> {
//     Html("Hello world! Im an amendobobo")
// }
