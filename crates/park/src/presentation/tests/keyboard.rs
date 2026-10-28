use super::displays::{labelled, reveal, write};
use super::shortcuts::{capture, click, named_button, settle};
use super::*;
use lince_interface::{
    host::{
        HostRequest,
        input::{InputOutcome, KeyboardSettings},
    },
    native::actions::Action,
};

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn keyboard_settings_use_native_fields_and_the_host_transaction_at_wide_and_narrow_sizes() {
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
        let path = directory.path().join(format!("keyboard-{}.json", size[0]));
        std::fs::write(&path, br#"{"keyboard":{"layout":"us"}}"#).unwrap();
        let mut server = crate::server::Server::new([size[0] as i32, size[1] as i32]).unwrap();
        server
            .configure_keyboard(KeyboardSettings {
                layout: "us".into(),
                ..Default::default()
            })
            .unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        canvas.host_requests(sender);
        canvas
            .world_mut()
            .insert_resource(server.keyboard_snapshot());
        let root = canvas.canvas_root().unwrap();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let open = named_button(&mut canvas, "Edit keyboard settings");
        reveal(&mut canvas, open);
        click(&mut canvas, open, size);
        settle(&mut canvas);
        let layout_caption = "XKB layout (empty uses the system default)";
        let rate_caption = "Repeat rate in Hz (0 disables repeat)";
        let delay_caption = "Repeat delay in milliseconds";
        let field = labelled(&mut canvas, layout_caption);
        write(&mut canvas, layout_caption, "br", size);
        write(&mut canvas, rate_caption, "999", size);
        let save = named_button(&mut canvas, "Save keyboard settings");
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        settle(&mut canvas);
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .value(),
            "br"
        );
        write(&mut canvas, rate_caption, "35", size);
        write(&mut canvas, delay_caption, "300", size);
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        settle(&mut canvas);
        let HostRequest::Keyboard(request) = receiver.try_recv().unwrap() else {
            panic!("Expected keyboard settings")
        };
        assert_eq!(request.settings.layout, "br");
        assert_eq!(request.settings.repeat_rate, 35);
        assert_eq!(request.settings.repeat_delay_ms, 300);
        server.keyboard_request(request);
        assert!(
            server
                .advance_keyboard(
                    |settings, expected| crate::commands::Commands::save_keyboard(
                        &path, settings, expected
                    )
                )
                .is_some()
        );
        assert!(matches!(
            server.keyboard_snapshot().response.unwrap().outcome,
            InputOutcome::Saved { .. }
        ));
        canvas
            .world_mut()
            .insert_resource(server.keyboard_snapshot());
        let frame = settle(&mut canvas);
        assert_eq!(
            crate::commands::Commands::load(&path)
                .unwrap()
                .keyboard
                .layout,
            "br"
        );
        assert_eq!(labelled(&mut canvas, layout_caption), field);
        capture(&format!("keyboard-saved-{}", size[0]), size, &frame);
        write(&mut canvas, layout_caption, "de", size);
        let cancel = named_button(&mut canvas, "Cancel keyboard changes");
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
            "br"
        );
        assert_eq!(
            crate::commands::Commands::load(&path)
                .unwrap()
                .keyboard
                .repeat_rate,
            35
        );
        capture(&format!("keyboard-cancelled-{}", size[0]), size, &frame);
    }
}
