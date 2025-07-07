use bevy::{color::Color, prelude::*};
use rand::Rng;

pub fn util_random_unit_float() -> f32 {
    rand::rng().random()
}

pub fn util_random_color() -> Color {
    Color::srgb(
        util_random_unit_float(),
        util_random_unit_float(),
        util_random_unit_float(),
    )
}

pub fn util_random_2d_size() -> Vec2 {
    vec2(
        rand::rng().random_range(100.0..200.),
        rand::rng().random_range(100.0..200.),
    )
}

pub fn util_random_1d_size() -> f32 {
    rand::rng().random_range(100.0..200.)
}

pub fn util_random_position() -> Transform {
    Transform::from_xyz(
        rand::rng().random_range(-10000.0..10000.0),
        rand::rng().random_range(-10000.0..10000.0),
        0.0,
    )
}

pub fn util_random_time_progressive_value(time: Res<Time<Virtual>>) -> f32 {
    rand::rng().random_range(0.0..0.0001 * time.elapsed_secs())
}
