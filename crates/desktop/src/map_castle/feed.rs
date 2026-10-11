use super::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(super) struct Marker {
    pub record: String,
    pub coordinate: Coordinate,
    pub live: bool,
    pub expires: Option<i64>,
    pub observed: Instant,
    pub radius_metres: Option<u32>,
}

#[derive(Clone, Copy)]
enum Kind {
    Saved,
    Context,
    Live,
    Save,
}

#[derive(Component)]
struct Feed {
    pending: HashMap<String, (String, Kind, Instant)>,
    next: Instant,
    records: Vec<String>,
    person: String,
    authority: Option<String>,
}

pub(super) fn reset(world: &mut World, owner: Entity) {
    world.entity_mut(owner).remove::<Feed>();
    if let Some(mut sand) = world.get_mut::<MapSand>(owner) {
        sand.markers.clear();
    }
}

pub(super) fn maintain(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<MapSand>>()
        .iter(world)
        .collect();
    for owner in owners {
        if !visible(world, owner) {
            world.entity_mut(owner).remove::<Feed>();
            world.get_mut::<MapSand>(owner).unwrap().markers.clear();
            continue;
        }
        let sand = world.get::<MapSand>(owner).unwrap();
        if sand.records.is_empty() {
            world.entity_mut(owner).remove::<Feed>();
            world.get_mut::<MapSand>(owner).unwrap().markers.clear();
            continue;
        }
        let changed = world.get::<Feed>(owner).is_none_or(|feed| {
            feed.records != sand.records
                || feed.person != sand.person
                || feed.authority != sand.authority_node_id
        });
        if changed {
            let (records, person, authority) = (
                sand.records.clone(),
                sand.person.clone(),
                sand.authority_node_id.clone(),
            );
            world.entity_mut(owner).insert(Feed {
                pending: HashMap::new(),
                next: Instant::now(),
                records,
                person,
                authority,
            });
            world.get_mut::<MapSand>(owner).unwrap().markers.clear();
        }
        let now = Instant::now();
        let stale: Vec<_> = world
            .get::<Feed>(owner)
            .unwrap()
            .pending
            .iter()
            .filter(|(_, (_, _, sent))| now.duration_since(*sent) > Duration::from_secs(10))
            .map(|(id, (record, kind, _))| (id.clone(), record.clone(), *kind))
            .collect();
        for (id, record, kind) in stale {
            world.get_mut::<Feed>(owner).unwrap().pending.remove(&id);
            clear(world, owner, &record, kind);
        }
        let feed = world.get::<Feed>(owner).unwrap();
        if feed.next > now {
            continue;
        }
        let pending = !feed.pending.is_empty();
        let records = feed.records.clone();
        let person = feed.person.clone();
        let authority = world
            .get::<MapSand>(owner)
            .unwrap()
            .authority_node_id
            .clone();
        world.get_mut::<Feed>(owner).unwrap().next = now + Duration::from_secs(5);
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.after(Duration::from_secs(5));
        }
        if pending {
            continue;
        }
        for record in records {
            request(
                world,
                owner,
                &record,
                Kind::Saved,
                engine::actions::Action::SavedLocation {
                    request: if let Some(node) = &authority {
                        nucleus::vocabulary::SavedLocationCommand::Observe {
                            record: record.clone(),
                            node_id: node.clone(),
                        }
                    } else {
                        nucleus::vocabulary::SavedLocationCommand::Inspect {
                            record: record.clone(),
                        }
                    },
                },
            );
            if let Some(node) = &authority {
                request(
                    world,
                    owner,
                    &record,
                    Kind::Live,
                    engine::actions::Action::Location {
                        request: nucleus::location::Command::Observe {
                            person: person.clone(),
                            record_uid: record.clone(),
                            node_id: node.clone(),
                        },
                    },
                );
                continue;
            }
            request(
                world,
                owner,
                &record,
                Kind::Context,
                engine::actions::Action::Location {
                    request: nucleus::location::Command::Context {
                        person: person.clone(),
                        record_uid: record.clone(),
                    },
                },
            );
        }
    }
}

fn request(
    world: &mut World,
    owner: Entity,
    record: &str,
    kind: Kind,
    action: engine::actions::Action,
) {
    let id = nucleus::new_uid("map-location");
    match crate::sand_panel::send(
        world,
        cell::ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            if let Some(mut feed) = world.get_mut::<Feed>(owner) {
                feed.pending
                    .insert(id, (record.into(), kind, Instant::now()));
            }
        }
        Err(_) => clear(world, owner, record, kind),
    }
}

fn clear(world: &mut World, owner: Entity, record: &str, kind: Kind) {
    if matches!(kind, Kind::Save) {
        return;
    }
    if let Some(mut sand) = world.get_mut::<MapSand>(owner) {
        sand.markers.retain(|marker| {
            marker.record != record
                || (matches!(kind, Kind::Saved) && marker.live)
                || (matches!(kind, Kind::Live | Kind::Context) && !marker.live)
        });
    }
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .cloned()
        .collect();
    for message in messages {
        let (id, result) = match message.0 {
            cell::ServerMessage::ActionOk { id, data, .. } => (id, Ok(data.unwrap_or_default())),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owner = world
            .query::<(Entity, &Feed)>()
            .iter(world)
            .find(|(_, feed)| feed.pending.contains_key(&id))
            .map(|(entity, _)| entity);
        let Some(owner) = owner else { continue };
        let Some((record, kind, _)) = world.get_mut::<Feed>(owner).unwrap().pending.remove(&id)
        else {
            continue;
        };
        clear(world, owner, &record, kind);
        let Ok(data) = result else {
            if matches!(kind, Kind::Save) {
                world.get_mut::<MapSand>(owner).unwrap().notice =
                    "The location could not be saved. Check Record permissions.".into();
            }
            continue;
        };
        match kind {
            Kind::Saved => {
                if let Ok(location) =
                    serde_json::from_value::<nucleus::vocabulary::SavedLocation>(data)
                {
                    if location.record_uid == record
                        && let Some(selected) = location.selected
                    {
                        let coordinate = Coordinate {
                            latitude: selected.latitude,
                            longitude: selected.longitude,
                        };
                        if coordinate.valid()
                            && selected
                                .radius_metres
                                .is_none_or(nucleus::place::valid_area_radius)
                        {
                            world
                                .get_mut::<MapSand>(owner)
                                .unwrap()
                                .markers
                                .push(Marker {
                                    record,
                                    coordinate,
                                    live: false,
                                    expires: None,
                                    observed: Instant::now(),
                                    radius_metres: selected.radius_metres,
                                });
                        }
                    }
                }
            }
            Kind::Context => {
                if let Ok(context) = serde_json::from_value::<nucleus::location::Context>(data) {
                    let person = world.get::<MapSand>(owner).unwrap().person.clone();
                    if !person.is_empty() {
                        add_live(world, owner, &record, context.view);
                    } else {
                        request(
                            world,
                            owner,
                            &record,
                            Kind::Live,
                            engine::actions::Action::Location {
                                request: nucleus::location::Command::Observe {
                                    person,
                                    record_uid: record.clone(),
                                    node_id: context.authority_node_id,
                                },
                            },
                        );
                    }
                }
            }
            Kind::Live => {
                if let Ok(view) = serde_json::from_value::<nucleus::location::View>(data) {
                    add_live(world, owner, &record, view);
                }
            }
            Kind::Save => {
                let mut sand = world.get_mut::<MapSand>(owner).unwrap();
                sand.notice = "Record location saved".into();
                sand.picked = None;
                world.get_mut::<Feed>(owner).unwrap().next = Instant::now();
            }
        }
    }
}

fn add_live(world: &mut World, owner: Entity, record: &str, view: nucleus::location::View) {
    if view.record_uid != record
        || view.status != nucleus::location::Status::Live
        || view.age_ms.is_none_or(|age| age > 60_000)
    {
        return;
    }
    let Some(fix) = view.fix else { return };
    let now = chrono::Utc::now().timestamp_millis();
    if !(0..=60_000).contains(&now.saturating_sub(fix.captured_at_ms))
        || view.expires_at_ms.is_some_and(|expires| expires <= now)
    {
        return;
    }
    let coordinate = Coordinate {
        latitude: fix.latitude,
        longitude: fix.longitude,
    };
    if coordinate.valid()
        && view
            .radius_metres
            .is_none_or(nucleus::place::valid_area_radius)
    {
        world
            .get_mut::<MapSand>(owner)
            .unwrap()
            .markers
            .push(Marker {
                record: record.into(),
                coordinate,
                live: true,
                expires: view.expires_at_ms,
                observed: Instant::now(),
                radius_metres: view.radius_metres,
            });
    }
}

pub(super) mod tests {
    use super::*;
    use crate::actions::Action;

    #[cfg_attr(test, test)]
    fn changing_shared_authority_discards_pending_location_replies() {
        let mut app = crate::sand_panel::tests::app();
        app.add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, receive);
        let root = app.world_mut().spawn_empty().id();
        let record = nucleus::new_uid("r");
        let owner = mount(
            app.world_mut(),
            root,
            Camera::default(),
            vec![record.clone()],
            String::new(),
        );
        app.world_mut().entity_mut(owner).insert(Feed {
            pending: HashMap::from([("old".into(), (record.clone(), Kind::Saved, Instant::now()))]),
            next: Instant::now(),
            records: vec![record.clone()],
            person: String::new(),
            authority: Some("11".repeat(32)),
        });
        let invitation = nucleus::location::Invitation {
            record_uid: record.clone(),
            authority_node_id: "22".repeat(32),
        };
        let field = crate::sand_panel::field(
            app.world_mut(),
            owner,
            "Reference",
            &invitation.reference().unwrap(),
        );
        Command::SharedReference(field).apply(app.world_mut(), owner);
        assert!(app.world().get::<Feed>(owner).is_none());
        assert_eq!(
            app.world().get::<MapSand>(owner).unwrap().authority_node_id,
            Some("22".repeat(32))
        );
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(
                cell::ServerMessage::ActionOk {
                    id: "old".into(),
                    created: None,
                    facts: 0,
                    warnings: vec![],
                    data: Some(serde_json::json!({"record_uid": record, "selected": {
                "assertion_uid": null, "record_uid": record, "head": "Old place", "slug": null,
                "latitude": -23.5, "longitude": -46.6, "radius_metres": null,
            }, "candidates": [], "explicit": false, "unavailable": false, "revision": 0})),
                },
            ));
        app.update();
        assert!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .markers
                .is_empty()
        );
    }

    #[cfg_attr(test, tokio::test)]
    async fn four_lince_transport_address_permissions_draw_exact_points_and_configurable_areas() {
        let evidence = simulation::location_workflows::transport_with_address().await;
        let mut app = crate::sand_panel::tests::app();
        app.add_plugins(crate::sand::SandPlugin)
            .init_resource::<Assets<Image>>()
            .init_resource::<render::Scenes>()
            .init_resource::<settings::Sources>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, receive);
        let root = app
            .world_mut()
            .spawn((
                crate::container::BoxRoot,
                crate::workspace::Workspaces::default(),
            ))
            .id();
        let mut owners = Vec::new();
        for (index, name) in ["b", "family", "outsider"].into_iter().enumerate() {
            let owner = spawn(
                app.world_mut(),
                root,
                1,
                DVec2::new(index as f64 * 900.0, 0.0),
                Camera::default(),
                vec![evidence.record_uid.clone()],
                String::new(),
            );
            app.world_mut()
                .get_mut::<MapSand>(owner)
                .unwrap()
                .authority_node_id = Some(evidence.authority_node_id.clone());
            let mut pending = HashMap::new();
            for (kind, suffix) in [(Kind::Saved, "saved"), (Kind::Live, "live")] {
                let id = format!("{name}-{suffix}");
                pending.insert(
                    id.clone(),
                    (evidence.record_uid.clone(), kind, Instant::now()),
                );
                let message = if let Some(projection) = evidence.viewers.get(name) {
                    cell::ServerMessage::ActionOk {
                        id,
                        created: None,
                        facts: 0,
                        warnings: vec![],
                        data: Some(match kind {
                            Kind::Saved => serde_json::to_value(&projection.saved).unwrap(),
                            _ => serde_json::to_value(&projection.live).unwrap(),
                        }),
                    }
                } else {
                    cell::ServerMessage::Error {
                        id,
                        message: "Location is private".into(),
                        code: None,
                    }
                };
                app.world_mut()
                    .write_message(crate::cell_bridge::CellMessage(message));
            }
            app.world_mut().entity_mut(owner).insert(Feed {
                pending,
                next: Instant::now(),
                records: vec![evidence.record_uid.clone()],
                person: String::new(),
                authority: Some(evidence.authority_node_id.clone()),
            });
            owners.push(owner);
        }
        app.update();
        for owner in &owners {
            let mut sand = app.world_mut().get_mut::<MapSand>(*owner).unwrap();
            sand.auto_center = false;
            sand.camera = Camera {
                center: Coordinate {
                    latitude: -23.5,
                    longitude: -46.6,
                },
                zoom: 12.0,
            };
        }
        ui::update(app.world_mut());
        app.update();
        for (index, radius) in [(0, None), (1, Some(3000))] {
            let sand = app.world().get::<MapSand>(owners[index]).unwrap();
            assert_eq!(sand.markers.len(), 2);
            assert!(
                sand.markers
                    .iter()
                    .all(|marker| marker.radius_metres == radius)
            );
            let overlay = sand.overlay;
            let shapes: Vec<_> = app
                .world()
                .get::<Children>(overlay)
                .unwrap()
                .iter()
                .filter_map(|entity| app.world().get::<Node>(entity))
                .collect();
            assert_eq!(shapes.len(), 2);
            assert!(
                shapes
                    .iter()
                    .all(|node| node.border_radius == BorderRadius::MAX)
            );
            assert!(shapes.iter().all(|node| matches!(node.width, Val::Px(width) if if radius.is_some() { width > 100.0 } else { width == 12.0 })));
        }
        assert!(
            app.world()
                .get::<MapSand>(owners[2])
                .unwrap()
                .markers
                .is_empty()
        );
        let saved = snapshot(app.world_mut(), root);
        assert_eq!(saved.len(), 3);
        assert!(saved.iter().all(SavedMap::valid));
        let serialized = serde_json::to_string(&saved).unwrap();
        assert!(!serialized.contains("-23.5"));
        assert!(!serialized.contains("-46.6"));
        assert!(!serialized.contains("radius_metres"));
        let owner = owners[1];
        app.world_mut()
            .get_mut::<Feed>(owner)
            .unwrap()
            .pending
            .insert(
                "family-revoked".into(),
                (evidence.record_uid.clone(), Kind::Live, Instant::now()),
            );
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(
                cell::ServerMessage::Error {
                    id: "family-revoked".into(),
                    message: evidence.family_live_denial,
                    code: None,
                },
            ));
        app.update();
        ui::update(app.world_mut());
        let sand = app.world().get::<MapSand>(owner).unwrap();
        assert_eq!(sand.markers.len(), 1);
        assert!(!sand.markers[0].live);
        assert_eq!(app.world().get::<Children>(sand.overlay).unwrap().len(), 1);
        assert_eq!(
            app.world().get::<MapSand>(owners[0]).unwrap().markers.len(),
            2
        );
    }

    #[cfg_attr(test, test)]
    fn permissions_are_observed_per_map_and_denial_removes_previous_fix() {
        let mut app = crate::sand_panel::tests::app();
        app.init_resource::<Assets<Image>>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, receive);
        let root = app.world_mut().spawn_empty().id();
        let record = nucleus::new_uid("r");
        let now = chrono::Utc::now().timestamp_millis();
        let mut owners = Vec::new();
        let view = nucleus::location::View {
            record_uid: record.clone(),
            status: nucleus::location::Status::Live,
            session_uid: Some(nucleus::new_uid("location")),
            expires_at_ms: Some(now + 30_000),
            source_kind: None,
            fix: Some(nucleus::location::Fix {
                sequence: 1,
                latitude: -23.55,
                longitude: -46.63,
                accuracy_metres: Some(10.0),
                captured_at_ms: now,
            }),
            age_ms: Some(0),
            radius_metres: None,
        };
        for index in 0..3 {
            let owner = mount(
                app.world_mut(),
                root,
                Camera::default(),
                vec![record.clone()],
                nucleus::new_uid("r"),
            );
            let id = format!("observe-{index}");
            app.world_mut().entity_mut(owner).insert(Feed {
                pending: HashMap::from([(
                    id.clone(),
                    (record.clone(), Kind::Live, Instant::now()),
                )]),
                next: Instant::now(),
                records: vec![record.clone()],
                person: String::new(),
                authority: None,
            });
            app.world_mut()
                .write_message(crate::cell_bridge::CellMessage(if index < 2 {
                    cell::ServerMessage::ActionOk {
                        id,
                        created: None,
                        facts: 0,
                        warnings: Vec::new(),
                        data: Some(serde_json::to_value(&view).unwrap()),
                    }
                } else {
                    cell::ServerMessage::Error {
                        id,
                        message: "Location is private".into(),
                        code: None,
                    }
                }));
            owners.push(owner);
        }
        app.update();
        assert_eq!(
            app.world().get::<MapSand>(owners[0]).unwrap().markers.len(),
            1
        );
        assert_eq!(
            app.world().get::<MapSand>(owners[1]).unwrap().markers.len(),
            1
        );
        assert!(
            app.world()
                .get::<MapSand>(owners[2])
                .unwrap()
                .markers
                .is_empty()
        );
        let owner = owners[0];
        app.world_mut()
            .get_mut::<Feed>(owner)
            .unwrap()
            .pending
            .insert(
                "revoked".into(),
                (record.clone(), Kind::Live, Instant::now()),
            );
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(
                cell::ServerMessage::Error {
                    id: "revoked".into(),
                    message: "Location is private".into(),
                    code: None,
                },
            ));
        app.update();
        assert!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .markers
                .is_empty()
        );
        assert_eq!(
            app.world().get::<MapSand>(owners[1]).unwrap().markers.len(),
            1
        );
        let mut stale = view;
        stale.fix.as_mut().unwrap().captured_at_ms = now - 70_000;
        add_live(app.world_mut(), owner, &record, stale);
        assert!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .markers
                .is_empty()
        );
    }

    crate::laboratory_cases! {
        changing_shared_authority_discards_pending_location_replies,
        async four_lince_transport_address_permissions_draw_exact_points_and_configurable_areas timeout 120,
        permissions_are_observed_per_map_and_denial_removes_previous_fix,
    }
}

pub(super) fn save_point(world: &mut World, owner: Entity) {
    let Some(sand) = world.get::<MapSand>(owner) else {
        return;
    };
    let Some(point) = sand.picked else {
        world.get_mut::<MapSand>(owner).unwrap().notice = "Click a reference point first".into();
        return;
    };
    if sand.records.len() != 1 {
        world.get_mut::<MapSand>(owner).unwrap().notice =
            "Open one Record to save its reference point".into();
        return;
    }
    let record = sand.records[0].clone();
    if world.get::<Feed>(owner).is_none() {
        world.entity_mut(owner).insert(Feed {
            pending: HashMap::new(),
            next: Instant::now(),
            records: vec![record.clone()],
            person: String::new(),
            authority: None,
        });
    }
    request(
        world,
        owner,
        &record,
        Kind::Save,
        engine::actions::Action::SetPlace {
            target: record.clone(),
            lat: point.latitude,
            lon: point.longitude,
            address: None,
        },
    );
}
