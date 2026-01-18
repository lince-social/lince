#![forbid(unsafe_code)]

#[cfg(feature = "karma")]
use application::karma::karma_deliver;
#[cfg(feature = "gui")]
use gui::app::gpui_app;
use injection::cross_cutting::InjectedServices;
use injection::cross_cutting::dependency_injection;
use persistence::management::{connection::connection, schema::schema};
use std::time::Duration;
use std::{env, io::Error, sync::Arc};
#[cfg(feature = "tui")]
use tui::tui_app;
use utils::logging::{LogEntry, log};

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

    #[cfg(feature = "gui")]
    if let Err(e) = start_gui(args.clone(), services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "tui")]
    if let Err(e) = start_tui(args.clone(), services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "karma")]
    start_karma(args.clone(), services.clone()).await?;

    Ok(())
}

#[cfg(feature = "gui")]
async fn start_gui(args: Vec<String>, services: InjectedServices) -> Result<(), Error> {
    if args.contains(&"gui".to_string()) {
        gpui_app(services).await;
    }
    Ok(())
}

#[cfg(feature = "tui")]
async fn start_tui(args: Vec<String>, services: InjectedServices) -> Result<(), Error> {
    if args.contains(&"tui".to_string()) {
        tui_app(services.clone()).await;
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
