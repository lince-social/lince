mod application;
mod domain;
mod infrastructure;
mod presentation;

use crate::{
    application::use_cases::karma::deliver::use_case_karma_deliver,
    infrastructure::{
        cross_cutting::dependency_injection,
        database::management::{lib::connection, migration::execute_migration, schema::schema},
        http::{
            handlers::section::handler_section_favicon,
            routers::{
                collection::collection_router, operation::operation_router,
                section::section_router, table::table_router, view::view_router,
            },
        },
        utils::log::{LogEntry, log},
    },
};
use axum::{Router, routing::get};
use std::{env, io::Error, sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let db = connection().await.map_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
        e
    })?;
    let db = Arc::new(db);

    schema(db.clone()).await.map_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
        e
    })?;

    let services = dependency_injection(db.clone());

    match env::args().nth(1).as_deref() {
        Some("migrate") => execute_migration(db.clone()).await.map_err(|e| {
            log(LogEntry::Error(e.kind(), e.to_string()));
            e
        }),
        _ => Ok(()),
    }?;

    let app = Router::new()
        .route("/preto_no_branco.ico", get(handler_section_favicon))
        .merge(section_router(services.clone()))
        .nest("/collection", collection_router(services.clone()))
        .nest("/view", view_router(services.clone()))
        .nest("/table", table_router(services.clone()))
        .nest("/operation", operation_router(services.clone()));

    tokio::spawn({
        async move {
            let services = services.clone();
            loop {
                println!("Delivering Karma...");
                let _ = use_case_karma_deliver(services.clone()).await;
                println!("Karma Delivered!");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    });

    let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();
    println!("Listening on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await
}
