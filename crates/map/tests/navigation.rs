use lince_map::{
    Coordinate,
    navigation::{Geocoder, Mode, Place, Provider, Route, Router, Selection, Settings},
};

#[tokio::test]
async fn local_search_requires_selection_and_refuses_stale_results() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("places.json");
    let place = Place {
        name: "Street Four".into(),
        coordinate: Coordinate {
            latitude: 10.0,
            longitude: 20.0,
        },
        credits: "Test fixture".into(),
    };
    std::fs::write(&path, serde_json::to_vec(&vec![place.clone()]).unwrap()).unwrap();
    let provider = Provider::new(Settings {
        places: path,
        ..Default::default()
    })
    .unwrap();
    let results = provider.search("street").await.unwrap();
    let mut selection = Selection::default();
    let first = selection.invalidate();
    assert!(selection.accept_places(first, results.clone()));
    assert!(selection.selected.is_none());
    assert_eq!(selection.choose(0), Some(&place));
    let second = selection.invalidate();
    assert!(selection.selected.is_none());
    assert!(!selection.accept_places(first, results.clone()));
    assert!(selection.accept_places(second, results));
    assert_eq!(selection.choose(0), Some(&place));
    assert_eq!(provider.reverse(place.coordinate).await.unwrap(), place);
    assert!(
        provider
            .reverse(Coordinate {
                latitude: 0.0,
                longitude: 0.0
            })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn routing_is_opt_in_bounded_and_old_geometry_cannot_replace_a_new_request() {
    let a = Coordinate {
        latitude: 10.0,
        longitude: 20.0,
    };
    let b = Coordinate {
        latitude: 10.1,
        longitude: 20.1,
    };
    let provider = Provider::new(Settings {
        router: "https://unused.invalid".into(),
        ..Default::default()
    })
    .unwrap();
    assert!(
        provider
            .route(&[a, b], Mode::Walking, 1)
            .await
            .unwrap_err()
            .contains("explicitly")
    );
    assert!(provider.route(&[a], Mode::Walking, 1).await.is_err());
    assert!(
        Provider::new(Settings {
            router: "http://external.invalid".into(),
            ..Default::default()
        })
        .is_err()
    );
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
            request.push(stream.read_u8().await.unwrap());
        }
        let body = r#"{"code":"Ok","routes":[{"distance":500.0,"duration":600.0,"geometry":{"coordinates":[[20.0,10.0],[20.1,10.1]]}}]}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        String::from_utf8(request).unwrap()
    });
    let online = Provider::new(Settings {
        router: format!("http://{address}"),
        online: true,
        credits: "Fixture roads".into(),
        ..Default::default()
    })
    .unwrap();
    let decoded = online.route(&[a, b], Mode::Walking, 1).await.unwrap();
    assert_eq!(decoded.points, vec![a, b]);
    assert_eq!(decoded.duration_seconds, 600.0);
    assert_eq!(decoded.mode, Mode::Walking);
    let outgoing = server.await.unwrap();
    assert!(outgoing.starts_with("GET /route/v1/walking/20,10;20.1,10.1?"));
    assert!(outgoing.contains("geometries=geojson"));
    assert!(
        online
            .route(&[a, b], Mode::Walking, 1)
            .await
            .unwrap_err()
            .contains("configured interval")
    );
    let mut selection = Selection::default();
    let old = selection.invalidate();
    let current = selection.invalidate();
    let route = Route {
        points: vec![a, b],
        distance_metres: 500.0,
        duration_seconds: 600.0,
        calculated_at: 1,
        mode: Mode::Walking,
        credits: "Fixture".into(),
    };
    assert!(!selection.accept_route(old, route.clone()));
    assert!(selection.route.is_none());
    assert!(selection.accept_route(current, route));
    selection.invalidate();
    assert!(selection.route.is_none());
}
