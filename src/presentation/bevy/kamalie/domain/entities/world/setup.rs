use bevy::prelude::*;

pub fn setup_world(mut commands: Commands) {
    commands.spawn(Camera2d);
}
