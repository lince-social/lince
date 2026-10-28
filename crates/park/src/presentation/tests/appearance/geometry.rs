use super::*;

fn marker(pixels: &[u8], size: [u32; 2]) -> ([f64; 2], usize) {
    let mut sum = [0.0; 2];
    let mut count = 0;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[0] < 35 && pixel[1] > 110 && pixel[2] < 35 {
            sum[0] += (index % size[0] as usize) as f64 + 0.5;
            sum[1] += (index / size[0] as usize) as f64 + 0.5;
            count += 1;
        }
    }
    assert!(count > 200, "The actual moving foreground marker was not visible");
    (sum.map(|value| value / count as f64), count)
}

fn center(geometry: &lince_interface::host::ItemGeometry) -> [f64; 2] {
    let origin = geometry.mapping.surface.origin();
    let size = geometry.mapping.surface.size();
    geometry.mapping.canvas_point(std::array::from_fn(|axis| origin[axis] + size[axis] * 0.5)).unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn moving_follow_uses_the_current_rendered_target_for_120_single_frames_with_appearance_enabled() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    for spatial in [false, true] {
        let mut canvas = launch(&directory.path().join(format!("follow-{spatial}.json")), size);
        scale(&mut canvas, size, 1.25);
        let root = canvas.canvas_root().unwrap();
        native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
        canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
        canvas.world_mut().get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
        open(&mut canvas);
        for (caption, value) in [(SHADOW, "12"), (BLUR, "8"), (FADE, "120")] { apply(&mut canvas, caption, value, size); }
        close(&mut canvas, size);
        let target = item(&mut canvas, root, Vec2::new(240.0, 160.0), [0.0; 2], Color::srgba(0.0, 1.0, 0.0, 0.75), 2, false);
        let id = canvas.app.world().get::<LayoutBox>(target).unwrap().id;
        drop(settle(&mut canvas));
        canvas.desktop_action(DesktopAction::SelectPlacement { workspace: 1, item: Some(id) }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let reference = loop {
            let frame = canvas.probe_frame().unwrap();
            if decorations(&mut canvas, target) == 2 && native::layout::desktop::focus::following(canvas.app.world(), root) {
                let geometry = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Native(id)).unwrap();
                let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
                break (center(geometry), marker(&pixels, size));
            }
            assert!(Instant::now() < deadline, "The real appearance and follow paths did not become ready");
        };
        let mut samples = Vec::new();
        for index in 0..120 {
            let angle = index as f64 * 0.19;
            canvas.world_mut().get_mut::<CanvasItem>(target).unwrap().position = bevy::math::DVec2::new(
                600.0 + index as f64 * 17.25 + angle.sin() * 35.0,
                -300.0 + index as f64 * 8.125 + angle.cos() * 25.0,
            );
            let frame = canvas.probe_frame().unwrap();
            assert!(native::layout::desktop::focus::following(canvas.app.world(), root));
            let geometry = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Native(id)).unwrap();
            let mapped = center(geometry);
            assert!((0..2).all(|axis| (mapped[axis] - reference.0[axis]).abs() <= 0.01), "The published camera lagged behind target frame {index}: {mapped:?} versus {:?}", reference.0);
            let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
            let (visible, count) = marker(&pixels, size);
            assert!((0..2).all(|axis| (visible[axis] - reference.1.0[axis]).abs() <= 1.0), "The rendered camera lagged behind target frame {index}: {visible:?} versus {:?}", reference.1.0);
            assert!(count.abs_diff(reference.1.1) <= reference.1.1 / 50 + 4, "The moving target changed its visible footprint at frame {index}");
            assert_eq!(canvas.app.world().get::<CanvasItem>(target).unwrap().size, Vec2::new(240.0, 160.0));
            samples.push(serde_json::json!({ "frame": index, "mapped_center": mapped, "pixel_center": visible, "marker_pixels": count, "opacity": alpha(&canvas, target) }));
            if index == 119 { super::super::layers::capture(&format!("appearance-follow-{spatial}"), size, &pixels); }
        }
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        drop(canvas.probe_frame().unwrap());
        assert!(!native::layout::desktop::focus::following(canvas.app.world(), root));
        std::fs::write(directory.path().join(format!("follow-{spatial}-samples.json")), serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
        if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
            let path = std::path::PathBuf::from(path);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join(format!("appearance-follow-{spatial}.json")), serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
        }
        canvas.finish().unwrap();
    }
}

#[derive(Resource, Default)]
struct Clicks([usize; 2]);

fn press(canvas: &mut Presentation, point: [f64; 2]) {
    canvas.pointer(point.map(|value| value as f32)).unwrap();
    drop(settle(canvas));
    canvas.button(bevy::input::mouse::MouseButton::Left, true).unwrap();
    drop(settle(canvas));
    canvas.button(bevy::input::mouse::MouseButton::Left, false).unwrap();
    drop(settle(canvas));
}

fn pixel(pixels: &[u8], size: [u32; 2], point: [u32; 2]) -> [u8; 4] {
    let index = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
    pixels[index..index + 4].try_into().unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn workspace_hole_effects_keep_actual_clipped_pixels_and_clicks_at_fractional_scales() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    for output_scale in [1.25, 1.5] {
        let mut canvas = launch(&directory.path().join(format!("holes-{output_scale}.json")), size);
        scale(&mut canvas, size, output_scale);
        let root = canvas.canvas_root().unwrap();
        native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
        canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
        let lower = item(&mut canvas, root, Vec2::splat(400.0), [0.0; 2], Color::srgb(0.02, 0.9, 0.05), 1, false);
        canvas.world_mut().insert_resource(Clicks::default());
        canvas.world_mut().entity_mut(lower).insert(Button).observe(|_: On<Pointer<Click>>, mut clicks: ResMut<Clicks>| { clicks.0[0] += 1; });
        native::workspace::create(canvas.world_mut(), root);
        canvas.desktop_action(DesktopAction::Workspace(2)).unwrap();
        native::workspace_config::set_physics(canvas.world_mut(), root, 2, false);
        canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
        *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
        let upper = item(&mut canvas, root, Vec2::new(600.0, 400.0), [0.0; 2], Color::srgba(0.02, 0.04, 0.95, 0.6), 2, false);
        canvas.world_mut().entity_mut(upper).insert((WorkspaceMember(2), Button)).observe(|_: On<Pointer<Click>>, mut clicks: ResMut<Clicks>| { clicks.0[1] += 1; });
        let mut opening = native::area::InfluenceArea::new(native::area::AreaShape::Square, bevy::math::DVec2::ZERO, bevy::math::DVec2::splat(200.0));
        opening.hole = true;
        let opening = native::area::spawn_area(canvas.world_mut(), root, 2, opening).unwrap();
        let ids = [lower, upper].map(|entity| ItemId::Native(canvas.app.world().get::<LayoutBox>(entity).unwrap().id));
        let points = [[size[0] / 2, size[1] / 2], [size[0] / 2 + (160.0 * output_scale) as u32, size[1] / 2]];
        let deadline = Instant::now() + Duration::from_secs(60);
        let (off, geometry) = loop {
            let frame = settle(&mut canvas);
            let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
            let colors = points.map(|point| pixel(&pixels, size, point));
            if colors[0][1] > 180 && colors[0][2] < 40 && u16::from(colors[1][2]) > u16::from(colors[1][1]) * 2 {
                let geometry = ids.map(|id| frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id).unwrap().clone());
                break (pixels, geometry);
            }
            assert!(Instant::now() < deadline, "The actual two-workspace opening did not render its distinct reference colors: {colors:?}");
        };
        for (index, point) in points.iter().enumerate() {
            let point = point.map(|value| f64::from(value) + 0.5);
            assert_eq!(geometry[0].surface_point(point).is_some(), index == 0);
            assert_eq!(geometry[1].surface_point(point).is_some(), index == 1);
        }
        open(&mut canvas);
        for (caption, value) in [(SHADOW, "12"), (BLUR, "8")] { apply(&mut canvas, caption, value, size); }
        close(&mut canvas, size);
        let deadline = Instant::now() + Duration::from_secs(15);
        let on = loop {
            let frame = canvas.probe_frame().unwrap();
            let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
            let changed = off.chunks_exact(4).zip(pixels.chunks_exact(4)).enumerate().filter(|(index, (before, after))| {
                let x = ((index % size[0] as usize) as f64 + 0.5 - f64::from(size[0]) * 0.5).abs() / output_scale;
                let y = ((index / size[0] as usize) as f64 + 0.5 - f64::from(size[1]) * 0.5).abs() / output_scale;
                (x.max(y) - 100.0).abs() < 24.0 && (0..3).any(|channel| before[channel].abs_diff(after[channel]) > 4)
            }).count();
            if [lower, upper].iter().all(|owner| decorations(&mut canvas, *owner) == 2) && changed > 30 {
                for (id, expected) in ids.iter().zip(&geometry) {
                    assert_eq!(frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == *id), Some(expected));
                }
                break pixels;
            }
            assert!(Instant::now() < deadline, "Appearance never changed actual clipped boundary pixels: {changed} changed pixels");
        };
        for point in points {
            assert!(pixel(&off, size, point).iter().zip(pixel(&on, size, point)).all(|(before, after)| before.abs_diff(after) <= 3), "Appearance filled a hole or changed an unrelated uniform interior at {point:?}");
            press(&mut canvas, point.map(|value| f64::from(value) / output_scale));
        }
        assert_eq!(canvas.app.world().resource::<Clicks>().0, [1, 1]);
        super::super::layers::capture(&format!("appearance-hole-{output_scale}"), size, &on);
        canvas.world_mut().get_mut::<native::area::InfluenceArea>(opening).unwrap().enabled = false;
        let frame = settle(&mut canvas);
        let closed = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let center = pixel(&closed, size, points[0]);
        assert!(u16::from(center[2]) > u16::from(center[1]) * 2, "Closing the opening left stale effect pixels: {center:?}");
        assert_eq!(decorations(&mut canvas, lower), 0);
        press(&mut canvas, points[0].map(|value| f64::from(value) / output_scale));
        assert_eq!(canvas.app.world().resource::<Clicks>().0, [1, 2]);
        canvas.finish().unwrap();
    }
}
