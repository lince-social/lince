#![forbid(unsafe_code)]

#[cfg(all(feature = "gui", feature = "tui"))]
compile_error!("Enable only one frontend feature at a time: `gui`, `tui`, or `html`.");
#[cfg(all(feature = "gui", feature = "html"))]
compile_error!("Enable only one frontend feature at a time: `gui`, `tui`, or `html`.");
#[cfg(all(feature = "tui", feature = "html"))]
compile_error!("Enable only one frontend feature at a time: `gui`, `tui`, or `html`.");

#[cfg(feature = "karma")]
use application::karma::karma_deliver;
#[cfg(feature = "html")]
use html::serve as serve_html;
use injection::cross_cutting::{InjectedServices, dependency_injection};
use persistence::connection::connection;
use persistence::seeder::seed;
#[cfg(feature = "karma")]
use std::time::Duration;
use std::{env, io::Error, sync::Arc};
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
        .map_err(Error::other)?;

    seed(&db).await.map_err(Error::other)?;

    let services = dependency_injection(db.clone());
    #[cfg(any(feature = "gui", feature = "html", feature = "tui"))]
    let frontend_enabled = should_start_frontend(&args);
    #[cfg(feature = "karma")]
    let karma_enabled = should_start_karma(&args);

    #[cfg(feature = "gui")]
    if frontend_enabled && let Err(e) = start_gui(services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "html")]
    if frontend_enabled && let Err(e) = start_html(services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "tui")]
    if frontend_enabled && let Err(e) = start_tui(services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "karma")]
    if karma_enabled {
        start_karma(services.clone()).await?;
    }

    Ok(())
}

fn print_help() {
    println!("Usage: lince [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -h, --help            Show this help message");
    println!("      --frontend       Start only the compiled frontend");
    #[cfg(feature = "karma")]
    println!("      --karma          Start only the karma delivery loop");
    #[cfg(feature = "karma")]
    println!("      --karmaless       Disable karma delivery loop");
    #[cfg(feature = "gui")]
    println!("      --guiless        Disable GUI startup");
    #[cfg(feature = "html")]
    println!("      --htmlless       Disable HTML frontend startup");
    #[cfg(feature = "tui")]
    println!("      --tuiless        Disable TUI startup");
    println!();
    println!("To learn more visit https://lince.social")
}

fn has_arg(args: &[String], expected: &str) -> bool {
    args.iter().any(|arg| arg == expected)
}

#[cfg(any(feature = "gui", feature = "html", feature = "tui"))]
fn should_start_frontend(args: &[String]) -> bool {
    let explicitly_selected = has_arg(args, "--frontend") || has_arg(args, "--karma");
    if explicitly_selected {
        return has_arg(args, "--frontend");
    }

    #[cfg(feature = "gui")]
    if has_arg(args, "--guiless") {
        return false;
    }

    #[cfg(feature = "html")]
    if has_arg(args, "--htmlless") {
        return false;
    }

    #[cfg(feature = "tui")]
    if has_arg(args, "--tuiless") {
        return false;
    }

    cfg!(any(feature = "gui", feature = "html", feature = "tui"))
}

#[cfg(feature = "karma")]
fn should_start_karma(args: &[String]) -> bool {
    let explicitly_selected = has_arg(args, "--frontend") || has_arg(args, "--karma");
    if explicitly_selected {
        return has_arg(args, "--karma");
    }

    !has_arg(args, "--karmaless")
}

#[cfg(feature = "gui")]
async fn start_gui(services: InjectedServices) -> Result<(), Error> {
    use application::gpui::get_gpui_startup_data;
    use gui::app::gpui_app;

    match get_gpui_startup_data(services.clone()).await {
        Ok(state) => gpui_app(services, state).await,
        Err(e) => log(LogEntry::Error(e.kind(), e.to_string())),
    }
    Ok(())
}

#[cfg(feature = "html")]
async fn start_html(services: InjectedServices) -> Result<(), Error> {
    serve_html(services).await
}

#[cfg(feature = "tui")]
async fn start_tui(services: InjectedServices) -> Result<(), Error> {
    while let Err(e) = tui_app(services.clone()).await {
        println!("Tui error: {e}")
    }
    Ok(())
}

#[cfg(feature = "karma")]
async fn start_karma(services: InjectedServices) -> Result<(), Error> {
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
