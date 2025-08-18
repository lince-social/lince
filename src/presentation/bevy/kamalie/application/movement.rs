use bevy::prelude::*;

use crate::presentation::bevy::kamalie::domain::{
    components::movement::{
        AccumulatedInput, PhysicalTranslation, PreviousPhysicalTranslation, Velocity,
    },
    entities::user::setup::Player,
};

pub const CAMERA_DECAY_RATE: f32 = 10.;

pub fn update_camera(
    mut camera: Single<&mut Transform, (With<Camera2d>, Without<Player>)>,
    player: Single<&Transform, (With<Player>, Without<Camera2d>)>,
    time: Res<Time>,
) {
    let Vec3 { x, y, .. } = player.translation;
    let direction = Vec3::new(x, y, camera.translation.z);
    camera
        .translation
        .smooth_nudge(&direction, CAMERA_DECAY_RATE, time.delta_secs());
}

pub fn handle_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut AccumulatedInput, &mut Velocity)>,
) {
    for (mut input, mut velocity) in query.iter_mut() {
        if keyboard_input.pressed(KeyCode::ArrowUp) {
            input.y += 1.0;
        }
        if keyboard_input.pressed(KeyCode::ArrowRight) {
            input.x += 1.0;
        }
        if keyboard_input.pressed(KeyCode::ArrowDown) {
            input.y -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::ArrowLeft) {
            input.x -= 1.0;
        }

        velocity.0 = input.extend(0.0).normalize_or_zero() * 500.;
    }
}

pub fn advance_physics(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(
        &mut PhysicalTranslation,
        &mut PreviousPhysicalTranslation,
        &mut AccumulatedInput,
        &mut Velocity,
    )>,
) {
    for (
        mut current_physical_translation,
        mut previous_physical_translation,
        mut input,
        velocity,
    ) in query.iter_mut()
    {
        previous_physical_translation.0 = current_physical_translation.0;
        current_physical_translation.0 += velocity.0 * fixed_time.delta_secs();

        input.0 = Vec2::ZERO;
    }
}

pub fn interpolate_rendered_transform(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(
        &mut Transform,
        &PhysicalTranslation,
        &PreviousPhysicalTranslation,
    )>,
) {
    for (mut transform, current_physical_translation, previous_physical_translation) in
        query.iter_mut()
    {
        let previous = previous_physical_translation.0;
        let current = current_physical_translation.0;
        let alpha = fixed_time.overstep_fraction();

        let rendered_transform = previous.lerp(current, alpha);
        transform.translation = rendered_transform;
    }
}
