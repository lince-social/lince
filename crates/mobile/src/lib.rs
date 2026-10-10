#![recursion_limit = "256"]

#[cfg(target_os = "android")]
mod accessibility;
#[cfg(target_os = "android")]
mod android;
pub mod app;
pub mod attachments;
mod body;
mod images;
mod location;
pub mod connection;
mod kanban;
pub mod navigation;
mod organ;
mod pages;
pub mod picker;
pub mod profiles;
mod qr;
pub mod record;
mod scroll;
pub mod session;
#[cfg(all(target_os = "android", feature = "android-smoke", debug_assertions))]
mod smoke;
pub mod storage;
pub mod views;

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
            location::LocationPlugin,
            scroll::ScrollPlugin,
            kanban::KanbanPlugin,
        ))
        .run();
}
