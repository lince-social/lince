mod application;
mod domain;
mod infrastructure;
mod macros;
mod presentation;

#[cfg(feature = "karma")]
use crate::application::karma::karma_deliver;
use crate::infrastructure::{
    cross_cutting::dependency_injection,
    database::management::{connection::connection, schema::schema},
    http::{
        handlers::section::handler_section_favicon,
        routers::{
            collection::collection_router, karma::karma_router, operation::operation_router,
            section::section_router, table::table_router, view::view_router,
        },
    },
    utils::logging::{LogEntry, log},
};
#[cfg(feature = "gpui")]
use crate::{
    application::gpui::get_gpui_startup_data, infrastructure::cross_cutting::InjectedServices,
    presentation::gpui::app::gpui_app,
};
use axum::{Router, routing::get};
#[cfg(feature = "karma")]
use std::time::Duration;
use std::{env, io::Error, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let db = Arc::new(connection().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?);
    schema(db.clone()).await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?;
    let services = dependency_injection(db.clone());
    let args = env::args().collect::<Vec<String>>();

    // if args.contains(&"migrate".to_string()) {
    //     println!("Executing migration...");
    //     execute_migration(db.clone()).await.inspect_err(|e| {
    //         log(LogEntry::Error(e.kind(), e.to_string()));
    //     })?;
    // }

    #[cfg(feature = "gpui")]
    if let Err(e) = start_gpui(args.clone(), services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }
    #[cfg(feature = "html")]
    if let Err(e) = start_html(args.clone(), services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }
    #[cfg(feature = "karma")]
    start_karma(args.clone(), services.clone()).await?;

    Ok(())
}

#[cfg(feature = "gpui")]
async fn start_gpui(args: Vec<String>, services: InjectedServices) -> Result<(), Error> {
    if args.contains(&"gpui".to_string()) {
        let gpui_startup_data = get_gpui_startup_data(services.clone()).await?;
        tokio::spawn(async move {
            gpui_app(services, gpui_startup_data).await;
        });
    }
    Ok(())
}

#[cfg(feature = "karma")]
async fn start_karma(args: Vec<String>, services: InjectedServices) -> Result<(), Error> {
    if args.contains(&"karma".to_string()) {
        loop {
            println!("Delivering Karma");
            let vec_karma = services.repository.karma.get_active(None).await;
            if let Err(e) = &vec_karma {
                log(LogEntry::Error(e.kind(), e.to_string()));
            } else if let Err(e) = karma_deliver(services.clone(), vec_karma.unwrap()).await {
                log(LogEntry::Error(e.kind(), e.to_string()));
            }
            println!("Karma Delivered!");
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
    Ok(())
}

#[cfg(feature = "html")]
async fn start_html(args: Vec<String>, services: InjectedServices) -> Result<(), Error> {
    if args.contains(&"html".to_string()) {
        let services_clone = services.clone();
        tokio::spawn(async move {
            let app = Router::new()
                .route("/preto_no_branco.ico", get(handler_section_favicon))
                .merge(section_router(services_clone.clone()))
                .nest("/collection", collection_router(services_clone.clone()))
                .nest("/view", view_router(services_clone.clone()))
                .nest("/table", table_router(services_clone.clone()))
                .nest("/karma", karma_router(services_clone.clone()))
                .nest("/operation", operation_router(services_clone.clone()));

            let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();
            println!("Listening on: {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await.unwrap();
        });
    }
    Ok(())
}
