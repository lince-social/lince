use super::*;
use crate::host::input::InputResponse;

fn fixture(available: bool) -> (App, Entity, std::sync::mpsc::Receiver<HostRequest>) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<InputFocus>();
    super::super::input_settings::install(&mut app);
    install(&mut app);
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    app.insert_resource(HostRequests(sender));
    app.insert_resource(Snapshot {
        available,
        ..default()
    });
    let root = app.world_mut().spawn(Node::default()).id();
    panel(app.world_mut(), root, root);
    settle(&mut app);
    let page = app
        .world_mut()
        .query_filtered::<Entity, With<Page>>()
        .single(app.world())
        .unwrap();
    Change::Toggle.apply(app.world_mut(), page);
    settle(&mut app);
    (app, page, receiver)
}

fn settle(app: &mut App) {
    app.update();
    let mut query = app.world_mut().query::<&mut EditableText>();
    for mut field in query.iter_mut(app.world_mut()) {
        field.pending_edits.clear();
    }
}

fn write(app: &mut App, entity: Entity, value: &str) {
    let mut text = app.world_mut().get_mut::<EditableText>(entity).unwrap();
    text.editor.set_text(value);
    text.pending_edits.clear();
}

#[test]
fn action_selection_preserves_literal_argument_drafts_and_saving_never_executes_them() {
    let (mut app, page, receiver) = fixture(true);
    let program = app.world().get::<Page>(page).unwrap().program;
    write(&mut app, program, "brightness-tool");
    Change::AddArgument.apply(app.world_mut(), page);
    settle(&mut app);
    Change::AddArgument.apply(app.world_mut(), page);
    settle(&mut app);
    let argument = app.world().get::<Page>(page).unwrap().arguments[1];
    write(&mut app, argument, "$(printf literal) with spaces");
    Change::Locked.apply(app.world_mut(), page);
    Change::Select(ExternalAction::MediaPlayPause).apply(app.world_mut(), page);
    settle(&mut app);
    write(&mut app, program, "media-tool");
    Change::Select(ExternalAction::BrightnessDown).apply(app.world_mut(), page);
    settle(&mut app);
    assert_eq!(app.world().get::<Page>(page).unwrap().program, program);
    assert_eq!(value(app.world(), program).unwrap(), "brightness-tool");
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::External(request) = receiver.try_recv().unwrap() else {
        panic!("settings request");
    };
    let command = &request.settings.0[&ExternalAction::BrightnessDown];
    assert_eq!(
        command.command.arguments,
        [String::new(), "$(printf literal) with spaces".into()]
    );
    assert!(command.allow_when_locked);
    assert_eq!(
        request.settings.0[&ExternalAction::MediaPlayPause]
            .command
            .program,
        "media-tool"
    );
    assert!(receiver.try_recv().is_err());
}

#[test]
fn pending_clipboard_input_blocks_saving_and_selection_and_cancel_discards_it() {
    let (mut app, page, receiver) = fixture(true);
    let program = app.world().get::<Page>(page).unwrap().program;
    write(&mut app, program, "first-tool");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(program, FocusCause::Pressed);
    app.world_mut()
        .get_mut::<EditableText>(program)
        .unwrap()
        .pending_paste = Some(bevy::clipboard::ClipboardRead::Pending(Default::default()));
    Change::Save.apply(app.world_mut(), page);
    Change::Select(ExternalAction::Editor).apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    assert_eq!(
        app.world().get::<Page>(page).unwrap().selected,
        ExternalAction::BrightnessDown
    );
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(program));
    assert_eq!(
        app.world().get::<EditableText>(program).unwrap().value(),
        "first-tool"
    );
    assert!(value(app.world(), program).is_err());
    Change::Cancel.apply(app.world_mut(), page);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        HostRequest::ExternalReload
    ));
    assert!(receiver.try_recv().is_err());
    assert!(
        app.world()
            .get::<EditableText>(program)
            .unwrap()
            .pending_paste
            .is_none()
    );
    settle(&mut app);
    assert_eq!(value(app.world(), program).unwrap(), "");
}

#[test]
fn failed_stale_and_late_replies_retain_fields_focus_and_newer_edits() {
    let (mut app, page, receiver) = fixture(true);
    let program = app.world().get::<Page>(page).unwrap().program;
    write(&mut app, program, "first-tool");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(program, FocusCause::Pressed);
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::External(first) = receiver.try_recv().unwrap() else {
        panic!("request");
    };
    write(&mut app, program, "newer-tool");
    app.insert_resource(Snapshot {
        available: true,
        revision: 1,
        settings: first.settings,
        last_request: first.id,
        response: Some(InputResponse {
            request: first.id,
            outcome: InputOutcome::Saved { warning: None },
        }),
        message: None,
    });
    settle(&mut app);
    assert_eq!(value(app.world(), program).unwrap(), "newer-tool");
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(program));
    assert!(app.world().get::<Page>(page).unwrap().pending.is_none());
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::External(second) = receiver.try_recv().unwrap() else {
        panic!("request");
    };
    assert_eq!(second.revision, 1);
    assert_eq!(
        second.settings.0[&ExternalAction::BrightnessDown]
            .command
            .program,
        "newer-tool"
    );
    app.world_mut().resource_mut::<Snapshot>().response = Some(InputResponse {
        request: second.id,
        outcome: InputOutcome::Failed("write failed".into()),
    });
    settle(&mut app);
    assert_eq!(value(app.world(), program).unwrap(), "newer-tool");
    assert!(
        app.world()
            .get::<Page>(page)
            .unwrap()
            .message
            .contains("write failed")
    );
    app.world_mut().resource_mut::<Snapshot>().revision = 2;
    settle(&mut app);
    assert_eq!(value(app.world(), program).unwrap(), "newer-tool");
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    Change::Cancel.apply(app.world_mut(), page);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        HostRequest::ExternalReload
    ));
    settle(&mut app);
    assert_eq!(value(app.world(), program).unwrap(), "first-tool");
}

#[test]
fn sandbox_and_pending_text_cannot_submit_or_enable_privileged_lock_actions() {
    let (mut app, page, receiver) = fixture(false);
    let program = app.world().get::<Page>(page).unwrap().program;
    write(&mut app, program, "tool");
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    app.world_mut().resource_mut::<Snapshot>().available = true;
    settle(&mut app);
    app.world_mut()
        .get_mut::<EditableText>(program)
        .unwrap()
        .queue_edit(bevy::text::TextEdit::Insert("pending".into()));
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    app.world_mut()
        .get_mut::<EditableText>(program)
        .unwrap()
        .pending_edits
        .clear();
    Change::Select(ExternalAction::ScreenshotScreen).apply(app.world_mut(), page);
    settle(&mut app);
    write(&mut app, program, "screenshot-tool");
    Change::Locked.apply(app.world_mut(), page);
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::External(request) = receiver.try_recv().unwrap() else {
        panic!("request");
    };
    let command = &request.settings.0[&ExternalAction::ScreenshotScreen];
    assert!(!command.allow_when_locked);
    assert_eq!(command.command.capabilities, [Capability::Capture]);
}
