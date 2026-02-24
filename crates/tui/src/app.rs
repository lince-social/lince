use injection::cross_cutting::InjectedServices;
use ratatui::{DefaultTerminal, crossterm};

use crate::components::render;

pub fn app(terminal: &mut DefaultTerminal, _services: InjectedServices) -> std::io::Result<()> {
    loop {
        terminal.draw(render)?;
        if crossterm::event::read()?.is_key_press() {
            break Ok(());
        }
    }
}
