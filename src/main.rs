mod application;
mod domain;
mod infrastructure;
mod presentation;

use application::use_cases::karma::deliver::use_case_karma_deliver;
use axum::{Router, routing::get};
use infrastructure::{
    database::management::{migration::execute_migration, schema::schema},
    http::{
        handlers::section::handler_section_favicon,
        routers::{
            collection::collection_router, operation::operation_router, section::section_router,
            table::table_router, tui::run_tui_mode, view::view_router,
        },
    },
};
use presentation::web::section::page::presentation_web_section_page;
use std::{env, sync::Arc, time::Duration};

use crate::infrastructure::database::management::lib::connection;

#[tokio::main]
async fn main() {
    if let Err(e) = schema().await {
        println!("Error creating schema: {}", e);
        return;
    }
    let db = Arc::new(connection().await.unwrap());

    execute_migration(db.clone()).await;

    tokio::spawn({
        async {
            loop {
                println!("Delivering Karma...");
                let _ = use_case_karma_deliver().await;
                println!("Karma Delivered!");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    });

    match env::args().nth(1).as_deref() {
        Some("tui") => run_tui_mode().await,
        _ => {
            let app = Router::new()
                .route("/", get(presentation_web_section_page))
                .route("/preto_no_branco.ico", get(handler_section_favicon))
                .nest("/section", section_router().await)
                .nest("/collection", collection_router().await)
                .nest("/view", view_router().await)
                .nest("/table", table_router().await)
                .nest("/operation", operation_router().await);

            let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();
            println!("Listening on: {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await.unwrap();
        }
    }
}
