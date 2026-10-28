use super::shortcuts::{click, named_button, settle};
use super::*;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::render::{
    sync_world::RenderEntity,
    view::{ViewDepthTexture, ViewTarget},
};
use bevy::{asset::AssetId, camera::visibility::RenderLayers, math::DVec2};
use lince_interface::host::{OutputId, OutputViewport, Rectangle, WorkspaceLayout};
use native::{
    canvas::{CanvasItem, CanvasView},
    fiote::character::{Appearance, Avatar, Preferences},
    layout::LayoutBox,
    sand_store::{SandKind, spawn_sand},
    tokens::{Token, TokenOverrides, TokenValue},
    topology::assets,
    workspace::Workspaces,
};
use nucleus::shader_effect::{
    Effect,
    spatial::{MeshSettings, Mode},
};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn playback(
    canvas: &Presentation,
    cameras: &[(Entity, AssetId<Image>)],
) -> BTreeMap<Entity, Vec<(usize, f32, f32)>> {
    let world = canvas.app.world();
    let mut result = BTreeMap::new();
    for (camera, _) in cameras {
        let mut pending = vec![world.get::<ChildOf>(*camera).unwrap().parent()];
        while let Some(entity) = pending.pop() {
            if let Some(player) = world.get::<AnimationPlayer>(entity) {
                let mut animations: Vec<_> = player
                    .playing_animations()
                    .map(|(index, animation)| {
                        (index.index(), animation.seek_time(), animation.weight())
                    })
                    .collect();
                animations.sort_by_key(|animation| animation.0);
                result.insert(entity, animations);
            }
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
        }
    }
    result
}

fn released(canvas: &Presentation, cameras: &[(Entity, AssetId<Image>)]) -> bool {
    let main = canvas.app.world();
    let render = canvas.app.sub_app(RenderApp).world();
    let assets = render.resource::<RenderAssets<GpuImage>>();
    cameras.iter().all(|(camera, image)| {
        let entity = main.get::<RenderEntity>(*camera).unwrap().id();
        !main.get::<Camera>(*camera).unwrap().is_active
            && assets
                .get(*image)
                .is_some_and(|image| image.texture.width() == 1 && image.texture.height() == 1)
            && render.get::<ViewTarget>(entity).is_none()
            && render.get::<ViewDepthTexture>(entity).is_none()
            && render.get::<ViewPrepassTextures>(entity).is_none()
    })
}

fn output(id: u64) -> Output {
    let bounds = Rectangle::new([800.0 * (id - 1) as f64, 0.0], [800.0, 600.0]).unwrap();
    Output {
        name: format!("Layer isolation {id}"),
        viewport: OutputViewport { rotation: Default::default(),
            id: OutputId(id),
            bounds,
            usable: Some(bounds),
            physical_size: [800, 600],
            raster: lince_interface::host::raster::RasterMapping {
                origin: bounds.origin(),
                scale: [1.0; 2],
            },
            workspace: id,
        },
    }
}

fn preview_cameras(canvas: &mut Presentation) -> Vec<(Entity, AssetId<Image>)> {
    let reserved = RenderLayers::from_layers(&(48..64).collect::<Vec<_>>());
    canvas
        .world_mut()
        .query_filtered::<(Entity, &RenderLayers, &RenderTarget), With<Camera3d>>()
        .iter(canvas.app.world())
        .filter_map(|(entity, layers, target)| {
            if !layers.intersects(&reserved) {
                return None;
            }
            let RenderTarget::Image(image) = target else {
                panic!("A character preview needs an image target")
            };
            Some((entity, image.handle.id()))
        })
        .collect()
}

pub(super) fn image_pixels(canvas: &Presentation, id: AssetId<Image>) -> ([u32; 2], Vec<u8>) {
    let image = canvas
        .app
        .sub_app(RenderApp)
        .world()
        .resource::<RenderAssets<GpuImage>>()
        .get(id)
        .unwrap();
    let size = [image.texture.width(), image.texture.height()];
    let format = image.texture.format();
    assert!(matches!(format, wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb));
    let row = (size[0] * 4).div_ceil(256) * 256;
    let buffer = canvas
        .renderer
        .device
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("character-preview-readback"),
            size: u64::from(row) * u64::from(size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = canvas.renderer.device.create_command_encoder(&default());
    encoder.copy_texture_to_buffer(
        image.texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(size[1]),
            },
        },
        image.texture.size(),
    );
    let submission = canvas.renderer.queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    canvas
        .renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let mut pixels = Vec::with_capacity((size[0] * size[1] * 4) as usize);
    for bytes in mapped.chunks_exact(row as usize).take(size[1] as usize) {
        pixels.extend_from_slice(&bytes[..size[0] as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    if format == wgpu::TextureFormat::Bgra8UnormSrgb {
        for pixel in pixels.chunks_exact_mut(4) { pixel.swap(0, 2); }
    }
    (size, pixels)
}

pub(super) fn capture(name: &str, size: [u32; 2], pixels: &[u8]) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    std::fs::create_dir_all(&directory).unwrap();
    let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    for pixel in pixels.chunks_exact(4) {
        ppm.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(
        std::path::Path::new(&directory).join(format!("layers-{name}.ppm")),
        ppm,
    )
    .unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn animated_character_palettes_are_bounded_and_do_not_leak_into_passive_outputs_or_shader_scenes() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new(
        [800, 600],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    canvas.outputs(vec![output(1), output(2)]).unwrap();
    canvas.focus_output(OutputId(2)).unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    let workspace = canvas.app.world().get::<Workspaces>(root).unwrap().active;
    let passive_workspace = canvas
        .app
        .world()
        .resource::<native::output_views::OutputViews>()
        .outputs
        .iter()
        .find(|output| output.id == OutputId(1))
        .unwrap()
        .workspace;
    {
        let mut spaces = canvas.world_mut().get_mut::<Workspaces>(root).unwrap();
        for space in &mut spaces.entries {
            space.desktop.set_mode(WorkspaceLayout::Free);
            if space.id == passive_workspace {
                space.center = [0.0, 1.0];
                space.zoom = 100.0;
            }
        }
    }
    native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    native::workspace_config::set_physics(canvas.world_mut(), root, passive_workspace, false);
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let source = directory.path().join("sculpture.glb");
    std::fs::write(&source, native::laboratory::showoff::MODEL).unwrap();
    let mut asset = assets::copy_import(&source, &directory.path().join("assets")).unwrap();
    asset.scale = 70.0;
    let model = assets::spawn(
        canvas.world_mut(),
        root,
        workspace,
        DVec2::new(-180.0, 80.0),
        asset,
    );
    let shader = spawn_sand(
        canvas.world_mut(),
        root,
        workspace,
        SandKind::Square,
        "WGSL layer isolation",
        DVec2::new(170.0, 90.0),
    );
    let size = Vec2::new(260.0, 220.0);
    canvas
        .world_mut()
        .get_mut::<CanvasItem>(shader)
        .unwrap()
        .size = size;
    canvas.world_mut().entity_mut(shader).insert((
        LayoutBox::new(size),
        native::shader_effects::Surface {
            effect: Effect {
                mode: Mode::Mesh(MeshSettings::default()),
                programs: [("green".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.1, 0.9, 0.2, 1.0); }".into())].into(),
                rules: Vec::new(), fallback: "green".into(), ..default()
            },
            motion: false, strength: 1.0, glow: 0.0, ..default()
        },
    ));
    let deadline = Instant::now() + Duration::from_secs(60);
    while canvas.app.world().get::<assets::Ready>(model).is_none()
        || !native::shader_effects::surface_reading(canvas.app.world(), shader)
            .is_some_and(|state| state.ready && state.image_available)
    {
        assert!(
            Instant::now() < deadline,
            "The ordinary GLB/WGSL content did not become ready"
        );
        canvas.probe_frame().unwrap();
    }
    let frame = settle(&mut canvas);
    let passive = frame
        .outputs
        .iter()
        .find(|output| output.id == OutputId(1))
        .unwrap();
    let baseline = probe_pixels(passive.dmabuf()).unwrap();
    let main = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let model_bounds = native::topology::presentation::bounds(canvas.app.world(), model).unwrap();
    let model_pixels = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(index, pixel)| {
                model_bounds.contains(Vec2::new((index % 800) as f32, (index / 800) as f32))
                    && pixel[..3]
                        .iter()
                        .max()
                        .unwrap()
                        .abs_diff(*pixel[..3].iter().min().unwrap())
                        > 60
            })
            .count()
    };
    assert!(
        model_pixels(&main) > 100,
        "The ordinary GLB contributes no colored pixels"
    );
    let green = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[1] > 150
                    && pixel[1] > pixel[0].saturating_add(70)
                    && pixel[1] > pixel[2].saturating_add(70)
            })
            .count()
    };
    assert!(
        green(&main) > 100,
        "The WGSL mesh is missing before the character load"
    );
    capture("before-character", [800, 600], &main);
    let mut owners = Vec::new();
    let mut avatars = Vec::new();
    for index in 0..17 {
        let owner = native::fiote::spawn(canvas.world_mut(), root);
        let avatar = canvas
            .app
            .world()
            .get::<Children>(owner)
            .unwrap()
            .iter()
            .find(|child| canvas.app.world().get::<Avatar>(*child).is_some())
            .unwrap();
        native::token_style::set_overrides(
            canvas.world_mut(),
            owner,
            TokenOverrides(
                [(
                    Token::SandBackground,
                    TokenValue::Color([32 + index * 8, 60, 80, 255]),
                )]
                .into(),
            ),
        );
        {
            let mut node = canvas.world_mut().get_mut::<Node>(owner).unwrap();
            node.left = px(12.0 + f32::from(index % 9) * 85.0);
            node.top = px(12.0 + f32::from(index / 9) * 90.0);
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        }
        owners.push(owner);
        avatars.push(avatar);
    }
    native::fiote::character::set_preferences(
        canvas.world_mut(),
        Preferences {
            appearance: Appearance::Animated,
            ..default()
        },
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        canvas.probe_frame().unwrap();
        let metrics = native::fiote::character::metrics(canvas.world_mut());
        if metrics.animated_scenes == 16 && metrics.motion_materials > 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The character scenes did not load: {}",
            serde_json::to_string(&metrics).unwrap()
        );
    }
    for _ in 0..12 {
        canvas.probe_frame().unwrap();
        std::thread::sleep(Duration::from_millis(16));
    }
    let metrics = native::fiote::character::metrics(canvas.world_mut());
    assert_eq!(metrics.avatars, 17);
    assert_eq!(metrics.visible_avatars, 17);
    assert_eq!(metrics.animated_scenes, 16);
    assert_eq!(metrics.limited_palettes, 1);
    let cameras = preview_cameras(&mut canvas);
    assert_eq!(cameras.len(), 16);
    let animated = canvas
        .app
        .world()
        .get::<ImageNode>(avatars[0])
        .unwrap()
        .image
        .id();
    let fallback = canvas
        .app
        .world()
        .get::<ImageNode>(avatars[16])
        .unwrap()
        .image
        .id();
    assert!(cameras.iter().any(|(_, image)| *image == animated));
    assert!(!cameras.iter().any(|(_, image)| *image == fallback));
    for (name, image) in [("animated", animated), ("static-fallback", fallback)] {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let (size, pixels) = image_pixels(&canvas, image);
            let opaque = pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 200)
                .count();
            capture(name, size, &pixels);
            if opaque > 1000 {
                break;
            }
            let nonzero = pixels
                .chunks_exact(4)
                .filter(|pixel| pixel.iter().any(|value| *value != 0))
                .count();
            assert!(
                Instant::now() < deadline,
                "The {name} portrait is blank: {size:?}, {opaque} opaque pixels, {nonzero} nonzero pixels"
            );
            canvas.probe_frame().unwrap();
            std::thread::sleep(Duration::from_millis(16));
        }
    }
    let frame = settle(&mut canvas);
    let main = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    assert!(
        green(&main) > 100,
        "Character loading hid the ordinary WGSL mesh"
    );
    assert!(
        model_pixels(&main) > 100,
        "Character loading hid the ordinary GLB"
    );
    assert!(canvas.app.world().get::<assets::Ready>(model).is_some());
    let passive = frame
        .outputs
        .iter()
        .find(|output| output.id == OutputId(1))
        .unwrap();
    let actual = probe_pixels(passive.dmabuf()).unwrap();
    assert_eq!(
        actual, baseline,
        "Character geometry or lights leaked into the passive output"
    );
    capture("bounded-palettes", [800, 600], &main);
    capture("passive-output", [800, 600], &actual);
    let settings = canvas
        .world_mut()
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(200),
                top: px(10),
                width: px(400),
                max_height: px(560),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.05, 0.05)),
            GlobalZIndex(100),
            ChildOf(root),
        ))
        .id();
    native::fiote::character::settings_button(canvas.world_mut(), settings, settings);
    settle(&mut canvas);
    let toggle = named_button(&mut canvas, "Fiote appearance");
    click(&mut canvas, toggle, [800, 600]);
    let frame = settle(&mut canvas);
    assert!(
        canvas
            .world_mut()
            .query::<&Text>()
            .iter(canvas.app.world())
            .any(|text| text.0.contains("Animation limit: 16 palettes"))
    );
    capture(
        "capacity-settings",
        [800, 600],
        &probe_pixels(frame.dmabuf().unwrap()).unwrap(),
    );
    let pose = playback(&canvas, &cameras);
    assert_eq!(pose.len(), 16);
    for owner in &owners {
        canvas.world_mut().get_mut::<Node>(*owner).unwrap().display = Display::None;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        canvas.probe_frame().unwrap();
        if released(&canvas, &cameras) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Hidden character targets or view attachments were not released"
        );
    }
    for _ in 0..12 {
        canvas.probe_frame().unwrap();
        std::thread::sleep(Duration::from_millis(16));
    }
    assert_eq!(playback(&canvas, &cameras), pose);
    let hidden = native::fiote::character::metrics(canvas.world_mut());
    assert_eq!(hidden.visible_avatars, 0);
    assert_eq!(hidden.render_target_color_bytes, 16 * 4);
    assert_eq!(hidden.limited_palettes, 0);
    let frame = settle(&mut canvas);
    let passive = frame
        .outputs
        .iter()
        .find(|output| output.id == OutputId(1))
        .unwrap();
    assert!(probe_pixels(passive.dmabuf()).unwrap() == baseline);
    for owner in &owners {
        canvas.world_mut().get_mut::<Node>(*owner).unwrap().display = Display::Flex;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        canvas.probe_frame().unwrap();
        let (size, pixels) = image_pixels(&canvas, animated);
        if size == [128, 128]
            && pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 200)
                .count()
                > 1000
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The same character image did not recover after showing"
        );
    }
    let resumed = preview_cameras(&mut canvas);
    assert_eq!(resumed.len(), cameras.len());
    assert!(resumed.iter().all(|camera| cameras.contains(camera)));
    canvas.world_mut().insert_resource(UiScale(4.0));
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert_eq!(image_pixels(&canvas, animated).0, [256, 256]);
    canvas.world_mut().insert_resource(UiScale(1.0));
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert_eq!(image_pixels(&canvas, animated).0, [128, 128]);
    for owner in owners {
        canvas.world_mut().despawn(owner);
    }
    settle(&mut canvas);
    assert_eq!(
        native::fiote::character::metrics(canvas.world_mut()).animated_scenes,
        0
    );
    assert_eq!(
        native::fiote::character::metrics(canvas.world_mut()).limited_palettes,
        0
    );
    assert!(preview_cameras(&mut canvas).is_empty());
    assert!(
        !canvas
            .world_mut()
            .query::<&Text>()
            .iter(canvas.app.world())
            .any(|text| text.0.contains("Animation limit:"))
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        canvas.probe_frame().unwrap();
        let assets = canvas
            .app
            .sub_app(RenderApp)
            .world()
            .resource::<RenderAssets<GpuImage>>();
        if cameras
            .iter()
            .all(|(_, image)| assets.get(*image).is_none())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Closed character targets remain resident"
        );
    }
    let frame = settle(&mut canvas);
    let passive = frame
        .outputs
        .iter()
        .find(|output| output.id == OutputId(1))
        .unwrap();
    assert_eq!(probe_pixels(passive.dmabuf()).unwrap(), baseline);
}
