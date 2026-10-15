use super::*;
use crate::native::actions::Action;

fn app() -> (App, Entity, Entity) {
    let mut app = crate::native::sand_panel::tests::app();
    app.init_resource::<Assets<Image>>()
        .add_plugins(VideoCastlePlugin);
    let world = app.world_mut();
    let root = world
        .spawn((
            crate::native::workspace::Workspaces::default(),
            crate::native::canvas::CanvasView::default(),
            ComputedNode {
                size: Vec2::new(800.0, 600.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
        ))
        .id();
    let sand = crate::native::sand_store::spawn_sand(
        world,
        root,
        1,
        crate::native::sand_store::SandKind::Video,
        "",
        bevy::math::DVec2::ZERO,
    );
    world.spawn((Window::default(), bevy::window::PrimaryWindow));
    (app, root, sand)
}

#[test]
fn castle_visibility_tracks_workspace_viewport_and_window_without_focus_requirement() {
    let (mut app, root, sand) = app();
    assert!(visible(app.world_mut(), root, sand));
    let entity = app
        .world_mut()
        .query_filtered::<Entity, With<Window>>()
        .single(app.world())
        .unwrap();
    app.world_mut().get_mut::<Window>(entity).unwrap().focused = false;
    assert!(visible(app.world_mut(), root, sand));
    app.world_mut().get_mut::<Window>(entity).unwrap().visible = false;
    assert!(!visible(app.world_mut(), root, sand));
    app.world_mut().get_mut::<Window>(entity).unwrap().visible = true;
    app.world_mut()
        .get_mut::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active = 2;
    assert!(!visible(app.world_mut(), root, sand));
    app.world_mut()
        .get_mut::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active = 1;
    app.world_mut()
        .get_mut::<crate::native::canvas::CanvasItem>(sand)
        .unwrap()
        .position
        .x = 100000.0;
    assert!(!visible(app.world_mut(), root, sand));
}

#[test]
fn video_reuses_texture_and_configuration_serializes_without_frames() {
    let (mut app, _, sand) = app();
    let fallback = Image::new(
        bevy::render::render_resource::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        vec![255; 4],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(Handle::<Image>::default().id(), fallback)
        .unwrap();
    let first = upload(
        app.world_mut(),
        Handle::default(),
        crate::native::media_sand::Pixels {
            width: 2,
            height: 1,
            rgba: vec![0; 8],
        },
    );
    for value in 0..100 {
        let handle = upload(
            app.world_mut(),
            first.clone(),
            crate::native::media_sand::Pixels {
                width: 2,
                height: 1,
                rgba: vec![value; 8],
            },
        );
        assert_eq!(handle.id(), first.id());
    }
    assert_eq!(app.world().resource::<Assets<Image>>().len(), 2);
    assert_eq!(
        app.world()
            .resource::<Assets<Image>>()
            .get(Handle::<Image>::default().id())
            .unwrap()
            .data
            .as_deref(),
        Some([255; 4].as_slice())
    );
    let settings = Settings {
        source: "live-camera".into(),
        file: false,
        paused: false,
        position_ms: 0,
    };
    app.world_mut().entity_mut(sand).insert(settings);
    let serialized = serde_json::to_value(app.world().get::<Settings>(sand).unwrap()).unwrap();
    assert_eq!(serialized["source"], "live-camera");
    assert!(serialized.get("pixels").is_none());
}

#[tokio::test]
async fn hiding_and_pausing_release_signal_demand_and_reopening_gets_fresh_epoch() {
    let (mut app, root, sand) = app();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    store::organs::ensure_local(&engine.store.pool, "http://video.test")
        .await
        .unwrap();
    let signal = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("camera".into()),
                kind: nucleus::RecordKind::Signal,
                head: "Camera".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .configure_stream(&signal, Some("scalar"), "", None, None)
        .await
        .unwrap();
    crate::native::sand_panel::tests::connect(&mut app, engine.clone());
    app.world_mut().get_mut::<Settings>(sand).unwrap().source = "camera".into();
    let channel = engine.stream_channel(&signal, None).await.unwrap();
    let owner = app
        .world()
        .get::<crate::native::sand_store::StoredSand>(sand)
        .unwrap()
        .content
        .unwrap();
    crate::native::sand_panel::tests::settle(&mut app, |_| channel.snapshot().consumers == 1).await;
    let epoch = channel.snapshot().epoch;
    app.world_mut()
        .get_mut::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active = 2;
    crate::native::sand_panel::tests::settle(&mut app, |_| channel.snapshot().consumers == 0).await;
    app.world_mut()
        .get_mut::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active = 1;
    crate::native::sand_panel::tests::settle(&mut app, |_| channel.snapshot().consumers == 1).await;
    assert_ne!(channel.snapshot().epoch, epoch);
    Control::Pause.apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |_| channel.snapshot().consumers == 0).await;
    assert!(app.world().get::<Settings>(sand).unwrap().paused);
    assert!(channel.snapshot().value.is_none());
}
