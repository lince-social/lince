use super::*;

fn fixture(handoff: bool) -> (App, Entity) {
    let mut app = crate::sand_panel::tests::app();
    app.add_plugins((BackupPlugin, crate::accessibility::AccessibilityPlugin));
    if handoff {
        app.insert_resource(BackupHandoff::default());
    }
    let parent = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), parent);
    let owner = app
        .world_mut()
        .query_filtered::<Entity, With<Form>>()
        .single(app.world())
        .unwrap();
    (app, owner)
}

fn fill(world: &mut World, owner: Entity, path: &str, password: &str, confirmation: &str) {
    let form = world.get::<Form>(owner).unwrap();
    let fields = [
        (form.destination, path),
        (form.passphrase, password),
        (form.confirmation, confirmation),
    ];
    for (field, value) in fields {
        *world.get_mut::<EditableText>(field).unwrap() = crate::sand::editable(value);
    }
}

fn cleared(world: &World, owner: Entity) {
    let form = world.get::<Form>(owner).unwrap();
    for field in [form.passphrase, form.confirmation] {
        assert!(
            world
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string()
                .is_empty()
        );
        assert!(world.get::<crate::sand_text::SandText>(field).is_none());
        assert_eq!(
            world
                .get::<bevy::a11y::AccessibilityNode>(field)
                .unwrap()
                .role(),
            accesskit::Role::PasswordInput
        );
    }
}

#[test]
fn private_backup_form_masks_secrets_blocks_copy_and_hands_off_once_before_exit() {
    let (mut app, owner) = fixture(true);
    let password = "native backup private passphrase";
    fill(
        app.world_mut(),
        owner,
        "/chosen/owner.lince-backup",
        password,
        password,
    );
    let form = app.world().get::<Form>(owner).unwrap();
    let fields = [form.passphrase, form.confirmation];
    for field in fields {
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .pending_edits
            .extend([bevy::text::TextEdit::Copy, bevy::text::TextEdit::Cut]);
    }
    app.update();
    for field in fields {
        assert!(
            !app.world()
                .get::<EditableText>(field)
                .unwrap()
                .pending_edits
                .iter()
                .any(|edit| matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut))
        );
        assert!(
            !app.world()
                .get::<bevy::a11y::AccessibilityNode>(field)
                .unwrap()
                .value()
                .unwrap()
                .contains(password)
        );
    }
    for text in app.world_mut().query::<&Text>().iter(app.world()) {
        assert!(!text.0.contains(password));
    }
    Submit.apply(app.world_mut(), owner);
    cleared(app.world(), owner);
    assert_eq!(app.should_exit(), Some(AppExit::Success));
    let handoff = app.world().resource::<BackupHandoff>().clone();
    let request = handoff.take().unwrap().unwrap();
    assert_eq!(
        request.destination,
        std::path::Path::new("/chosen/owner.lince-backup")
    );
    assert_eq!(request.passphrase.expose(), password);
    assert!(!format!("{request:?}").contains(password));
    assert!(handoff.take().unwrap().is_none());
}

#[test]
fn invalid_or_unavailable_backup_handoff_keeps_app_open_and_clears_fields() {
    for (enabled, password, repeated) in [
        (false, "long passphrase", "long passphrase"),
        (true, "long passphrase", "different passphrase"),
        (true, "", ""),
    ] {
        let (mut app, owner) = fixture(enabled);
        fill(
            app.world_mut(),
            owner,
            "owner.lince-backup",
            password,
            repeated,
        );
        Submit.apply(app.world_mut(), owner);
        cleared(app.world(), owner);
        assert!(app.should_exit().is_none());
        if let Some(handoff) = app.world().get_resource::<BackupHandoff>() {
            assert!(handoff.take().unwrap().is_none());
        }
        let status = app.world().get::<Form>(owner).unwrap().status;
        assert!(
            !app.world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("Closing Lince")
        );
    }
}

#[test]
fn queued_input_is_refused_instead_of_capturing_a_partial_passphrase() {
    let (mut app, owner) = fixture(true);
    fill(
        app.world_mut(),
        owner,
        "owner.lince-backup",
        "long passphrase",
        "long passphrase",
    );
    let field = app.world().get::<Form>(owner).unwrap().passphrase;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .pending_edits
        .push(bevy::text::TextEdit::Insert("final character".into()));
    Submit.apply(app.world_mut(), owner);
    assert!(app.should_exit().is_none());
    assert!(
        app.world()
            .resource::<BackupHandoff>()
            .take()
            .unwrap()
            .is_none()
    );
    cleared(app.world(), owner);
    let status = app.world().get::<Form>(owner).unwrap().status;
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("Finish typing")
    );
}

#[test]
fn refused_interface_exit_cancels_backup_instead_of_saving_it_on_a_later_quit() {
    let (mut app, owner) = fixture(true);
    fill(
        app.world_mut(),
        owner,
        "owner.lince-backup",
        "long passphrase",
        "long passphrase",
    );
    Submit.apply(app.world_mut(), owner);
    app.world_mut().resource_mut::<Messages<AppExit>>().clear();
    app.update();
    assert!(
        app.world()
            .resource::<BackupHandoff>()
            .take()
            .unwrap()
            .is_none()
    );
    let status = app.world().get::<Form>(owner).unwrap().status;
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("cancelled")
    );
    app.world_mut().write_message(AppExit::Success);
    assert!(
        app.world()
            .resource::<BackupHandoff>()
            .take()
            .unwrap()
            .is_none()
    );
}

#[test]
fn repeated_submit_cannot_replace_the_reviewed_backup_request() {
    let (mut app, owner) = fixture(true);
    fill(
        app.world_mut(),
        owner,
        "first.lince-backup",
        "first passphrase",
        "first passphrase",
    );
    Submit.apply(app.world_mut(), owner);
    fill(
        app.world_mut(),
        owner,
        "second.lince-backup",
        "second passphrase",
        "second passphrase",
    );
    Submit.apply(app.world_mut(), owner);
    cleared(app.world(), owner);
    let request = app
        .world()
        .resource::<BackupHandoff>()
        .take()
        .unwrap()
        .unwrap();
    assert_eq!(
        request.destination,
        std::path::Path::new("first.lince-backup")
    );
    assert_eq!(request.passphrase.expose(), "first passphrase");
}
