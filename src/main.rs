// #![allow(dead_code)]
mod application;
mod domain;
mod infrastructure;
mod macros;
mod presentation;

#[cfg(feature = "gpui")]
use crate::{application::gpui::get_gpui_startup_data, presentation::gpui::app::gpui_app};

#[cfg(feature = "karma")]
use crate::application::karma::karma_deliver;
#[cfg(feature = "karma")]
use std::time::Duration;

use crate::infrastructure::{
    cross_cutting::dependency_injection,
    database::management::{connection::connection, migration::execute_migration, schema::schema},
    http::{
        handlers::section::handler_section_favicon,
        routers::{
            collection::collection_router, karma::karma_router, operation::operation_router,
            section::section_router, table::table_router, view::view_router,
        },
    },
    utils::logging::{LogEntry, log},
};
use axum::{Router, routing::get};
use std::{env, io::Error, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let db = connection().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?;
    let db = Arc::new(db);

    schema(db.clone()).await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?;

    let services = dependency_injection(db.clone());

    #[cfg(feature = "karma")]
    let move_services = services.clone();
    #[cfg(feature = "karma")]
    tokio::spawn({
        async move {
            let services = move_services.clone();
            loop {
                let vec_karma = futures::executor::block_on(async {
                    services.repository.karma.get_active(None).await
                });

                if let Err(e) = vec_karma {
                    log(LogEntry::Error(e.kind(), e.to_string()));
                } else if let Err(e) = karma_deliver(services.clone(), vec_karma.unwrap()).await {
                    log(LogEntry::Error(e.kind(), e.to_string()));
                }

                println!("Karma Delivered!");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    });

    let args = env::args().collect::<Vec<String>>();

    if args.contains(&"migrate".to_string()) {
        println!("Executing migration...");
        execute_migration(db.clone()).await.inspect_err(|e| {
            log(LogEntry::Error(e.kind(), e.to_string()));
        })?;
    }
    #[cfg(feature = "gpui")]
    if args.contains(&"gpui".to_string()) {
        let cloned_services = services.clone();
        let gpui_startup_data = get_gpui_startup_data(services.clone()).await?;
        tokio::spawn(async move {
            gpui_app(cloned_services.clone(), gpui_startup_data).await;
        });
    }
    if args.contains(&"html".to_string()) {
        let app = Router::new()
            .route("/preto_no_branco.ico", get(handler_section_favicon))
            .merge(section_router(services.clone()))
            .nest("/collection", collection_router(services.clone()))
            .nest("/view", view_router(services.clone()))
            .nest("/table", table_router(services.clone()))
            .nest("/karma", karma_router(services.clone()))
            .nest("/operation", operation_router(services.clone()));

        let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();
        println!("Listening on: {}", listener.local_addr().unwrap());
        axum::serve(listener, app).await?;
    }

    Ok(())
}
