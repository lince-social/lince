use bevy::{
    app::{App, Startup},
    prelude::Plugin,
};

pub struct CollectionPlugin;

impl Plugin for CollectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup() {
    println!("Hello from bevy plugin");
}
