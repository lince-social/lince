use super::shortcuts::{capture, click, key, named_button, settle};
use super::*;
use lince_interface::native::canvas::CanvasItem;
use lince_interface::{
    host::{DesktopAction, Direction, SurfaceId, WorkspaceLayout},
    native::actions::Action,
};

fn shortcut(canvas: &mut Presentation, key_code: KeyCode, shift: bool) {
    key(canvas, KeyCode::SuperLeft, true);
    if shift {
        key(canvas, KeyCode::ShiftLeft, true);
    }
    key(canvas, key_code, true);
    key(canvas, key_code, false);
    if shift {
        key(canvas, KeyCode::ShiftLeft, false);
    }
    key(canvas, KeyCode::SuperLeft, false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_tiling_controls_and_chords_restore_the_free_canvas_without_recreating_windows() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1280, 720];
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    assert!(native::workspace_config::set_physics(
        canvas.world_mut(),
        root,
        1,
        false
    ));
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .mode(),
        WorkspaceLayout::Tiled
    );
    canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::AddSand(native::sand_store::SandKind::Square)
        .apply(canvas.world_mut(), root);
    for id in [SurfaceId(101), SurfaceId(102)] {
        canvas
            .open_window(id, "Generic application", "tiling.validation")
            .unwrap();
    }
    settle(&mut canvas);
    native::edit_mode::EditAction::ToggleAlwaysShowControls.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    canvas.focus_application(SurfaceId(101));
    let entities: Vec<_> = canvas
        .world_mut()
        .query::<(Entity, &CanvasItem, &ChildOf)>()
        .iter(canvas.app.world())
        .filter(|(entity, _, parent)| {
            parent.parent() == root
                && (canvas
                    .app
                    .world()
                    .get::<native::layout::desktop::ApplicationWindow>(*entity)
                    .is_some()
                    || canvas
                        .app
                        .world()
                        .get::<native::sand_store::StoredSand>(*entity)
                        .is_some())
        })
        .map(|(entity, _, _)| entity)
        .collect();
    assert_eq!(entities.len(), 3);
    for (index, entity) in entities.iter().enumerate() {
        canvas
            .world_mut()
            .get_mut::<CanvasItem>(*entity)
            .unwrap()
            .position =
            bevy::math::DVec2::new(120.0 + index as f64 * 500.0, -100.0 + index as f64 * 80.0);
    }
    let original_view = native::canvas::CanvasView {
        center: bevy::math::DVec2::new(500.0, -120.0),
        zoom: 0.7,
    };
    *canvas
        .world_mut()
        .get_mut::<native::canvas::CanvasView>(root)
        .unwrap() = original_view;
    let before = settle(&mut canvas);
    let original_items: Vec<_> = entities
        .iter()
        .map(|entity| {
            (
                *entity,
                *canvas.app.world().get::<CanvasItem>(*entity).unwrap(),
            )
        })
        .collect();
    capture("tiling-free-before", size, &before);
    let button = named_button(&mut canvas, lince_interface::i18n::Token::TilingOff.text());
    click(&mut canvas, button, size);
    let tiled = settle(&mut canvas);
    assert_eq!(
        tiled
            .windows
            .iter()
            .map(|window| window.surface)
            .collect::<std::collections::BTreeSet<_>>(),
        [SurfaceId(101), SurfaceId(102)].into_iter().collect()
    );
    let model = &canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries[0]
        .desktop;
    assert_eq!(model.mode(), WorkspaceLayout::Tiled);
    let focused = model.focused().unwrap();
    let rows = model.tiled_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 3);
    let index = rows[0].iter().position(|id| *id == focused).unwrap();
    let (direction, expected) = if index + 1 < rows[0].len() {
        (KeyCode::KeyL, rows[0][index + 1])
    } else {
        (KeyCode::KeyH, rows[0][index - 1])
    };
    shortcut(&mut canvas, direction, false);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .focused(),
        Some(expected)
    );
    shortcut(&mut canvas, KeyCode::KeyJ, true);
    let model = &canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries[0]
        .desktop;
    assert_eq!(model.tiled_rows().len(), 2);
    assert_eq!(model.focused(), Some(expected));
    let rows = model.tiled_rows();
    shortcut(&mut canvas, KeyCode::KeyT, true);
    let model = &canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries[0]
        .desktop;
    assert!(
        model
            .items()
            .iter()
            .find(|item| item.id == expected)
            .unwrap()
            .floating
    );
    assert_eq!(model.mode(), WorkspaceLayout::Tiled);
    shortcut(&mut canvas, KeyCode::KeyT, true);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .tiled_rows(),
        rows
    );
    let native = entities
        .iter()
        .copied()
        .find(|entity| {
            canvas
                .app
                .world()
                .get::<native::sand_store::StoredSand>(*entity)
                .is_some()
        })
        .unwrap();
    native::layout::desktop::focus_item(canvas.world_mut(), native);
    let frame = settle(&mut canvas);
    capture("tiling-rows", size, &frame);
    shortcut(&mut canvas, KeyCode::KeyT, false);
    let frame = settle(&mut canvas);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .mode(),
        WorkspaceLayout::Free
    );
    let restored = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    assert_eq!(restored.center, original_view.center);
    assert_eq!(restored.zoom, original_view.zoom);
    for (entity, original) in original_items {
        let item = canvas.app.world().get::<CanvasItem>(entity).unwrap();
        assert_eq!(item.position, original.position);
        assert_eq!(item.size, original.size);
    }
    assert_eq!(
        frame
            .windows
            .iter()
            .map(|window| window.surface)
            .collect::<std::collections::BTreeSet<_>>(),
        [SurfaceId(101), SurfaceId(102)].into_iter().collect()
    );
    capture("tiling-free-restored", size, &frame);
    shortcut(&mut canvas, KeyCode::KeyT, false);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .tiled_rows(),
        rows
    );
    canvas
        .desktop_action(DesktopAction::Focus(Direction::Up))
        .unwrap();
    settle(&mut canvas);
    canvas.finish().unwrap();
}
