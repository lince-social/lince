use super::*;
use bevy::picking::{
    backend::HitData,
    pointer::{Location, PointerButton, PointerId},
};
use serde_json::Value;

fn fixture(directory: &std::path::Path) -> (World, Entity, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<lince_interface::theme::Typography>();
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.init_resource::<lince_interface::tokens::ThemeSettings>();
    let mut mobile = Mobile::new(directory.into());
    mobile.ready = true;
    mobile.identity = crate::session::Identity::Person(nucleus::new_uid("r"));
    mobile.navigation.open(Page::Map);
    world.insert_resource(mobile);
    let parent = world.spawn(Node::default()).id();
    let window = world.spawn(Window::default()).id();
    render(&mut world, parent);
    (world, parent, window)
}

fn position(window: Entity, x: f32) -> Location {
    Location {
        target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
            .normalize(None)
            .unwrap(),
        position: Vec2::new(x, 0.0),
    }
}

#[test]
fn actual_two_pointer_gesture_and_accessible_commands_share_one_view_and_old_actor_replies_are_discarded()
 {
    let temporary = tempfile::tempdir().unwrap();
    let (mut world, parent, window) = fixture(temporary.path());
    let owner = world.resource::<State>().surface.unwrap();
    let viewport = world.get::<Surface>(owner).unwrap().viewport;
    world.get_mut::<ComputedNode>(viewport).unwrap().size = Vec2::splat(400.0);
    world
        .get_mut::<ComputedNode>(viewport)
        .unwrap()
        .inverse_scale_factor = 1.0;
    world
        .entity_mut(viewport)
        .insert(UiGlobalTransform::default());
    for (id, x) in [(1, -50.0), (2, 50.0)] {
        world.trigger(Pointer::new(
            PointerId::Touch(id),
            position(window, x),
            Press {
                button: PointerButton::Primary,
                hit: HitData::new(window, 0.0, None, None),
                count: 1,
            },
            viewport,
        ));
        world.flush();
    }
    let before = world.resource::<State>().camera;
    world.trigger(Pointer::new(
        PointerId::Touch(2),
        position(window, 100.0),
        Drag {
            button: PointerButton::Primary,
            distance: Vec2::new(50.0, 0.0),
            delta: Vec2::new(50.0, 0.0),
        },
        viewport,
    ));
    world.flush();
    let after = world.resource::<State>().camera;
    assert!(after.zoom - before.zoom > 0.5 && after.zoom - before.zoom < 0.7);
    assert_eq!(world.resource::<State>().surface, Some(owner));
    let entity = world
        .query::<(Entity, &Text)>()
        .iter(&world)
        .find(|(_, text)| text.0 == "North")
        .unwrap()
        .0;
    let button = world.get::<ChildOf>(entity).unwrap().parent();
    let app::Intent::Map(intent) = world.get::<app::ButtonIntent>(button).unwrap().0.clone() else {
        panic!("Map control")
    };
    apply(&mut world, intent).unwrap();
    assert_ne!(world.resource::<State>().camera.center, after.center);
    let record = nucleus::new_uid("r");
    let epoch = world.resource::<State>().epoch;
    world.resource_mut::<State>().pending.insert(
        "mobile-map_old".into(),
        Pending {
            kind: Kind::Saved,
            epoch,
            sent: Instant::now(),
        },
    );
    world.resource_mut::<State>().markers.push(Marker {
        record: record.clone(),
        point: Coordinate {
            latitude: 10.0,
            longitude: 20.0,
        },
        radius: Some(1000),
        layer: 1,
        view: None,
        received: Instant::now(),
    });
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person(nucleus::new_uid("r"));
    receive(
        &mut world,
        &cell::ServerMessage::ActionOk {
            id: "mobile-map_old".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(
                json!({"generation":"old","features":[{"record":record,"head":"Private old actor"}]}),
            ),
        },
    );
    assert!(world.resource::<State>().markers.is_empty());
    assert!(world.resource::<State>().rows.is_empty());
    assert!(world.resource::<State>().pending.is_empty());
    for _ in 0..20 {
        close_surface(&mut world);
        render(&mut world, parent);
        assert_eq!(world.query::<&Surface>().iter(&world).count(), 1);
    }
    world
        .resource_mut::<Mobile>()
        .navigation
        .open(Page::Records);
    maintain(&mut world);
    assert_eq!(world.query::<&Surface>().iter(&world).count(), 0);
    assert!(world.resource::<map_view::Scenes>().0.is_empty());
}

#[tokio::test]
async fn packaged_overview_and_example_are_offline_and_received_invitation_starts_no_observation() {
    let temporary = tempfile::tempdir().unwrap();
    data::pack::package(temporary.path()).unwrap();
    let maps = temporary.path().join("maps");
    let overview: Value =
        serde_json::from_str(&std::fs::read_to_string(maps.join("world-land.geojson")).unwrap())
            .unwrap();
    assert_eq!(overview["type"], "FeatureCollection");
    assert!(!overview["features"].as_array().unwrap().is_empty());
    let packaged = TileStore::new(
        Settings {
            local_directory: maps.join("tiles"),
            ..Default::default()
        },
        temporary.path().join("cache"),
    )
    .unwrap();
    assert!(packaged.load(data::Tile { z: 0, x: 0, y: 0 }).await.is_ok());
    assert_eq!(
        packaged
            .progress(&[data::Tile { z: 0, x: 0, y: 0 }])
            .downloaded_bytes,
        0
    );
    for license in [
        "Natural-Earth.txt",
        "Galileo-MIT.txt",
        "PMTiles-MIT.txt",
        "wgpu-MIT.txt",
        "ODbL.txt",
    ] {
        assert!(
            std::fs::metadata(maps.join("licenses").join(license))
                .unwrap()
                .len()
                > 20
        );
    }
    let (mut world, _, _) = fixture(temporary.path());
    world.init_resource::<Messages<lince_interface::location::Request>>();
    let person = crate::workflows::person(&world);
    let invitation = nucleus::location::Invitation {
        record_uid: nucleus::new_uid("r"),
        authority_node_id: "b".repeat(64),
        grant: Some(nucleus::location::InvitationGrant {
            uid: nucleus::new_uid("li"),
            issuer: nucleus::new_uid("r"),
            recipient: person,
            radius_metres: Some(3000),
            expires_at_ms: chrono::Utc::now().timestamp_millis() + 60_000,
        }),
    };
    crate::location::open_reference(&mut world, &invitation.reference().unwrap()).unwrap();
    assert!(
        world
            .resource::<Messages<lince_interface::location::Request>>()
            .is_empty()
    );
    assert!(world.resource::<State>().invitation.is_none());
    assert!(world.resource::<State>().pending.is_empty());
    observe(&mut world, invitation.clone()).unwrap();
    assert_eq!(
        world.resource::<State>().invitation.as_ref(),
        Some(&invitation)
    );
    assert!(!world.resource::<State>().settings.online);
    assert!(world.resource::<State>().settings.server.is_empty());
    close_surface(&mut world);
    render(&mut world, Entity::PLACEHOLDER);
    assert_eq!(
        world.resource::<State>().invitation.as_ref(),
        Some(&invitation)
    );
    let state = State::default();
    assert!(state.invitation.is_none());
    assert!(state.markers.is_empty());
    assert!(state.pending.is_empty());
    assert!(state.locate_epoch.is_none());
}

#[test]
fn ordinary_zoom_needs_no_detail_form_and_persisted_offline_mode_is_respected() {
    let temporary = tempfile::tempdir().unwrap();
    let (mut world, _, _) = fixture(temporary.path());
    assert!(world.resource::<State>().settings.online);
    assert_eq!(
        world.resource::<State>().settings.server,
        data::source::PUBLIC_SERVER
    );
    assert!(
        !world
            .query::<&lince_interface::controls::Editor>()
            .iter(&world)
            .any(|editor| editor.title.contains("Area detail"))
    );
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0 == "Save selected area offline")
    );
    let before = world.resource::<State>().camera.zoom;
    apply(&mut world, Intent::Zoom(1.0)).unwrap();
    assert_eq!(world.resource::<State>().camera.zoom, before + 1.0);
    assert!(world.resource::<State>().selection.is_none());
    assert!(world.resource::<State>().download.is_none());
    let path = temporary.path().join("maps/settings.json");
    Settings::default().save_atomic(&path).unwrap();
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person(nucleus::new_uid("r"));
    scope(&mut world);
    assert!(!world.resource::<State>().settings.online);
    std::fs::write(&path, b"invalid settings").unwrap();
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person(nucleus::new_uid("r"));
    scope(&mut world);
    assert!(!world.resource::<State>().settings.online);
    assert!(
        world
            .resource::<State>()
            .notice
            .contains("offline overview")
    );
}
