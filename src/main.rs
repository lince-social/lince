mod application;
mod domain;
mod infrastructure;
mod presentation;

use application::use_cases::karma::deliver::use_case_karma_deliver;
use axum::{Router, routing::get};
use infrastructure::{
    database::management::schema::schema,
    http::routers::{
        configuration::configuration_router, operation::operation_router, record::record_router,
        section::section_router, table::table_router, tui::run_tui_mode, view::view_router,
    },
};
use presentation::web::section::page::presentation_web_section_page;
use std::{env, time::Duration};

#[tokio::main]
async fn main() {
    if let Err(e) = schema().await {
        println!("Error creating schema: {}", e);
        return;
    }

    // Spawn karma delivery task
    tokio::spawn({
        async {
            loop {
                use_case_karma_deliver().await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    });

    match env::args().nth(1).as_deref() {
        Some("tui") => run_tui_mode().await,
        _ => {
            let app = Router::new()
                .route("/", get(presentation_web_section_page))
                .nest("/section", section_router().await)
                .nest("/configuration", configuration_router().await)
                .nest("/record", record_router().await)
                .nest("/view", view_router().await)
                .nest("/table", table_router().await)
                .nest("/operation", operation_router().await);

            let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();
            println!("Listening on: {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await.unwrap();
        }
    }
}
