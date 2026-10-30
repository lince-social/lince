use super::layers::capture;
use super::shortcuts::settle;
use super::*;
use lince_interface::{
    host::{DesktopAction, ItemId, OutputId, OutputViewport, Rectangle},
    native::{
        actions::Action,
        canvas::{CanvasItem, CanvasView},
        layout::LayoutBox,
        sand_placement::PlacementAction,
        workspace::{WorkspaceMember, Workspaces},
    },
};

fn check(canvas: &mut Presentation, owner: Entity, color: [u8; 3], label: &str, passive: bool) {
    if !passive {
        canvas.pointer([400.0, 300.0]).unwrap();
    }
    let frame = settle(canvas);
    let buffer = if passive {
        frame
            .outputs
            .iter()
            .find(|output| output.id == OutputId(1))
            .unwrap()
            .dmabuf()
    } else {
        frame.dmabuf().unwrap()
    };
    let pixels = probe_pixels(buffer).unwrap();
    capture(label, [800, 600], &pixels);
    let index = (300 * 800 + 400) * 4;
    assert!(
        (0..3).all(|channel| pixels[index + channel].abs_diff(color[channel]) < 8),
        "{label}: expected {color:?}, got {:?}",
        &pixels[index..index + 4]
    );
    let id = canvas.app.world().get::<LayoutBox>(owner).unwrap().id;
    let output = if passive {
        Some(OutputId(1))
    } else {
        frame.output
    };
    assert_eq!(
        frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .rev()
            .find(|item| output.is_none_or(|output| item.output == output)
                && item.surface_point([400.0, 300.0]).is_some())
            .unwrap()
            .id,
        ItemId::Native(id),
        "{label}: published input order differs from the visible item"
    );
    if !passive {
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<native::topology::input::PointerState>()
                .hit
                .map(|hit| hit.0),
            Some(owner),
            "{label}: native hit testing differs from the visible item"
        );
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn overlapping_items_share_visual_native_pointer_and_published_input_order() {
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
    let root = canvas.canvas_root().unwrap();
    let workspace = canvas.app.world().get::<Workspaces>(root).unwrap().active;
    native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let items: Vec<_> = [Color::srgb(1.0, 0.0, 0.0), Color::srgb(0.0, 0.0, 1.0)]
        .into_iter()
        .enumerate()
        .map(|(index, color)| {
            let dimensions = if index == 0 { Vec2::new(1000.0, 700.0) } else { Vec2::new(520.0, 1000.0) };
            canvas
                .world_mut()
                .spawn((
                    CanvasItem {
                        position: if index == 0 {
                            bevy::math::DVec2::ZERO
                        } else {
                            bevy::math::DVec2::new(144.0, 420.0)
                        },
                        size: dimensions,
                    },
                    LayoutBox::new(dimensions),
                    Node::default(),
                    BackgroundColor(color),
                    WorkspaceMember(workspace),
                    ZIndex(index as i32),
                    ChildOf(root),
                ))
                .id()
        })
        .collect();
    settle(&mut canvas);
    check(
        &mut canvas,
        items[1],
        [0, 0, 255],
        "stacking-blue-front",
        false,
    );
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Front.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    check(
        &mut canvas,
        items[0],
        [255, 0, 0],
        "stacking-red-front",
        false,
    );
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Back.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    check(
        &mut canvas,
        items[1],
        [0, 0, 255],
        "stacking-blue-restored",
        false,
    );
    let outputs = (1..=2)
        .map(|id| {
            let bounds = Rectangle::new([800.0 * (id - 1) as f64, 0.0], [800.0, 600.0]).unwrap();
            Output {
                name: format!("Stacking output {id}"),
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
        })
        .collect();
    canvas.outputs(outputs).unwrap();
    canvas.focus_output(OutputId(2)).unwrap();
    check(
        &mut canvas,
        items[1],
        [0, 0, 255],
        "stacking-passive-blue",
        true,
    );
    canvas.focus_output(OutputId(1)).unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Front.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    canvas.focus_output(OutputId(2)).unwrap();
    check(
        &mut canvas,
        items[0],
        [255, 0, 0],
        "stacking-passive-red",
        true,
    );
    canvas.focus_output(OutputId(1)).unwrap();
    check(
        &mut canvas,
        items[0],
        [255, 0, 0],
        "stacking-primary-red",
        false,
    );
    canvas
        .world_mut()
        .entity_mut(items[0])
        .insert(native::topology::Spatial {
            elevation: 64.0,
            ..default()
        });
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Front.apply(canvas.world_mut(), items[1]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    *canvas
        .world_mut()
        .get_mut::<native::topology::view::View>(root)
        .unwrap() = native::topology::view::View {
        spatial: true,
        position: [0.0, 800.0, 56.0],
        pitch: -1.5,
        ..default()
    };
    check(
        &mut canvas,
        items[0],
        [255, 0, 0],
        "stacking-spatial-elevation",
        false,
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::topology::Spatial>(items[0])
            .unwrap()
            .elevation,
        64.0
    );
    canvas
        .world_mut()
        .entity_mut(items[0])
        .insert(native::topology::Spatial::default());
    for item in &items {
        canvas
            .world_mut()
            .get_mut::<CanvasItem>(*item)
            .unwrap()
            .position = bevy::math::DVec2::ZERO;
    }
    check(
        &mut canvas,
        items[1],
        [0, 0, 255],
        "stacking-spatial-coincident-blue",
        false,
    );
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Front.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    check(
        &mut canvas,
        items[0],
        [255, 0, 0],
        "stacking-spatial-coincident-red",
        false,
    );
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Back.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    check(
        &mut canvas,
        items[1],
        [0, 0, 255],
        "stacking-spatial-coincident-restored",
        false,
    );
    canvas
        .world_mut()
        .get_mut::<CanvasItem>(items[0])
        .unwrap()
        .position = bevy::math::DVec2::new(-32.0, -48.0);
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Front.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    check(
        &mut canvas,
        items[0],
        [255, 0, 0],
        "stacking-spatial-offset-red",
        false,
    );
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    PlacementAction::Back.apply(canvas.world_mut(), items[0]);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    check(
        &mut canvas,
        items[1],
        [0, 0, 255],
        "stacking-spatial-offset-restored",
        false,
    );
}
