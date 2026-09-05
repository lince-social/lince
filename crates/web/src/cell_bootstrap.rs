use std::io::{self, IsTerminal, Write};

use crossterm::{
    event::{Event, KeyCode, KeyEventKind, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use store::Store;
use utils::auth::hash_password;
use utils::desktop_setup::DesktopInstallSetup;
use utils::logging::status;

pub async fn bootstrap_cell(
    store: &Store,
    auth_required: bool,
    local_base_url: &str,
    staged: Option<&DesktopInstallSetup>,
    admin_is_mandatory: bool,
) -> Result<(), io::Error> {
    let permissions: Vec<(&str, &str)> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|permission| (permission.subject, permission.action))
        .collect();
    store::seed::seed(&store.pool, &permissions)
        .await
        .map_err(io::Error::other)?;
    store::organs::ensure_local(&store.pool, local_base_url)
        .await
        .map_err(io::Error::other)?;

    if let Some(language) = staged
        .and_then(|setup| setup.language.as_deref())
        .map(str::trim)
        .filter(|language| !language.is_empty())
    {
        store::config::set_language(&store.pool, language)
            .await
            .map_err(io::Error::other)?;
    }

    if !auth_required {
        return Ok(());
    }
    if store::auth::admin_exists(&store.pool)
        .await
        .map_err(io::Error::other)?
    {
        return Ok(());
    }

    let (username, password) = if let Some(password) = staged
        .and_then(|setup| setup.initial_admin_password.as_deref())
        .map(str::trim)
        .filter(|password| !password.is_empty())
    {
        ("user".to_string(), password.to_string())
    } else if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        if admin_is_mandatory {
            return Err(io::Error::other(
                "Refusing to start: server mode requires login, but this store \
                 has no admin user and there is no terminal to create one on. \
                 Pass --initial-admin-password-file <path> (or --initial-admin-password), \
                 or run once interactively.",
            ));
        }
        status(
            "Auth is required but the new store has no admin yet. \
             Start Lince once in an interactive terminal (or complete the \
             installer's setup) to create the initial admin.",
        );
        return Ok(());
    } else {
        tokio::task::spawn_blocking(prompt_admin_credentials)
            .await
            .map_err(io::Error::other)??
    };
    let password_hash = hash_password(&password)?;
    let admin_role = store::auth::ensure_role(&store.pool, store::auth::ADMIN_ROLE)
        .await
        .map_err(io::Error::other)?;
    store::auth::create_person_login(
        &store.pool,
        &username,
        &username,
        &password_hash,
        admin_role,
    )
    .await
    .map_err(io::Error::other)?;
    status(format!(
        "Created initial admin user `{username}` on the new store."
    ));
    Ok(())
}

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
