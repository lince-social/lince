use super::*;
use crate::actions::Action;
use bevy::text::EditableText;

fn fixture() -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<crate::tokens::ThemeSettings>();
    world.init_resource::<worker::Worker>();
    world.init_resource::<runtime::Watching>();
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    (world, root)
}

#[test]
fn explorer_keyboard_selects_without_opening_and_scrolls_to_the_selection() {
    use bevy::input_focus::{FocusCause, InputFocus};
    let (mut world, root) = fixture();
    world.init_resource::<InputFocus>();
    world.init_resource::<ButtonInput<KeyCode>>();
    let owner = spawn(&mut world, root, 1, DVec2::ZERO, FileExplorer::default());
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.restore = false;
    view.dirty = false;
    view.rows = (0..100)
        .map(|n| Row {
            entry: Entry {
                path: PathBuf::from(format!("/root/file{n}")),
                directory: false,
                link: false,
            },
            root: PathBuf::from("/root"),
            depth: 1,
        })
        .collect();
    let viewport = view.viewport;
    world
        .resource_mut::<InputFocus>()
        .set(viewport, FocusCause::Navigated);
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    navigation::keyboard(&mut world);
    assert_eq!(
        world
            .get::<View>(owner)
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .entry
            .path,
        PathBuf::from("/root/file0")
    );
    world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::End);
    navigation::keyboard(&mut world);
    assert_eq!(
        world
            .get::<View>(owner)
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .entry
            .path,
        PathBuf::from("/root/file99")
    );
    assert!(world.get::<ScrollPosition>(viewport).unwrap().y > 0.0);
    assert_eq!(
        world
            .query_filtered::<Entity, With<crate::ide::Ide>>()
            .iter(&world)
            .count(),
        0
    );
}

#[test]
fn returning_focus_invalidates_cached_directory_contents() {
    use bevy::ecs::system::RunSystemOnce;
    let (mut world, root) = fixture();
    let owner = spawn(&mut world, root, 1, DVec2::ZERO, FileExplorer::default());
    world.get_mut::<View>(owner).unwrap().restore = false;
    world.get_mut::<View>(owner).unwrap().cache.insert(
        PathBuf::from("/root"),
        Listing {
            entries: Vec::new(),
            truncated: false,
        },
    );
    world.init_resource::<Messages<bevy::window::WindowFocused>>();
    world.write_message(bevy::window::WindowFocused {
        window: Entity::PLACEHOLDER,
        focused: true,
    });
    world.run_system_once(runtime::focus).unwrap();
    runtime::update(&mut world);
    assert!(world.get::<View>(owner).unwrap().cache.is_empty());
}

#[test]
fn tree_and_grid_only_spawn_the_visible_rows() {
    let (mut world, root) = fixture();
    let owner = spawn(&mut world, root, 1, DVec2::ZERO, FileExplorer::default());
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.restore = false;
    view.dirty = false;
    view.rows = (0..20_000)
        .map(|index| Row {
            entry: Entry {
                path: PathBuf::from(format!("/project/file-{index}")),
                directory: false,
                link: false,
            },
            root: PathBuf::from("/project"),
            depth: 1,
        })
        .collect();
    let (viewport, content) = (view.viewport, view.content);
    world.get_mut::<ComputedNode>(viewport).unwrap().size = Vec2::new(500.0, 600.0);
    runtime::update(&mut world);
    assert!(world.get::<Children>(content).unwrap().len() < 30);
    let count = world.entities().len();
    runtime::update(&mut world);
    assert_eq!(world.entities().len(), count);
    world.get_mut::<ScrollPosition>(viewport).unwrap().y = 150_000.0;
    runtime::update(&mut world);
    assert!(world.get::<Children>(content).unwrap().len() < 30);
    world.get_mut::<FileExplorer>(owner).unwrap().grid = true;
    runtime::update(&mut world);
    assert!(world.get::<Children>(content).unwrap().len() <= 160);
}

#[test]
fn picker_returns_a_path_without_importing_and_preserves_changed_inputs() {
    let (mut world, root) = fixture();
    let input = input(&mut world, root, "Model path", "", 4096);
    let picker = shell(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        "Choose a path",
        Vec2::splat(500.0),
    );
    populate(
        &mut world,
        picker,
        FileExplorer::default(),
        Target::Input {
            entity: input,
            original: String::new(),
            extensions: vec!["glb".into()],
            directories: false,
        },
    );
    let row = Row {
        entry: Entry {
            path: PathBuf::from("/assets/model.glb"),
            directory: false,
            link: false,
        },
        root: PathBuf::from("/assets"),
        depth: 1,
    };
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("typed meanwhile");
    actions::Control::Entry(row.clone()).apply(&mut world, picker);
    assert_eq!(
        world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "typed meanwhile"
    );
    assert!(world.get_entity(picker).is_ok());
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("");
    actions::Control::Entry(row).apply(&mut world, picker);
    assert_eq!(
        world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "/assets/model.glb"
    );
    assert!(world.get_entity(picker).is_err());
}

fn await_operation(world: &mut World, owner: Entity, root: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        runtime::update(world);
        let view = world.get::<View>(owner).unwrap();
        if !view.busy && view.scopes.contains_key(root) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "file operation did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn select_path(
    world: &mut World,
    owner: Entity,
    root: &std::path::Path,
    path: PathBuf,
    directory: bool,
) {
    world.get_mut::<View>(owner).unwrap().selected = Some(Row {
        entry: Entry {
            path,
            directory,
            link: false,
        },
        root: root.into(),
        depth: 1,
    });
}

#[test]
fn explorer_creates_renames_moves_and_restores_deleted_items() {
    let (mut world, root) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let owner = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        FileExplorer {
            roots: vec![directory.path().into()],
            ..default()
        },
    );
    await_operation(&mut world, owner, directory.path());
    select_path(
        &mut world,
        owner,
        directory.path(),
        directory.path().into(),
        true,
    );
    let input = world.get::<View>(owner).unwrap().destination;
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("folder");
    operations::Control::NewFolder.apply(&mut world, owner);
    await_operation(&mut world, owner, directory.path());
    let path = directory.path().join("folder");
    assert!(path.is_dir());
    select_path(&mut world, owner, directory.path(), path, true);
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("renamed");
    operations::Control::Rename.apply(&mut world, owner);
    await_operation(&mut world, owner, directory.path());
    let renamed = directory.path().join("renamed");
    assert!(renamed.is_dir());
    select_path(&mut world, owner, directory.path(), renamed.clone(), true);
    operations::Control::Delete.apply(&mut world, owner);
    await_operation(&mut world, owner, directory.path());
    assert!(
        renamed.is_dir(),
        "the first delete must only ask for confirmation"
    );
    operations::Control::Delete.apply(&mut world, owner);
    await_operation(&mut world, owner, directory.path());
    assert!(!renamed.exists());
    let label = world.get::<View>(owner).unwrap().status;
    assert!(
        world
            .get::<Text>(label)
            .unwrap()
            .0
            .ends_with("Undo delete restores it.")
    );
    operations::Control::UndoDelete.apply(&mut world, owner);
    await_operation(&mut world, owner, directory.path());
    assert!(renamed.is_dir());
    select_path(&mut world, owner, directory.path(), renamed.clone(), true);
    let target = directory.path().join("moved");
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&target.display().to_string());
    operations::Control::Move.apply(&mut world, owner);
    await_operation(&mut world, owner, directory.path());
    assert!(target.is_dir());
    assert!(!renamed.exists());
}
#[test]
fn folder_picking_requires_selection_and_grid_browses_one_directory() {
    let (mut world, root) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("child");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("file.txt"), "text").unwrap();
    let field = input(&mut world, root, "Folder", "", 4096);
    let picker = world.spawn(Node::default()).id();
    populate(
        &mut world,
        picker,
        FileExplorer {
            roots: vec![dir.path().into()],
            grid: true,
            ..default()
        },
        Target::Input {
            entity: field,
            original: String::new(),
            extensions: Vec::new(),
            directories: true,
        },
    );
    await_operation(&mut world, picker, dir.path());
    let row = Row {
        entry: Entry {
            path: child.clone(),
            directory: true,
            link: false,
        },
        root: dir.path().into(),
        depth: 1,
    };
    actions::Control::Entry(row).apply(&mut world, picker);
    assert_eq!(
        world
            .get::<EditableText>(field)
            .unwrap()
            .value()
            .to_string(),
        ""
    );
    assert_eq!(
        world.get::<View>(picker).unwrap().directory.as_ref(),
        Some(&child)
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        runtime::update(&mut world);
        if world.get::<View>(picker).unwrap().rows.len() == 1 {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        world.get::<View>(picker).unwrap().rows[0].entry.path,
        child.join("file.txt")
    );
    actions::Control::Up.apply(&mut world, picker);
    assert_eq!(
        world.get::<View>(picker).unwrap().directory.as_deref(),
        Some(dir.path())
    );
    actions::Control::Back.apply(&mut world, picker);
    assert_eq!(
        world.get::<View>(picker).unwrap().directory.as_ref(),
        Some(&child)
    );
    actions::Control::SelectFolder.apply(&mut world, picker);
    assert_eq!(
        world
            .get::<EditableText>(field)
            .unwrap()
            .value()
            .to_string(),
        child.to_str().unwrap()
    );
    assert!(world.get_entity(picker).is_err());
}
