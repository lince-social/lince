use super::*;

fn transparent_backdrop(world: &mut World) {
    let Some(camera) = world.get_resource::<native::topology::presentation::BackgroundCamera>().map(|camera| camera.0) else { return; };
    world.get_mut::<Camera>(camera).unwrap().clear_color = ClearColorConfig::Custom(Color::NONE);
    let roots = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().iter(world).collect::<Vec<_>>();
    for root in roots {
        world.entity_mut(root).remove::<BackgroundColor>();
        let grids = world.query::<(Entity, &ZIndex, &ChildOf)>().iter(world)
            .filter(|(_, index, parent)| index.0 == -1 && parent.parent() == root)
            .map(|(entity, _, _)| entity).collect::<Vec<_>>();
        for entity in grids {
            if let Some(mut node) = world.get_mut::<Node>(entity) { node.display = Display::None; }
        }
    }
}

fn encode(value: f64) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.0031308 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
    (encoded * 255.0).round() as u8
}

fn premultiplied(color: [f64; 4]) -> [f64; 4] {
    [color[0] * color[3], color[1] * color[3], color[2] * color[3], color[3]]
}

fn expected(offset: f64, radius: f64) -> [u8; 4] {
    let left = premultiplied([0.65, 0.12, 0.04, 0.4]);
    let right = premultiplied([0.03, 0.18, 0.75, 0.6]);
    let front = premultiplied([0.12, 0.55, 0.08, 0.25]);
    let mut backdrop = [0.0; 4];
    for (distance, weight) in [(-radius, 0.25), (0.0, 0.5), (radius, 0.25)] {
        let source = if offset + distance < 0.0 { left } else { right };
        for channel in 0..4 { backdrop[channel] += source[channel] * weight; }
    }
    let composed: [f64; 4] = std::array::from_fn(|channel| front[channel] + backdrop[channel] * (1.0 - front[3]));
    [encode(composed[0]), encode(composed[1]), encode(composed[2]), (composed[3] * 255.0).round() as u8]
}

fn samples(size: [u32; 2], scale: f64, blur: bool) -> Vec<([u32; 2], [u8; 4])> {
    [-28, -7, -2, 2, 7, 28].into_iter().map(|offset| {
        let x = (i64::from(size[0] / 2) + offset) as u32;
        let radius = if blur { 8.0 * scale } else { 0.0 };
        ([x, size[1] / 2], expected(f64::from(x) + 0.5 - f64::from(size[0]) * 0.5, radius))
    }).collect()
}

fn differences(pixels: &[u8], size: [u32; 2], references: &[([u32; 2], [u8; 4])]) -> Vec<serde_json::Value> {
    references.iter().filter_map(|(point, expected)| {
        let index = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
        let actual = &pixels[index..index + 4];
        (0..4).any(|channel| actual[channel].abs_diff(expected[channel]) > 4)
            .then(|| serde_json::json!({ "point": point, "expected": expected, "actual": actual }))
    }).collect()
}

fn reference_frame(canvas: &mut Presentation, size: [u32; 2], references: &[([u32; 2], [u8; 4])]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        drop(canvas.probe_frame().unwrap());
        let (actual_size, pixels) = super::super::layers::image_pixels(canvas, canvas.target.id());
        assert_eq!(actual_size, size);
        let failures = differences(&pixels, size, references);
        if failures.is_empty() { return pixels; }
        assert!(Instant::now() < deadline, "Actual premultiplied GPU output disagrees with independent linear-light source-over references: {failures:?}");
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn backdrop_filter_matches_independent_linear_premultiplied_rgba_at_multiple_output_scales() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    for output_scale in [1.0, 1.25, 1.5] {
        let mut canvas = launch(&directory.path().join(format!("reference-{output_scale}.json")), size);
        scale(&mut canvas, size, output_scale);
        let root = canvas.canvas_root().unwrap();
        native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
        canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
        *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
        canvas.app.add_systems(Last, transparent_backdrop);
        let backdrop = item(&mut canvas, root, Vec2::new(560.0, 360.0), [0.0; 2], Color::NONE, 0, false);
        for (left, color) in [(0.0, Color::linear_rgba(0.65, 0.12, 0.04, 0.4)), (280.0, Color::linear_rgba(0.03, 0.18, 0.75, 0.6))] {
            canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(left), top: px(0), width: px(280), height: percent(100), ..default() }, BackgroundColor(color), ChildOf(backdrop)));
        }
        let front = item(&mut canvas, root, Vec2::new(360.0, 240.0), [0.0; 2], Color::linear_rgba(0.12, 0.55, 0.08, 0.25), 2, false);
        let id = ItemId::Native(canvas.app.world().get::<LayoutBox>(front).unwrap().id);
        let off = reference_frame(&mut canvas, size, &samples(size, output_scale, false));
        let geometry = canvas.probe_frame().unwrap().geometry.as_ref().unwrap().items.iter().find(|item| item.id == id).unwrap().clone();
        open(&mut canvas);
        apply(&mut canvas, BLUR, "8", size);
        close(&mut canvas, size);
        let on = reference_frame(&mut canvas, size, &samples(size, output_scale, true));
        assert!(samples(size, output_scale, true).iter().any(|(point, _)| {
            let index = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
            (0..4).any(|channel| off[index + channel].abs_diff(on[index + channel]) > 8)
        }), "The reference accepted an unchanged blur-off frame");
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id), Some(&geometry));
        super::super::layers::capture(&format!("appearance-premultiplied-off-{output_scale}"), size, &off);
        super::super::layers::capture(&format!("appearance-premultiplied-on-{output_scale}"), size, &on);
        canvas.finish().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn shifted_render_viewport_clamps_backdrop_taps_and_replaces_its_previous_viewport_request() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = launch(&directory.path().join("viewport.json"), size);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    canvas.app.add_systems(Last, transparent_backdrop);
    let backdrop = item(&mut canvas, root, Vec2::new(2000.0, 1500.0), [0.0; 2], Color::NONE, 0, false);
    for (left, color) in [(0.0, Color::linear_rgba(0.65, 0.12, 0.04, 0.4)), (1000.0, Color::linear_rgba(0.03, 0.18, 0.75, 0.6))] {
        canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(left), top: px(0), width: px(1000), height: percent(100), ..default() }, BackgroundColor(color), ChildOf(backdrop)));
    }
    item(&mut canvas, root, Vec2::new(2000.0, 1500.0), [0.0; 2], Color::linear_rgba(0.12, 0.55, 0.08, 0.25), 2, false);
    let off = reference_frame(&mut canvas, size, &samples(size, 1.0, false));
    open(&mut canvas);
    apply(&mut canvas, BLUR, "8", size);
    close(&mut canvas, size);
    let camera = canvas.app.world().resource::<native::topology::presentation::SceneCamera>().0;
    for (index, (position, extent)) in [([120, 90], [760, 540]), ([140, 110], [720, 500])].into_iter().enumerate() {
        canvas.world_mut().get_mut::<Camera>(camera).unwrap().viewport = Some(bevy::camera::Viewport {
            physical_position: UVec2::from_array(position), physical_size: UVec2::from_array(extent), ..default()
        });
        let mut references = samples(size, 1.0, true);
        for point in [
            [position[0] + 1, size[1] / 2],
            [position[0] + extent[0] - 2, size[1] / 2],
            [size[0] / 2 - 28, position[1] + 1],
            [size[0] / 2 + 28, position[1] + extent[1] - 2],
        ] {
            references.push((point, expected(f64::from(point[0]) + 0.5 - f64::from(size[0]) * 0.5, 8.0)));
        }
        let pixels = reference_frame(&mut canvas, size, &references);
        let seam = ((size[1] / 2) as usize * size[0] as usize + (size[0] / 2 - 2) as usize) * 4;
        assert!((0..4).any(|channel| pixels[seam + channel].abs_diff(off[seam + channel]) > 8), "The viewport reference accepted a blur-off frame");
        for point in [[position[0] - 1, size[1] / 2], [position[0] + extent[0], size[1] / 2]] {
            let pixel = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
            assert_eq!(&pixels[pixel..pixel + 4], &[0, 0, 0, 0], "The previous full/shifted viewport left pixels in a transparent gutter at {point:?}");
        }
        super::super::layers::capture(&format!("appearance-viewport-{index}"), size, &pixels);
    }
    canvas.world_mut().get_mut::<Camera>(camera).unwrap().viewport = None;
    drop(canvas.probe_frame().unwrap());
    canvas.finish().unwrap();
}
