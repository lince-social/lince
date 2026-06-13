use crate::bootstrap_config;
use application::{
    file_sync,
    karma::refresh_karma_cache,
    write::{set_active_configuration_language_if_unset, set_desktop_startup_for_active},
};
use injection::cross_cutting::{InjectedServices, dependency_injection};
use persistence::{
    bootstrap_database,
    connection::{connection, read_only_connection},
    storage::StorageService,
    write_coordinator::{SqlParameter, WriteCoordinatorHandle, spawn_write_coordinator},
};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};
use tokio::sync::oneshot;
use utils::{
    auth::hash_password,
    desktop_setup::{DesktopInstallSetup, read_staged_setup, remove_staged_setup},
};
use web::{HttpServeMode, serve_with_bound_addr_sender};

#[derive(Debug, Clone)]
pub struct DesktopRuntime {
    pub url: String,
    pub services: InjectedServices,
    pub start_on_login: bool,
    pub start_silent: bool,
}

pub async fn start_desktop_server() -> Result<DesktopRuntime, Error> {
    let staged_setup = read_staged_setup()?;
    if let Some(auth_enabled) = staged_setup.as_ref().and_then(|setup| setup.auth_enabled) {
        bootstrap_config::set_auth_enabled(auth_enabled)?;
    }
    let bootstrap = bootstrap_config::load_or_init_bootstrap_config()?;

    let db = Arc::new(connection().await?);
    bootstrap_database(&db, "http://127.0.0.1:0").await?;

    let writer = spawn_write_coordinator().await?;
    let read_db = Arc::new(read_only_connection().await?);
    ensure_local_admin_if_needed(
        &read_db,
        &writer,
        bootstrap.auth_enabled,
        staged_setup
            .as_ref()
            .and_then(|setup| setup.initial_admin_password.as_deref()),
    )
    .await?;

    let storage = Arc::new(StorageService::from_database(&read_db).await?);
    if storage.is_enabled() {
        storage.ensure_bucket_exists().await?;
    }
    let services = dependency_injection(read_db, storage, writer);

    if let Some(setup) = staged_setup.as_ref() {
        import_staged_setup(services.clone(), setup).await?;
        remove_staged_setup()?;
    }

    let active_configuration = services.repository.configuration.get_active().await?;
    let start_on_login = active_configuration.desktop_start_on_login == Some(1);
    let start_silent = active_configuration.desktop_start_silent == Some(1);

    refresh_karma_cache(services.clone()).await?;
    file_sync::configure_from_active_configuration(services.clone()).await?;
    file_sync::start_if_enabled(services.clone()).await?;

    let (addr_tx, addr_rx) = oneshot::channel();
    let server_services = services.clone();
    tokio::spawn(async move {
        if let Err(error) = serve_with_bound_addr_sender(
            server_services,
            bootstrap.secret,
            bootstrap.auth_enabled,
            Some("127.0.0.1:0".to_string()),
            HttpServeMode::FullUi,
            Some(addr_tx),
        )
        .await
        {
            eprintln!("Lince desktop server stopped: {error}");
        }
    });

    let addr = addr_rx.await.map_err(Error::other)?;
    Ok(DesktopRuntime {
        url: format!("http://{addr}"),
        services,
        start_on_login,
        start_silent,
    })
}

async fn ensure_local_admin_if_needed(
    read_db: &Arc<sqlx::Pool<sqlx::Sqlite>>,
    writer: &WriteCoordinatorHandle,
    auth_enabled: bool,
    staged_password: Option<&str>,
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

    let password = staged_password
        .map(str::trim)
        .filter(|password| !password.is_empty())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::PermissionDenied,
                "Auth is enabled but no installer-provided admin password was found.",
            )
        })?;

    seed_root_user(writer, "user", password).await
}

async fn seed_root_user(
    writer: &WriteCoordinatorHandle,
    username: &str,
    password: &str,
) -> Result<(), Error> {
    let password_hash = hash_password(password)?;
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
    Ok(())
}

async fn import_staged_setup(
    services: InjectedServices,
    setup: &DesktopInstallSetup,
) -> Result<(), Error> {
    if setup.start_on_login.is_some() || setup.start_silent.is_some() {
        set_desktop_startup_for_active(services.clone(), setup.start_on_login, setup.start_silent)
            .await?;
    }
    if let Some(language) = setup
        .language
        .as_deref()
        .map(str::trim)
        .filter(|language| !language.is_empty())
    {
        set_active_configuration_language_if_unset(services, language).await?;
    }
    Ok(())
}
