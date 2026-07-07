//! First-run bootstrap for the new store (the Cell). Seeds the native app tables
//! (configuration + roles + the full permission catalog granted to `admin`) and,
//! when auth is required and no admin exists yet, creates the initial admin user.
//!
//! On Linux/CLI the admin is created by prompting in the terminal (username +
//! hidden password). On platforms that ask their own way (an installer form on
//! Windows/macOS) there is no interactive terminal here, so we skip the prompt
//! and leave admin creation to that platform flow — same policy as the legacy
//! boot.

use std::io::{self, IsTerminal, Write};

use crossterm::{
    event::{read, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use store::Store;
use utils::auth::hash_password;
use utils::logging::status;

/// Seed the Cell's app tables and ensure an initial admin exists on first run.
pub async fn bootstrap_cell(store: &Store, auth_required: bool) -> Result<(), io::Error> {
    // The permission catalog is owned by `application::auth`; store stays
    // decoupled and just persists whatever pairs it is handed.
    let permissions: Vec<(&str, &str)> = ::application::auth::ALL_PERMISSIONS
        .iter()
        .map(|permission| (permission.subject, permission.action))
        .collect();
    store::seed::seed(&store.pool, &permissions)
        .await
        .map_err(io::Error::other)?;

    if !auth_required {
        return Ok(());
    }
    if store::auth::admin_exists(&store.pool).await.map_err(io::Error::other)? {
        return Ok(());
    }

    // First run with auth on and no admin yet.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        status(
            "Auth is required but the new store has no admin yet. \
             Start Lince once in an interactive terminal (or complete the \
             installer's setup) to create the initial admin.",
        );
        return Ok(());
    }

    let (username, password) = tokio::task::spawn_blocking(prompt_admin_credentials)
        .await
        .map_err(io::Error::other)??;
    let password_hash = hash_password(&password)?;
    let admin_role = store::auth::ensure_role(&store.pool, store::auth::ADMIN_ROLE)
        .await
        .map_err(io::Error::other)?;
    store::auth::create_user(&store.pool, &username, &username, &password_hash, admin_role)
        .await
        .map_err(io::Error::other)?;
    status(format!("Created initial admin user `{username}` on the new store."));
    Ok(())
}

/// Prompt (blocking, on a terminal) for the initial admin's username and a
/// confirmed hidden password. Mirrors the legacy CLI bootstrap.
fn prompt_admin_credentials() -> Result<(String, String), io::Error> {
    println!("Auth is enabled and the new store has no admin user yet.");
    let username = prompt_username()?;
    let password = prompt_password_with_confirmation()?;
    Ok((username, password))
}

fn prompt_username() -> Result<String, io::Error> {
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

fn prompt_password_with_confirmation() -> Result<String, io::Error> {
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

fn prompt_password(prompt: &str) -> Result<String, io::Error> {
    let mut stdout = io::stdout();
    print!("{prompt}");
    stdout.flush()?;
    enable_raw_mode().map_err(io::Error::other)?;

    let mut password = String::new();
    loop {
        match read().map_err(io::Error::other)? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    disable_raw_mode().map_err(io::Error::other)?;
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
                KeyCode::Esc => {
                    disable_raw_mode().map_err(io::Error::other)?;
                    println!();
                    return Err(io::Error::other("admin setup cancelled"));
                }
                _ => {}
            },
            _ => {}
        }
    }
}
