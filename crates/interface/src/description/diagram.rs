use super::*;
use std::sync::{Arc, Mutex, mpsc};

type Pixels = Result<(u32, u32, Vec<u8>), String>;
#[derive(Component)]
struct Diagram {
    source: String,
    label: Entity,
}
#[derive(Resource, Default)]
struct Worker {
    active: Option<(String, Arc<Mutex<mpsc::Receiver<Pixels>>>)>,
}

pub(super) fn spawn(world: &mut World, parent: Entity, source: &str) {
    let owner = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let label = crate::edit_mode::label(world, owner, "Rendering diagram…", 14.0);
    world.entity_mut(owner).insert(Diagram {
        source: source.into(),
        label,
    });
}

pub(super) fn rasterize(source: &str) -> Pixels {
    if source.len() > 16_384 || source.lines().count() > 256 {
        return Err("Diagram is too large to render (256 lines / 16 KB).".into());
    }
    let mut rendering = mermaid_rs_renderer::RenderOptions::default();
    rendering.theme.font_family = "Lato".into();
    let svg = mermaid_rs_renderer::render_with_options(source, rendering)
        .map_err(|error| error.to_string())?;
    let mut options = resvg::usvg::Options {
        font_family: "Lato".into(),
        ..default()
    };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    options
        .fontdb_mut()
        .load_font_data(include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec());
    options.fontdb_mut().set_sans_serif_family("Lato");
    options.fontdb_mut().set_serif_family("Lato");
    options.fontdb_mut().set_monospace_family("Lato");
    let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|error| error.to_string())?;
    let size = tree.size();
    let scale = (1600.0 / size.width().max(size.height())).min(1.5);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("Diagram size is invalid")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    for rgba in pixels.chunks_exact_mut(4) {
        if rgba[3] > 0 {
            for index in 0..3 {
                rgba[index] = ((rgba[index] as u32 * 255) / rgba[3] as u32).min(255) as u8;
            }
        }
    }
    Ok((width, height, pixels))
}

pub(super) fn update(world: &mut World) {
    world.init_resource::<Worker>();
    let completed = world
        .resource::<Worker>()
        .active
        .as_ref()
        .and_then(|(source, receiver)| {
            let result = receiver.lock().ok()?.try_recv();
            match result {
                Ok(result) => Some((source.clone(), result)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some((source.clone(), Err("Diagram renderer stopped.".into())))
                }
                Err(_) => None,
            }
        });
    if let Some((source, result)) = completed {
        world.resource_mut::<Worker>().active = None;
        let waiting: Vec<_> = world
            .query::<(Entity, &Diagram)>()
            .iter(world)
            .filter(|(_, diagram)| diagram.source == source)
            .map(|(entity, diagram)| (entity, diagram.label))
            .collect();
        let image = result.as_ref().ok().map(|(width, height, pixels)| {
            world.init_resource::<Assets<Image>>();
            world.resource_mut::<Assets<Image>>().add(Image::new(
                bevy::render::render_resource::Extent3d {
                    width: *width,
                    height: *height,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                pixels.clone(),
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::RENDER_WORLD,
            ))
        });
        for (owner, label) in waiting {
            world.entity_mut(owner).remove::<Diagram>();
            if let Some(image) = image.clone() {
                world.despawn(label);
                world.spawn((
                    ImageNode::new(image),
                    Node {
                        width: percent(100),
                        height: Val::Auto,
                        ..default()
                    },
                    ChildOf(owner),
                ));
            } else if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = format!(
                    "Diagram could not be rendered: {}\n\n{source}",
                    result.as_ref().unwrap_err()
                );
            }
        }
    }
    if world.resource::<Worker>().active.is_some() {
        return;
    }
    let source = world
        .query::<&Diagram>()
        .iter(world)
        .next()
        .map(|diagram| diagram.source.clone());
    if let Some(source) = source {
        let (sender, receiver) = mpsc::channel();
        world.resource_mut::<Worker>().active =
            Some((source.clone(), Arc::new(Mutex::new(receiver))));
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| rasterize(&source))
                .unwrap_or_else(|_| Err("Diagram syntax could not be rendered.".into()));
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
}
