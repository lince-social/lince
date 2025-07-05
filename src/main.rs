mod application;
mod domain;
mod infrastructure;
mod presentation;

use crate::{
    application::{
        schemas::karma_filters::KarmaFilters, use_cases::karma::deliver::use_case_karma_deliver,
    },
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
    presentation::bevy::collection::CollectionPlugin,
};
use axum::{Router, routing::get};
use bevy::{DefaultPlugins, app::App};
use std::{env, io::Error, sync::Arc, time::Duration};

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

    let move_services = services.clone();
    tokio::spawn({
        async move {
            let services = move_services.clone();
            loop {
                println!("Delivering Karma...");
                let vec_karma = futures::executor::block_on(async {
                    services.providers.karma.get(KarmaFilters::default()).await
                });

                if let Err(e) = vec_karma {
                    log(LogEntry::Error(e.kind(), e.to_string()));
                } else {
                    if let Err(e) =
                        use_case_karma_deliver(services.clone(), vec_karma.unwrap()).await
                    {
                        log(LogEntry::Error(e.kind(), e.to_string()));
                    }
                }

                println!("Karma Delivered!");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    });

    match env::args().nth(1).as_deref() {
        Some("migrate") => execute_migration(db.clone()).await.inspect_err(|e| {
            log(LogEntry::Error(e.kind(), e.to_string()));
        }),
        Some("bevy") => {
            App::new()
                .add_plugins(DefaultPlugins)
                .add_plugins(CollectionPlugin)
                .run();
            Ok(())
        }
        _ => {
            let app = Router::new()
                .route("/preto_no_branco.ico", get(handler_section_favicon))
                .merge(section_router(services.clone()))
                .nest("/collection", collection_router(services.clone()))
                .nest("/view", view_router(services.clone()))
                .nest("/table", table_router(services.clone()))
                .nest("/operation", operation_router(services.clone()));

            let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();
            println!("Listening on: {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await?;
            Ok(())
        }
    }?;

    Ok(())
}
