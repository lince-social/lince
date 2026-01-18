#![cfg(feature = "tui")]

pub mod app;

use crate::app::tui_components;
use injection::cross_cutting::InjectedServices;

pub async fn tui_app(services: InjectedServices) {
    println!("TUI app started with ratatui");
    tui_components(services.clone());
}
