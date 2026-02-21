#![forbid(unsafe_code)]

#[cfg(feature = "karma")]
use application::karma::karma_deliver;
use injection::cross_cutting::{InjectedServices, dependency_injection};
use persistence::connection::connection;
use persistence::seeder::seed;
use std::{env, io::Error, sync::Arc, time::Duration};
#[cfg(feature = "tui")]
use tui::tui_app;
use utils::logging::{LogEntry, log};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = env::args().collect::<Vec<String>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let db = Arc::new(connection().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?);
    sqlx::migrate!("../../migrations")
        .run(&*db)
        .await
        .map_err(|e| Error::new(std::io::ErrorKind::Other, e))?;

    let _ = seed(&*db)
        .await
        .map_err(|e| Error::new(std::io::ErrorKind::Other, e))?;

    let services = dependency_injection(db.clone());

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

fn print_help() {
    println!("Usage: lince [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -h, --help            Show this help message");
    #[cfg(feature = "gui")]
    println!("      --guiless        Disable GUI startup");
    #[cfg(feature = "karma")]
    println!("      --karmaless       Disable karma delivery loop");
    println!();
    println!("To learn more visit https://lince.social")
}

#[cfg(feature = "gui")]
async fn start_gui(args: Vec<String>, services: InjectedServices) -> Result<(), Error> {
    if !args.contains(&"--guiless".to_string()) {
        use application::gpui::get_gpui_startup_data;
        use gui::app::gpui_app;

        match get_gpui_startup_data(services.clone()).await {
            Ok(state) => gpui_app(services, state).await,
            Err(e) => log(LogEntry::Error(e.kind(), e.to_string())),
        }
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
    if !args.contains(&"--karmaless".to_string()) {
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
