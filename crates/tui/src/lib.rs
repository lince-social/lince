#![cfg(feature = "tui")]

pub mod app;
pub mod components;
pub mod pages;

use crate::app::app;
use injection::cross_cutting::InjectedServices;
use ratatui::crossterm::{
    event::{
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use std::io::stdout;

pub async fn tui_app(services: InjectedServices) -> color_eyre::Result<()> {
    color_eyre::install()?;
    let _ = execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    );
    let mut terminal = ratatui::init();
    let result = app(&mut terminal, services).await;
    ratatui::restore();
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    result.map_err(Into::into)
}
