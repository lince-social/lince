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
        include_bytes!("../../../../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf")
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
fn unavailable_host_feedback_preserves_valid_settings_and_the_unsaved_draft() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, page) = fixture(directory.path());
    let field = app.world().get::<Page>(page).unwrap().fields[0];
    edit(&mut app, field, "750");
    app.world_mut()
        .insert_resource(crate::host::FileDragSupport {
            available: false,
            reason: Some("This host cannot deliver file drags.".into()),
        });
    app.update();
    let state = app.world().get::<Page>(page).unwrap();
    assert_eq!(
        app.world().get::<Text>(state.support).unwrap().0,
        "This host cannot deliver file drags."
    );
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(state.save)
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(field)
            .unwrap()
            .value()
            .to_string(),
        "750"
    );
    assert_eq!(current(app.world()).hover_delay_ms, 400);
    app.world_mut()
        .insert_resource(crate::host::FileDragSupport {
            available: true,
            reason: None,
        });
    app.update();
    assert_eq!(
        app.world()
            .get::<EditableText>(field)
            .unwrap()
            .value()
            .to_string(),
        "750"
    );
    assert_eq!(current(app.world()).hover_delay_ms, 400);
}

#[test]
fn overview_save_merges_bindings_and_other_preferences_and_loads_at_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, page) = fixture(directory.path());
    let state = app.world().get::<Page>(page).unwrap();
    let (fields, save) = (state.fields, state.save);
    edit(&mut app, fields[0], "900");
    edit(&mut app, fields[1], "25");
    let mut shortcuts = app.world().resource::<Settings>().clone();
    shortcuts
        .set(crate::native::shortcuts::Shortcut::Overview, "Mod+Alt+G")
        .unwrap();
    let mut layout = app.world().resource::<LayoutSettings>().clone();
    layout.gestures.sensitivity_percent = 175;
    configuration::commit(app.world_mut(), shortcuts, layout).unwrap();
    app.update();
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_none()
    );
    activate(&mut app, save);
    assert_eq!(
        current(app.world()),
        OverviewSettings {
            hover_delay_ms: 900,
            edge_scroll_percent: 25
        }
    );
    assert_eq!(
        app.world()
            .resource::<LayoutSettings>()
            .gestures
            .sensitivity_percent,
        175
    );
    drop(app);
    let (app, page) = fixture(directory.path());
    assert_eq!(
        app.world().get::<Page>(page).unwrap().origin,
        OverviewSettings {
            hover_delay_ms: 900,
            edge_scroll_percent: 25
        }
    );
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(crate::native::shortcuts::Shortcut::Overview),
        "Mod+Alt+G"
    );
    assert_eq!(
        app.world()
            .resource::<LayoutSettings>()
            .gestures
            .sensitivity_percent,
        175
    );
}

#[test]
fn invalid_or_stale_overview_drafts_preserve_saved_values_and_cancel_reloads_them() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, page) = fixture(directory.path());
    let fields = app.world().get::<Page>(page).unwrap().fields;
    let save = app.world().get::<Page>(page).unwrap().save;
    edit(&mut app, fields[0], "99");
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_some()
    );
    assert_eq!(current(app.world()), OverviewSettings::default());
    edit(&mut app, fields[0], "800");
    let shortcuts = app.world().resource::<Settings>().clone();
    let mut layout = app.world().resource::<LayoutSettings>().clone();
    layout.overview.hover_delay_ms = 600;
    configuration::commit(app.world_mut(), shortcuts, layout).unwrap();
    app.update();
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_some()
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(fields[0])
            .unwrap()
            .value()
            .to_string(),
        "800"
    );
    let status = app.world().get::<Page>(page).unwrap().status;
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("changed")
    );
    let cancel = app
        .world_mut()
        .query::<(Entity, &Text)>()
        .iter(app.world())
        .find(|(_, text)| text.0 == "Cancel overview changes")
        .map(|(entity, _)| app.world().get::<ChildOf>(entity).unwrap().parent())
        .unwrap();
    activate(&mut app, cancel);
    assert_eq!(
        app.world()
            .get::<EditableText>(fields[0])
            .unwrap()
            .value()
            .to_string(),
        "600"
    );
    assert_eq!(current(app.world()).hover_delay_ms, 600);
}
