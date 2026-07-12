use crate::bootstrap_config;
use std::io::Error;
use tokio::sync::oneshot;
use utils::desktop_setup::{read_staged_setup, remove_staged_setup};
use web::serve_cell_api_only;

const DESKTOP_LISTEN_ADDR: &str = "127.0.0.1:6174";

#[derive(Clone)]
pub struct DesktopRuntime {
    pub url: String,
    pub start_on_login: bool,
    pub start_silent: bool,
}

pub async fn start_desktop_server() -> Result<DesktopRuntime, Error> {
    let staged_setup = read_staged_setup()?;
    if let Some(auth_enabled) = staged_setup.as_ref().and_then(|setup| setup.auth_enabled) {
        bootstrap_config::set_auth_enabled(auth_enabled)?;
    }
    let bootstrap = bootstrap_config::load_or_init_bootstrap_config()?;

    let start_on_login = staged_setup
        .as_ref()
        .and_then(|setup| setup.start_on_login)
        .unwrap_or(false);
    let start_silent = staged_setup
        .as_ref()
        .and_then(|setup| setup.start_silent)
        .unwrap_or(false);
    if staged_setup.is_some() {
        remove_staged_setup()?;
    }

    let (addr_tx, addr_rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Err(error) = serve_cell_api_only(
            Some(DESKTOP_LISTEN_ADDR.to_string()),
            bootstrap.secret,
            bootstrap.auth_enabled,
            staged_setup,
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
        start_on_login,
        start_silent,
    })
}
