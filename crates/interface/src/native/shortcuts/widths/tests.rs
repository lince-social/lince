use super::*;
use bevy::input::InputPlugin;

#[test]
fn layout_rebase_keeps_the_local_preset_and_independent_external_focus_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let first = app.world().resource::<WidthEditor>().0.as_ref().unwrap().entries[0].input;
    app.world_mut().get_mut::<EditableText>(first).unwrap().editor.set_text("25%");
    {
        let mut active = app.world_mut().resource_mut::<LayoutSettings>();
        active.gaps = 19;
        active.focus.pointer_follow = true;
    }
    app.update();
    let nodes: Vec<_> = app.world_mut().query_filtered::<Entity, With<Node>>().iter(app.world()).collect();
    for node in nodes { app.world_mut().entity_mut(node).insert(InheritedVisibility::VISIBLE); }
    let rebase = named(&mut app, "Review draft against file");
    activate(&mut app, rebase);
    let confirm = named(&mut app, "Continue with reviewed draft");
    activate(&mut app, confirm);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let draft = candidate(app.world(), modal).unwrap();
    assert_eq!(draft.width_presets[0], [1, 4]);
    assert_eq!(draft.gaps, 19);
    assert!(draft.focus.pointer_follow);
    assert_eq!(app.world().resource::<LayoutSettings>().width_presets[0], [1, 1]);
    let save = named(&mut app, "Save window layout");
    activate(&mut app, save);
    let active = app.world().resource::<LayoutSettings>();
    assert_eq!(active.width_presets[0], [1, 4]);
    assert_eq!(active.gaps, 19);
    assert!(active.focus.pointer_follow);
}

fn fixture(directory: &std::path::Path) -> (App, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        InputPlugin,
        crate::native::actions::ActionsPlugin,
        super::super::EditorPlugin,
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
    let font = Font::from_bytes(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../institute/assets/fonts/Lato/Lato-Regular.ttf"
        ))
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
    panel(app.world_mut(), root, root);
    app.update();
    (app, root)
}

fn activate(app: &mut App, entity: Entity) {
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    app.update();
}

fn named(app: &mut App, caption: &str) -> Entity {
    app.world_mut()
        .query::<(Entity, &ActionButton, &Children)>()
        .iter(app.world())
        .find(|(_, _, children)| {
            children.iter().any(|child| {
                app.world()
                    .get::<Text>(child)
                    .is_some_and(|text| text.0 == caption)
            })
        })
        .map(|(entity, _, _)| entity)
        .unwrap()
}

#[test]
fn layout_save_and_width_reset_preserve_opening_preferences_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let (overlay, gap, first) = (modal.overlay, modal.gap, modal.entries[0].input);
    app.world_mut()
        .get_mut::<EditableText>(gap)
        .unwrap()
        .editor
        .set_text("0");
    app.world_mut()
        .get_mut::<EditableText>(first)
        .unwrap()
        .editor
        .set_text("25%");
    Change::Opening.apply(app.world_mut(), overlay);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let edited = candidate(app.world(), modal).unwrap();
    assert_eq!(edited.gaps, 0);
    assert_eq!(edited.application_opening, ApplicationOpening::Maximized);
    assert_eq!(edited.width_presets[0], [1, 4]);
    Change::Reset(Preset::Width).apply(app.world_mut(), overlay);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let reset = candidate(app.world(), modal).unwrap();
    assert_eq!(reset.width_presets, LayoutSettings::default().width_presets);
    assert_eq!(reset.gaps, 0);
    assert_eq!(reset.application_opening, ApplicationOpening::Maximized);
    assert_eq!(modal.gap, gap);
    Change::Save.apply(app.world_mut(), overlay);
    assert!(!active(app.world()));
    let (restarted, _) = fixture(directory.path());
    assert_eq!(restarted.world().resource::<LayoutSettings>(), &reset);
}

#[test]
fn row_heights_and_resize_step_share_validation_reordering_save_and_restart_without_resetting_widths()
 {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    app.world_mut()
        .resource_mut::<Settings>()
        .set(super::super::Shortcut::CycleRowHeight, "Mod+F8")
        .unwrap();
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let (overlay, height, width, resize, up, save) = (
        modal.overlay,
        modal.heights[0].input,
        modal.entries[0].input,
        modal.resize_step,
        modal.heights[2].up,
        modal.save,
    );
    app.world_mut()
        .get_mut::<EditableText>(height)
        .unwrap()
        .editor
        .set_text("1/2");
    app.update();
    assert!(app.world().get::<InteractionDisabled>(save).is_some());
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert!(
        candidate(app.world(), modal)
            .unwrap_err()
            .contains("Row height presets must not repeat")
    );
    assert_eq!(modal.heights[0].input, height);
    app.world_mut()
        .get_mut::<EditableText>(height)
        .unwrap()
        .editor
        .set_text("25%");
    app.world_mut()
        .get_mut::<EditableText>(width)
        .unwrap()
        .editor
        .set_text("75%");
    app.world_mut()
        .get_mut::<EditableText>(resize)
        .unwrap()
        .editor
        .set_text("0");
    app.update();
    assert!(app.world().get::<InteractionDisabled>(save).is_some());
    app.world_mut()
        .get_mut::<EditableText>(resize)
        .unwrap()
        .editor
        .set_text("37");
    activate(&mut app, up);
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert_eq!(modal.heights[0].input, height);
    let edited = candidate(app.world(), modal).unwrap();
    assert_eq!(edited.height_presets, vec![[1, 4], [1, 3], [1, 2], [2, 3]]);
    assert_eq!(edited.width_presets[0], [3, 4]);
    assert_eq!(edited.resize_step, 37);
    Change::Save.apply(app.world_mut(), overlay);
    assert!(!active(app.world()));
    let (mut restarted, root) = fixture(directory.path());
    assert_eq!(restarted.world().resource::<LayoutSettings>(), &edited);
    assert_eq!(
        restarted
            .world()
            .resource::<Settings>()
            .text(super::super::Shortcut::CycleRowHeight),
        "Mod+F8"
    );
    Change::Open.apply(restarted.world_mut(), root);
    restarted.update();
    let overlay = restarted
        .world()
        .resource::<WidthEditor>()
        .0
        .as_ref()
        .unwrap()
        .overlay;
    Change::Reset(Preset::Width).apply(restarted.world_mut(), overlay);
    restarted.update();
    let modal = restarted
        .world()
        .resource::<WidthEditor>()
        .0
        .as_ref()
        .unwrap();
    let reset = candidate(restarted.world(), modal).unwrap();
    assert_eq!(reset.height_presets, edited.height_presets);
    assert_eq!(reset.resize_step, 37);
    Change::Reset(Preset::RowHeight).apply(restarted.world_mut(), overlay);
    restarted.update();
    let modal = restarted
        .world()
        .resource::<WidthEditor>()
        .0
        .as_ref()
        .unwrap();
    assert_eq!(
        candidate(restarted.world(), modal).unwrap().height_presets,
        LayoutSettings::default().height_presets
    );
    assert_eq!(candidate(restarted.world(), modal).unwrap().resize_step, 37);
    Change::Cancel.apply(restarted.world_mut(), overlay);
    assert_eq!(restarted.world().resource::<LayoutSettings>(), &edited);
}

#[test]
fn invalid_gaps_and_stale_layout_save_retain_the_same_draft_and_focus() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let (overlay, gap, save) = (modal.overlay, modal.gap, modal.save);
    app.world_mut()
        .get_mut::<EditableText>(gap)
        .unwrap()
        .editor
        .set_text("999");
    app.update();
    assert!(app.world().get::<InteractionDisabled>(save).is_some());
    Change::Save.apply(app.world_mut(), overlay);
    assert!(active(app.world()));
    app.world_mut()
        .get_mut::<EditableText>(gap)
        .unwrap()
        .editor
        .set_text("0");
    Change::Opening.apply(app.world_mut(), overlay);
    focus(app.world_mut(), Some(gap));
    app.world_mut().resource_mut::<LayoutSettings>().gaps = 16;
    Change::Save.apply(app.world_mut(), overlay);
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert_eq!(modal.overlay, overlay);
    assert_eq!(modal.gap, gap);
    assert!(modal.message.contains("Active layout settings changed"));
    assert_eq!(app.world().get::<EditableText>(gap).unwrap().value(), "0");
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(gap));
    assert_eq!(app.world().resource::<LayoutSettings>().gaps, 16);
    Change::Cancel.apply(app.world_mut(), overlay);
    assert!(!active(app.world()));
    assert_eq!(app.world().resource::<LayoutSettings>().gaps, 16);
}

#[test]
fn pending_layout_text_cannot_commit_an_older_field_value() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let (overlay, gap) = (modal.overlay, modal.gap);
    app.world_mut()
        .get_mut::<EditableText>(gap)
        .unwrap()
        .queue_edit(bevy::text::TextEdit::Insert("0".into()));
    Change::Save.apply(app.world_mut(), overlay);
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert!(
        app.world()
            .get::<Text>(modal.status)
            .unwrap()
            .0
            .contains("Finish typing")
    );
    assert_eq!(app.world().resource::<LayoutSettings>().gaps, 8);
    assert!(!directory.path().join("interface.lingua").exists());
}

#[test]
fn percentages_and_exact_fractions_are_bounded_and_normalized() {
    for (value, expected) in [
        ("100%", [1, 1]),
        ("50%", [1, 2]),
        (" 2 / 6 ", [1, 3]),
        ("66.67%", [6667, 10000]),
        ("12,5%", [1, 8]),
        ("0.5", [1, 2]),
        (".25", [1, 4]),
    ] {
        assert_eq!(ratio(value).unwrap(), expected, "{value}");
    }
    for value in [
        "",
        "0",
        "-1/3",
        "1/0",
        "101%",
        "1/2/3",
        "nan",
        "inf",
        "1e4",
        "33.333333%",
        "9999999999999999999999999",
        "0.00000000001",
    ] {
        assert!(ratio(value).is_err(), "{value}");
    }
    assert_eq!(text([1, 3]), "1/3");
    assert_eq!(text([2, 3]), "2/3");
    assert_eq!(text([2, 4]), "50%");
}

#[test]
fn focused_width_summary_shows_actual_size_separately_from_preset_intent() {
    use crate::{
        layout::desktop::{Geometry, Item, ItemKind, PlacementId, Width},
        native::{canvas::CanvasItem, layout::desktop::Managed, workspace::Workspaces},
    };
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let id = PlacementId([1; 16]);
    {
        let mut spaces = app.world_mut().get_mut::<Workspaces>(root).unwrap();
        let model = &mut spaces.entries[0].desktop;
        model.set_mode(crate::host::WorkspaceLayout::Tiled);
        model
            .insert(Item {
                id,
                kind: ItemKind::Application,
                rules: crate::layout::Rules::fixed([650.0, 200.0]),
                free: Geometry {
                    origin: [0.0; 2],
                    size: [650.0, 200.0],
                },
                free_width: None,
                floating: false,
            })
            .unwrap();
        model.set_item_width(id, Width::Fraction(1, 3)).unwrap();
    }
    app.world_mut().spawn((
        Managed {
            id,
            floating: false,
        },
        CanvasItem {
            position: bevy::math::DVec2::ZERO,
            size: Vec2::new(650.0, 200.0),
        },
        ChildOf(root),
    ));
    refresh(app.world_mut());
    let summary = app
        .world_mut()
        .query_filtered::<&Text, With<Summary>>()
        .iter(app.world())
        .next()
        .unwrap();
    assert!(summary.0.contains("650 logical px"));
    assert!(summary.0.contains("preset 1/3"));
}

#[test]
fn native_controls_edit_reorder_and_save_widths_without_losing_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    app.world_mut()
        .resource_mut::<Settings>()
        .set(super::super::Shortcut::FocusLeft, "Mod+F2")
        .unwrap();
    let open = named(&mut app, "Edit window layout");
    activate(&mut app, open);
    assert!(super::super::keyboard_owned(app.world()));
    assert!(!super::super::keyboard_capturing(app.world()));
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let (entry, default, move_step) = (modal.entries[2].up, modal.default, modal.move_step);
    activate(&mut app, entry);
    app.world_mut()
        .get_mut::<EditableText>(default)
        .unwrap()
        .editor
        .set_text("2/3");
    app.world_mut()
        .get_mut::<EditableText>(move_step)
        .unwrap()
        .editor
        .set_text("37");
    app.update();
    let save = app
        .world()
        .resource::<WidthEditor>()
        .0
        .as_ref()
        .unwrap()
        .save;
    activate(&mut app, save);
    assert!(!active(app.world()));
    let saved = app.world().resource::<LayoutSettings>().clone();
    assert_eq!(saved.width_presets, vec![[1, 1], [1, 3], [1, 2], [2, 3]]);
    assert_eq!(saved.default_width, [2, 3]);
    assert_eq!(saved.move_step, 37);
    assert_eq!(
        app.world()
            .resource::<Settings>()
            .text(super::super::Shortcut::FocusLeft),
        "Mod+F2"
    );
    let (mut restarted, _) = fixture(directory.path());
    assert_eq!(restarted.world().resource::<LayoutSettings>(), &saved);
    assert_eq!(
        restarted
            .world()
            .resource::<Settings>()
            .text(super::super::Shortcut::FocusLeft),
        "Mod+F2"
    );
    let open = named(&mut restarted, "Edit window layout");
    activate(&mut restarted, open);
    assert!(active(restarted.world()));
    assert!(app.world().get_entity(root).is_ok());
}

#[test]
fn invalid_values_and_external_revision_errors_preserve_the_draft_and_cancel_is_safe() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let previous = app.world_mut().spawn_empty().id();
    focus(app.world_mut(), Some(previous));
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    let (overlay, field, save, list) = (
        modal.overlay,
        modal.entries[1].input,
        modal.save,
        modal.list,
    );
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("2/2");
    app.update();
    assert!(app.world().get::<InteractionDisabled>(save).is_some());
    assert!(
        candidate(
            app.world(),
            app.world().resource::<WidthEditor>().0.as_ref().unwrap()
        )
        .unwrap_err()
        .contains("repeat")
    );
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("25%");
    app.update();
    assert!(app.world().get::<InteractionDisabled>(save).is_none());
    std::fs::write(directory.path().join("interface.lingua"), "external edit").unwrap();
    focus(app.world_mut(), Some(field));
    app.world_mut().get_mut::<ScrollPosition>(list).unwrap().0.y = 100.0;
    activate(&mut app, save);
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert_eq!(modal.overlay, overlay);
    assert!(modal.message.contains("changed on disk"));
    assert_eq!(
        app.world().get::<EditableText>(field).unwrap().value(),
        "25%"
    );
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().0.y, 100.0);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
    assert_eq!(
        app.world().resource::<LayoutSettings>(),
        &LayoutSettings::default()
    );
    Change::Cancel.apply(app.world_mut(), overlay);
    assert!(!active(app.world()));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(previous));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("interface.lingua")).unwrap(),
        "external edit"
    );
}

#[test]
fn adding_removing_and_resetting_presets_preserves_controls_and_keeps_one_entry() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let overlay = app
        .world()
        .resource::<WidthEditor>()
        .0
        .as_ref()
        .unwrap()
        .overlay;
    Change::Add(Preset::Width).apply(app.world_mut(), overlay);
    assert!(
        candidate(
            app.world(),
            app.world().resource::<WidthEditor>().0.as_ref().unwrap()
        )
        .is_err()
    );
    for _ in 0..4 {
        let row = app
            .world()
            .resource::<WidthEditor>()
            .0
            .as_ref()
            .unwrap()
            .entries[0]
            .row;
        Change::Remove(row).apply(app.world_mut(), row);
    }
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert_eq!(modal.entries.len(), 1);
    let row = modal.entries[0].row;
    assert!(
        app.world()
            .get::<InteractionDisabled>(modal.entries[0].remove)
            .is_some()
    );
    Change::Remove(row).apply(app.world_mut(), row);
    assert_eq!(
        app.world()
            .resource::<WidthEditor>()
            .0
            .as_ref()
            .unwrap()
            .entries
            .len(),
        1
    );
    Change::Reset(Preset::Width).apply(app.world_mut(), overlay);
    app.update();
    let modal = app.world().resource::<WidthEditor>().0.as_ref().unwrap();
    assert_eq!(
        candidate(app.world(), modal).unwrap(),
        LayoutSettings::default()
    );
    assert_eq!(modal.overlay, overlay);
}

#[test]
fn modal_ownership_suppresses_native_desktop_actions_until_held_keys_are_released() {
    use bevy::{
        input::{
            ButtonState, InputSystems,
            keyboard::{Key, KeyboardInput},
        },
        input_focus::{InputFocusSystems, dispatch_focused_input},
        window::PrimaryWindow,
    };
    #[derive(Resource, Default)]
    struct Calls(usize);
    struct Call;
    impl Action for Call {
        fn apply(&self, world: &mut World, _: Entity) {
            world.resource_mut::<Calls>().0 += 1;
        }
    }
    fn emit(app: &mut App, window: Entity, key_code: KeyCode, pressed: bool, repeat: bool) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: None,
            repeat,
            window,
        });
        app.update();
    }
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    app.init_resource::<Calls>().add_systems(
        PreUpdate,
        dispatch_focused_input::<KeyboardInput>
            .in_set(InputFocusSystems::Dispatch)
            .after(InputSystems),
    );
    let mut binding = KeyBinding::new(
        KeyCode::KeyR,
        Modifiers {
            super_key: true,
            ..Modifiers::NONE
        },
        crate::native::actions![Call],
    );
    binding.repeat = true;
    app.world_mut().entity_mut(root).insert((
        crate::native::shortcuts::ConfiguredBindings(vec![binding]),
        KeyBindings(vec![KeyBinding::new(
            KeyCode::KeyP,
            Modifiers {
                super_key: true,
                ..Modifiers::NONE
            },
            crate::native::actions![Call],
        )]),
    ));
    let window = app
        .world_mut()
        .spawn((
            Window::default(),
            PrimaryWindow,
            crate::native::actions::WindowActionTarget(root),
        ))
        .id();
    Change::Open.apply(app.world_mut(), root);
    app.update();
    emit(&mut app, window, KeyCode::SuperLeft, true, false);
    emit(&mut app, window, KeyCode::KeyP, true, false);
    emit(&mut app, window, KeyCode::KeyP, false, false);
    emit(&mut app, window, KeyCode::KeyR, true, false);
    assert_eq!(app.world().resource::<Calls>().0, 0);
    Change::Cancel.apply(app.world_mut(), root);
    emit(&mut app, window, KeyCode::KeyR, true, true);
    assert_eq!(app.world().resource::<Calls>().0, 0);
    emit(&mut app, window, KeyCode::KeyR, false, false);
    emit(&mut app, window, KeyCode::SuperLeft, false, false);
    emit(&mut app, window, KeyCode::SuperLeft, true, false);
    emit(&mut app, window, KeyCode::KeyR, true, false);
    assert_eq!(app.world().resource::<Calls>().0, 1);
}

#[test]
fn plain_escape_cancels_a_modal_from_its_text_field_without_saving() {
    use bevy::input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    };
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let previous = app.world_mut().spawn_empty().id();
    focus(app.world_mut(), Some(previous));
    Change::Open.apply(app.world_mut(), root);
    app.update();
    let input = app
        .world()
        .resource::<WidthEditor>()
        .0
        .as_ref()
        .unwrap()
        .entries[0]
        .input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("25%");
    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut().write_message(KeyboardInput {
        window,
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        text: None,
        repeat: false,
        state: ButtonState::Pressed,
    });
    app.update();
    assert!(!active(app.world()));
    assert!(!super::super::keyboard_owned(app.world()));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(previous));
    assert_eq!(
        app.world().resource::<LayoutSettings>(),
        &LayoutSettings::default()
    );
    assert!(!directory.path().join("interface.lingua").exists());
}
