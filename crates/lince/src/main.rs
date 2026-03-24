#![forbid(unsafe_code)]

mod bootstrap_config;

#[cfg(all(feature = "gui", feature = "tui"))]
compile_error!("Enable only one frontend feature at a time: `gui`, `tui`, or `http`.");
#[cfg(all(feature = "gui", feature = "http"))]
compile_error!("Enable only one frontend feature at a time: `gui`, `tui`, or `http`.");
#[cfg(all(feature = "tui", feature = "http"))]
compile_error!("Enable only one frontend feature at a time: `gui`, `tui`, or `http`.");

#[cfg(feature = "karma")]
use application::karma::karma_deliver;
use crossterm::{
    event::{Event, KeyCode, KeyEventKind, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use injection::cross_cutting::{InjectedServices, dependency_injection};
use persistence::{
    connection::{connection, read_only_connection},
    seeder::seed,
    storage::StorageService,
    write_coordinator::{SqlParameter, WriteCoordinatorHandle, spawn_write_coordinator},
};
#[cfg(feature = "karma")]
use std::time::Duration;
use std::{
    env,
    io::{self, Error, IsTerminal, Write},
    sync::Arc,
};
#[cfg(feature = "tui")]
use tui::tui_app;
use utils::auth::hash_password;
use utils::logging::{LogEntry, log};
#[cfg(feature = "http")]
use web::{HttpServeMode, serve as serve_web};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = env::args().collect::<Vec<String>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let bootstrap = bootstrap_config::load_or_init_bootstrap_config()?;
    let listen_addr = arg_value(&args, "--listen-addr");

    let db = Arc::new(connection().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?);
    sqlx::migrate!("../../migrations")
        .run(&*db)
        .await
        .map_err(Error::other)?;

    seed(&db).await.map_err(Error::other)?;

    #[cfg(any(feature = "gui", feature = "http", feature = "tui"))]
    let frontend_enabled = should_start_frontend(&args);
    #[cfg(feature = "karma")]
    let karma_enabled = should_start_karma(&args);

    let writer = spawn_write_coordinator().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?;

    let read_db = Arc::new(read_only_connection().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?);
    ensure_local_admin_if_needed(&read_db, &writer, bootstrap.auth_enabled).await?;
    let storage = Arc::new(
        StorageService::from_database(&read_db)
            .await
            .inspect_err(|e| {
                log(LogEntry::Error(e.kind(), e.to_string()));
            })?,
    );
    storage.ensure_bucket_exists().await.inspect_err(|e| {
        log(LogEntry::Error(e.kind(), e.to_string()));
    })?;

    let services = dependency_injection(read_db.clone(), storage, writer.clone());

    #[cfg(feature = "karma")]
    let karma_handle = if karma_enabled {
        Some(tokio::spawn(start_karma(services.clone())))
    } else {
        None
    };

    #[cfg(feature = "gui")]
    if frontend_enabled && let Err(e) = start_gui(services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "http")]
    if frontend_enabled
        && let Err(e) = start_html(
            services.clone(),
            listen_addr.clone(),
            bootstrap.clone(),
            &args,
        )
        .await
    {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "tui")]
    if frontend_enabled && let Err(e) = start_tui(services.clone()).await {
        log(LogEntry::Error(e.kind(), e.to_string()));
    }

    #[cfg(feature = "karma")]
    if !frontend_enabled && let Some(handle) = karma_handle {
        handle.await.map_err(Error::other)??;
    }

    Ok(())
}

fn print_help() {
    println!("Usage: lince [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -h, --help            Show this help message");
    println!("      --listen-addr <addr>  Override the HTTP listen address");
    #[cfg(feature = "http")]
    println!(
        "      --http-api-only  Serve only the HTTP API. Do not expose the board UI or host widget routes."
    );
    println!("      --frontend       Start only the compiled frontend");
    #[cfg(feature = "karma")]
    println!("      --karma          Start only the karma delivery loop");
    #[cfg(feature = "karma")]
    println!("      --karmaless       Disable karma delivery loop");
    #[cfg(feature = "gui")]
    println!("      --guiless        Disable GUI startup");
    #[cfg(feature = "http")]
    println!("      --htmlless       Disable HTML frontend startup");
    #[cfg(feature = "tui")]
    println!("      --tuiless        Disable TUI startup");
    println!();
    println!("To learn more visit https://lince.social")
}

fn has_arg(args: &[String], expected: &str) -> bool {
    args.iter().any(|arg| arg == expected)
}

fn arg_value(args: &[String], expected: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == expected).then(|| window[1].clone()))
}

#[cfg(any(feature = "gui", feature = "http", feature = "tui"))]
fn should_start_frontend(args: &[String]) -> bool {
    let explicitly_selected = has_arg(args, "--frontend") || has_arg(args, "--karma");
    if explicitly_selected {
        return has_arg(args, "--frontend");
    }

    #[cfg(feature = "gui")]
    if has_arg(args, "--guiless") {
        return false;
    }

    #[cfg(feature = "http")]
    if has_arg(args, "--htmlless") {
        return false;
    }

    #[cfg(feature = "tui")]
    if has_arg(args, "--tuiless") {
        return false;
    }

    cfg!(any(feature = "gui", feature = "http", feature = "tui"))
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

#[cfg(feature = "http")]
async fn start_html(
    services: InjectedServices,
    listen_addr: Option<String>,
    bootstrap: bootstrap_config::BootstrapConfig,
    args: &[String],
) -> Result<(), Error> {
    let mode = if has_arg(args, "--http-api-only") {
        HttpServeMode::ApiOnly
    } else {
        HttpServeMode::FullUi
    };

    serve_web(
        services,
        bootstrap.secret,
        bootstrap.auth_enabled,
        listen_addr,
        mode,
    )
    .await
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

async fn seed_root_user(
    writer: &WriteCoordinatorHandle,
    username: &str,
    password: &str,
) -> Result<(), Error> {
    let password_hash = hash_password(password)?;
    let username = username.trim();
    let _ = writer
        .execute_statement(
            "
            INSERT INTO app_user(name, username, password_hash, role_id)
            SELECT ?, ?, ?, (SELECT id FROM role WHERE name = 'admin')
            WHERE NOT EXISTS (
                SELECT 1 FROM app_user WHERE username = ?
            )
            "
            .to_string(),
            vec![
                SqlParameter::Text(username.to_string()),
                SqlParameter::Text(username.to_string()),
                SqlParameter::Text(password_hash),
                SqlParameter::Text(username.to_string()),
            ],
        )
        .await?;
    let _ = writer
        .execute_statement(
            "
            UPDATE app_user
            SET role_id = (SELECT id FROM role WHERE name = 'admin')
            WHERE username = ?
            "
            .to_string(),
            vec![SqlParameter::Text(username.to_string())],
        )
        .await?;
    println!("Admin user ensured for username {username}");
    Ok(())
}

async fn ensure_local_admin_if_needed(
    read_db: &Arc<sqlx::Pool<sqlx::Sqlite>>,
    writer: &WriteCoordinatorHandle,
    auth_enabled: bool,
) -> Result<(), Error> {
    if !auth_enabled {
        return Ok(());
    }

    let admin_count = sqlx::query_scalar::<_, i64>(
        "
        SELECT COUNT(1)
        FROM app_user
        WHERE role_id = (SELECT id FROM role WHERE name = 'admin')
        ",
    )
    .fetch_one(&**read_db)
    .await
    .map_err(Error::other)?;

    if admin_count > 0 {
        return Ok(());
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::other(
            "Auth is enabled in ~/.config/lince/lince.toml but no admin user exists. Run lince in an interactive terminal once to create the initial admin.",
        ));
    }

    println!("Auth is enabled and no admin user exists yet.");
    let username = prompt_username()?;
    let password = prompt_password_with_confirmation()?;
    seed_root_user(writer, &username, &password).await
}

fn prompt_username() -> Result<String, Error> {
    let mut stdout = io::stdout();
    let mut input = String::new();

    loop {
        print!("Admin username [user]: ");
        stdout.flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;

        let trimmed = input.trim();
        let username = if trimmed.is_empty() { "user" } else { trimmed };
        if !username.is_empty() {
            return Ok(username.to_string());
        }
    }
}

fn prompt_password_with_confirmation() -> Result<String, Error> {
    loop {
        let password = prompt_password("Admin password: ")?;
        if password.is_empty() {
            println!("Password cannot be empty.");
            continue;
        }

        let confirmation = prompt_password("Confirm password: ")?;
        if password != confirmation {
            println!("Passwords do not match.");
            continue;
        }

        return Ok(password);
    }
}

fn prompt_password(prompt: &str) -> Result<String, Error> {
    let mut stdout = io::stdout();
    print!("{prompt}");
    stdout.flush()?;
    enable_raw_mode().map_err(Error::other)?;

    let mut password = String::new();
    loop {
        match read().map_err(Error::other)? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    disable_raw_mode().map_err(Error::other)?;
                    println!();
                    return Ok(password);
                }
                KeyCode::Char(ch) => {
                    password.push(ch);
                    print!("*");
                    stdout.flush()?;
                }
                KeyCode::Backspace => {
                    if password.pop().is_some() {
                        print!("\u{8} \u{8}");
                        stdout.flush()?;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
