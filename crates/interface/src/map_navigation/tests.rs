use super::*;

fn fixture(path: PathBuf) -> (World, Entity, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<Typography>();
    let owner = world.spawn(Node::default()).id();
    mount(
        &mut world,
        owner,
        owner,
        path,
        Coordinate {
            latitude: 10.0,
            longitude: 20.0,
        },
        Some(tokio::runtime::Handle::current()),
    );
    let panel = world.query::<(Entity, &Panel)>().single(&world).unwrap().0;
    assert!(
        world
            .query::<(Entity, &Button)>()
            .iter(&world)
            .all(|(entity, _)| world.get::<Node>(entity).is_some())
    );
    update(
        &mut world,
        owner,
        Coordinate {
            latitude: 10.0,
            longitude: 20.0,
        },
        "alice",
    );
    (world, owner, panel)
}

async fn settle(world: &mut World, panel: Entity) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while world.get::<Panel>(panel).unwrap().job.is_some() {
            poll(world);
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn choosing_a_search_result_is_unsaved_and_scope_change_clears_private_query() {
    let directory = tempfile::tempdir().unwrap();
    let place = Place {
        name: "Private Street Four".into(),
        coordinate: Coordinate {
            latitude: 10.0,
            longitude: 20.0,
        },
        credits: "Fixture".into(),
    };
    let index = directory.path().join("places.json");
    let bytes = serde_json::to_vec(&vec![place.clone()]).unwrap();
    std::fs::write(&index, &bytes).unwrap();
    let path = directory.path().join("navigation.json");
    Settings {
        places: index.clone(),
        ..Default::default()
    }
    .save_atomic(&path)
    .unwrap();
    let saved = std::fs::read(&path).unwrap();
    let (mut world, owner, panel) = fixture(path.clone());
    let query = world.get::<Panel>(panel).unwrap().fields[0];
    world
        .get_mut::<bevy::text::EditableText>(query)
        .unwrap()
        .editor
        .set_text("Private Street");
    operate(&mut world, panel, Intent::Search).unwrap();
    settle(&mut world, panel).await;
    assert!(
        update(&mut world, owner, place.coordinate, "alice")
            .1
            .is_none()
    );
    operate(&mut world, panel, Intent::Choose(0)).unwrap();
    assert_eq!(
        update(&mut world, owner, place.coordinate, "alice").1,
        Some(place.clone())
    );
    assert!(
        update(&mut world, owner, place.coordinate, "alice")
            .1
            .is_none()
    );
    assert_eq!(std::fs::read(path).unwrap(), saved);
    assert_eq!(std::fs::read(index).unwrap(), bytes);
    operate(&mut world, panel, Intent::Origin).unwrap();
    update(&mut world, owner, place.coordinate, "bob");
    assert_eq!(controls::value(&world, query).unwrap(), "");
    assert!(world.get::<Panel>(panel).unwrap().origin.is_none());
    assert!(
        world
            .get::<Panel>(panel)
            .unwrap()
            .selection
            .selected
            .is_none()
    );
}

#[tokio::test]
async fn routing_sends_only_reviewed_visible_points_and_cancelled_replies_stay_cleared() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let directory = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let path = directory.path().join("navigation.json");
    Settings {
        router: format!("http://{}", listener.local_addr().unwrap()),
        online: true,
        ..Default::default()
    }
    .save_atomic(&path)
    .unwrap();
    let (sent, received) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
        }
        sent.send(String::from_utf8(request).unwrap()).unwrap();
        released.await.unwrap();
        let body = r#"{"code":"Ok","routes":[{"distance":500,"duration":600,"geometry":{"coordinates":[[20,10],[21,11]]}}]}"#;
        let _ = socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await;
    });
    let (mut world, owner, panel) = fixture(path);
    operate(&mut world, panel, Intent::Origin).unwrap();
    update(
        &mut world,
        owner,
        Coordinate {
            latitude: 11.0,
            longitude: 21.0,
        },
        "alice",
    );
    operate(&mut world, panel, Intent::Destination).unwrap();
    update(
        &mut world,
        owner,
        Coordinate {
            latitude: 33.0,
            longitude: 44.0,
        },
        "alice",
    );
    operate(&mut world, panel, Intent::Route).unwrap();
    let request = tokio::time::timeout(std::time::Duration::from_secs(5), received)
        .await
        .unwrap()
        .unwrap();
    assert!(request.starts_with("GET /route/v1/driving/20,10;21,11?"));
    assert!(!request.contains("44,33"));
    operate(&mut world, panel, Intent::Cancel).unwrap();
    update(
        &mut world,
        owner,
        Coordinate {
            latitude: 0.0,
            longitude: 0.0,
        },
        "bob",
    );
    release.send(()).unwrap();
    server.await.unwrap();
    tokio::task::yield_now().await;
    poll(&mut world);
    let state = world.get::<Panel>(panel).unwrap();
    assert!(state.job.is_none());
    assert!(state.origin.is_none() && state.destination.is_none());
    assert!(state.selection.route.is_none() && state.selected.is_none());
}
