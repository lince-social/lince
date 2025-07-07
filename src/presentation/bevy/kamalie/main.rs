pub mod application;
pub mod domain;
pub mod presentation;

use crate::{
    application::movement::{
        advance_physics, handle_input, interpolate_rendered_transform, update_camera,
    },
    domain::entities::{
        npc::{
            setup::setup_npcs,
            spawn::{SpawnTimer, entity_npc_check_spawn},
        },
        user::setup::setup_user,
        world::setup::setup_world,
    },
};
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(SpawnTimer(Timer::from_seconds(1., TimerMode::Repeating)))
        .add_systems(Startup, (setup_user, setup_world, setup_npcs))
        .add_systems(Update, entity_npc_check_spawn)
        .add_systems(FixedUpdate, advance_physics)
        .add_systems(
            RunFixedMainLoop,
            (
                handle_input.in_set(RunFixedMainLoopSystem::BeforeFixedMainLoop),
                interpolate_rendered_transform.in_set(RunFixedMainLoopSystem::AfterFixedMainLoop),
                update_camera.in_set(RunFixedMainLoopSystem::AfterFixedMainLoop),
            ),
        )
        .run();
}
