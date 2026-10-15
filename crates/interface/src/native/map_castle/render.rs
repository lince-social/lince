use super::*;
use bevy::{
    asset::AssetId,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
use lince_map::galileo::{galileo_types::cartesian::Size, render::WgpuRenderer};
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct Target {
    pub scene: Arc<Mutex<Scene>>,
    pub image: AssetId<Image>,
    pub size: [u32; 2],
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub(super) struct Scenes(pub HashMap<Entity, Target>);

#[derive(Resource, Default)]
struct Renderers(HashMap<Entity, (WgpuRenderer, u64, AssetId<Image>, [u32; 2])>);

pub(super) fn install(app: &mut App) {
    app.add_plugins(ExtractResourcePlugin::<Scenes>::default());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.init_resource::<Renderers>().add_systems(
            Render,
            draw.after(RenderSystems::PrepareAssets)
                .before(RenderSystems::Render),
        );
    }
}

fn draw(
    scenes: Res<Scenes>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut renderers: ResMut<Renderers>,
) {
    renderers
        .0
        .retain(|entity, _| scenes.0.contains_key(entity));
    for (entity, target) in &scenes.0 {
        let Some(image) = images.get(target.image) else {
            continue;
        };
        let scene = target
            .scene
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let revision = scene.revision.load(Ordering::Relaxed);
        let renderer = renderers.0.entry(*entity).or_insert_with(|| {
            (
                WgpuRenderer::new_with_device_and_texture(
                    device.wgpu_device().clone(),
                    queue.0.as_ref().clone().into_inner(),
                    Size::new(target.size[0], target.size[1]),
                ),
                0,
                target.image,
                target.size,
            )
        });
        if renderer.3 != target.size {
            renderer.0.resize(Size::new(target.size[0], target.size[1]));
            renderer.3 = target.size;
            renderer.1 = 0;
        }
        if renderer.1 != revision || renderer.2 != target.image {
            renderer.0.set_background(scene.background());
            renderer
                .0
                .render_to_texture_view(&scene.map, &image.texture_view);
            renderer.1 = revision;
            renderer.2 = target.image;
        }
    }
}
