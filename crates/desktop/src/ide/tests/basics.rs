use super::*;
use std::time::{Duration, Instant};

#[test]
fn japanese_word_navigation_and_deletion_follow_pending_typing() {
    let (mut world, _, owner, _dir, path) = fixture("こんにちは世界");
    let editor = world.get::<View>(owner).unwrap().editor;
    editing::select(&mut world, owner, [0, 0], 0);
    runtime::render(&mut world, owner);
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::WordRight(false));
    editing::prepare(&mut world);
    assert_eq!(world.get::<View>(owner).unwrap().selection, [5, 5]);
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::DeleteWord);
    editing::prepare(&mut world);
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "こんにちは"
    );
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::Insert("world".into()));
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::BackspaceWord);
    editing::prepare(&mut world);
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "こんにちは"
    );
}

fn drain(world: &mut World, ready: impl Fn(&World) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        crate::file_explorer::worker::poll(world);
        if ready(world) {
            return;
        }
        assert!(Instant::now() < until, "File work did not complete");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn input(world: &mut World, entity: Entity, text: &str) {
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(text);
}

#[test]
fn keyboard_tab_indents_and_control_f_focuses_find() {
    use bevy::input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    };
    use bevy::input_focus::{FocusCause, InputDispatchPlugin, InputFocus};
    let mut app = App::new();
    let (mut world, _, owner, _dir, path) = fixture_in(std::mem::take(app.world_mut()), "one\n");
    let editor = world.get::<View>(owner).unwrap().editor;
    editing::select(&mut world, owner, [0, 3], 0);
    runtime::render(&mut world, owner);
    let window = world
        .spawn((Window::default(), bevy::window::PrimaryWindow))
        .id();
    *app.world_mut() = world;
    app.add_plugins((
        MinimalPlugins,
        bevy::input::InputPlugin,
        InputDispatchPlugin,
        crate::actions::ActionsPlugin,
    ))
    .add_systems(
        PostUpdate,
        editing::prepare.before(crate::actions::ApplyActions),
    );
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Navigated);
    app.world_mut().write_message(KeyboardInput {
        window,
        state: ButtonState::Pressed,
        key_code: KeyCode::Tab,
        logical_key: Key::Tab,
        text: None,
        repeat: false,
    });
    app.update();
    assert_eq!(
        app.world().resource::<Documents>().0[&path].buffer.text(),
        "    one\n"
    );
    for (key_code, logical_key, state) in [
        (KeyCode::Tab, Key::Tab, ButtonState::Released),
        (KeyCode::ControlLeft, Key::Control, ButtonState::Pressed),
        (
            KeyCode::KeyF,
            Key::Character("f".into()),
            ButtonState::Pressed,
        ),
    ] {
        app.world_mut().write_message(KeyboardInput {
            window,
            state,
            key_code,
            logical_key,
            text: None,
            repeat: false,
        });
    }
    app.update();
    let world = app.world();
    assert!(world.get::<Ide>(owner).unwrap().settings.search_visible);
    assert_eq!(
        world.resource::<InputFocus>().get(),
        Some(world.get::<View>(owner).unwrap().find)
    );
}

#[test]
fn tabs_keep_positions_and_transform_inactive_cursors_through_edits() {
    let (mut world, _, owner, dir, path) = fixture(&"line\n".repeat(200));
    world.init_resource::<crate::file_explorer::worker::Worker>();
    let other = dir.path().join("other.txt");
    std::fs::write(&other, "other").unwrap();
    editing::select(&mut world, owner, [60, 64], 12);
    runtime::render(&mut world, owner);
    let viewport = world.get::<View>(owner).unwrap().viewport;
    *world.get_mut::<ScrollPosition>(viewport).unwrap() = ScrollPosition(Vec2::new(20.0, 240.0));
    open(
        &mut world,
        owner,
        Scope::open(dir.path()).unwrap(),
        other.clone(),
    );
    drain(&mut world, |world| {
        world.resource::<Documents>().0.contains_key(&other)
    });
    runtime::render(&mut world, owner);
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..0,
            text: "new\n".into(),
        })
        .unwrap();
    assert_eq!(tabs::saved(&world, owner)[&path].selection, [64, 68]);
    actions::Control::Tab(path).apply(&mut world, owner);
    runtime::render(&mut world, owner);
    assert_eq!(world.get::<View>(owner).unwrap().selection, [64, 68]);
    let scroll = world.get::<ScrollPosition>(viewport).unwrap();
    assert_eq!((scroll.x, scroll.y), (20.0, 240.0));
}

#[test]
fn project_search_rejects_changed_queries_and_opens_a_matching_line() {
    let (mut world, _, owner, dir, _) = fixture("base\n");
    world.init_resource::<crate::file_explorer::worker::Worker>();
    let path = dir.path().join("other.txt");
    std::fs::write(&path, "first\n猫 needle\n").unwrap();
    world.get_mut::<Ide>(owner).unwrap().settings.project_search = true;
    let find = world.get::<View>(owner).unwrap().find;
    input(&mut world, find, "needle");
    actions::Control::ProjectSearch.apply(&mut world, owner);
    input(&mut world, find, "changed");
    drain(&mut world, |world| {
        world.get::<View>(owner).unwrap().notice == "Search changed; run it again"
    });
    input(&mut world, find, "needle");
    actions::Control::ProjectSearch.apply(&mut world, owner);
    drain(&mut world, |world| {
        world
            .get::<View>(owner)
            .unwrap()
            .notice
            .starts_with("1 matching lines")
    });
    project::render(&mut world, owner);
    let button = world
        .query::<(&crate::icons::Tooltip, &crate::actions::ActionButton)>()
        .iter(&world)
        .find(|(tip, action)| action.target == owner && tip.0.starts_with("other.txt:2"))
        .unwrap()
        .1
        .clone();
    button.actions.run(&mut world, owner);
    drain(&mut world, |world| {
        world.resource::<Documents>().0.contains_key(&path)
    });
    runtime::render(&mut world, owner);
    project::render(&mut world, owner);
    assert_eq!(world.get::<View>(owner).unwrap().selection, [8, 8]);
    assert_eq!(
        world.get::<Ide>(owner).unwrap().active.as_ref(),
        Some(&path)
    );
}

#[test]
fn find_and_replace_all_are_background_operations_with_one_undo() {
    let (mut world, _, owner, _dir, path) = fixture("猫 café CAFÉ caféine\n");
    world.init_resource::<crate::file_explorer::worker::Worker>();
    let view = world.get::<View>(owner).unwrap();
    let (find, replacement) = (view.find, view.replace);
    input(&mut world, find, "CAFÉ");
    input(&mut world, replacement, "茶");
    world.get_mut::<Ide>(owner).unwrap().settings.search = lince_editor::search::Options {
        case_sensitive: false,
        whole_word: true,
    };
    actions::Control::ReplaceAll.apply(&mut world, owner);
    drain(&mut world, |world| {
        world.get::<View>(owner).unwrap().notice != "Searching…"
    });
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "猫 茶 茶 caféine\n"
    );
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .undo(false)
        .unwrap();
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "猫 café CAFÉ caféine\n"
    );
    runtime::render(&mut world, owner);
    actions::Control::Find.apply(&mut world, owner);
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..0,
            text: "x".into(),
        })
        .unwrap();
    drain(&mut world, |world| {
        world.get::<View>(owner).unwrap().notice != "Searching…"
    });
    assert!(world.get::<View>(owner).unwrap().notice.contains("changed"));
}

#[test]
fn autosave_is_opt_in_debounced_and_stops_at_a_conflict() {
    let (mut world, _, owner, _dir, path) = fixture("base\n");
    world.init_resource::<crate::file_explorer::worker::Worker>();
    let now = Instant::now();
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..0,
            text: "a".into(),
        })
        .unwrap();
    autosave::update(&mut world, now);
    assert!(world.resource::<Documents>().0[&path].saving.is_none());
    world
        .get_mut::<Ide>(owner)
        .unwrap()
        .settings
        .autosave_seconds = 2;
    autosave::update(&mut world, now);
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..0,
            text: "b".into(),
        })
        .unwrap();
    autosave::update(&mut world, now + Duration::from_secs(1));
    autosave::update(&mut world, now + Duration::from_millis(2100));
    assert!(world.resource::<Documents>().0[&path].saving.is_none());
    autosave::update(&mut world, now + Duration::from_millis(3100));
    wait_for_files(&mut world, &path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "babase\n");
    let mut docs = world.resource_mut::<Documents>();
    let buffer = &mut docs.0.get_mut(&path).unwrap().buffer;
    buffer
        .edit(lince_editor::Edit {
            range: 0..6,
            text: "local".into(),
        })
        .unwrap();
    buffer.reconcile("external\n").unwrap();
    std::fs::write(&path, "external\n").unwrap();
    autosave::update(&mut world, now + Duration::from_secs(4));
    autosave::update(&mut world, now + Duration::from_secs(10));
    assert!(world.resource::<Documents>().0[&path].saving.is_none());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external\n");
}

#[test]
fn closing_can_cancel_or_save_and_close_without_discarding_the_draft() {
    let (mut world, _, owner, _dir, path) = fixture("base");
    world.init_resource::<crate::file_explorer::worker::Worker>();
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..0,
            text: "new ".into(),
        })
        .unwrap();
    runtime::render(&mut world, owner);
    actions::Control::Close.apply(&mut world, owner);
    assert_eq!(
        world.get::<View>(owner).unwrap().closing,
        Some(path.clone())
    );
    actions::Control::CancelClose.apply(&mut world, owner);
    assert!(world.get::<Ide>(owner).unwrap().paths.contains(&path));
    actions::Control::Close.apply(&mut world, owner);
    actions::Control::SaveClose.apply(&mut world, owner);
    wait_for_files(&mut world, &path);
    closing::update(&mut world, owner);
    assert!(!world.get::<Ide>(owner).unwrap().paths.contains(&path));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "new base");
}

#[test]
fn preview_discards_mutating_input_and_never_saves_its_contents() {
    let (mut world, _, owner, dir, path) = fixture("preview");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .preview = Some("Large file".into());
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::Insert("bad".into()));
    editing::prepare(&mut world);
    assert!(
        world
            .get::<EditableText>(editor)
            .unwrap()
            .pending_edits
            .is_empty()
    );
    actions::Control::Indent(false).apply(&mut world, owner);
    actions::save_copy(&mut world, owner, path.clone(), dir.path().join("copy.txt"));
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "preview"
    );
    assert!(!dir.path().join("copy.txt").exists());
}

#[test]
fn settings_and_tab_positions_round_trip_with_the_castle() {
    let (mut world, root, owner, _dir, path) = fixture("hello world");
    let settings = Settings {
        autosave_seconds: 5,
        project_search: true,
        search_visible: true,
        sidebar_width: 320.0,
        ..default()
    };
    world.get_mut::<Ide>(owner).unwrap().settings = settings;
    editing::select(&mut world, owner, [2, 5], 0);
    runtime::render(&mut world, owner);
    let saved = snapshot(&mut world, root).pop().unwrap();
    let bytes = serde_json::to_vec(&saved).unwrap();
    let saved: SavedIde = serde_json::from_slice(&bytes).unwrap();
    assert!(saved.valid());
    world.entity_mut(owner).despawn();
    saved.restore(&mut world, root);
    let (owner, ide) = world.query::<(Entity, &Ide)>().single(&world).unwrap();
    assert_eq!(ide.settings, settings);
    assert_eq!(
        world.get::<View>(owner).unwrap().positions[&path]
            .saved
            .selection,
        [2, 5]
    );
}
#[test]
fn save_as_replacement_waits_for_confirmation_and_rejects_disk_races() {
    let (mut world, _, owner, dir, path) = fixture("source\n");
    world.init_resource::<crate::file_explorer::worker::Worker>();
    let destination = dir.path().join("existing.txt");
    std::fs::write(&destination, "destination\n").unwrap();
    actions::save_copy(&mut world, owner, path.clone(), destination.clone());
    drain(&mut world, |world| {
        world.get::<View>(owner).unwrap().replacement.is_some()
    });
    assert_eq!(
        std::fs::read_to_string(&destination).unwrap(),
        "destination\n"
    );
    std::fs::write(&destination, "external\n").unwrap();
    save_as::confirm(&mut world, owner, true);
    drain(&mut world, |world| {
        world.resource::<Documents>().0[&path].saving.is_none()
    });
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "external\n");
    actions::save_copy(&mut world, owner, path, destination.clone());
    drain(&mut world, |world| {
        world.get::<View>(owner).unwrap().replacement.is_some()
    });
    save_as::confirm(&mut world, owner, true);
    drain(&mut world, |world| {
        world.get::<Ide>(owner).unwrap().active.as_ref() == Some(&destination)
    });
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "source\n");
}
