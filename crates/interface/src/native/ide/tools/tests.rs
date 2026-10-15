use super::*;

fn fixture() -> (World, Entity, tempfile::TempDir, PathBuf) {
    let (mut world, _, owner, dir, old) = super::super::tests::fixture("let value = 1;\n");
    let path = old.with_extension("rs");
    std::fs::rename(&old, &path).unwrap();
    let mut doc = world.resource_mut::<Documents>().0.remove(&old).unwrap();
    doc.file = Some(Arc::new(
        Scope::open(dir.path()).unwrap().bind(&path).unwrap(),
    ));
    world
        .resource_mut::<Documents>()
        .0
        .insert(path.clone(), doc);
    let mut ide = world.get_mut::<Ide>(owner).unwrap();
    ide.paths = vec![path.clone()];
    ide.active = Some(path.clone());
    runtime::render(&mut world, owner);
    update(&mut world);
    (world, owner, dir, path)
}

#[test]
fn tool_edits_are_one_undo_and_stale_results_cannot_replace_typing() {
    let (mut world, owner, _dir, path) = fixture();
    let before = ticket(&world, owner, path.clone()).unwrap();
    let edits = [lince_editor::Edit {
        range: 4..9,
        text: "name".into(),
    }];
    apply(&mut world, &before, &edits).unwrap();
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "let name = 1;\n"
    );
    assert!(apply(&mut world, &before, &edits).is_err());
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
        "let value = 1;\n"
    );
}

#[test]
fn formatter_command_updates_only_the_buffer_and_preserves_disk_until_save() {
    let (mut world, owner, _dir, path) = fixture();
    let fields = world.get::<Panel>(owner).unwrap().fields;
    world
        .get_mut::<EditableText>(fields[1])
        .unwrap()
        .editor
        .set_text("[\"tr\",\"a-z\",\"A-Z\"]");
    action(&mut world, owner, &actions::Control::FormatCommand);
    let until = Instant::now() + Duration::from_secs(10);
    while !world.resource::<Hub>().jobs.is_empty() {
        update(&mut world);
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        world.resource::<Documents>().0[&path].buffer.text(),
        "LET VALUE = 1;\n"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "let value = 1;\n");
    assert_eq!(
        world.get::<Ide>(owner).unwrap().language_tools["rust"].formatter,
        ["tr", "a-z", "A-Z"]
    );
}

#[test]
fn absent_language_server_reports_a_useful_error_without_retrying() {
    let (mut world, owner, _dir, _path) = fixture();
    let fields = world.get::<Panel>(owner).unwrap().fields;
    world
        .get_mut::<EditableText>(fields[0])
        .unwrap()
        .editor
        .set_text("[\"/definitely-missing/language-server\"]");
    action(&mut world, owner, &actions::Control::Connect);
    let until = Instant::now() + Duration::from_secs(5);
    while !world
        .resource::<Hub>()
        .sessions
        .values()
        .any(|session| session.failed)
    {
        update(&mut world);
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let status = world.get::<Panel>(owner).unwrap().status;
    assert!(
        world
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("Install it through your distribution or Nix")
    );
    for _ in 0..10 {
        update(&mut world);
    }
    assert!(
        world
            .resource::<Hub>()
            .sessions
            .values()
            .all(|session| session.failed)
    );
    action(&mut world, owner, &actions::Control::Disconnect);
    update(&mut world);
    assert!(world.resource::<Hub>().sessions.is_empty());
}
