use super::*;
use crate::native::file_explorer::worker;
use std::{
    fs,
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};

const CHILD_DIRECTORY: &str = "LINCE_PARK_PRIVATE_IDE_CRASH_DIRECTORY";
const SAVED_FILE: &str = "explicit-save.txt";
const DRAFT_FILE: &str = "untouched-disk.txt";
const SAVE_ACK: &str = "save-close-acknowledged";
const ORIGINAL: &str = "Private original\n";
const SAVED: &str = "Explicitly saved café 👩‍💻\nSecond line\n";
const DRAFT: &str = "Dirty adjacent view 界\n";

struct CrashChild(Child);

impl Drop for CrashChild {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn startup(directory: &Path) -> (World, Entity) {
    let mut world = editor_world(World::new());
    enable_recovery(&mut world, directory);
    let root = world
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        &mut world,
        root,
        1,
        DVec2::ZERO,
        Ide {
            explorer: crate::native::file_explorer::FileExplorer {
                roots: vec![directory.into()],
                ..default()
            },
            ..default()
        },
    );
    let viewport = world.get::<View>(owner).unwrap().viewport;
    world.get_mut::<ComputedNode>(viewport).unwrap().size = Vec2::new(650.0, 500.0);
    until(&mut world, recovery::ready);
    assert!(recovery::error(&world).is_none());
    (world, owner)
}

fn until(world: &mut World, done: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker::poll(world);
        runtime::update(world);
        if done(world) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Private IDE file operation did not settle"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn open_file(world: &mut World, owner: Entity, path: &Path) {
    let scope = Scope::open(path.parent().unwrap()).unwrap();
    open(world, owner, scope, path.into());
    until(world, |world| {
        file_ready(world, owner)
            && world.get::<View>(owner).unwrap().path.as_deref() == Some(path)
            && world.get::<View>(owner).unwrap().window.is_some()
    });
}

fn edit(world: &mut World, owner: Entity, path: &Path, text: &str) {
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(text);
    editing::capture_one(world, owner);
    assert_eq!(world.resource::<Documents>().0[path].buffer.text(), text);
    assert!(world.resource::<Documents>().0[path].buffer.is_dirty());
    assert_eq!(fs::read_to_string(path).unwrap(), ORIGINAL);
}

#[test]
#[ignore = "Private child of the exact IDE crash/reopen test"]
fn private_ide_crash_child() {
    let Some(directory) = std::env::var_os(CHILD_DIRECTORY).map(PathBuf::from) else {
        return;
    };
    let saved = directory.join(SAVED_FILE);
    let draft = directory.join(DRAFT_FILE);
    assert_eq!(fs::read_to_string(&saved).unwrap(), ORIGINAL);
    assert_eq!(fs::read_to_string(&draft).unwrap(), ORIGINAL);
    let (mut world, owner) = startup(&directory);
    assert_eq!(
        world.get::<Ide>(owner).unwrap().settings.autosave_seconds,
        0
    );

    open_file(&mut world, owner, &draft);
    edit(&mut world, owner, &draft, DRAFT);
    until(&mut world, recovery::settled);
    open_file(&mut world, owner, &saved);
    edit(&mut world, owner, &saved, SAVED);
    actions::Control::Close.apply(&mut world, owner);
    assert_eq!(
        world.get::<View>(owner).unwrap().closing.as_ref(),
        Some(&saved)
    );
    actions::Control::SaveClose.apply(&mut world, owner);
    assert!(world.get::<View>(owner).unwrap().close_waiting);
    wait_for_files(&mut world, &saved);
    assert!(saved_text(&world, owner, SAVED));
    closing::update(&mut world, owner);
    assert!(!world.get::<Ide>(owner).unwrap().paths.contains(&saved));
    fs::File::create_new(directory.join(SAVE_ACK)).unwrap();

    std::thread::sleep(Duration::from_secs(30));
    panic!("The parent did not terminate the private IDE child after save acknowledgement");
}

#[test]
fn explicit_ide_save_and_close_survives_abrupt_process_exit() {
    let directory = tempfile::tempdir().unwrap();
    for name in [SAVED_FILE, DRAFT_FILE] {
        fs::write(directory.path().join(name), ORIGINAL).unwrap();
    }
    let executable = std::env::current_exe().unwrap();
    let module = module_path!().split_once("::").unwrap().1;
    let child_test = format!("{module}::private_ide_crash_child");
    let mut child = CrashChild(
        Command::new(executable)
            .args(["--exact", &child_test, "--ignored", "--nocapture"])
            .env(CHILD_DIRECTORY, directory.path())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while !directory.path().join(SAVE_ACK).is_file() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "The private IDE child exited before acknowledging Save and close"
        );
        assert!(
            Instant::now() < deadline,
            "The private IDE child did not acknowledge Save and close"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    child.0.kill().unwrap();
    let status = child.0.wait().unwrap();
    assert!(!status.success());
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9));
    }

    let saved = directory.path().join(SAVED_FILE);
    let draft = directory.path().join(DRAFT_FILE);
    let (mut world, owner) = startup(directory.path());
    until(&mut world, |world| {
        world.get::<Ide>(owner).unwrap().paths.contains(&draft)
    });
    assert!(!world.get::<Ide>(owner).unwrap().paths.contains(&saved));
    assert_eq!(world.resource::<Documents>().0[&draft].buffer.text(), DRAFT);
    assert!(world.resource::<Documents>().0[&draft].buffer.is_dirty());
    assert_eq!(fs::read_to_string(&draft).unwrap(), ORIGINAL);
    open_file(&mut world, owner, &saved);
    assert!(saved_text(&world, owner, SAVED));
    let editor = world.get::<View>(owner).unwrap().editor;
    assert_eq!(
        world
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        SAVED
    );
    assert_eq!(fs::read_to_string(&saved).unwrap(), SAVED);
}
