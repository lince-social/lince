mod controller;
mod model;
mod view;

use axum::{Router, routing::get};
use controller::{configuration::configuration_router, section::section_router, tui::run_tui_mode};
use model::{database::management::schema::schema, karma::karma::karma};
use std::{env, thread, time::Duration};
use view::web::section::page::page;

#[tokio::main]
async fn main() {
    let schema_database = schema().await;
    if schema_database.is_err() {
        println!("Error creating schema: {}", schema_database.err().unwrap());
        return;
    }

    thread::spawn(|| {
        loop {
            karma();
            thread::sleep(Duration::from_secs(60));
        }
    });

    let args = env::args().nth(1);

    if args.is_some() && args.unwrap() == "tui" {
        run_tui_mode().await
    } else {
        let app = Router::new()
            .route("/", get(page))
            .nest("/section", section_router().await)
            .nest("/configuration", configuration_router().await);
        // .nest("/view", view_route)

        let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();

        println!("Listening on: {}", listener.local_addr().unwrap());
        axum::serve(listener, app).await.unwrap();
    }
}
