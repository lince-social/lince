#![cfg(feature = "tui")]

pub mod app;
pub mod components;

use crate::app::app;
use injection::cross_cutting::InjectedServices;

pub async fn tui_app(services: InjectedServices) -> color_eyre::Result<()> {
    color_eyre::install()?;

    ratatui::run(|terminal| app(terminal, services.clone()))?;

    Ok(())
}
