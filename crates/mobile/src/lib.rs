#![recursion_limit = "256"]

#[cfg(target_os = "android")]
mod android;
pub mod app;
pub mod connection;
pub mod navigation;
mod pages;
pub mod record;
mod scroll;
pub mod storage;

pub fn run(directory: std::path::PathBuf) {
    use bevy::prelude::*;
    App::new()
        .insert_resource(app::Mobile::new(directory))
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_secs(1),
            ),
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_secs(5),
            ),
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Lince Mobile".into(),
                resolution: (430, 820).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            lince_interface::theme::TypographyPlugin,
            bevy::input_focus::tab_navigation::TabNavigationPlugin,
            app::MobilePlugin,
            scroll::ScrollPlugin,
        ))
        .run();
}
