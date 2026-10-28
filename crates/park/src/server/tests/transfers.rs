use super::Window as ClientWindow;
use super::*;
use bevy::prelude::*;
use smithay::reexports::wayland_server::Resource;
use lince_interface::{
    host::{DesktopAction, Direction, ItemId, WorkspaceLayout},
    layout::desktop::{PlacementId, Width},
    native,
};
use std::io::Write;

#[cfg(feature = "nested")]
#[path = "transfers/overview.rs"]
mod overview;
#[cfg(feature = "nested")]
#[path = "transfers/hover.rs"]
mod hover;
#[cfg(feature = "nested")]
#[path = "transfers/dispatch/mod.rs"]
mod dispatch;
#[path = "transfers/windowed_fullscreen.rs"]
mod windowed_fullscreen;

fn paint(
    harness: &mut Harness,
    window: &ClientWindow,
    size: [u32; 2],
    color: [u8; 4],
) -> wl_buffer::WlBuffer {
    let bytes = color.repeat((size[0] * size[1]) as usize);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let qh = harness.queue.handle();
    let pool =
        harness
            .client
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), bytes.len() as i32, &qh, ());
    let buffer = pool.create_buffer(
        0,
        size[0] as i32,
        size[1] as i32,
        size[0] as i32 * 4,
        wl_shm::Format::Argb8888,
        &qh,
        (),
    );
    pool.destroy();
    window.surface.attach(Some(&buffer), 0, 0);
    window
        .surface
        .damage_buffer(0, 0, size[0] as i32, size[1] as i32);
    window.surface.commit();
    harness.sync();
    buffer
}

fn settle(canvas: &mut crate::presentation::Presentation) -> crate::presentation::Frame {
    for _ in 0..5 {
        canvas.probe_frame().unwrap();
    }
    canvas.probe_frame().unwrap()
}

fn present(
    canvas: &mut crate::presentation::Presentation,
    harness: &mut Harness,
    clients: &[(&ClientWindow, SurfaceId, [u8; 4])],
    retained: &mut Vec<wl_buffer::WlBuffer>,
) -> crate::presentation::Frame {
    canvas.sync_windows(&harness.server.windows()).unwrap();
    let frame = settle(canvas);
    for request in &frame.windows {
        harness
            .server
            .configure_state(
                request.surface,
                request.size.map(|value| value as i32),
                request.state,
            )
            .unwrap();
    }
    harness.sync();
    for (window, id, color) in clients {
        let size = frame
            .windows
            .iter()
            .find(|request| request.surface == *id)
            .unwrap()
            .size;
        retained.push(paint(harness, window, size, *color));
    }
    canvas.sync_windows(&harness.server.windows()).unwrap();
    settle(canvas)
}

fn member(
    canvas: &mut crate::presentation::Presentation,
    id: SurfaceId,
) -> (Entity, u64, PlacementId) {
    let world = canvas.world_mut();
    world
        .query::<(
            Entity,
            &native::layout::desktop::ApplicationWindow,
            &native::workspace::WorkspaceMember,
            &native::layout::LayoutBox,
        )>()
        .iter(world)
        .find(|(_, window, _, _)| window.surface == id)
        .map(|(entity, _, member, layout)| (entity, member.0, PlacementId(layout.id)))
        .unwrap()
}

fn model(
    canvas: &mut crate::presentation::Presentation,
    root: Entity,
    id: u64,
) -> &lince_interface::layout::desktop::Workspace {
    &canvas
        .world_mut()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .unwrap()
        .desktop
}

fn green(frame: &crate::presentation::Frame) {
    let count = crate::presentation::probe_pixels(frame.dmabuf().unwrap())
        .unwrap()
        .chunks_exact(4)
        .filter(|pixel| pixel[1] > 200 && pixel[0] < 30 && pixel[2] < 30)
        .count();
    assert!(
        count > 1000,
        "The transferred client lost its content: {count}"
    );
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn live_wayland_item_and_row_transfers_preserve_surfaces_pixels_widths_and_output_ownership() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = crate::presentation::Presentation::new(
        [900, 600],
        directory.path().join("assets"),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    let mut harness = Harness::new();
    harness
        .server
        .configure_outputs(&[
            Monitor {
                id: OutputId(1),
                name: "Transfer-A".into(),
                origin: [0, 0],
                size: [900, 600],
            },
            Monitor {
                id: OutputId(2),
                name: "Transfer-B".into(),
                origin: [900, 0],
                size: [640, 480],
            },
        ])
        .unwrap();
    harness.sync();
    canvas
        .outputs(
            harness
                .server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect(),
        )
        .unwrap();
    let first = harness.window();
    first.role.set_app_id("transfer.first".into());
    first.role.set_min_size(450, 80);
    let second = harness.window();
    second.role.set_app_id("transfer.second".into());
    let mut retained = vec![
        paint(&mut harness, &first, [450, 240], [0, 255, 0, 255]),
        paint(&mut harness, &second, [320, 240], [255, 0, 0, 255]),
    ];
    let snapshots = harness.server.windows();
    let first_id = snapshots
        .iter()
        .find(|snapshot| snapshot.app_id == "transfer.first")
        .unwrap()
        .id;
    let second_id = snapshots
        .iter()
        .find(|snapshot| snapshot.app_id == "transfer.second")
        .unwrap()
        .id;
    let clients = [
        (&first, first_id, [0, 255, 0, 255]),
        (&second, second_id, [255, 0, 0, 255]),
    ];
    let original_surface = snapshots
        .iter()
        .find(|snapshot| snapshot.id == first_id)
        .unwrap()
        .surface
        .clone();
    present(&mut canvas, &mut harness, &clients, &mut retained);
    canvas.focus_output(OutputId(1)).unwrap();
    canvas.focus_application(first_id);
    settle(&mut canvas);
    let initial_first = member(&mut canvas, first_id);
    let initial_second = member(&mut canvas, second_id);
    assert_eq!(initial_first.1, initial_second.1);
    let source = initial_first.1;
    assert_eq!(
        model(&mut canvas, root, source).mode(),
        WorkspaceLayout::Tiled
    );
    canvas
        .world_mut()
        .get_mut::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .iter_mut()
        .find(|entry| entry.id == source)
        .unwrap()
        .desktop
        .set_item_width(initial_first.2, Width::TwoThirds)
        .unwrap();
    present(&mut canvas, &mut harness, &clients, &mut retained);
    let original = model(&mut canvas, root, source)
        .items()
        .iter()
        .find(|item| item.id == initial_first.2)
        .unwrap()
        .clone();
    canvas
        .desktop_action(DesktopAction::MoveToWorkspace(9))
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    let numbers = native::workspace::numbers(canvas.world_mut(), root);
    assert_eq!(numbers.len(), 9);
    let ninth = numbers[8];
    assert_eq!(
        member(&mut canvas, first_id),
        (initial_first.0, ninth, initial_first.2)
    );
    assert_eq!(member(&mut canvas, second_id), initial_second);
    assert_eq!(
        model(&mut canvas, root, ninth).item_width(initial_first.2),
        Some(Width::TwoThirds)
    );
    let moved = model(&mut canvas, root, ninth)
        .items()
        .iter()
        .find(|item| item.id == initial_first.2)
        .unwrap();
    assert_eq!(moved.free, original.free);
    assert_eq!(moved.free_width, original.free_width);
    assert_eq!(
        frame.geometry.as_ref().unwrap().focused,
        Some(ItemId::Application(first_id))
    );
    green(&frame);
    canvas
        .desktop_action(DesktopAction::MoveToWorkspace(1))
        .unwrap();
    present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(member(&mut canvas, first_id), initial_first);
    let row = model(&mut canvas, root, source).tiled_rows()[0].clone();
    canvas
        .desktop_action(DesktopAction::MoveRowToOutput(Direction::Right))
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(frame.output, Some(OutputId(2)));
    let destination = member(&mut canvas, first_id).1;
    assert_ne!(destination, source);
    assert_eq!(member(&mut canvas, second_id).1, destination);
    let rows = model(&mut canvas, root, destination).tiled_rows();
    let transferred = rows
        .iter()
        .find(|candidate| candidate.contains(&initial_first.2))
        .unwrap();
    assert_eq!(transferred, &row);
    assert_eq!(
        model(&mut canvas, root, destination).item_width(initial_first.2),
        Some(Width::TwoThirds)
    );
    assert!(
        !model(&mut canvas, root, source).items().iter().any(|item| [
            initial_first.2,
            initial_second.2
        ]
        .contains(&item.id))
    );
    green(&frame);
    canvas
        .desktop_action(DesktopAction::ToggleFloating)
        .unwrap();
    present(&mut canvas, &mut harness, &clients, &mut retained);
    let before = model(&mut canvas, root, destination).clone();
    canvas
        .desktop_action(DesktopAction::MoveRowToOutput(Direction::Left))
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(frame.output, Some(OutputId(2)));
    assert_eq!(member(&mut canvas, first_id).1, destination);
    assert_eq!(model(&mut canvas, root, destination), &before);
    canvas
        .desktop_action(DesktopAction::MoveToOutput(Direction::Left))
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(frame.output, Some(OutputId(1)));
    assert_eq!(member(&mut canvas, first_id), initial_first);
    assert_eq!(
        member(&mut canvas, second_id),
        (initial_second.0, destination, initial_second.2)
    );
    assert!(
        model(&mut canvas, root, source)
            .items()
            .iter()
            .find(|item| item.id == initial_first.2)
            .unwrap()
            .floating
    );
    canvas
        .desktop_action(DesktopAction::ToggleFloating)
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(
        model(&mut canvas, root, source).item_width(initial_first.2),
        Some(Width::TwoThirds)
    );
    assert!(
        !model(&mut canvas, root, source)
            .items()
            .iter()
            .find(|item| item.id == initial_first.2)
            .unwrap()
            .floating
    );
    assert_eq!(
        frame.geometry.as_ref().unwrap().focused,
        Some(ItemId::Application(first_id))
    );
    assert!(
        frame
            .windows
            .iter()
            .find(|window| window.surface == first_id)
            .unwrap()
            .size[0]
            >= 450
    );
    green(&frame);
    focus_tests::capture("transfers-real-clients", &frame);
    for expected in [Width::Full, Width::Half, Width::Third, Width::TwoThirds] {
        canvas
            .desktop_action(DesktopAction::CycleItemWidth)
            .unwrap();
        let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
        assert_eq!(
            model(&mut canvas, root, source)
                .item_width(initial_first.2)
                .and_then(Width::ratio),
            expected.ratio()
        );
        let width = frame
            .windows
            .iter()
            .find(|window| window.surface == first_id)
            .unwrap()
            .size[0];
        if matches!(expected, Width::Half | Width::Third) {
            assert_eq!(width, 450);
        } else {
            assert!(width > 450);
        }
        assert_eq!(member(&mut canvas, second_id).1, destination);
        green(&frame);
    }
    let neighbor_before = frame
        .windows
        .iter()
        .find(|window| window.surface == second_id)
        .unwrap()
        .size;
    let widths_before = model(&mut canvas, root, source).item_width(initial_first.2);
    for ratio in [[1, 2], [1, 3], [2, 3], [1, 1]] {
        canvas
            .desktop_action(DesktopAction::CycleRowHeight)
            .unwrap();
        let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
        assert_eq!(
            model(&mut canvas, root, source)
                .row_height(initial_first.2)
                .and_then(Width::ratio),
            Some(ratio)
        );
        assert_eq!(
            model(&mut canvas, root, source).item_width(initial_first.2),
            widths_before
        );
        assert_eq!(
            frame
                .windows
                .iter()
                .find(|window| window.surface == second_id)
                .unwrap()
                .size,
            neighbor_before
        );
        assert_eq!(member(&mut canvas, first_id), initial_first);
        green(&frame);
    }
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    let before = frame
        .windows
        .iter()
        .find(|window| window.surface == first_id)
        .unwrap()
        .size;
    canvas
        .desktop_action(DesktopAction::Resize(Direction::Right))
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    let grown = frame
        .windows
        .iter()
        .find(|window| window.surface == first_id)
        .unwrap()
        .size;
    assert_eq!(grown, [before[0] + 64, before[1]]);
    assert!(
        model(&mut canvas, root, source)
            .item_width(initial_first.2)
            .unwrap()
            .ratio()
            .is_none()
    );
    assert_eq!(
        model(&mut canvas, root, source)
            .row_height(initial_first.2)
            .unwrap()
            .ratio(),
        Some([1, 1])
    );
    canvas
        .desktop_action(DesktopAction::Resize(Direction::Up))
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(
        frame
            .windows
            .iter()
            .find(|window| window.surface == first_id)
            .unwrap()
            .size,
        [grown[0], grown[1] - 64]
    );
    assert_eq!(
        frame
            .windows
            .iter()
            .find(|window| window.surface == second_id)
            .unwrap()
            .size,
        neighbor_before
    );
    green(&frame);
    focus_tests::capture("resizing-real-clients", &frame);
    let order = native::workspace::numbers(canvas.world_mut(), root);
    canvas
        .desktop_action(DesktopAction::MoveWorkspaceLater)
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(frame.output, Some(OutputId(1)));
    assert_eq!(member(&mut canvas, first_id), initial_first);
    assert_eq!(
        frame.geometry.as_ref().unwrap().focused,
        Some(ItemId::Application(first_id))
    );
    green(&frame);
    canvas
        .desktop_action(DesktopAction::PreviousWorkspace)
        .unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(frame.output, Some(OutputId(2)));
    assert_eq!(member(&mut canvas, second_id).1, destination);
    canvas.desktop_action(DesktopAction::NextWorkspace).unwrap();
    let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(frame.output, Some(OutputId(1)));
    assert_eq!(member(&mut canvas, first_id), initial_first);
    green(&frame);
    canvas
        .desktop_action(DesktopAction::MoveWorkspaceEarlier)
        .unwrap();
    present(&mut canvas, &mut harness, &clients, &mut retained);
    assert_eq!(native::workspace::numbers(canvas.world_mut(), root), order);
    assert_eq!(
        harness
            .server
            .windows()
            .iter()
            .find(|snapshot| snapshot.id == first_id)
            .unwrap()
            .surface,
        original_surface
    );
    canvas.finish().unwrap();
}
