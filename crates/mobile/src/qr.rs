use bevy::prelude::*;

pub fn show(world: &mut World, parent: Entity, text: &str) {
    let Ok((width, pixels)) = lince_interface::qr::pixels(text) else {
        crate::app::label(
            world,
            parent,
            "QR unavailable. Copy the code instead.",
            16.0,
        );
        return;
    };
    world.init_resource::<Assets<Image>>();
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d {
            width,
            height: width,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        pixels,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    let image = world.resource_mut::<Assets<Image>>().add(image);
    world.spawn((
        ChildOf(parent),
        ImageNode::new(image),
        Node {
            width: percent(100),
            max_width: px(360.0),
            aspect_ratio: Some(1.0),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}
