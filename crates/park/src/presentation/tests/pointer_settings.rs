use super::displays::{labelled, reveal, write};
use super::shortcuts::{capture, click, named_button, settle};
use super::*;
use crate::input_settings::{self, PointerControl};
use lince_interface::{
    host::{
        HostRequest,
        input::{InputOutcome, PointerCapabilities, PointerDevice, PointerSettings},
    },
    native::actions::Action,
};
use std::{collections::BTreeMap, sync::mpsc::Receiver};

fn bridge(
    canvas: &mut Presentation,
    control: &mut PointerControl,
    receiver: &Receiver<HostRequest>,
    path: &std::path::Path,
    fail: bool,
) {
    let HostRequest::Pointer(request) = receiver.try_recv().unwrap() else {
        panic!("Expected pointer settings")
    };
    assert!(receiver.try_recv().is_err());
    control.request(request, false);
    let request = control.ready(false, true).unwrap();
    let before = control.snapshot.devices.clone();
    let mut effective = before
        .iter()
        .map(|device| (device.id.clone(), device.effective.clone()))
        .collect::<BTreeMap<_, _>>();
    let result = input_settings::apply(
        &before,
        &request.settings,
        |id, settings| {
            effective.insert(id.into(), settings.clone());
            Ok(())
        },
        || {
            if fail {
                return Err(std::io::Error::other("Fixture write failure"));
            }
            crate::commands::Commands::save_pointer(
                path,
                &request.settings,
                &control.snapshot.settings,
            )
        },
    );
    if fail {
        let failure = result.err().unwrap();
        assert!(failure.restoration.is_empty());
        assert!(
            before
                .iter()
                .all(|device| effective[&device.id] == device.effective)
        );
        control.failed(request.id, failure.reason);
    } else {
        control.saved(request.id, request.settings, result.ok().unwrap());
        control.devices(
            before
                .into_iter()
                .map(|mut device| {
                    device.effective = effective.remove(&device.id).unwrap();
                    device
                })
                .collect(),
        );
        assert_eq!(
            control
                .snapshot
                .devices
                .iter()
                .find(|device| device.id == "mouse")
                .unwrap()
                .effective
                .tap,
            None
        );
    }
    canvas.world_mut().insert_resource(control.snapshot.clone());
    settle(canvas);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn pointer_settings_validate_save_restore_and_cancel_at_wide_and_narrow_sizes() {
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
        let path = directory.path().join(format!("pointer-{}.json", size[0]));
        std::fs::write(
            &path,
            br#"{"keyboard":{"layout":"br"},"terminal":{"program":"chosen-terminal"}}"#,
        )
        .unwrap();
        let mut control = PointerControl::new(PointerSettings::default(), true);
        control.devices(
            ["touchpad", "mouse"]
                .into_iter()
                .map(|id| {
                    let capabilities = PointerCapabilities {
                        tap: id == "touchpad",
                        natural_scroll: true,
                        left_handed: true,
                        acceleration: true,
                    };
                    let effective = PointerSettings {
                        tap: capabilities.tap.then_some(false),
                        natural_scroll: Some(false),
                        left_handed: Some(false),
                        acceleration: Some(0.0),
                    };
                    PointerDevice {
                        id: id.into(),
                        name: id.into(),
                        capabilities,
                        defaults: effective.clone(),
                        effective,
                        error: None,
                    }
                })
                .collect(),
        );
        canvas.world_mut().insert_resource(control.snapshot.clone());
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        canvas.host_requests(sender);
        let root = canvas.canvas_root().unwrap();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let open = named_button(&mut canvas, "Edit pointer settings");
        reveal(&mut canvas, open);
        click(&mut canvas, open, size);
        settle(&mut canvas);
        let caption = "Acceleration speed (-1 to 1; empty uses device defaults)";
        let field = labelled(&mut canvas, caption);
        write(&mut canvas, caption, "2", size);
        let save = named_button(&mut canvas, "Save pointer settings");
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        assert!(receiver.try_recv().is_err());
        write(&mut canvas, caption, "0.4", size);
        let tap = named_button(&mut canvas, "Tap to click: Device default");
        reveal(&mut canvas, tap);
        click(&mut canvas, tap, size);
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        bridge(&mut canvas, &mut control, &receiver, &path, true);
        assert!(matches!(
            control.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Failed(_)
        ));
        assert_eq!(labelled(&mut canvas, caption), field);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .value(),
            "0.4"
        );
        let frame = settle(&mut canvas);
        capture(&format!("pointer-failed-{}", size[0]), size, &frame);
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        bridge(&mut canvas, &mut control, &receiver, &path, false);
        let commands = crate::commands::Commands::load(&path).unwrap();
        assert_eq!(commands.pointer.tap, Some(true));
        assert_eq!(commands.pointer.acceleration, Some(0.4));
        assert_eq!(commands.keyboard.layout, "br");
        assert_eq!(commands.terminal.unwrap().program, "chosen-terminal");
        let frame = settle(&mut canvas);
        let world = canvas.world_mut();
        let details = world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, name)| name.as_str() == "pointer.devices")
            .unwrap()
            .0;
        let viewport = world.get::<ChildOf>(details).unwrap().parent();
        let bounds = |entity| {
            Rect::from_center_size(
                world
                    .get::<UiGlobalTransform>(entity)
                    .unwrap()
                    .transform_point2(Vec2::ZERO),
                world.get::<ComputedNode>(entity).unwrap().size(),
            )
        };
        let content = bounds(details);
        let visible = bounds(viewport);
        assert!(
            content.min.y >= visible.min.y - 1.0 && content.max.y <= visible.max.y + 1.0,
            "Both fixture devices must be readable without clipped lines: {content:?} within {visible:?}"
        );
        capture(&format!("pointer-saved-{}", size[0]), size, &frame);
        write(&mut canvas, caption, "0.8", size);
        let cancel = named_button(&mut canvas, "Cancel pointer changes");
        reveal(&mut canvas, cancel);
        click(&mut canvas, cancel, size);
        let frame = settle(&mut canvas);
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .value(),
            "0.4"
        );
        capture(&format!("pointer-cancelled-{}", size[0]), size, &frame);
    }
}
