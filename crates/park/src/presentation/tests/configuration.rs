use super::*;
use super::{displays::{labelled, reveal, write}, shortcuts::{capture, click, key, named_button, settle}};
use lince_interface::{host::{HostRequest, OutputId}, native::actions::Action};
use std::{collections::BTreeMap, io::Write, path::Path, sync::mpsc::Receiver, time::{Duration, Instant}};

mod owners;

fn launch(directory: &Path, size: [u32; 2], scale: f64) -> (Presentation, Receiver<HostRequest>) {
    launch_with_cell(directory, size, scale, None)
}

fn launch_with_cell(directory: &Path, size: [u32; 2], scale: f64, cell: Option<native::app::CellHandle>) -> (Presentation, Receiver<HostRequest>) {
    let connected = cell.is_some();
    let mut canvas = Presentation::new_with_cell(size, directory.join("assets"), WakeSignal::new(|| {}), cell.map(|cell| (cell, directory.into()))).unwrap();
    if !connected { canvas.world_mut().insert_resource(native::workspace::WorkspaceFile::new(directory.join("interface.json"))); }
    let mut server = crate::server::Server::new(size.map(|value| value as i32)).unwrap();
    server.configure_scaled_outputs(&[crate::server::Monitor { id: OutputId(1), name: "configuration-fixture".into(), origin: [0, 0], size: size.map(|value| value as i32) }], &BTreeMap::from([(OutputId(1), scale)])).unwrap();
    canvas.outputs(server.output_viewports().into_iter().map(|(name, viewport)| Output { name, viewport }).collect()).unwrap();
    let (sender, receiver) = std::sync::mpsc::sync_channel(32);
    canvas.host_requests(sender);
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    (canvas, receiver)
}

fn press(canvas: &mut Presentation, caption: &str, size: [u32; 2]) {
    let entity = named_button(canvas, caption);
    reveal(canvas, entity);
    click(canvas, entity, size);
    settle(canvas);
}

fn named(canvas: &mut Presentation, name: &str) -> Option<Entity> {
    let world = canvas.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find(|(_, candidate)| candidate.as_str() == name).map(|(entity, _)| entity)
}

fn tab_to(canvas: &mut Presentation, target: Entity) {
    for _ in 0..256 {
        if canvas.app.world().resource::<bevy::input_focus::InputFocus>().get() == Some(target) { return; }
        key(canvas, KeyCode::Tab, true);
        key(canvas, KeyCode::Tab, false);
    }
    panic!("The review control was not reachable through ordinary Tab navigation");
}

fn replace_number(path: &Path, field: &str, value: u32) {
    let mut source = std::fs::read_to_string(path).unwrap();
    let name = source.find(field).unwrap() + field.len();
    let start = name + source[name..].find(|character: char| character.is_ascii_digit()).unwrap();
    assert!(start - name <= 64);
    let end = start + source[start..].find(|character: char| !character.is_ascii_digit()).unwrap();
    source.replace_range(start..end, &value.to_string());
    let mut replacement = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
    replacement.write_all(source.as_bytes()).unwrap();
    replacement.as_file().sync_all().unwrap();
    replacement.persist(path).unwrap();
}

fn wait_dwell(canvas: &mut Presentation, value: u16) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        settle(canvas);
        if active_dwell(canvas, value) { return; }
        assert!(Instant::now() < deadline, "The real configuration watcher did not accept the external dwell edit");
        std::thread::sleep(Duration::from_millis(3));
    }
}

fn active_dwell(canvas: &mut Presentation, value: u16) -> bool {
    let prefix = format!("File hover: {value} ms ·");
    let world = canvas.world_mut();
    world.query::<&Text>().iter(world).any(|text| text.0.starts_with(&prefix))
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn every_configuration_owner_reaches_review_cancel_update_and_continue_without_implicitly_saving_at_scaled_sizes() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let families = [
        ("bindings", "Edit all shortcuts", "Review draft against file"),
        ("layout", "Edit window layout", "Review draft against file"),
        ("focus", "Edit focus settings", "Review focus draft against file"),
        ("gestures", "Edit desktop gestures", "Review gesture draft against file"),
        ("overview", "Edit overview transfers", "Review overview draft against file"),
        ("opening", "Edit opening rules", "Review rule order against file"),
    ];
    for (size, scale) in [([1000, 720], 1.0), ([1000, 720], 1.25), ([360, 720], 1.5)] {
        for (name, open, review) in families {
            let profile = directory.path().join(format!("{name}-{}-{scale}", size[0]));
            std::fs::create_dir_all(&profile).unwrap();
            let (mut canvas, _receiver) = launch(&profile, size, scale);
            press(&mut canvas, open, size);
            press(&mut canvas, review, size);
            assert!(named(&mut canvas, "configuration.review").is_some());
            let frame = settle(&mut canvas);
            capture(&format!("configuration-{name}-{}-{scale}", size[0]), size, &frame);
            key(&mut canvas, KeyCode::Escape, true);
            key(&mut canvas, KeyCode::Escape, false);
            assert!(named(&mut canvas, "configuration.review").is_none());
            assert!(!profile.join("interface.lingua").exists());
            press(&mut canvas, review, size);
            press(&mut canvas, "Update comparison", size);
            let continue_button = named(&mut canvas, "configuration.review.confirm").unwrap();
            assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(continue_button).is_none());
            tab_to(&mut canvas, continue_button);
            key(&mut canvas, KeyCode::Space, true);
            key(&mut canvas, KeyCode::Space, false);
            assert!(named(&mut canvas, "configuration.review").is_none());
            assert!(!profile.join("interface.lingua").exists());
            canvas.finish().unwrap();
        }
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_review_of_real_external_revisions_preserves_the_draft_and_rejects_an_older_comparison() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    const DWELL: &str = "File drag hover delay (100–2000 ms)";
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let (mut canvas, _receiver) = launch(&profile, size, scale);
        press(&mut canvas, "Edit overview transfers", size);
        write(&mut canvas, DWELL, "650", size);
        press(&mut canvas, "Save overview transfers", size);
        write(&mut canvas, DWELL, "800", size);
        let path = profile.join("interface.lingua");
        replace_number(&path, "hover_delay_ms", 900);
        wait_dwell(&mut canvas, 900);
        press(&mut canvas, "Review overview draft against file", size);
        let confirm = named(&mut canvas, "configuration.review.confirm").unwrap();
        assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_some());
        press(&mut canvas, "Use my draft value", size);
        let frame = settle(&mut canvas);
        capture(&format!("configuration-resolved-{}-{scale}", size[0]), size, &frame);
        replace_number(&path, "hover_delay_ms", 1000);
        wait_dwell(&mut canvas, 1000);
        assert!(canvas.app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_some());
        press(&mut canvas, "Update comparison", size);
        press(&mut canvas, "Use my draft value", size);
        press(&mut canvas, "Continue with reviewed draft", size);
        let field = labelled(&mut canvas, DWELL);
        assert_eq!(canvas.app.world().get::<bevy::text::EditableText>(field).unwrap().value().to_string(), "800");
        assert!(active_dwell(&mut canvas, 1000));
        press(&mut canvas, "Save overview transfers", size);
        assert!(active_dwell(&mut canvas, 800));
        canvas.finish().unwrap();
        drop(canvas);
        drop(_receiver);
        let (mut reopened, _receiver) = launch(&profile, size, scale);
        press(&mut reopened, "Edit overview transfers", size);
        let field = labelled(&mut reopened, DWELL);
        assert_eq!(reopened.app.world().get::<bevy::text::EditableText>(field).unwrap().value().to_string(), "800");
        assert!(active_dwell(&mut reopened, 800));
        reopened.finish().unwrap();
    }
}
