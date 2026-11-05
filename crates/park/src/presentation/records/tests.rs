use super::*;

#[tokio::test]
async fn windows_save_settings_without_creating_binding_or_relaunching_records() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: Default::default(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        speech: None,
        fiote: None,
    };
    let mut session = runtime.local_session();
    let before = session
        .handle(native::cell_bridge::records_subscription())
        .await;
    let rows = |messages: Vec<cell::ServerMessage>| {
        messages
            .into_iter()
            .find_map(|message| match message {
                cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
                _ => None,
            })
            .unwrap()
    };
    let before = rows(before);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profiles.json");
    let mut records = Records::load(path.clone()).unwrap();
    let mut world = World::new();
    let root = world.spawn(native::workspace::Workspaces::default()).id();
    let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
    let identity = PlacementId(layout.id);
    let item = world
        .spawn((
            layout,
            native::canvas::CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::splat(300.0),
            },
            native::workspace::WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    let surface = SurfaceId(1);
    let mut windows = HashMap::from([(surface, clients::Window::fixture(item))]);
    let mut identities = HashMap::from([(surface, identity)]);
    records.tick(&mut world, &windows, &identities);
    records.flush().unwrap();
    assert!(
        world
            .get::<native::protein_area::RecordBinding>(item)
            .is_none()
    );
    assert!(records.profile(identity).is_some());
    let document = std::fs::read_to_string(&path).unwrap();
    assert!(!document.contains("uid"));
    assert!(!document.contains("creation"));
    world.despawn(item);
    windows.clear();
    identities.clear();
    records.tick(&mut world, &windows, &identities);
    let mut second_session = runtime.local_session();
    let after = rows(
        second_session
            .handle(native::cell_bridge::records_subscription())
            .await,
    );
    assert_eq!(before, after);
    assert!(Records::load(path).unwrap().profile(identity).is_some());
}

#[test]
fn capture_exclusion_is_bound_to_placement_without_a_record() {
    let directory = tempfile::tempdir().unwrap();
    let mut records = Records::load(directory.path().join("profiles.json")).unwrap();
    let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
    let identity = PlacementId(layout.id);
    records.saved.insert(
        identity,
        Saved {
            layout,
            workspace: 1,
            position: [0.0; 2],
            size: [300.0; 2],
            tokens: Default::default(),
            launch: None,
        },
    );
    assert!(records.capture_metadata_excluded(identity, ""));
    records.capture_exclusion(identity, "", false).unwrap();
    assert!(!records.capture_metadata_excluded(identity, ""));
    assert!(
        records
            .capture_exclusion(identity, &nucleus::new_uid("r"), true)
            .is_err()
    );
    assert!(
        records
            .capture_exclusion(PlacementId([0; 16]), "", true)
            .is_err()
    );
}
