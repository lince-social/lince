use crate::domain::components::movement::{
    AccumulatedInput, PhysicalTranslation, PreviousPhysicalTranslation, Velocity,
};
use bevy::{color::palettes::css::PURPLE, prelude::*};

#[derive(Component)]
pub struct Player;

pub fn setup_user(mut commands: Commands) {
    let size: f32 = 100.0;
    commands.spawn((
        Player,
        Name::new("Player"),
        Sprite::from_color(PURPLE, vec2(size, size)),
        Transform::from_scale(Vec3::splat(0.3)),
        AccumulatedInput::default(),
        Velocity::default(),
        PhysicalTranslation::default(),
        PreviousPhysicalTranslation::default(),
    ));
}
