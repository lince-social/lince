use super::*;
use crate::actions::Action;
use bevy::text::{FontCx, LayoutCx, TextEdit};

mod basics;

#[test]
fn editor_schedule_runs_with_the_action_and_file_services() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::input::InputPlugin,
        crate::actions::ActionsPlugin,
        crate::file_explorer::FileExplorerPlugin,
        IdePlugin,
    ));
    app.update();
}

fn fixture(value: &str) -> (World, Entity, Entity, tempfile::TempDir, PathBuf) {
    fixture_in(World::new(), value)
}

fn fixture_in(
    mut world: World,
    value: &str,
) -> (World, Entity, Entity, tempfile::TempDir, PathBuf) {
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<crate::tokens::ThemeSettings>();
    world.init_resource::<Documents>();
    world.init_resource::<FontCx>();
    world.init_resource::<LayoutCx>();
    world.init_resource::<bevy::input_focus::InputFocus>();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, value).unwrap();
    let scope = Scope::open(dir.path()).unwrap();
    let file = scope.bind(&path).unwrap();
    let disk = file.read().unwrap();
    world.resource_mut::<Documents>().0.insert(
        path.clone(),
        Document {
            preview: None,
            buffer: Buffer::new(value).unwrap(),
            file: Some(Arc::new(file)),
            disk,
            reading: false,
            saving: None,
            refresh: false,
            moving: false,
            error: None,
        },
    );
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    let owner = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        Ide {
            explorer: crate::file_explorer::FileExplorer {
                roots: vec![dir.path().into()],
                ..default()
            },
            paths: vec![path.clone()],
            active: Some(path.clone()),
            ..default()
        },
    );
    world.get_mut::<View>(owner).unwrap().restore = false;
    let viewport = world.get::<View>(owner).unwrap().viewport;
    world.get_mut::<ComputedNode>(viewport).unwrap().size = Vec2::new(650.0, 500.0);
    runtime::render(&mut world, owner);
    (world, root, owner, dir, path)
}

#[test]
fn disk_reconciliation_and_saving_complete_through_the_file_service() {
    let (mut world, _, owner, _dir, path) = fixture("alpha\nbeta\n");
    world.init_resource::<crate::file_explorer::worker::Worker>();
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..5,
            text: "local".into(),
        })
        .unwrap();
    std::fs::write(&path, "alpha\nexternal\n").unwrap();
    disk_changed(
        &mut world,
        lince_editor::watch::Changes {
            paths: BTreeSet::from([path.clone()]),
            ..default()
        },
    );
    runtime::update(&mut world);
    wait_for_files(&mut world, &path);
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "local\nexternal\n"
    );
    runtime::render(&mut world, owner);
    actions::Control::Save.apply(&mut world, owner);
    wait_for_files(&mut world, &path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "local\nexternal\n");
    assert!(!world.resource::<Documents>().0[&path].buffer.is_dirty());
}

fn wait_for_files(world: &mut World, path: &PathBuf) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        crate::file_explorer::worker::poll(world);
        let doc = &world.resource::<Documents>().0[path];
        if !doc.reading && doc.saving.is_none() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "file service timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn idle_frames_do_not_mark_the_text_or_its_layout_changed() {
    let (mut world, _, owner, _dir, _) = fixture("idle text\n");
    let view = world.get::<View>(owner).unwrap();
    let (editor, content) = (view.editor, view.content);
    world.clear_trackers();
    assert!(
        !world
            .entity(editor)
            .get_ref::<EditableText>()
            .unwrap()
            .is_changed(),
        "initially changed"
    );
    editing::prepare(&mut world);
    assert!(
        !world
            .entity(editor)
            .get_ref::<EditableText>()
            .unwrap()
            .is_changed(),
        "prepare changed text"
    );
    editing::capture(&mut world);
    assert!(
        !world
            .entity(editor)
            .get_ref::<EditableText>()
            .unwrap()
            .is_changed(),
        "capture changed text"
    );
    let view = world.get::<View>(owner).unwrap();
    let path = view.path.as_ref().unwrap();
    assert_eq!(
        world
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        view.window.as_ref().unwrap().text
    );
    assert_eq!(
        view.window.as_ref().unwrap().revision,
        world.resource::<Documents>().0[path].buffer.revision()
    );
    assert_eq!(view.window.as_ref().unwrap().first_line, 0);
    assert_eq!(
        view.window.as_ref().unwrap().total_lines,
        world.resource::<Documents>().0[path]
            .buffer
            .snapshot()
            .len_lines()
    );
    runtime::render(&mut world, owner);
    assert!(
        !world
            .entity(editor)
            .get_ref::<EditableText>()
            .unwrap()
            .is_changed()
    );
    assert!(
        !world
            .entity(content)
            .get_ref::<Node>()
            .unwrap()
            .is_changed()
    );
}

#[test]
fn returning_to_the_start_reveals_the_caret_after_horizontal_scrolling() {
    let (mut world, _, owner, _dir, _) = fixture(&"x".repeat(500));
    let view = world.get::<View>(owner).unwrap();
    let (editor, viewport) = (view.editor, view.viewport);
    world.get_mut::<ScrollPosition>(viewport).unwrap().x = 400.0;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::TextStart(false));
    editing::prepare(&mut world);
    assert_eq!(world.get::<View>(owner).unwrap().selection, [0, 0]);
    assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().x, 0.0);
}

#[test]
fn bounded_editor_edits_the_correct_part_of_a_large_file() {
    let value = "unchanged line\n".repeat(30_000);
    let (mut world, _, owner, _dir, path) = fixture(&value);
    let viewport = world.get::<View>(owner).unwrap().viewport;
    world.get_mut::<ScrollPosition>(viewport).unwrap().y = 20_000.0 * LINE_HEIGHT;
    runtime::render(&mut world, owner);
    let view = world.get::<View>(owner).unwrap();
    let editor = view.editor;
    let window = view.window.clone().unwrap();
    assert!(window.text.len() < lince_editor::MAX_WINDOW_BYTES);
    assert!(window.start > 200_000);
    assert_eq!(
        world.get::<Node>(editor).unwrap().height,
        px(window.text.split('\n').count() as f32 * LINE_HEIGHT)
    );
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(&format!("typed\n{}", window.text));
    editing::capture_one(&mut world, owner);
    let buffer = &world.resource::<Documents>().0[&path].buffer;
    assert!(buffer.is_dirty());
    assert!(
        buffer
            .snapshot()
            .slice(window.start..)
            .to_string()
            .starts_with("typed\n")
    );
    assert_eq!(
        buffer.snapshot().slice(..window.start).to_string(),
        value.chars().take(window.start).collect::<String>()
    );
}

#[test]
fn typing_a_long_line_expands_the_scrollable_content_without_replacing_the_widget() {
    let (mut world, _, owner, _dir, _) = fixture("short\n");
    let view = world.get::<View>(owner).unwrap();
    let (editor, content) = (view.editor, view.content);
    let value = format!("{}\n", "x".repeat(300));
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(&value);
    editing::capture_one(&mut world, owner);
    world.clear_trackers();
    runtime::render(&mut world, owner);
    assert_eq!(world.get::<Node>(content).unwrap().width, px(5120));
    assert_eq!(world.get::<Node>(editor).unwrap().width, px(5056));
    assert!(
        !world
            .entity(editor)
            .get_ref::<EditableText>()
            .unwrap()
            .is_changed()
    );
}

#[test]
fn text_projection_tracks_the_visible_height_when_the_viewport_resizes() {
    let (mut world, _, owner, _dir, path) = fixture(&"line\n".repeat(1000));
    let view = world.get::<View>(owner).unwrap();
    let viewport = view.viewport;
    let first_end = view.window.as_ref().unwrap().end;
    assert!(first_end < 200);
    world.get_mut::<ComputedNode>(viewport).unwrap().size.y = 1100.0;
    runtime::render(&mut world, owner);
    assert!(
        world
            .get::<View>(owner)
            .unwrap()
            .window
            .as_ref()
            .unwrap()
            .end
            > first_end
    );
    assert!(!world.resource::<Documents>().0[&path].buffer.is_dirty());
}

#[test]
fn select_all_reaches_beyond_the_visible_window_and_replace_undoes() {
    let value = "line\n".repeat(10_000);
    let (mut world, _, owner, _dir, path) = fixture(&value);
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::SelectAll);
    editing::prepare(&mut world);
    editing::capture(&mut world);
    assert_eq!(
        world.get::<View>(owner).unwrap().selection,
        [0, value.chars().count()]
    );
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::Insert("replacement".into()));
    editing::prepare(&mut world);
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "replacement"
    );
    actions::Control::Undo(false).apply(&mut world, owner);
    assert_eq!(world.resource::<Documents>().0[&path].buffer.text(), value);
}

#[test]
fn ime_commit_can_replace_a_selection_larger_than_the_viewport() {
    let (mut world, _, owner, _dir, path) = fixture(&"line\n".repeat(1000));
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::SelectAll);
    editing::prepare(&mut world);
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .queue_edit(TextEdit::ImeCommit {
            value: "猫".into()
        });
    editing::prepare(&mut world);
    assert_eq!(world.resource::<Documents>().0[&path].buffer.text(), "猫");
}

#[test]
fn unsaved_edits_block_close_and_discard_needs_a_second_choice() {
    let (mut world, root, owner, _dir, path) = fixture("original");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..8,
            text: "unsaved".into(),
        })
        .unwrap();
    runtime::render(&mut world, owner);
    assert!(protect(&mut world, Some(root)));
    actions::Control::Close.apply(&mut world, owner);
    assert!(world.get::<Ide>(owner).unwrap().active.is_some());
    actions::Control::Discard.apply(&mut world, owner);
    assert!(world.resource::<Documents>().0.contains_key(&path));
    actions::Control::Discard.apply(&mut world, owner);
    assert!(!world.resource::<Documents>().0.contains_key(&path));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}

#[test]
fn stale_visible_drafts_are_retained_and_cannot_be_saved_or_closed() {
    let (mut world, root, owner, _dir, path) = fixture("original");
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text("my draft");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .reconcile("on disk")
        .unwrap();
    editing::capture_one(&mut world, owner);
    runtime::render(&mut world, owner);
    assert_eq!(
        world
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "my draft"
    );
    assert!(protect(&mut world, Some(root)));
    actions::Control::Close.apply(&mut world, owner);
    assert!(world.get::<Ide>(owner).unwrap().active.is_some());
}

#[test]
fn workspace_persistence_stores_paths_and_roots_without_file_contents() {
    let (mut world, root, owner, _dir, path) = fixture("PRIVATE FILE CONTENT");
    let saved = snapshot(&mut world, root).remove(0);
    assert!(saved.valid());
    let json = serde_json::to_string(&saved).unwrap();
    assert!(!json.contains("PRIVATE FILE CONTENT"));
    world.despawn(owner);
    let saved: SavedIde = serde_json::from_str(&json).unwrap();
    saved.restore(&mut world, root);
    let ide = world.query::<&Ide>().single(&world).unwrap();
    assert_eq!(ide.paths, vec![path]);
    assert_eq!(ide.explorer.roots.len(), 1);
}

fn enable_recovery(world: &mut World, directory: &std::path::Path) {
    world.init_resource::<crate::file_explorer::worker::Worker>();
    world.init_resource::<Messages<AppExit>>();
    world.init_resource::<recovery::Recovery>();
    world.insert_resource(crate::workspace::WorkspaceFile::new(
        directory.join("interface.json"),
    ));
}

fn settle_recovery(world: &mut World) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        runtime::update(world);
        crate::file_explorer::worker::poll(world);
        if recovery::protect_exit(world) == Some(false) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "recovery did not settle: {:?}",
            recovery::error(world)
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn restart_restores_drafts_and_merges_later_disk_changes() {
    let (mut world, _, _, directory, path) = fixture("alpha\nbeta\n");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..5,
            text: "local".into(),
        })
        .unwrap();
    enable_recovery(&mut world, directory.path());
    settle_recovery(&mut world);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "alpha\nbeta\n");
    drop(world);
    std::fs::write(&path, "alpha\nexternal\n").unwrap();
    let (mut world, _, owner, _other, _) = fixture("unused");
    world.resource_mut::<Documents>().0.clear();
    world.get_mut::<Ide>(owner).unwrap().paths.clear();
    world.get_mut::<Ide>(owner).unwrap().active = None;
    enable_recovery(&mut world, directory.path());
    settle_recovery(&mut world);
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "local\nexternal\n"
    );
    assert!(world.get::<Ide>(owner).unwrap().paths.contains(&path));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "alpha\nexternal\n");
}

#[test]
fn retained_visible_drafts_are_checkpointed_separately() {
    let (mut world, _, owner, directory, path) = fixture("original");
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text("retained local text");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .reconcile("new disk text")
        .unwrap();
    editing::capture_one(&mut world, owner);
    assert!(world.get::<View>(owner).unwrap().draft);
    enable_recovery(&mut world, directory.path());
    settle_recovery(&mut world);
    drop(world);
    let store =
        lince_editor::recovery::Store::open(&directory.path().join("editor-drafts")).unwrap();
    let (drafts, errors) = store.load().unwrap();
    assert!(errors.is_empty());
    assert!(drafts.iter().any(|draft| draft.detached
        && draft.checkpoint.restore().unwrap().text() == "retained local text"));
}

#[test]
fn missing_sources_can_be_saved_to_a_new_path_without_overwriting_files() {
    let (mut world, _, owner, directory, path) = fixture("alpha\n");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..5,
            text: "recovered".into(),
        })
        .unwrap();
    enable_recovery(&mut world, directory.path());
    settle_recovery(&mut world);
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .file = None;
    std::fs::remove_file(&path).unwrap();
    let destination = directory.path().join("copy.txt");
    actions::save_copy(&mut world, owner, path.clone(), destination.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !world.resource::<Documents>().0.contains_key(&destination) {
        crate::file_explorer::worker::poll(&mut world);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&destination).unwrap(),
        "recovered\n"
    );
    assert!(!path.exists());
    assert_eq!(world.get::<Ide>(owner).unwrap().active, Some(destination));
    settle_recovery(&mut world);
    drop(world);
    let store =
        lince_editor::recovery::Store::open(&directory.path().join("editor-drafts")).unwrap();
    assert!(store.load().unwrap().0.is_empty());
}

#[test]
fn moving_a_dirty_file_keeps_its_buffer_and_rebinds_all_tabs() {
    let (mut world, root, owner, directory, path) = fixture("original");
    let config = world.get::<Ide>(owner).unwrap().clone();
    let second = spawn(&mut world, root, 1, DVec2::ZERO, config);
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..0,
            text: "local ".into(),
        })
        .unwrap();
    assert!(reserve_change(&mut world, &path, true).is_err());
    assert_eq!(
        reserve_change(&mut world, &path, false).unwrap(),
        vec![path.clone()]
    );
    let next = directory.path().join("renamed.txt");
    let scope = Scope::open(directory.path()).unwrap();
    scope
        .move_entry(&scope.entry(&path).unwrap(), &next)
        .unwrap();
    let bindings = BTreeMap::from([(path.clone(), Some(Arc::new(scope.bind(&next).unwrap())))]);
    finish_change(&mut world, &path, Some(&next), bindings, true);
    assert_eq!(world.get::<Ide>(owner).unwrap().active, Some(next.clone()));
    assert_eq!(world.get::<Ide>(second).unwrap().active, Some(next.clone()));
    assert_eq!(world.get::<Ide>(second).unwrap().paths, vec![next.clone()]);
    assert!(!world.resource::<Documents>().0.contains_key(&path));
    assert_eq!(
        world.resource::<Documents>().0[&next].buffer.text(),
        "local original"
    );
    assert_eq!(
        world.resource::<Documents>().0[&next]
            .file
            .as_ref()
            .unwrap()
            .path,
        next
    );
    assert!(world.resource::<Documents>().0[&next].buffer.is_dirty());
}

#[test]
fn reopening_the_same_path_does_not_reuse_an_older_recovery_revision() {
    let (mut world, _, owner, directory, path) = fixture("base");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..4,
            text: "old draft".into(),
        })
        .unwrap();
    enable_recovery(&mut world, directory.path());
    settle_recovery(&mut world);
    let mut replacement = Buffer::new("base").unwrap();
    replacement
        .edit(lince_editor::Edit {
            range: 0..4,
            text: "new draft".into(),
        })
        .unwrap();
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer = replacement;
    runtime::render(&mut world, owner);
    let editor = world.get::<View>(owner).unwrap().editor;
    assert_eq!(
        world
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "new draft"
    );
    settle_recovery(&mut world);
    drop(world);
    let store =
        lince_editor::recovery::Store::open(&directory.path().join("editor-drafts")).unwrap();
    assert_eq!(
        store.load().unwrap().0[0]
            .checkpoint
            .restore()
            .unwrap()
            .text(),
        "new draft"
    );
}

#[test]
fn failed_checkpoints_cancel_closing_and_retry_does_not_close_the_window() {
    let (mut world, _, owner, directory, path) = fixture("base");
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..4,
            text: "first draft".into(),
        })
        .unwrap();
    enable_recovery(&mut world, directory.path());
    settle_recovery(&mut world);
    world.resource_mut::<Messages<AppExit>>().clear();
    let checkpoint = std::fs::read_dir(directory.path().join("editor-drafts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "draft")
        })
        .unwrap();
    std::fs::remove_file(&checkpoint).unwrap();
    std::fs::create_dir(&checkpoint).unwrap();
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(lince_editor::Edit {
            range: 0..11,
            text: "second draft".into(),
        })
        .unwrap();
    runtime::render(&mut world, owner);
    assert_eq!(recovery::protect_exit(&mut world), Some(true));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while recovery::error(&world).is_none() {
        runtime::update(&mut world);
        crate::file_explorer::worker::poll(&mut world);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(recovery::error(&world).unwrap().contains("close canceled"));
    assert!(world.resource::<Messages<AppExit>>().is_empty());
    std::fs::remove_dir(&checkpoint).unwrap();
    recovery::retry(&mut world);
    while !recovery::settled(&world) {
        runtime::update(&mut world);
        crate::file_explorer::worker::poll(&mut world);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    runtime::update(&mut world);
    assert!(world.resource::<Messages<AppExit>>().is_empty());
    assert!(checkpoint.is_file());
}
