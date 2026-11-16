pub mod controls;
mod data;
mod dungeon;
mod fracture;
mod host;
mod input;
mod objects;
mod render;
mod run;
mod wake;

pub use host::KamaliePlugin;
pub(crate) use host::populate;

pub const SIZE: bevy::prelude::Vec2 = bevy::prelude::Vec2::new(816.0, 592.0);
