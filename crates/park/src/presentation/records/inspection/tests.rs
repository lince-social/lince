use super::*;
use crate::presentation::records::tests::{pump, window};

fn target(root: Entity, surface: SurfaceId, identity: PlacementId) -> WindowInspection {
    WindowInspection {
        owner: root.to_bits(),
        workspace: 1,
        position: [-70.0, 123.0],
        surface,
        placement: identity.0,
    }
}

#[tokio::test]
async fn inspection_preserves_the_in_flight_creation_and_original_destination_after_close() {
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
    let directory = tempfile::tempdir().unwrap();
    let mut world = World::new();
    world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
    world.insert_resource(WakeSignal::new(|| {}));
    world.insert_non_send(native::cell_bridge::connect(
        runtime,
        WakeSignal::new(|| {}),
    ));
    let root = world
        .spawn((
            native::workspace::Workspaces::default(),
            native::canvas::CanvasView::default(),
        ))
        .id();
    let (surface, window, identity) = window(&mut world, root);
    let entity = window.entity;
    let mut windows = HashMap::from([(surface, window)]);
    let mut identities = HashMap::from([(surface, identity)]);
    let mut records = Records::load(directory.path().join("windows.json")).unwrap();
    records.tick(&mut world, &windows, &identities);
    let pending = records.requests.clone();
    assert_eq!(pending.len(), 1);
    let uid = records.profile(identity).unwrap().creation.clone().unwrap();
    let origin = target(root, surface, identity);
    records
        .inspect(origin, "Captured title".into(), "example.client".into())
        .unwrap();
    records
        .inspect(origin, "Captured title".into(), "example.client".into())
        .unwrap();
    assert_eq!(records.requests, pending);
    assert_eq!(
        records.profile(identity).unwrap().creation.as_ref(),
        Some(&uid)
    );
    let inspection = records.take_inspection().unwrap();
    assert_eq!(inspection.target, origin);
    assert_eq!(inspection.uid, uid);
    assert!(matches!(
        records.creation_state(identity.0, &uid),
        Some(CreationState::Pending)
    ));
    assert!(records.take_inspection().is_none());
    world
        .get_mut::<native::workspace::Workspaces>(root)
        .unwrap()
        .active = 88;
    windows.clear();
    identities.clear();
    world.despawn(entity);
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records.profile(identity).unwrap().uid.as_deref() == Some(uid.as_str())
    })
    .await;
    assert_eq!(inspection.target, origin);
    assert_eq!(inspection.uid, uid);
    assert_eq!(inspection.title, "Captured title");
    assert_eq!(inspection.app_id, "example.client");
    assert!(records.take_inspection().is_none());
    assert!(records.creation_state(identity.0, &uid).is_none());
    assert_eq!(records.saved.len(), 1);
    assert!(records.requests.is_empty());
}

#[test]
fn inspection_requires_its_own_association_and_only_accepts_its_captured_uid() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let (surface, window, identity) = window(&mut world, root);
    let mut records = Records::load(directory.path().join("windows.json")).unwrap();
    let origin = target(root, surface, identity);
    assert!(
        records
            .inspect(origin, String::new(), String::new())
            .is_err()
    );
    assert!(records.inspect.is_none());
    records.capture(&world, &window, identity);
    let uid = records.profile(identity).unwrap().creation.clone().unwrap();
    records
        .inspect(origin, "Title".into(), "App ID".into())
        .unwrap();
    records.saved.get_mut(&identity).unwrap().uid = Some(nucleus::new_uid("r"));
    records.saved.get_mut(&identity).unwrap().creation = None;
    records.creations.remove(&identity);
    assert!(records.take_inspection().is_none());
    records.saved.get_mut(&identity).unwrap().uid = Some(uid.clone());
    assert_eq!(records.take_inspection().unwrap().uid, uid);
}
