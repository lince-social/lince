mod application;
mod domain;
mod infrastructure;
mod presentation;

// use application::karma::karma::karma;
use axum::{Router, routing::get};
use infrastructure::{
    database::management::schema::schema,
    http::routers::{
        configuration::configuration_router, operation::operation_router, record::record_router,
        section::section_router, table::table_router, tui::run_tui_mode, view::view_router,
    },
};
use presentation::web::section::page::presentation_web_section_page;
use std::env;

#[tokio::main]
async fn main() {
    let schema_database = schema().await;
    if schema_database.is_err() {
        println!("Error creating schema: {}", schema_database.err().unwrap());
        return;
    }

    // thread::spawn(|| {
    //     loop {
    //         karma();
    //         thread::sleep(Duration::from_secs(60));
    //     }
    // });

    let args = env::args().nth(1);

    if args.is_some() && args.unwrap() == "tui" {
        run_tui_mode().await
    } else {
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
