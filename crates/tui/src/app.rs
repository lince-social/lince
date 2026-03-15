mod components;
mod logic;
mod rendering;

use injection::cross_cutting::InjectedServices;
use logic::{
    TuiState, handle_key_event_result, open_sqlite_monitor_connection, sqlite_data_version,
};
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};
use rendering::render;
use std::time::Duration;

pub async fn app(
    terminal: &mut DefaultTerminal,
    services: InjectedServices,
) -> std::io::Result<()> {
    let mut state = TuiState::load(services.clone()).await?;
    let mut monitor_connection = open_sqlite_monitor_connection().await;
    let mut last_data_version = match monitor_connection.as_mut() {
        Some(connection) => sqlite_data_version(connection).await.ok(),
        None => None,
    };

    loop {
        state.prune_toasts();
        terminal.draw(|frame| render(frame, &mut state))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            match handle_key_event_result(&mut state, services.clone(), key).await {
                Ok(true) => break Ok(()),
                Ok(false) => {}
                Err(error) => state.push_error(error.to_string()),
            }
        }

        if let Some(connection) = monitor_connection.as_mut()
            && let Ok(current_data_version) = sqlite_data_version(connection).await
            && last_data_version != Some(current_data_version)
        {
            last_data_version = Some(current_data_version);
            if let Err(error) = state.reload(services.clone()).await {
                state.push_error(error.to_string());
            }
        }
    }
}
