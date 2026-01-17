#![forbid(unsafe_code)]

#[cfg(feature = "karma")]
use application::karma::karma_deliver;
use infrastructure::{
    cross_cutting::dependency_injection,
    database::management::{connection::connection, schema::schema},
    utils::logging::{LogEntry, log},
};
#[cfg(feature = "gpui")]
use application::gpui::get_gpui_startup_data, infrastructure::cross_cutting::InjectedServices;
#[cfg(feature = "gpui")]
use presentation::gpui::app::gpui_app;
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
