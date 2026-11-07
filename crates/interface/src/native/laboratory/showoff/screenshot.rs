use crate::native::{self, app::OutputTarget};
use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
};
use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

const SIZE: [u32; 2] = [1920, 1080];

#[derive(Resource, Default)]
struct Captured(Option<Image>);

pub fn capture(output: &Path) -> io::Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("lince-showcase-")
        .tempdir()?;
    let plugins = DefaultPlugins
        .build()
        .disable::<bevy::log::LogPlugin>()
        .disable::<bevy::winit::WinitPlugin>()
        .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
        .set(bevy::render::RenderPlugin {
            synchronous_pipeline_compilation: true,
            ..default()
        })
        .set(WindowPlugin {
            primary_window: Some(Window {
                resolution: SIZE.into(),
                ..default()
            }),
            close_when_requested: false,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        });
    let mut app = native::app::presentation_app(directory.path().join("assets"), plugins);
    native::laboratory::isolate(app.world_mut());
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            SIZE[0],
            SIZE[1],
            bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb,
            None,
        ));
    app.insert_resource(OutputTarget(RenderTarget::Image(ImageRenderTarget {
        handle: target.clone(),
        scale_factor: 1.0,
    })))
    .insert_resource(crate::host::WakeSignal::new(|| {}))
    .init_resource::<Captured>();
    let root = app.world_mut().spawn(native::container::BoxRoot).id();
    app.finish();
    app.cleanup();
    let deadline = Instant::now() + Duration::from_secs(60);
    while app
        .world()
        .get::<ComputedNode>(root)
        .is_none_or(|node| node.size().min_element() <= 0.0)
    {
        advance(&mut app, deadline)?;
    }
    app.insert_resource(super::Launch);
    let mut ready_frames = 0;
    while ready_frames < 8 {
        advance(&mut app, deadline)?;
        if ready(app.world_mut()) {
            ready_frames += 1;
        } else {
            ready_frames = 0;
        }
    }
    frame_workspace(app.world_mut(), root)?;
    for _ in 0..8 {
        advance(&mut app, deadline)?;
    }
    app.world_mut().spawn(Screenshot::image(target)).observe(
        |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
            captured.0 = Some(event.image.clone());
        },
    );
    let image = loop {
        advance(&mut app, deadline)?;
        if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
            break image;
        }
    };
    write_png(image, output)
}

fn frame_workspace(world: &mut World, root: Entity) -> io::Result<()> {
    let active = world
        .get::<native::workspace::Workspaces>(root)
        .ok_or_else(|| io::Error::other("The temporary showcase workspace is unavailable"))?
        .active;
    let mut minimum = bevy::math::DVec2::splat(f64::INFINITY);
    let mut maximum = bevy::math::DVec2::splat(f64::NEG_INFINITY);
    for (item, member, parent) in world
        .query::<(
            &native::canvas::CanvasItem,
            &native::workspace::WorkspaceMember,
            &ChildOf,
        )>()
        .iter(world)
    {
        if member.0 == active && parent.parent() == root {
            let half = item.size.as_dvec2() * 0.5;
            minimum = minimum.min(item.position - half);
            maximum = maximum.max(item.position + half);
        }
    }
    let size = maximum - minimum;
    if !size.is_finite() || size.min_element() <= 0.0 {
        return Err(io::Error::other(
            "The temporary showcase has no frameable content",
        ));
    }
    let viewport = bevy::math::DVec2::new(f64::from(SIZE[0]), f64::from(SIZE[1]));
    *world.get_mut::<native::canvas::CanvasView>(root).unwrap() = native::canvas::CanvasView {
        center: (minimum + maximum) * 0.5,
        zoom: ((viewport - bevy::math::DVec2::splat(160.0)) / size)
            .min_element()
            .min(0.65),
    };
    Ok(())
}

fn advance(app: &mut App, deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        return Err(io::Error::other(
            "Showcase rendering timed out; the previous screenshot was preserved",
        ));
    }
    app.update();
    std::thread::sleep(Duration::from_millis(5));
    Ok(())
}

fn ready(world: &mut World) -> bool {
    let workspace_ready = world
        .query::<&native::workspace::Workspaces>()
        .iter(world)
        .any(|spaces| {
            spaces
                .entries
                .iter()
                .any(|entry| entry.id == spaces.active && entry.name == super::NAME)
        });
    let model_ready = world
        .query_filtered::<Entity, (
            With<native::topology::assets::ImportedAsset>,
            With<native::topology::assets::Ready>,
        )>()
        .iter(world)
        .next()
        .is_some();
    let surfaces: Vec<_> = world
        .query_filtered::<Entity, With<native::shader_effects::Surface>>()
        .iter(world)
        .collect();
    workspace_ready
        && model_ready
        && !surfaces.is_empty()
        && surfaces.into_iter().all(|entity| {
            native::shader_effects::surface_reading(world, entity).is_some_and(|state| {
                state.ready && state.image_available && state.enabled && state.error.is_empty()
            })
        })
}

fn write_png(image: Image, output: &Path) -> io::Result<()> {
    let image = image.try_into_dynamic().map_err(io::Error::other)?;
    if [image.width(), image.height()] != SIZE {
        return Err(io::Error::other(
            "The showcase capture has unexpected dimensions",
        ));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    image
        .write_to(&mut pending, image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    pending.as_file().sync_all()?;
    pending.persist(output).map_err(io::Error::other)?;
    Ok(())
}

#[cfg_attr(test, test)]
fn capture_replaces_only_the_png_and_preserves_it_on_failure() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("showcase.png");
    let unrelated = directory.path().join("interface.json");
    std::fs::write(&unrelated, b"user workspace").unwrap();
    std::fs::write(&output, b"old screenshot").unwrap();
    assert!(write_png(Image::default(), &output).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"old screenshot");
    let mut image = Image::new_target_texture(
        SIZE[0],
        SIZE[1],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        None,
    );
    let data = image.data.as_mut().unwrap();
    for pixel in data.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[25, 80, 160, 255]);
    }
    write_png(image, &output).unwrap();
    let decoded = image::open(&output).unwrap().into_rgba8();
    assert_eq!(decoded.dimensions(), (SIZE[0], SIZE[1]));
    assert_eq!(decoded.get_pixel(0, 0).0, [25, 80, 160, 255]);
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"user workspace");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

crate::native::laboratory_cases! {
    capture_replaces_only_the_png_and_preserves_it_on_failure,
}
