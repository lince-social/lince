use crate::presentation::bevy::kamalie::{
    application::util::rand::{util_random_1d_size, util_random_color, util_random_position},
    domain::components::life::Ego,
};
use bevy::prelude::*;

#[derive(Resource)]
pub struct SpawnTimer(pub Timer);

pub fn entity_npc_check_spawn(
    mut timer: ResMut<SpawnTimer>,
    repeating_period_time: Res<Time>,
    commands: Commands,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<ColorMaterial>>,
) {
    if timer.0.tick(repeating_period_time.delta()).just_finished() {
        entity_npc_spawn(commands, meshes, materials, 1);
    }
}

pub fn entity_npc_spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    num_spawn: i32,
) {
    for _ in 0..num_spawn {
        commands.spawn((
            Ego::Ish,
            Mesh2d(meshes.add(Circle::new(util_random_1d_size()))),
            MeshMaterial2d(materials.add(util_random_color())),
            util_random_position(),
        ));
    }
}
