use bevy::{
    asset::AssetId,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
pub use lince_map as data;
use lince_map::{
    diagnostics::{DeviceSnapshot, device_snapshot},
    galileo::{
        galileo_types::cartesian::Size,
        render::{
            WgpuRenderer,
            text::{RustybuzzRasterizer, text_service::TextService},
        },
    },
    scene::{Palette, Scene},
};
use std::{
    array::from_fn,
    collections::HashMap,
    sync::{Arc, Mutex, Once, PoisonError, atomic::Ordering::Relaxed},
    time::Instant,
};

pub fn palette(surface: Color, ink: Color, accent: Color) -> Palette {
    let rgba = |color: Color| {
        let linear = color.to_linear();
        [linear.red, linear.green, linear.blue, linear.alpha]
            .map(|value| (value * 255.0).round() as u8)
    };
    let land = rgba(surface);
    let ink = rgba(ink);
    let accent = rgba(accent);
    let mix = |a: [u8; 4], b: [u8; 4], amount: f64| {
        from_fn(|i| {
            if i == 3 {
                255
            } else {
                (f64::from(a[i]) * (1.0 - amount) + f64::from(b[i]) * amount).round() as u8
            }
        })
    };
    Palette {
        land,
        water: mix(land, accent, 0.35),
        roads: mix(land, ink, 0.7),
        nature: mix(land, accent, 0.15),
        ink,
    }
}
#[derive(Clone)]
pub struct Target {
    pub scene: Arc<Mutex<Scene>>,
    pub image: AssetId<Image>,
    pub size: [u32; 2],
    pub input_sent: Option<Instant>,
    pub on_render: Option<Arc<dyn Fn(Rendered) + Send + Sync>>,
}

#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct Scenes(pub HashMap<Entity, Target>);

#[derive(Resource, Default)]
struct Renderers(HashMap<Entity, Renderer>);

struct Renderer {
    gpu: WgpuRenderer,
    revision: u64,
    image: AssetId<Image>,
    size: [u32; 2],
}

#[derive(Clone)]
pub struct Rendered {
    pub started: Instant,
    pub submitted: Instant,
    pub input: Option<Instant>,
    pub device: DeviceSnapshot,
}

#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct Readback(pub Arc<Mutex<DeviceSnapshot>>);

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
    static INITIALIZED: Once = Once::new();
    INITIALIZED.call_once(|| {
        let service = TextService::initialize(RustybuzzRasterizer::default());
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
        let started = Instant::now();
        let Some(image) = images.get(target.image) else {
            continue;
        };
        let scene = target.scene.lock().unwrap_or_else(PoisonError::into_inner);
        let revision = scene.revision.load(Relaxed);
        let [width, height] = target.size;
        let renderer = renderers.0.entry(*entity).or_insert_with(|| Renderer {
            gpu: WgpuRenderer::new_with_device_and_texture(
                device.wgpu_device().clone(),
                (**queue).clone(),
                Size::new(width, height),
            ),
            revision: 0,
            image: target.image,
            size: target.size,
        });
        if renderer.size != target.size {
            renderer.gpu.resize(Size::new(width, height));
            renderer.size = target.size;
            renderer.revision = 0;
        }
        if renderer.revision == revision && renderer.image == target.image {
            continue;
        }
        renderer.gpu.set_background(scene.background());
        renderer
            .gpu
            .render_to_texture_view(&scene.map, &image.texture_view);
        renderer.revision = revision;
        renderer.image = target.image;
        if let Some(callback) = &target.on_render {
            let rendered = Rendered {
                started,
                submitted: Instant::now(),
                input: target.input_sent,
                device: device_snapshot(device.wgpu_device()),
            };
            // Render callbacks can inspect the same scene.
            drop(scene);
            callback(rendered);
        }
    }
    *metrics.0.lock().unwrap_or_else(PoisonError::into_inner) =
        device_snapshot(device.wgpu_device());
}
