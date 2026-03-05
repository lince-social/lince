#![cfg(feature = "tui")]

pub mod app;
pub mod components;
pub mod pages;

use crate::app::app;
use injection::cross_cutting::InjectedServices;

pub async fn tui_app(services: InjectedServices) -> color_eyre::Result<()> {
    color_eyre::install()?;

    ratatui::run(|terminal| app(terminal, services.clone()))?;

    Ok(())
}
