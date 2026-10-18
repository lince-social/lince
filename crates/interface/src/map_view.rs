use bevy::prelude::*;
pub use lince_map as data;
use lince_map::scene::Scene;
use std::sync::{Arc, Mutex, atomic::Ordering};

pub fn palette(surface: Color, ink: Color, accent: Color) -> lince_map::scene::Palette {
    let rgba = |color: Color| {
        let linear = color.to_linear();
        [linear.red, linear.green, linear.blue, linear.alpha]
            .map(|value| (value * 255.0).round() as u8)
    };
    let land = rgba(surface);
    let ink = rgba(ink);
    let accent = rgba(accent);
    let mix = |a: [u8; 4], b: [u8; 4], amount: f64| {
        std::array::from_fn(|i| {
            if i == 3 {
                255
            } else {
                (f64::from(a[i]) * (1.0 - amount) + f64::from(b[i]) * amount).round() as u8
            }
        })
    };
    lince_map::scene::Palette {
        land,
        water: mix(land, accent, 0.35),
        roads: mix(land, ink, 0.7),
        nature: mix(land, accent, 0.15),
        ink,
    }
}
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
pub struct Target {
    pub scene: Arc<Mutex<Scene>>,
    pub image: AssetId<Image>,
    pub size: [u32; 2],
    pub input_sent: Option<std::time::Instant>,
    pub on_render: Option<Arc<dyn Fn(Rendered) + Send + Sync>>,
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct Scenes(pub HashMap<Entity, Target>);

#[derive(Resource, Default)]
struct Renderers(HashMap<Entity, (WgpuRenderer, u64, AssetId<Image>, [u32; 2])>);

#[derive(Clone)]
pub struct Rendered {
    pub started: std::time::Instant,
    pub submitted: std::time::Instant,
    pub input: Option<std::time::Instant>,
    pub device: lince_map::diagnostics::DeviceSnapshot,
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct Readback(pub Arc<Mutex<lince_map::diagnostics::DeviceSnapshot>>);

pub struct MapRendererPlugin;
impl Plugin for MapRendererPlugin {
    fn build(&self, app: &mut App) {
        initialize_fonts();
        app.init_resource::<Scenes>()
            .init_resource::<Readback>()
            .add_plugins((
                ExtractResourcePlugin::<Scenes>::default(),
                ExtractResourcePlugin::<Readback>::default(),
            ));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.init_resource::<Renderers>().add_systems(
                Render,
                draw.after(RenderSystems::PrepareAssets)
                    .before(RenderSystems::Render),
            );
        }
    }
}

pub fn initialize_fonts() {
    static INITIALIZED: std::sync::Once = std::sync::Once::new();
    INITIALIZED.call_once(|| {
        let service = lince_map::galileo::render::text::text_service::TextService::initialize(
            lince_map::galileo::render::text::RustybuzzRasterizer::default(),
        );
        service.load_font(Arc::new(
            include_bytes!("../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
        ));
        service.load_font(Arc::new(
            include_bytes!("../../../institute/assets/fonts/DejaVuSans/DejaVuSans.ttf").to_vec(),
        ));
    });
}

fn draw(
    scenes: Res<Scenes>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut renderers: ResMut<Renderers>,
    metrics: Res<Readback>,
) {
    renderers
        .0
        .retain(|entity, _| scenes.0.contains_key(entity));
    for (entity, target) in &scenes.0 {
        let started = std::time::Instant::now();
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
            if let Some(callback) = &target.on_render {
                callback(Rendered {
                    started,
                    submitted: std::time::Instant::now(),
                    input: target.input_sent,
                    device: lince_map::diagnostics::device_snapshot(device.wgpu_device()),
                });
            }
        }
    }
    *metrics.0.lock().unwrap_or_else(|error| error.into_inner()) =
        lince_map::diagnostics::device_snapshot(device.wgpu_device());
}
