use crate::presentation::bevy::kamalie::domain::clean::npc::spawn::entity_npc_spawn;
use bevy::prelude::*;
use rand::random_range;

pub fn setup_npcs(
    commands: Commands,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<ColorMaterial>>,
) {
    let n = 0..random_range(100..200);
    entity_npc_spawn(commands, meshes, materials, n.end);
}
