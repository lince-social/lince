use super::*;

fn fixture(directory: &std::path::Path) -> (App, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        bevy::input::InputPlugin,
        crate::native::actions::ActionsPlugin,
        crate::native::shortcuts::EditorPlugin,
        crate::controls::editing::TextEditingPlugin,
    ))
    .init_resource::<Assets<Font>>()
    .init_resource::<bevy::text::FontCx>()
    .init_resource::<bevy::text::LayoutCx>()
    .init_resource::<bevy::clipboard::Clipboard>()
    .init_resource::<crate::native::theme::Typography>()
    .add_systems(
        PostUpdate,
        bevy::text::apply_text_edits.in_set(bevy::text::EditableTextSystems),
    )
    .insert_resource(crate::native::workspace::WorkspaceFile::new(
        directory.join("interface.json"),
    ));
    install(&mut app);
    let font = Font::from_bytes(
        include_bytes!("../../../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf")
            .to_vec(),
    );
    app.world_mut()
        .resource_mut::<bevy::text::FontCx>()
        .collection
        .register_fonts(font.data, None);
    app.world_mut()
        .resource_mut::<bevy::text::FontCx>()
        .set_sans_serif_family("Lato")
        .unwrap();
    configuration::load(app.world_mut());
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    panel(app.world_mut(), root);
    app.update();
    let page = app
        .world_mut()
        .query_filtered::<Entity, With<Page>>()
        .single(app.world())
        .unwrap();
    let toggle = app.world().get::<Page>(page).unwrap().toggle.0;
    activate(&mut app, toggle);
    (app, page)
}

fn activate(app: &mut App, entity: Entity) {
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    app.update();
}

fn edit(app: &mut App, entity: Entity, value: &str) {
    app.world_mut()
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
    app.update();
}

#[test]
fn gesture_save_merges_other_layout_and_binding_changes_and_loads_at_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, page) = fixture(directory.path());
    let state = app.world().get::<Page>(page).unwrap();
    let (sensitivity, threshold, save) = (state.sensitivity, state.threshold, state.save);
    edit(&mut app, sensitivity, "175");
    edit(&mut app, threshold, "112");
    let mut shortcuts = app.world().resource::<Settings>().clone();
    shortcuts
        .set(crate::native::shortcuts::Shortcut::Terminal, "Mod+Alt+N")
        .unwrap();
    let mut layout = app.world().resource::<LayoutSettings>().clone();
    layout.gaps = 23;
    configuration::commit(app.world_mut(), shortcuts, layout).unwrap();
    app.update();
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_none()
    );
    activate(&mut app, save);
    assert_eq!(
        settings(app.world()),
        GestureSettings {
            sensitivity_percent: 175,
            commit_threshold: 112,
            ..Default::default()
        }
    );
    assert_eq!(app.world().resource::<LayoutSettings>().gaps, 23);
    drop(app);
    let (restarted, _) = fixture(directory.path());
    assert_eq!(
        settings(restarted.world()),
        GestureSettings {
            sensitivity_percent: 175,
            commit_threshold: 112,
            ..Default::default()
        }
    );
    assert_eq!(restarted.world().resource::<LayoutSettings>().gaps, 23);
    assert_eq!(
        restarted
            .world()
            .resource::<Settings>()
            .text(crate::native::shortcuts::Shortcut::Terminal),
        "Mod+Alt+N"
    );
}

#[test]
fn invalid_or_stale_gesture_drafts_keep_saved_values_and_can_be_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, page) = fixture(directory.path());
    let state = app.world().get::<Page>(page).unwrap();
    let (sensitivity, save, status) = (state.sensitivity, state.save, state.status);
    edit(&mut app, sensitivity, "0");
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_some()
    );
    assert!(app.world().get::<Text>(status).unwrap().0.contains("10%"));
    assert_eq!(settings(app.world()), GestureSettings::default());
    edit(&mut app, sensitivity, "150");
    let mut layout = app.world().resource::<LayoutSettings>().clone();
    layout.gestures.sensitivity_percent = 200;
    let shortcuts = app.world().resource::<Settings>().clone();
    configuration::commit(app.world_mut(), shortcuts, layout).unwrap();
    app.update();
    activate(&mut app, save);
    assert_eq!(settings(app.world()).sensitivity_percent, 200);
    assert_eq!(
        app.world()
            .get::<EditableText>(sensitivity)
            .unwrap()
            .value()
            .to_string(),
        "150"
    );
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("changed")
    );
    Change::Cancel.apply(app.world_mut(), page);
    app.update();
    assert_eq!(
        app.world()
            .get::<EditableText>(sensitivity)
            .unwrap()
            .value()
            .to_string(),
        "200"
    );
}

#[test]
fn host_availability_changes_keep_the_gesture_draft_and_saved_preference() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, page) = fixture(directory.path());
    let state = app.world().get::<Page>(page).unwrap();
    let (sensitivity, availability) = (state.sensitivity, state.availability);
    edit(&mut app, sensitivity, "123");
    assert!(
        app.world()
            .get::<Text>(availability)
            .unwrap()
            .0
            .contains("no desktop")
    );
    app.world_mut().resource_mut::<GestureSupport>().0 = true;
    app.update();
    assert!(
        app.world()
            .get::<Text>(availability)
            .unwrap()
            .0
            .contains("is available")
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(sensitivity)
            .unwrap()
            .value()
            .to_string(),
        "123"
    );
    assert_eq!(settings(app.world()), GestureSettings::default());
    assert!(!directory.path().join("interface.lingua").exists());
}
