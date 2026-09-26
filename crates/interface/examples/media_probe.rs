use bevy::prelude::*;
use lince_interface::communication::{MediaPreviewPlugin, populate};

fn main() {
    let mut app = lince_interface::app::interface_app();
    app.add_plugins(MediaPreviewPlugin);
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.add_systems(Startup, setup);
    app.run();
}

fn setup(world: &mut World) {
    let sand = world
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(20)),
            ..default()
        })
        .id();
    populate(world, sand).expect("media preview starts");
}
