use super::displays::{labelled, reveal, write};
use super::shortcuts::{capture, click, named_button, settle};
use super::*;
use lince_interface::native::actions::Action;

fn open(canvas: &mut Presentation, size: [u32; 2]) -> Entity {
    let control = named_button(canvas, "Edit window layout");
    reveal(canvas, control);
    click(canvas, control, size);
    assert!(settle(canvas).input.keyboard_modal);
    labelled(canvas, "Window gaps (0 to 256)")
}

fn press(canvas: &mut Presentation, caption: &str, size: [u32; 2]) {
    let control = named_button(canvas, caption);
    reveal(canvas, control);
    click(canvas, control, size);
    settle(canvas);
}

fn value(canvas: &Presentation, field: Entity) -> String {
    canvas
        .app
        .world()
        .get::<bevy::text::EditableText>(field)
        .unwrap()
        .value()
        .to_string()
}

fn saved_preferences(canvas: &mut Presentation, gap: Entity) {
    assert_eq!(value(canvas, gap), "0");
    let caption = "New application windows: Maximized";
    let control = named_button(canvas, caption);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::a11y::AccessibilityNode>(control)
            .unwrap()
            .label(),
        Some(caption)
    );
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn window_layout_edits_gaps_opening_and_widths_with_scrolling_validation_cancel_and_preserved_preferences()
 {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 720]] {
        let mut canvas = Presentation::new(
            size,
            directory.path().join(format!("assets-{}", size[0])),
            WakeSignal::new(|| {}),
        )
        .unwrap();
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let field = open(&mut canvas, size);
        assert_eq!(value(&canvas, field), "8");
        write(&mut canvas, "Window gaps (0 to 256)", "999", size);
        let save = named_button(&mut canvas, "Save window layout");
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
        assert_eq!(labelled(&mut canvas, "Window gaps (0 to 256)"), field);
        capture(
            &format!("window-layout-invalid-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        write(&mut canvas, "Window gaps (0 to 256)", "0", size);
        write(&mut canvas, "Row height preset 1", "50%", size);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
        capture(
            &format!("window-layout-row-height-invalid-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        write(&mut canvas, "Row height preset 1", "25%", size);
        let up = labelled(&mut canvas, "Move up row height preset 3");
        reveal(&mut canvas, up);
        click(&mut canvas, up, size);
        settle(&mut canvas);
        let second_height = labelled(&mut canvas, "Row height preset 2");
        assert_eq!(value(&canvas, second_height), "1/3");
        write(&mut canvas, "Resize step (logical pixels)", "0", size);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
        write(&mut canvas, "Resize step (logical pixels)", "37", size);
        press(&mut canvas, "New application windows: Normal", size);
        capture(
            &format!("window-layout-options-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        press(&mut canvas, "Save window layout", size);
        let frame = settle(&mut canvas);
        assert!(!frame.input.keyboard_modal);
        capture(&format!("window-layout-saved-{}", size[0]), size, &frame);
        let surface = lince_interface::host::SurfaceId(u64::from(size[0]));
        canvas
            .open_window(surface, "Opening settings", "validation")
            .unwrap();
        let frame = settle(&mut canvas);
        let window = frame
            .windows
            .iter()
            .find(|window| window.surface == surface)
            .unwrap();
        assert!(window.state.maximized);
        assert!(!window.state.fullscreen);
        assert_eq!(window.size, size);
        canvas.window_request(
            surface,
            lince_interface::host::WindowRequest {
                maximized: Some(false),
                ..default()
            },
        );
        let frame = settle(&mut canvas);
        let window = frame
            .windows
            .iter()
            .find(|window| window.surface == surface)
            .unwrap();
        assert!(!window.state.maximized);
        assert_eq!(window.size[0], size[0] / 2);
        canvas.close_window(surface);
        settle(&mut canvas);

        let field = open(&mut canvas, size);
        saved_preferences(&mut canvas, field);
        let height = labelled(&mut canvas, "Row height preset 1");
        assert_eq!(value(&canvas, height), "25%");
        let resize = labelled(&mut canvas, "Resize step (logical pixels)");
        assert_eq!(value(&canvas, resize), "37");
        write(&mut canvas, "Window gaps (0 to 256)", "16", size);
        press(&mut canvas, "New application windows: Maximized", size);
        press(&mut canvas, "Cancel layout draft", size);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        let field = open(&mut canvas, size);
        saved_preferences(&mut canvas, field);
        write(&mut canvas, "Width preset 1", "25%", size);
        press(&mut canvas, "Save window layout", size);

        let field = open(&mut canvas, size);
        saved_preferences(&mut canvas, field);
        press(&mut canvas, "Reset widths", size);
        let height = labelled(&mut canvas, "Row height preset 1");
        assert_eq!(value(&canvas, height), "25%");
        let resize = labelled(&mut canvas, "Resize step (logical pixels)");
        assert_eq!(value(&canvas, resize), "37");
        reveal(&mut canvas, height);
        capture(
            &format!("window-layout-row-heights-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        let first = labelled(&mut canvas, "Width preset 1");
        assert_eq!(value(&canvas, first), "100%");
        saved_preferences(&mut canvas, field);
        let frame = settle(&mut canvas);
        capture(&format!("window-layout-reset-{}", size[0]), size, &frame);
        press(&mut canvas, "Save window layout", size);
        let field = open(&mut canvas, size);
        saved_preferences(&mut canvas, field);
        press(&mut canvas, "Cancel layout draft", size);
        canvas.finish().unwrap();
    }
}
