use bevy::{
    prelude::*,
    winit::{UpdateMode, WinitSettings},
};
pub use lince_interface::theme::{INK, PAPER, PURPLE, Typography};
use std::time::Duration;

pub struct ThemePlugin;
impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            lince_interface::theme::TypographyPlugin,
            crate::token_style::TokenStylePlugin,
        ));
    }
}

pub fn idle_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(Duration::MAX),
        unfocused_mode: UpdateMode::reactive_low_power(Duration::MAX),
    }
}
