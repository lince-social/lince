use super::displays::{labelled, reveal, write};
use super::shortcuts::{capture, click, click_with_keys, named_button, settle};
use super::*;
use lince_interface::{
    host::{
        HostRequest,
        external::{Action as ExternalAction, Command, Settings},
        launch::Description,
    },
    native::actions::Action,
};

fn caption_bounds(canvas: &Presentation, control: Entity) {
    let world = canvas.app.world();
    let parent = world.get::<ChildOf>(control).unwrap().parent();
    let bounds = world.get::<ComputedNode>(control).unwrap().size();
    assert!(bounds.min_element() > 0.0);
    assert!(bounds.x <= world.get::<ComputedNode>(parent).unwrap().size().x);
    for child in world.get::<Children>(control).unwrap().iter() {
        if world
            .get::<Text>(child)
            .is_some_and(|text| !text.0.is_empty())
        {
            let size = world.get::<ComputedNode>(child).unwrap().size();
            assert!(
                size.min_element() > 0.0,
                "Command caption collapsed: {size:?}"
            );
            assert!(size.x <= bounds.x);
            assert!(
                !world
                    .get::<bevy::text::TextLayoutInfo>(child)
                    .unwrap()
                    .glyphs
                    .is_empty()
            );
        }
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn external_commands_use_real_fields_literal_arguments_save_retry_cancel_and_shared_binding_capture()
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
        let initial = Settings(std::collections::BTreeMap::from([(
            ExternalAction::VolumeUp,
            Command {
                command: Description {
                    program: "chosen-volume-tool".into(),
                    arguments: vec!["first".into(), String::new()],
                    directory: None,
                    capabilities: Vec::new(),
                },
                allow_when_locked: false,
            },
        )]));
        let path = directory.path().join(format!("external-{}.json", size[0]));
        std::fs::write(&path, serde_json::to_vec(&serde_json::json!({"keyboard":{"layout":"br"}, "terminal":{"program":"terminal-tool"}, "external":initial})).unwrap()).unwrap();
        let mut state = crate::external_commands::ExternalCommands::default();
        state.configure(initial);
        canvas.world_mut().insert_resource(state.snapshot.clone());
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        canvas.host_requests(sender);
        let root = canvas.canvas_root().unwrap();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let open = named_button(&mut canvas, "Edit external commands");
        reveal(&mut canvas, open);
        click(&mut canvas, open, size);
        let dropdown = labelled(&mut canvas, "External command");
        reveal(&mut canvas, dropdown);
        click(&mut canvas, dropdown, size);
        let volume = named_button(&mut canvas, ExternalAction::VolumeUp.label());
        reveal(&mut canvas, volume);
        click(&mut canvas, volume, size);
        settle(&mut canvas);
        let program_caption = "Executable (empty disables this command)";
        let directory_caption = "Working directory (absolute; empty uses Park's directory)";
        let program = labelled(&mut canvas, program_caption);
        let argument = labelled(&mut canvas, "Argument 1");
        write(
            &mut canvas,
            program_caption,
            "replacement-volume-tool",
            size,
        );
        write(
            &mut canvas,
            "Argument 1",
            "$(printf literal) ; with spaces",
            size,
        );
        write(&mut canvas, directory_caption, "relative", size);
        let save = named_button(&mut canvas, "Save external commands");
        reveal(&mut canvas, save);
        settle(&mut canvas);
        caption_bounds(&canvas, save);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
        assert!(receiver.try_recv().is_err());
        capture(
            &format!("external-invalid-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        write(&mut canvas, directory_caption, "", size);
        let locked = named_button(&mut canvas, "Allow while locked: off");
        reveal(&mut canvas, locked);
        click(&mut canvas, locked, size);
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        let HostRequest::External(request) = receiver.try_recv().unwrap() else {
            panic!("external settings request");
        };
        assert_eq!(
            request.settings.0[&ExternalAction::VolumeUp]
                .command
                .arguments,
            ["$(printf literal) ; with spaces".to_owned(), String::new()]
        );
        assert!(request.settings.0[&ExternalAction::VolumeUp].allow_when_locked);
        state.request(request, false, |_, _| {
            Err(std::io::Error::other("fixture write failure"))
        });
        canvas.world_mut().insert_resource(state.snapshot.clone());
        settle(&mut canvas);
        assert_eq!(labelled(&mut canvas, program_caption), program);
        assert_eq!(labelled(&mut canvas, "Argument 1"), argument);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(program)
                .unwrap()
                .value(),
            "replacement-volume-tool"
        );
        capture(
            &format!("external-failed-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        let HostRequest::External(request) = receiver.try_recv().unwrap() else {
            panic!("retry request");
        };
        state.request(request, false, |candidate, expected| {
            crate::commands::Commands::save_external(&path, candidate, expected)
        });
        canvas.world_mut().insert_resource(state.snapshot.clone());
        settle(&mut canvas);
        let saved = crate::commands::Commands::load(&path).unwrap();
        assert_eq!(saved.keyboard.layout, "br");
        assert_eq!(saved.terminal.unwrap().program, "terminal-tool");
        assert_eq!(saved.external, state.snapshot.settings);
        assert!(receiver.try_recv().is_err());
        capture(
            &format!("external-saved-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        write(&mut canvas, program_caption, "unsaved-program", size);
        assert!(
            canvas
                .world_mut()
                .query::<&Text>()
                .iter(canvas.app.world())
                .any(|text| text.0.contains("Unsaved command changes"))
        );
        let cancel = named_button(&mut canvas, "Cancel external changes");
        reveal(&mut canvas, cancel);
        caption_bounds(&canvas, cancel);
        capture(
            &format!("external-cancel-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        click(&mut canvas, cancel, size);
        assert!(matches!(
            receiver.try_recv().unwrap(),
            HostRequest::ExternalReload
        ));
        settle(&mut canvas);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(program)
                .unwrap()
                .value(),
            "replacement-volume-tool"
        );
        let binding = canvas
            .world_mut()
            .query::<(Entity, &bevy::a11y::AccessibilityNode)>()
            .iter(canvas.app.world())
            .find(|(_, node)| {
                node.label()
                    .is_some_and(|label| label.starts_with("Edit binding ·"))
            })
            .map(|(entity, _)| entity)
            .unwrap();
        reveal(&mut canvas, binding);
        caption_bounds(&canvas, binding);
        capture(
            &format!("external-binding-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        let frame = settle(&mut canvas);
        let position = canvas
            .app
            .world()
            .get::<UiGlobalTransform>(binding)
            .unwrap()
            .transform_point2(Vec2::ZERO)
            .as_dvec2()
            .to_array();
        assert!(
            frame
                .input
                .keyboard_modal_triggers
                .iter()
                .any(|bounds| bounds.contains(position))
        );
        click_with_keys(
            &mut canvas,
            binding,
            size,
            &[
                (KeyCode::AudioVolumeUp, true),
                (KeyCode::AudioVolumeUp, false),
            ],
        );
        let frame = settle(&mut canvas);
        assert!(frame.input.keyboard_modal);
        assert!(frame.input.keyboard_capture);
        assert!(receiver.try_recv().is_err());
        let cancel = named_button(&mut canvas, "Cancel draft");
        reveal(&mut canvas, cancel);
        click(&mut canvas, cancel, size);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        canvas.finish().unwrap();
    }
}
