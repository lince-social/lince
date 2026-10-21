#[path = "offline_map/packs.rs"]
mod packs;

use geozero::mvt::{Message, Tile as VectorTile, tile::Layer};
use lince_map::{Bounds, Camera, Coordinate, Settings, Tile, TileStore};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn fixture() -> Vec<u8> {
    VectorTile {
        layers: vec![
            Layer {
                name: "streets".into(),
                version: 2,
                extent: Some(4096),
                ..Default::default()
            },
            Layer {
                name: "buildings".into(),
                version: 2,
                extent: Some(4096),
                ..Default::default()
            },
            Layer {
                name: "water_polygons".into(),
                version: 2,
                extent: Some(4096),
                ..Default::default()
            },
        ],
    }
    .encode_to_vec()
}

#[test]
fn coordinates_and_pointer_anchor_agree_at_street_detail() {
    for center in [
        Coordinate {
            latitude: -23.5505,
            longitude: -46.6333,
        },
        Coordinate {
            latitude: 65.0,
            longitude: 179.99,
        },
    ] {
        let mut camera = Camera { center, zoom: 18.0 };
        let size = [900.0, 500.0];
        let clicked = [315.0, 217.0];
        let before = camera.coordinate(clicked, size);
        let screen = camera.screen(before, size);
        assert!((screen[0] - clicked[0]).abs() < 0.00001);
        assert!((screen[1] - clicked[1]).abs() < 0.00001);
        camera.zoom_at(1.5, clicked, size);
        let after = camera.coordinate(clicked, size);
        assert!((before.latitude - after.latitude).abs() < 1e-9);
        assert!((before.longitude - after.longitude).abs() < 1e-9);
    }
    assert_eq!(
        Coordinate::from_normalized(0.5, 0.5),
        Coordinate {
            latitude: 0.0,
            longitude: 0.0
        }
    );
}

#[test]
fn world_and_dateline_selections_are_bounded_before_allocating() {
    let world = Bounds {
        west: -180.0,
        east: 180.0,
        south: -85.0,
        north: 85.0,
    };
    assert_eq!(world.tiles(0).unwrap(), vec![Tile { z: 0, x: 0, y: 0 }]);
    assert!(world.tiles(14).is_err());
    assert!(world.pyramid(14).is_err());
    let dateline = Bounds {
        west: 179.9,
        east: -179.9,
        south: -0.1,
        north: 0.1,
    };
    let tiles = dateline.tiles(10).unwrap();
    assert!(
        tiles
            .iter()
            .all(|tile| tile.valid() && (tile.x == 0 || tile.x == 1023))
    );
    assert_eq!(
        Camera::default()
            .area([0.0, 0.0], [900.0, 400.0], [900.0, 400.0])
            .west,
        -180.0
    );
    let camera = Camera {
        center: Coordinate {
            latitude: 0.0,
            longitude: 0.0,
        },
        zoom: 2.0,
    };
    let area = camera.area([50.0, 10.0], [950.0, 490.0], [1000.0, 500.0]);
    assert!(area.west < -150.0 && area.east > 150.0);
    let [a, b] = camera.area_screen(area, [1000.0, 500.0]);
    assert!((b[0] - a[0] - 900.0).abs() < 1e-9);
    assert_eq!(
        Camera {
            zoom: 3.5,
            ..camera
        }
        .data_zoom(14),
        4
    );
    assert!(camera.visible_tiles([900.0, 500.0], 14).len() <= 30);
    assert!(
        Camera {
            zoom: 20.0,
            ..camera
        }
        .visible_tiles([900.0, 500.0], 14)
        .iter()
        .all(|tile| tile.z == 14)
    );
}

#[test]
fn compressed_tiles_are_validated_and_buildings_are_removed() {
    let normalized = lince_map::source::normalize(&fixture()).unwrap();
    let tile = VectorTile::decode(normalized).unwrap();
    assert_eq!(
        tile.layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect::<Vec<_>>(),
        vec!["streets", "water_polygons"]
    );
    assert!(lince_map::source::normalize(b"not a vector tile").is_err());
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(
        &mut encoder,
        &vec![0; lince_map::source::MAX_DECODED_BYTES + 1],
    )
    .unwrap();
    assert!(lince_map::source::normalize(&encoder.finish().unwrap()).is_err());
}

async fn server(body: Vec<u8>, count: Arc<AtomicUsize>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/{{z}}/{{x}}/{{y}}",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let length = socket.read(&mut request).await.unwrap();
            assert!(!String::from_utf8_lossy(&request[..length]).contains("record"));
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(header.as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
        }
    });
    (url, task)
}

#[tokio::test]
async fn concurrent_castles_share_one_download_and_reopen_offline() {
    let directory = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let (url, server) = server(fixture(), count.clone()).await;
    let settings = Settings {
        server: url,
        dataset_version: "test-v1".into(),
        online: true,
        ..Default::default()
    };
    let store = TileStore::new(settings.clone(), directory.path()).unwrap();
    let tile = Tile { z: 5, x: 10, y: 10 };
    let mut jobs = Vec::new();
    for _ in 0..4 {
        let store = store.clone();
        jobs.push(tokio::spawn(async move { store.load(tile).await.unwrap() }));
    }
    for job in jobs {
        assert!(!job.await.unwrap().is_empty());
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let progress = store.progress(&[tile]);
    assert!(progress.finished());
    assert_eq!(progress.completed, 1);
    assert!(progress.downloaded_bytes > 0);
    server.abort();
    let offline = TileStore::new(
        Settings {
            online: false,
            ..settings
        },
        directory.path(),
    )
    .unwrap();
    assert!(offline.load(tile).await.is_ok());
    assert!(offline.load(Tile { x: 11, ..tile }).await.is_err());
    assert_eq!(offline.progress(&[tile]).downloaded_bytes, 0);
}

#[tokio::test]
async fn local_tiles_win_and_cancelled_area_download_does_not_fetch() {
    let directory = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(local.path().join("4/2")).unwrap();
    std::fs::write(local.path().join("4/2/3.pbf"), fixture()).unwrap();
    packs::manifest(
        local.path(),
        lince_map::pack::Format::Directory,
        &["4/2/3.pbf"],
        "local-v1",
    );
    let store = TileStore::new(
        Settings {
            local_directory: local.path().into(),
            online: false,
            ..Default::default()
        },
        directory.path(),
    )
    .unwrap();
    let tile = Tile { z: 4, x: 2, y: 3 };
    assert!(store.load(tile).await.is_ok());
    let cancelled = Arc::new(AtomicBool::new(true));
    let progress = store.download(vec![Tile { x: 3, ..tile }], cancelled).await;
    assert!(progress.cancelled);
    assert_eq!(progress.downloaded_bytes, 0);
}

#[test]
fn map_sources_require_bounded_zoom_storage_and_valid_urls() {
    assert!(Settings::default().validate().is_ok());
    for server in [
        "file:///tmp/{z}/{x}/{y}",
        "http://example.com/{z}/{x}/{y}",
        "https://user:secret@example.com/{z}/{x}/{y}",
        "https://example.com/map",
    ] {
        assert!(
            Settings {
                server: server.into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Settings {
            max_zoom: 15,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Settings {
            storage_megabytes: 0,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Settings {
            local_directory: std::path::PathBuf::from("elsewhere"),
            ..Default::default()
        }
        .source_id()
            == Settings::default().source_id()
    );
}

#[tokio::test]
async fn packaged_pmtiles_works_offline_and_rejects_malformed_sections() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("region.pmtiles");
    let mut writer = pmtiles::PmTilesWriter::new(pmtiles::TileType::Mvt)
        .metadata(r#"{"vector_layers":[{"id":"streets"}]}"#)
        .create(std::fs::File::create(&path).unwrap())
        .unwrap();
    writer
        .add_tile(pmtiles::TileCoord::new(5, 10, 10).unwrap(), &fixture())
        .unwrap();
    writer.finalize().unwrap();
    packs::manifest(
        directory.path(),
        lince_map::pack::Format::Pmtiles,
        &["region.pmtiles"],
        "pmtiles-v1",
    );
    let settings = Settings {
        archives: vec![path.clone()],
        online: false,
        ..Default::default()
    };
    let store = TileStore::new(settings.clone(), directory.path().join("cache")).unwrap();
    let tile = Tile { z: 5, x: 10, y: 10 };
    let data = VectorTile::decode(store.load(tile).await.unwrap()).unwrap();
    assert!(data.layers.iter().all(|layer| layer.name != "buildings"));
    assert_eq!(store.progress(&[tile]).downloaded_bytes, 0);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    packs::manifest(
        directory.path(),
        lince_map::pack::Format::Pmtiles,
        &["region.pmtiles"],
        "pmtiles-bad-sections",
    );
    let store = TileStore::new(settings, directory.path().join("cache")).unwrap();
    assert!(store.load(tile).await.unwrap_err().contains("sections"));
}

#[tokio::test]
async fn invalid_server_payload_is_not_cached_or_reported_as_complete() {
    let directory = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let (url, server) = server(b"invalid vector data".to_vec(), count).await;
    let store = TileStore::new(
        Settings {
            server: url,
            dataset_version: "test-v1".into(),
            online: true,
            ..Default::default()
        },
        directory.path(),
    )
    .unwrap();
    let tile = Tile { z: 5, x: 10, y: 10 };
    assert!(store.load(tile).await.is_err());
    let progress = store.progress(&[tile]);
    assert_eq!(progress.failed, 1);
    assert_eq!(progress.completed, 0);
    assert!(
        !directory
            .path()
            .join(store.settings().source_id())
            .join("5/10/10.pbf")
            .exists()
    );
    server.abort();
}

#[tokio::test]
async fn cancelling_a_running_area_keeps_completed_tiles_and_stops_more_requests() {
    let directory = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let (url, server) = server(fixture(), count.clone()).await;
    let store = TileStore::new(
        Settings {
            server: url,
            dataset_version: "test-v1".into(),
            online: true,
            ..Default::default()
        },
        directory.path(),
    )
    .unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let job = tokio::spawn(store.clone().download(
        (0..16).map(|x| Tile { z: 5, x, y: 10 }).collect(),
        cancelled.clone(),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while count.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancelled.store(true, Ordering::Relaxed);
    let progress = job.await.unwrap();
    assert!(progress.cancelled);
    assert!(progress.completed < progress.total);
    assert!(count.load(Ordering::SeqCst) <= 4);
    server.abort();
}

#[tokio::test]
async fn world_scene_clamps_polar_centers_to_the_flat_projection() {
    use lince_map::galileo::galileo_types::geo::GeoPoint;
    let directory = tempfile::tempdir().unwrap();
    let store = TileStore::new(
        Settings {
            online: false,
            ..Default::default()
        },
        directory.path(),
    )
    .unwrap();
    let palette = lince_map::scene::Palette {
        land: [30, 30, 30, 255],
        water: [20, 20, 40, 255],
        roads: [200, 200, 200, 255],
        nature: [30, 40, 30, 255],
        ink: [255; 4],
    };
    for latitude in [-90.0, 0.0, 90.0] {
        let scene = lince_map::scene::Scene::new(
            store.clone(),
            Camera {
                center: Coordinate {
                    latitude,
                    longitude: 0.0,
                },
                zoom: 1.0,
            },
            [800, 360],
            palette,
            None,
        )
        .unwrap();
        let position = scene.map.view().position().unwrap();
        assert!(position.lat().is_finite());
        assert!(position.lat().abs() <= lince_map::geometry::MAX_LATITUDE + 1e-8);
        assert!(scene.map.view().get_bbox().is_some());
    }
}

#[tokio::test]
async fn zooming_fetches_visible_street_detail_and_reuses_it_without_area_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let (url, server) = server(fixture(), count.clone()).await;
    let settings = Settings {
        server: url,
        ..Settings::browsing()
    };
    settings.validate().unwrap();
    let store = TileStore::new(settings.clone(), directory.path()).unwrap();
    let palette = lince_map::scene::Palette {
        land: [30, 30, 30, 255],
        water: [20, 20, 40, 255],
        roads: [200, 200, 200, 255],
        nature: [30, 40, 30, 255],
        ink: [255; 4],
    };
    let mut scene =
        lince_map::scene::Scene::new(store.clone(), Camera::default(), [400, 400], palette, None)
            .unwrap();
    scene.load();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let camera = Camera {
        center: Coordinate {
            latitude: -23.5505,
            longitude: -46.6333,
        },
        zoom: 16.0,
    };
    let visible = camera.visible_tiles([400.0, 400.0], settings.max_zoom);
    let pending = lince_map::source::progress_caption(&store.progress(&visible), 0);
    assert!(pending.contains("Loading map"));
    scene.set_camera(camera, [400, 400]);
    scene.load();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let progress = store.progress(&visible);
            if progress.completed == visible.len() {
                break;
            }
            assert_eq!(progress.failed, 0, "{:?}", progress.errors);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let progress = store.progress(&visible);
    assert!(progress.downloaded_bytes > 0);
    assert!(lince_map::source::progress_caption(&progress, 0).contains("saved on this device"));
    let requests = count.load(Ordering::SeqCst);
    assert!(requests > 0);
    drop(scene);
    let offline = TileStore::new(
        Settings {
            online: false,
            ..settings
        },
        directory.path(),
    )
    .unwrap();
    for tile in &visible {
        offline.load(*tile).await.unwrap();
    }
    assert_eq!(offline.progress(&visible).downloaded_bytes, 0);
    let unavailable = Tile { z: 14, x: 0, y: 0 };
    assert!(offline.load(unavailable).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), requests);
    server.abort();
}
