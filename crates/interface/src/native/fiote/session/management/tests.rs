use super::super::tests::{deliver, fixture_status, panel_fixture};
use super::*;

fn click(world: &mut World, label: &str) {
    let button = world
        .query::<(
            &crate::native::actions::ActionButton,
            &bevy::a11y::AccessibilityNode,
        )>()
        .iter(world)
        .find(|(_, node)| node.label() == Some(label))
        .map(|(button, _)| button.clone())
        .unwrap_or_else(|| panic!("Missing UI control {label}"));
    button.actions.run(world, button.target);
}

async fn backend(session: &mut cell::Session, request: FioteRequest) -> FioteStatus {
    let result = session
        .handle(cell::ClientMessage::Fiote {
            id: "directory-test".into(),
            request,
        })
        .await;
    result
        .into_iter()
        .find_map(|message| match message {
            cell::ServerMessage::Fiote { status, .. } => Some(status),
            _ => None,
        })
        .unwrap()
}

#[test]
fn working_directory_summary_reports_own_inherited_unset_and_invalid_values() {
    let mut saved = fixture_status("child");
    assert!(directory_summary(&saved).contains("none set"));
    saved.working_directory.effective = Some("/project".into());
    saved.working_directory.source = Some("parent".into());
    saved.fiotes = serde_json::from_value(serde_json::json!([
        {"record":"parent", "title":"Project Fiote"}
    ]))
    .unwrap();
    assert_eq!(
        directory_summary(&saved),
        "Working directory: /project · inherited from Project Fiote"
    );
    saved.working_directory.source = Some(saved.record.clone());
    assert!(directory_summary(&saved).contains("set on this Fiote"));
    saved.working_directory.error = Some("Directory no longer exists".into());
    assert_eq!(
        directory_summary(&saved),
        "Working directory: Directory no longer exists"
    );
}

#[test]
fn working_directory_draft_survives_polling_and_resets_when_switching_fiotes() {
    let (mut app, owner, mut saved) = panel_fixture(Step::Manage);
    let world = app.world_mut();
    let field = world.get::<DirectoryEditor>(owner).unwrap().field;
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("/my/draft");
    saved.working_directory.effective = Some("/parent/changed".into());
    apply_status(world, owner, saved.clone());
    assert_eq!(world.get::<DirectoryEditor>(owner).unwrap().field, field);
    assert_eq!(value(world, field).unwrap(), "/my/draft");
    let summary = world.get::<DirectoryEditor>(owner).unwrap().summary;
    assert!(
        world
            .get::<Text>(summary)
            .unwrap()
            .0
            .contains("/parent/changed")
    );
    saved.record = "another-fiote".into();
    saved.working_directory.configured = "/other/project".into();
    apply_status(world, owner, saved);
    let replaced = world.get::<DirectoryEditor>(owner).unwrap().field;
    assert_ne!(replaced, field);
    assert_eq!(value(world, replaced).unwrap(), "/other/project");
}

#[tokio::test]
async fn working_directory_ui_saves_overrides_and_restores_updated_parent_without_login() {
    let directory = tempfile::tempdir().unwrap();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let record = engine
        .act(
            engine::actions::Action::CreateAgent {
                head: "Project Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let host = std::sync::Arc::new(
        cell::fiote::Host::open(engine.clone(), directory.path().join("settings"))
            .await
            .unwrap(),
    );
    let mut setup = cell::Session::local(
        engine.clone(),
        std::sync::Arc::new(cell::LaneHub::new()),
        "directory-setup",
    )
    .with_fiote(host.clone());
    let saved = backend(
        &mut setup,
        FioteRequest::Prepare {
            record: record.clone(),
        },
    )
    .await;
    let parent = saved.behavior.prompt_parent.unwrap();
    backend(
        &mut setup,
        FioteRequest::WorkingDirectory {
            record: parent.clone(),
            directory: directory.path().into(),
        },
    )
    .await;
    let runtime = cell::CellRuntime {
        speech: None,
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: Some(host.clone()),
    };
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<crate::native::cell_bridge::CellMessage>()
        .add_plugins(Plugin);
    app.world_mut()
        .insert_non_send(crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
    let root = app.world_mut().spawn_empty().id();
    populate(
        app.world_mut(),
        root,
        RecordBinding {
            area: root,
            uid: record.clone(),
            source: Source::Local,
        },
    );
    deliver(&mut app).await;
    let owner = app
        .world_mut()
        .query_filtered::<Entity, With<Panel>>()
        .single(app.world())
        .unwrap();
    super::super::show(app.world_mut(), owner, Step::Manage);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("inherited from Lince Fiote"))
    );
    for _ in 0..2 {
        click(app.world_mut(), "Save working directory");
        deliver(&mut app).await;
        assert!(app.world().get::<DirectoryEditor>(owner).is_some());
    }
    let branch = directory.path().join("branch");
    let updated = directory.path().join("updated-parent");
    std::fs::create_dir(&branch).unwrap();
    std::fs::create_dir(&updated).unwrap();
    let field = app.world().get::<DirectoryEditor>(owner).unwrap().field;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(branch.to_str().unwrap());
    click(app.world_mut(), "Save working directory");
    deliver(&mut app).await;
    let saved = app
        .world()
        .get::<Panel>(owner)
        .unwrap()
        .saved
        .as_ref()
        .unwrap();
    assert_eq!(
        saved.working_directory.configured,
        branch.canonicalize().unwrap()
    );
    assert_eq!(
        saved.working_directory.source.as_deref(),
        Some(record.as_str())
    );
    assert!(!saved.settings.enabled);
    backend(
        &mut setup,
        FioteRequest::WorkingDirectory {
            record: parent.clone(),
            directory: updated.clone(),
        },
    )
    .await;
    click(app.world_mut(), "Use parent directory");
    deliver(&mut app).await;
    let saved = app
        .world()
        .get::<Panel>(owner)
        .unwrap()
        .saved
        .as_ref()
        .unwrap();
    assert!(saved.working_directory.configured.as_os_str().is_empty());
    assert_eq!(
        saved.working_directory.effective,
        Some(updated.canonicalize().unwrap())
    );
    assert_eq!(
        saved.working_directory.source.as_deref(),
        Some(parent.as_str())
    );
    let field = app.world().get::<DirectoryEditor>(owner).unwrap().field;
    assert!(value(app.world(), field).unwrap().is_empty());
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("relative/path");
    click(app.world_mut(), "Save working directory");
    deliver(&mut app).await;
    let panel = app.world().get::<Panel>(owner).unwrap();
    assert!(
        app.world()
            .get::<Text>(panel.status)
            .unwrap()
            .0
            .contains("existing working directory")
    );
    assert!(
        panel
            .saved
            .as_ref()
            .unwrap()
            .working_directory
            .configured
            .as_os_str()
            .is_empty()
    );
}
