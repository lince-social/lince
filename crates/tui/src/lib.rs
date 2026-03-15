#![cfg(feature = "tui")]

pub mod app;
pub mod components;
pub mod pages;

use crate::app::app;
use injection::cross_cutting::InjectedServices;

pub async fn tui_app(services: InjectedServices) -> color_eyre::Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let result = app(&mut terminal, services).await;
    ratatui::restore();
    result.map_err(Into::into)
}
