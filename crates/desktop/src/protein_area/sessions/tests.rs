use super::*;
use serde_json::json;

fn connection(
    app: &mut App,
    organ: &str,
) -> (
    tokio::sync::mpsc::Receiver<ClientMessage>,
    tokio::sync::mpsc::Sender<ServerMessage>,
) {
    let (outgoing, requests) = tokio::sync::mpsc::channel(32);
    let (responses, incoming) = tokio::sync::mpsc::channel(32);
    let task = tokio::spawn(std::future::pending());
    app.world_mut().resource_mut::<Runtime>().sessions.insert(
        organ.into(),
        Session::connected(Remote {
            outgoing,
            incoming,
            task,
        }),
    );
    (requests, responses)
}

fn area(app: &mut App, root: Entity, organ: &str, title: &str) -> Entity {
    let mut area = InfluenceArea::new(
        crate::area::AreaShape::Square,
        bevy::math::DVec2::ZERO,
        bevy::math::DVec2::splat(500.0),
    );
    let mut config = Config {
        source: Source::Organ(organ.into()),
        ..default()
    };
    config.draft.query["where"] = json!([{"all":[{"uid_eq": title}]}]);
    area.protein = Some(config.clone());
    let owner = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    start(app.world_mut(), owner, config);
    owner
}

fn authenticated() -> ServerMessage {
    ServerMessage::SessionAuthenticated {
        id: "auth".into(),
        session_id: "session".into(),
        person: "alice".into(),
        key_id: "key".into(),
    }
}

fn subscription(app: &App, owner: Entity) -> String {
    app.world().resource::<Runtime>().areas[&owner]
        .subscription
        .clone()
        .unwrap()
}

#[tokio::test]
async fn one_login_unlocks_current_and_later_areas_and_replies_stay_scoped() {
    let (mut app, root, local) = super::super::tests::fixture();
    let (mut requests, responses) = connection(&mut app, "organ-a");
    let (mut other_requests, other_responses) = connection(&mut app, "organ-b");
    let first = area(&mut app, root, "organ-a", "First");
    let second = area(&mut app, root, "organ-a", "Second");
    let other = area(&mut app, root, "organ-b", "Other");
    responses
        .try_send(ServerMessage::LiveHello {
            login_required: true,
        })
        .unwrap();
    other_responses
        .try_send(ServerMessage::LiveHello {
            login_required: true,
        })
        .unwrap();
    app.update();
    assert!(requests.try_recv().is_err());
    assert_eq!(app.world().resource::<Runtime>().sessions.len(), 2);
    login(app.world_mut(), "organ-a", "alice".into(), "secret".into()).unwrap();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::LiveLogin { username, password } if username == "alice" && password == "secret")
    );
    assert!(login(app.world_mut(), "organ-a", "bob".into(), "different".into()).is_err());
    for owner in [first, second] {
        assert!(app.world().resource::<Runtime>().areas[&owner].login_pending);
    }
    assert!(!app.world().resource::<Runtime>().areas[&other].login_pending);
    let waiting = area(&mut app, root, "organ-a", "Waiting");
    assert!(app.world().resource::<Runtime>().areas[&waiting].login_pending);
    responses.try_send(authenticated()).unwrap();
    app.update();
    let ids: HashSet<_> = (0..3)
        .map(|_| match requests.try_recv().unwrap() {
            ClientMessage::Subscribe { id, .. } => id,
            message => panic!("Unexpected message: {message:?}"),
        })
        .collect();
    assert_eq!(
        ids,
        HashSet::from([
            subscription(&app, first),
            subscription(&app, second),
            subscription(&app, waiting)
        ])
    );
    assert!(other_requests.try_recv().is_err());
    assert!(!app.world().resource::<Runtime>().areas[&other].ready);
    responses
        .try_send(ServerMessage::Snapshot {
            id: subscription(&app, first),
            rows: vec![json!({"uid":"first", "head":"First"})],
        })
        .unwrap();
    responses
        .try_send(ServerMessage::Snapshot {
            id: subscription(&app, second),
            rows: vec![json!({"uid":"second", "head":"Second"})],
        })
        .unwrap();
    app.update();
    assert_eq!(
        app.world().resource::<Runtime>().areas[&first].data[0]["uid"],
        "first"
    );
    assert_eq!(
        app.world().resource::<Runtime>().areas[&second].data[0]["uid"],
        "second"
    );
    for owner in [local, other] {
        assert!(
            app.world().resource::<Runtime>().areas[&owner]
                .data
                .is_empty()
        );
    }
    let later = area(&mut app, root, "organ-a", "Later");
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Subscribe { id, .. } if id == subscription(&app, later))
    );
    assert_eq!(app.world().resource::<Runtime>().sessions.len(), 2);
    assert!(requests.try_recv().is_err());
    assert!(app.world().resource::<Runtime>().areas[&later].ready);
    let previous = subscription(&app, later);
    app.world_mut()
        .get_mut::<InfluenceArea>(later)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .source = Source::Organ("organ-b".into());
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == previous)
    );
    responses
        .try_send(ServerMessage::Snapshot {
            id: previous,
            rows: vec![json!({"uid":"stale"})],
        })
        .unwrap();
    app.update();
    assert!(
        app.world().resource::<Runtime>().areas[&later]
            .data
            .is_empty()
    );
    assert!(!app.world().resource::<Runtime>().areas[&later].ready);
    assert!(other_requests.try_recv().is_err());
}

#[tokio::test]
async fn pausing_editing_and_closing_areas_preserve_the_shared_login_and_unsubscribe_individually()
{
    let (mut app, root, _) = super::super::tests::fixture();
    let (mut requests, responses) = connection(&mut app, "organ-a");
    let first = area(&mut app, root, "organ-a", "First");
    let second = area(&mut app, root, "organ-a", "Second");
    responses.try_send(authenticated()).unwrap();
    app.update();
    requests.try_recv().unwrap();
    requests.try_recv().unwrap();
    let previous = subscription(&app, first);
    app.world_mut()
        .get_mut::<InfluenceArea>(first)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .enabled = false;
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == previous)
    );
    assert!(app.world().resource::<Runtime>().areas[&second].ready);
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(first).unwrap();
        let config = area.protein.as_mut().unwrap();
        config.enabled = true;
        config.draft.query["limit"] = json!(7);
    }
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Subscribe { id, .. } if id == subscription(&app, first))
    );
    assert_eq!(
        app.world().resource::<Runtime>().sessions["organ-a"].phase,
        Phase::Ready
    );
    let previous = subscription(&app, second);
    app.world_mut().despawn(second);
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == previous)
    );
    assert!(app.world().resource::<Runtime>().areas[&first].ready);
    let previous = subscription(&app, first);
    app.world_mut()
        .get_mut::<InfluenceArea>(first)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .enabled = false;
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == previous)
    );
    assert_eq!(
        app.world().resource::<Runtime>().sessions["organ-a"].phase,
        Phase::Ready
    );
    app.world_mut().despawn(first);
    app.update();
    assert!(
        !app.world()
            .resource::<Runtime>()
            .sessions
            .contains_key("organ-a")
    );
    assert!(requests.is_closed());
}

#[tokio::test]
async fn disconnect_locks_every_area_clears_data_and_never_replays_mutations_on_reconnect() {
    let (mut app, root, _) = super::super::tests::fixture();
    let (mut requests, responses) = connection(&mut app, "organ-a");
    let (mut other_requests, other_responses) = connection(&mut app, "organ-b");
    let first = area(&mut app, root, "organ-a", "First");
    let second = area(&mut app, root, "organ-a", "Second");
    let other = area(&mut app, root, "organ-b", "Other");
    responses.try_send(authenticated()).unwrap();
    other_responses.try_send(authenticated()).unwrap();
    app.update();
    requests.try_recv().unwrap();
    requests.try_recv().unwrap();
    other_requests.try_recv().unwrap();
    for owner in [first, second] {
        responses
            .try_send(ServerMessage::Snapshot {
                id: subscription(&app, owner),
                rows: vec![json!({"uid":format!("row-{owner}"), "head":"Record"})],
            })
            .unwrap();
    }
    app.update();
    let rows: Vec<_> = [first, second]
        .into_iter()
        .flat_map(|owner| {
            app.world().resource::<Runtime>().areas[&owner]
                .row_entities
                .values()
                .copied()
        })
        .collect();
    app.world_mut()
        .resource_mut::<Runtime>()
        .areas
        .get_mut(&first)
        .unwrap()
        .pending
        .push_back(ClientMessage::Act {
            id: "pending-delete".into(),
            action: engine::actions::Action::DeleteRecord {
                target: "record".into(),
            },
        });
    responses
        .try_send(ServerMessage::Error {
            id: "connection".into(),
            message: "Session expired".into(),
            code: None,
        })
        .unwrap();
    app.update();
    for owner in [first, second] {
        let state = &app.world().resource::<Runtime>().areas[&owner];
        assert!(!state.ready);
        assert!(state.data.is_empty());
        assert!(state.pending.is_empty());
        assert_eq!(state.status, "Session expired");
    }
    for row in rows {
        assert!(app.world().get_entity(row).is_err());
    }
    assert!(app.world().resource::<Runtime>().areas[&other].ready);
    assert!(requests.try_recv().is_err());
    let old = subscription(&app, first);
    app.world_mut()
        .resource_mut::<Runtime>()
        .sessions
        .get_mut("organ-a")
        .unwrap()
        .retry_at = Some(Instant::now());
    app.update();
    assert_ne!(subscription(&app, first), old);
    for owner in [first, second] {
        assert!(
            app.world().resource::<Runtime>().areas[&owner]
                .pending
                .iter()
                .all(|message| matches!(message, ClientMessage::Subscribe { .. }))
        );
    }
}
