use super::super::displays::{labelled, reveal, write};
use super::*;

fn launch(
    directory: &std::path::Path,
    size: [u32; 2],
) -> (
    Presentation,
    std::sync::mpsc::Receiver<lince_interface::host::HostRequest>,
) {
    let mut canvas =
        Presentation::new(size, directory.join("assets"), WakeSignal::new(|| {})).unwrap();
    canvas
        .world_mut()
        .insert_resource(native::workspace::WorkspaceFile::new(
            directory.join("interface.json"),
        ));
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    canvas.host_requests(sender);
    settle(&mut canvas);
    (canvas, receiver)
}

fn press(canvas: &mut Presentation, caption: &str, size: [u32; 2]) {
    let button = named_button(canvas, caption);
    reveal(canvas, button);
    click(canvas, button, size);
    settle(canvas);
}

fn open(canvas: &mut Presentation, size: [u32; 2]) {
    let root = canvas.canvas_root().unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    settle(canvas);
    press(canvas, "Edit overview transfers", size);
}

fn value(canvas: &mut Presentation, caption: &str) -> String {
    let field = labelled(canvas, caption);
    canvas
        .app
        .world()
        .get::<bevy::text::EditableText>(field)
        .unwrap()
        .value()
        .to_string()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn overview_timing_controls_validate_save_cancel_and_load_at_real_presentation_startup() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    const DELAY: &str = "File drag hover delay (100–2000 ms)";
    const SPEED: &str = "Edge scroll speed (0–100% of viewport/s)";
    for size in [[1000, 720], [360, 720]] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let (mut canvas, receiver) = launch(&profile, size);
        open(&mut canvas, size);
        assert_eq!(value(&mut canvas, DELAY), "400");
        assert_eq!(value(&mut canvas, SPEED), "100");
        write(&mut canvas, DELAY, "99", size);
        let save = named_button(&mut canvas, "Save overview transfers");
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
        assert!(!profile.join("interface.lingua").exists());
        let error = canvas
            .world_mut()
            .query::<(Entity, &Text)>()
            .iter(canvas.app.world())
            .find(|(_, text)| {
                text.0 == "Use an overview hover delay from 100 to 2000 milliseconds."
            })
            .map(|(entity, _)| entity)
            .expect("An invalid value must explain why Save is disabled");
        reveal(&mut canvas, error);
        let frame = settle(&mut canvas);
        capture(
            &format!("overview-timing-invalid-{}", size[0]),
            size,
            &frame,
        );
        press(&mut canvas, "Cancel overview changes", size);
        assert_eq!(value(&mut canvas, DELAY), "400");
        write(&mut canvas, DELAY, "650", size);
        write(&mut canvas, SPEED, "25", size);
        press(&mut canvas, "Save overview transfers", size);
        assert!(profile.join("interface.lingua").is_file());
        assert_eq!(value(&mut canvas, DELAY), "650");
        write(&mut canvas, DELAY, "800", size);
        press(&mut canvas, "Cancel overview changes", size);
        assert_eq!(value(&mut canvas, DELAY), "650");
        assert_eq!(value(&mut canvas, SPEED), "25");
        let frame = settle(&mut canvas);
        capture(&format!("overview-timing-saved-{}", size[0]), size, &frame);
        canvas.finish().unwrap();
        drop(canvas);
        drop(receiver);
        let (mut reopened, _receiver) = launch(&profile, size);
        open(&mut reopened, size);
        assert_eq!(value(&mut reopened, DELAY), "650");
        assert_eq!(value(&mut reopened, SPEED), "25");
        let field = labelled(&mut reopened, DELAY);
        reveal(&mut reopened, field);
        let frame = settle(&mut reopened);
        capture(
            &format!("overview-timing-restarted-{}", size[0]),
            size,
            &frame,
        );
        reopened.finish().unwrap();
    }
}
