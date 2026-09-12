use bevy::{
    prelude::*,
    winit::{UpdateMode, WinitSettings},
};
use std::time::Duration;

pub const PAPER: Color = Color::srgb(18.0 / 255.0, 18.0 / 255.0, 20.0 / 255.0);
pub const INK: Color = Color::srgb(248.0 / 255.0, 250.0 / 255.0, 252.0 / 255.0);
pub const PURPLE: Color = Color::srgb(99.0 / 255.0, 102.0 / 255.0, 241.0 / 255.0);

#[derive(Resource)]
pub struct Typography(pub Handle<Font>);

impl FromWorld for Typography {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
            include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
        )))
    }
}

impl Typography {
    pub fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

pub struct ThemePlugin;
impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typography>()
            .add_plugins(crate::token_style::TokenStylePlugin);
    }
}

pub fn idle_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(Duration::MAX),
        unfocused_mode: UpdateMode::reactive_low_power(Duration::MAX),
    }
}
