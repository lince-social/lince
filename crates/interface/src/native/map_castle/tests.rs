use super::*;
use crate::native::actions::Action;

pub(super) fn fixture() -> (App, Entity, Entity) {
    let mut app = crate::native::sand_panel::tests::app();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Image>>()
        .add_plugins(MapPlugin);
    let root = app
        .world_mut()
        .spawn((
            crate::native::container::BoxRoot,
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        Camera::default(),
        Vec::new(),
        String::new(),
    );
    (app, root, owner)
}

#[test]
fn pointer_pan_latency_uses_each_gesture_instead_of_the_previous_button() {
    use bevy::picking::{
        events::Drag,
        pointer::{Location, PointerId},
    };
    use std::time::{Duration, Instant};

    let (mut app, _, owner) = fixture();
    let viewport = app.world().get::<MapSand>(owner).unwrap().viewport;
    let previous = Instant::now() - Duration::from_secs(10);
    app.world_mut()
        .get_mut::<MapSand>(owner)
        .unwrap()
        .pending_input = Some(previous);
    for delta in [Vec2::new(12.0, 5.0), Vec2::new(-6.0, 4.0)] {
        let before = Instant::now();
        let camera = app.world().get::<MapSand>(owner).unwrap().camera;
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            Location {
                target: bevy::camera::NormalizedRenderTarget::Window(
                    bevy::window::WindowRef::Entity(viewport)
                        .normalize(None)
                        .unwrap(),
                ),
                position: Vec2::ZERO,
            },
            Drag {
                button: PointerButton::Primary,
                distance: delta,
                delta,
            },
            viewport,
        ));
        app.world_mut().flush();
        let sand = app.world().get::<MapSand>(owner).unwrap();
        let input = sand.pending_input.unwrap();
        assert!(input >= before);
        assert_ne!(sand.camera, camera);
        diagnostics::rendered(
            app.world().resource::<diagnostics::Diagnostics>(),
            owner,
            crate::map_view::Rendered {
                started: input,
                submitted: input + Duration::from_millis(7),
                input: Some(input),
                device: Default::default(),
            },
        );
    }
    let trace = diagnostics::snapshot(app.world());
    let views = trace["metrics"]["views"].as_object().unwrap();
    assert_eq!(
        views[&format!("{owner:?}")]["input_to_submission_micros"],
        serde_json::json!([7000, 7000])
    );
}

#[cfg_attr(test, test)]
fn independent_maps_keep_camera_theme_and_record_bindings_in_workspace() {
    let (mut app, root, first) = fixture();
    let record = nucleus::new_uid("r");
    let second = spawn(
        app.world_mut(),
        root,
        2,
        DVec2::new(900.0, 0.0),
        Camera::default(),
        vec![record.clone()],
        String::new(),
    );
    Command::Zoom(3.0).apply(app.world_mut(), first);
    assert_eq!(app.world().get::<MapSand>(first).unwrap().camera.zoom, 4.0);
    assert_eq!(app.world().get::<MapSand>(second).unwrap().camera.zoom, 1.0);
    let initial = ui::palette(app.world(), first);
    app.world_mut()
        .entity_mut(first)
        .insert(crate::native::tokens::TokenOverrides(
            std::collections::BTreeMap::from([(
                crate::native::tokens::Token::Surface,
                crate::native::tokens::TokenValue::Color([20, 40, 60, 255]),
            )]),
        ));
    assert_ne!(ui::palette(app.world(), first), initial);
    assert_eq!(ui::palette(app.world(), second), initial);
    app.world_mut()
        .get_mut::<MapSand>(second)
        .unwrap()
        .authority_node_id = Some("12".repeat(32));
    app.world_mut()
        .get_mut::<MapSand>(second)
        .unwrap()
        .camera
        .center = Coordinate {
        latitude: -23.5505,
        longitude: -46.6333,
    };
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(SavedMap::valid));
    let encoded = serde_json::to_string(&saved).unwrap();
    assert!(!encoded.contains("markers"));
    assert!(!encoded.contains("fix"));
    assert!(!encoded.contains("-23.5505"));
    assert!(!encoded.contains("-46.6333"));
    app.world_mut().despawn(first);
    app.world_mut().despawn(second);
    for saved in saved {
        saved.restore(app.world_mut(), root);
    }
    let maps: Vec<_> = app
        .world_mut()
        .query::<&MapSand>()
        .iter(app.world())
        .collect();
    assert!(maps.iter().any(|map| map.camera.zoom == 4.0));
    assert!(maps.iter().any(|map| map.records == [record.clone()]
        && map.camera.zoom == 1.0
        && map.authority_node_id.as_deref() == Some(&"12".repeat(32))));
}

#[cfg_attr(test, test)]
fn record_map_control_reuses_one_inline_map_and_closes_it() {
    let (mut app, _, owner) = fixture();
    let record = nucleus::new_uid("r");
    let parent = crate::native::sand_panel::column(app.world_mut(), owner);
    let open = Open {
        records: vec![record.clone()],
        person: String::new(),
    };
    open.apply(app.world_mut(), parent);
    let inline = app
        .world()
        .get::<Children>(parent)
        .unwrap()
        .iter()
        .find(|child| app.world().get::<MapSand>(*child).is_some())
        .unwrap();
    assert_eq!(
        app.world().get::<MapSand>(inline).unwrap().records,
        [record]
    );
    assert!(
        app.world()
            .get::<crate::native::canvas::CanvasItem>(inline)
            .is_none()
    );
    open.apply(app.world_mut(), parent);
    assert!(app.world().get_entity(inline).is_err());
    assert_eq!(
        app.world_mut()
            .query::<&MapSand>()
            .iter(app.world())
            .count(),
        1
    );
}

#[cfg_attr(test, test)]
fn expired_live_overlay_and_empty_record_source_are_cleared() {
    let (mut app, _, owner) = fixture();
    let record = nucleus::new_uid("r");
    app.world_mut()
        .get_mut::<MapSand>(owner)
        .unwrap()
        .markers
        .push(feed::Marker {
            record: record.clone(),
            coordinate: Coordinate {
                latitude: 10.0,
                longitude: 20.0,
            },
            live: true,
            view: None,
            observed: engine::location::clock::Instant::now(),
            radius_metres: None,
        });
    ui::update(app.world_mut());
    assert!(
        app.world()
            .get::<MapSand>(owner)
            .unwrap()
            .markers
            .is_empty()
    );
    app.world_mut()
        .get_mut::<MapSand>(owner)
        .unwrap()
        .markers
        .push(feed::Marker {
            record,
            coordinate: Coordinate {
                latitude: 10.0,
                longitude: 20.0,
            },
            live: false,
            view: None,
            observed: engine::location::clock::Instant::now(),
            radius_metres: None,
        });
    paging::maintain(app.world_mut());
    feed::maintain(app.world_mut());
    assert!(
        app.world()
            .get::<MapSand>(owner)
            .unwrap()
            .markers
            .is_empty()
    );
}

#[cfg_attr(test, test)]
fn cancelling_and_closing_a_map_stop_its_area_job() {
    let (mut app, _, owner) = fixture();
    let cancelled = Arc::new(AtomicBool::new(false));
    app.world_mut().get_mut::<MapSand>(owner).unwrap().job = Some(Job {
        tiles: Vec::new(),
        cancelled: cancelled.clone(),
        finished: Arc::new(Mutex::new(None)),
    });
    Command::Cancel.apply(app.world_mut(), owner);
    assert!(cancelled.load(Ordering::Relaxed));
    let cancelled = Arc::new(AtomicBool::new(false));
    app.world_mut().get_mut::<MapSand>(owner).unwrap().job = Some(Job {
        tiles: Vec::new(),
        cancelled: cancelled.clone(),
        finished: Arc::new(Mutex::new(None)),
    });
    app.world_mut().despawn(owner);
    assert!(cancelled.load(Ordering::Relaxed));
}

crate::native::laboratory_cases! {
    independent_maps_keep_camera_theme_and_record_bindings_in_workspace,
    record_map_control_reuses_one_inline_map_and_closes_it,
    expired_live_overlay_and_empty_record_source_are_cleared,
    cancelling_and_closing_a_map_stop_its_area_job,
}
