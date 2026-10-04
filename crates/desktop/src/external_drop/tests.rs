use super::*;
use crate::actions::Action;
use std::{
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};

fn fixture() -> (App, Entity, Entity, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .insert_resource(crate::topology::assets::AssetDirectory(
            directory.path().join("assets"),
        ))
        .add_plugins((
            MinimalPlugins,
            crate::workspace::WorkspacePlugin,
            crate::media_sand::MediaSandPlugin,
            crate::document_viewer::DocumentViewerPlugin,
            ExternalDropPlugin,
        ));
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    app.update();
    app.world_mut().entity_mut(root).insert((
        ComputedNode {
            size: Vec2::new(800.0, 600.0),
            ..default()
        },
        UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
            400.0, 300.0,
        ))),
    ));
    let mut window = Window::default();
    window.set_cursor_position(Some(Vec2::new(500.0, 350.0)));
    let window = app.world_mut().spawn(window).id();
    (app, root, window, directory)
}

fn png(path: &Path) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(80, 40, image::Rgba([40, 150, 220, 255]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = bytes.into_inner();
    std::fs::write(path, &bytes).unwrap();
    bytes
}

fn wait_preview(app: &mut App, root: Entity) -> Entity {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.update();
        if let Some(preview) = app
            .world()
            .resource::<Sessions>()
            .entries
            .get(&root)
            .and_then(|session| session.preview)
        {
            return preview;
        }
        assert!(Instant::now() < deadline, "Preview timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn no_records(world: &mut World) {
    assert_eq!(
        world
            .query::<&crate::workspace::RecordPlacement>()
            .iter(world)
            .count(),
        0
    );
    assert_eq!(
        world
            .query::<&crate::area::RecordProperties>()
            .iter(world)
            .count(),
        0
    );
    assert_eq!(
        world
            .query::<&crate::protein_area::RecordBinding>()
            .iter(world)
            .count(),
        0
    );
}

#[cfg_attr(test, test)]
fn supported_formats_and_urls_have_explicit_viewer_choices() {
    for (path, expected) in [
        ("photo.PNG", source::Kind::Image),
        ("book.pdf", source::Kind::Document),
        ("book.EPUB", source::Kind::Document),
        ("scene.glb", source::Kind::Model),
        ("scene.gltf", source::Kind::Model),
        ("cloud.gcloud", source::Kind::Model),
        ("note.md", source::Kind::Text),
    ] {
        assert_eq!(source::kind(std::path::Path::new(path)), Ok(expected));
    }
    for path in [
        "record.lingua",
        "archive.zip",
        "model.obj",
        "movie.mp4",
        "image.svg",
    ] {
        assert!(source::kind(std::path::Path::new(path)).is_err());
    }
    assert!(matches!(
        source::Source::parse("https://example.com/book.pdf?download=1"),
        source::Source::Url(_)
    ));
    assert_eq!(
        source::Source::parse("file:///tmp/a%20b.png"),
        source::Source::File("/tmp/a b.png".into())
    );
    for url in [
        "javascript:alert(1)",
        "data:image/png,abc",
        "ftp://example.com/image.png",
        "https://user:pass@example.com/a.pdf",
    ] {
        assert!(matches!(
            source::Source::parse(url),
            source::Source::Invalid(_)
        ));
    }
}

#[cfg_attr(test, test)]
fn hovering_is_translucent_unsaved_and_cancelled_without_creating_records() {
    let (mut app, root, window, directory) = fixture();
    let stale = app.world_mut().spawn_empty().id();
    app.world_mut().despawn(stale);
    app.world_mut()
        .init_resource::<bevy::picking::hover::HoverMap>();
    app.world_mut()
        .resource_mut::<bevy::picking::hover::HoverMap>()
        .entry(bevy::picking::pointer::PointerId::Mouse)
        .or_default()
        .insert(
            stale,
            bevy::picking::backend::HitData::new(root, 0.0, None, None),
        );
    let path = directory.path().join("picture.png");
    png(&path);
    app.world_mut().write_message(FileDragAndDrop::HoveredFile {
        window,
        path_buf: path,
    });
    let preview = wait_preview(&mut app, root);
    let node = app
        .world_mut()
        .query::<(&ImageNode, &crate::sand::ImageSand)>()
        .single(app.world())
        .unwrap()
        .0;
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(&node.image)
        .unwrap();
    assert_eq!((image.width(), image.height()), (80, 40));
    assert_eq!(
        app.world()
            .get::<crate::canvas::CanvasItem>(preview)
            .unwrap()
            .position,
        DVec2::new(100.0, 50.0)
    );
    assert!(crate::media_sand::snapshot(app.world_mut(), root).is_empty());
    assert!(!crate::canvas_selection::eligible(
        app.world(),
        root,
        preview
    ));
    let alpha = app
        .world_mut()
        .query::<(&ImageNode, &crate::sand::ImageSand)>()
        .single(app.world())
        .unwrap()
        .0
        .color
        .alpha();
    assert!((alpha - 0.45).abs() < 0.001);
    no_records(app.world_mut());
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(600.0, 200.0)));
    app.update();
    assert_eq!(
        app.world()
            .get::<crate::canvas::CanvasItem>(preview)
            .unwrap()
            .position,
        DVec2::new(200.0, -100.0)
    );
    app.world_mut()
        .write_message(FileDragAndDrop::HoveredFileCanceled { window });
    app.update();
    assert!(app.world().get_entity(preview).is_err());
    assert!(app.world().resource::<Sessions>().entries.is_empty());
    assert_eq!(
        std::fs::read_dir(directory.path().join("assets"))
            .unwrap()
            .count(),
        0
    );
    no_records(app.world_mut());
}

#[cfg_attr(test, test)]
fn release_requires_a_choice_then_keeps_the_real_image_and_its_file() {
    let (mut app, root, window, directory) = fixture();
    let path = directory.path().join("picture.png");
    png(&path);
    app.world_mut().write_message(FileDragAndDrop::HoveredFile {
        window,
        path_buf: path.clone(),
    });
    let preview = wait_preview(&mut app, root);
    let image = app
        .world_mut()
        .query::<(&ImageNode, &crate::sand::ImageSand)>()
        .single(app.world())
        .unwrap()
        .0
        .image
        .clone();
    app.world_mut().write_message(FileDragAndDrop::DroppedFile {
        window,
        path_buf: path.clone(),
    });
    app.world_mut()
        .write_message(FileDragAndDrop::HoveredFileCanceled { window });
    app.update();
    assert!(app.world().get::<Preview>(preview).is_some());
    assert!(crate::media_sand::snapshot(app.world_mut(), root).is_empty());
    assert!(!app.world().resource::<Sessions>().entries[&root].hovering);
    std::fs::remove_file(path).unwrap();
    ui::Control::Show.apply(app.world_mut(), root);
    assert!(app.world().get::<Preview>(preview).is_none());
    assert!(crate::canvas_selection::eligible(
        app.world(),
        root,
        preview
    ));
    let (node, _) = app
        .world_mut()
        .query::<(&ImageNode, &crate::sand::ImageSand)>()
        .single(app.world())
        .unwrap();
    assert_eq!(node.image, image);
    assert_eq!(node.color.alpha(), 1.0);
    let crate::media_sand::MediaSand::Image { path } = app
        .world()
        .get::<crate::media_sand::MediaSand>(preview)
        .unwrap()
    else {
        panic!("Expected image")
    };
    assert!(Path::new(path).is_file());
    let saved = crate::media_sand::snapshot(app.world_mut(), root).remove(0);
    let bytes = serde_json::to_vec(&saved).unwrap();
    assert!(saved.valid());
    app.world_mut().despawn(preview);
    let saved: crate::media_sand::SavedMedia = serde_json::from_slice(&bytes).unwrap();
    saved.restore(app.world_mut(), root);
    let restored = app
        .world_mut()
        .query_filtered::<Entity, With<crate::media_sand::MediaSand>>()
        .single(app.world())
        .unwrap();
    assert_eq!(
        app.world()
            .get::<crate::canvas::CanvasItem>(restored)
            .unwrap()
            .position,
        DVec2::new(100.0, 50.0)
    );
    no_records(app.world_mut());
}

#[cfg_attr(test, test)]
fn workspace_changes_cancel_pending_previews_and_old_choices() {
    let (mut app, root, _, directory) = fixture();
    let path = directory.path().join("picture.png");
    png(&path);
    begin(
        app.world_mut(),
        root,
        None,
        source::Source::File(path),
        DVec2::ZERO,
        0.0,
        false,
    );
    app.world_mut()
        .get_mut::<crate::workspace::Workspaces>(root)
        .unwrap()
        .active = 99;
    ui::Control::Show.apply(app.world_mut(), root);
    update(app.world_mut());
    assert!(app.world().resource::<Sessions>().entries.is_empty());
    assert!(crate::media_sand::snapshot(app.world_mut(), root).is_empty());
    no_records(app.world_mut());
}

#[cfg_attr(test, test)]
fn model_packages_are_removed_on_cancel_and_retained_on_accept() {
    for accept in [false, true] {
        let (mut app, root, _, directory) = fixture();
        let path = directory.path().join("scene.gltf");
        std::fs::write(&path, include_bytes!("../../fixtures/open_scene.gltf")).unwrap();
        begin(
            app.world_mut(),
            root,
            None,
            source::Source::File(path),
            DVec2::new(12.0, 25.0),
            40.0,
            false,
        );
        let preview = wait_preview(&mut app, root);
        let asset = app
            .world()
            .get::<crate::topology::assets::ImportedAsset>(preview)
            .unwrap()
            .clone();
        let package = directory.path().join("assets").join(&asset.id);
        assert!(package.is_dir());
        assert!(crate::topology::assets::snapshot(app.world_mut(), root).is_empty());
        if accept {
            app.world_mut()
                .entity_mut(preview)
                .insert(crate::topology::assets::Ready);
            update(app.world_mut());
            ui::Control::Show.apply(app.world_mut(), root);
            let saved = crate::topology::assets::snapshot(app.world_mut(), root);
            assert_eq!(saved.len(), 1);
            assert!(saved[0].centered);
            assert!(package.is_dir());
            assert_eq!(
                crate::topology::spatial(app.world(), preview).elevation,
                40.0
            );
        } else {
            cancel(app.world_mut(), root);
            assert!(!package.exists());
            assert!(app.world().get_entity(preview).is_err());
        }
        no_records(app.world_mut());
    }
}

#[cfg_attr(test, test)]
fn urls_are_not_fetched_until_requested_and_can_be_saved_as_links() {
    let (mut app, root, _, _) = fixture();
    let url = "https://example.com/book.pdf";
    begin(
        app.world_mut(),
        root,
        None,
        source::Source::parse(url),
        DVec2::new(12.0, 20.0),
        0.0,
        false,
    );
    assert!(!app.world().resource::<Sessions>().entries[&root].busy);
    assert!(
        app.world().resource::<Sessions>().entries[&root]
            .prepared
            .is_none()
    );
    ui::Control::Link.apply(app.world_mut(), root);
    let sand = app
        .world_mut()
        .query::<&crate::media_sand::MediaSand>()
        .single(app.world())
        .unwrap();
    assert!(matches!(sand, crate::media_sand::MediaSand::Link { url: value } if value == url));
    assert_eq!(crate::media_sand::snapshot(app.world_mut(), root).len(), 1);
    no_records(app.world_mut());
}

#[cfg_attr(test, test)]
fn invalid_large_and_escaping_files_fail_and_leave_no_preview_assets() {
    let directory = tempfile::tempdir().unwrap();
    let assets = directory.path().join("assets");
    let cancelled = AtomicBool::new(false);
    for name in ["corrupt.png", "corrupt.pdf", "corrupt.epub"] {
        let path = directory.path().join(name);
        std::fs::write(&path, b"not a supported file").unwrap();
        assert!(worker::prepare(&source::Source::File(path), Some(&assets), &cancelled).is_err());
    }
    let large = directory.path().join("large.png");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(crate::media_sand::MAX_BYTES + 1)
        .unwrap();
    assert!(worker::prepare(&source::Source::File(large), Some(&assets), &cancelled).is_err());
    let text = directory.path().join("large.txt");
    std::fs::write(&text, "x".repeat(4097)).unwrap();
    assert!(worker::prepare(&source::Source::File(text), Some(&assets), &cancelled).is_err());
    let gltf = directory.path().join("escape.gltf");
    std::fs::write(
        &gltf,
        r#"{"asset":{"version":"2.0"},"meshes":[{}],"buffers":[{"uri":"../outside.bin"}]}"#,
    )
    .unwrap();
    assert!(worker::prepare(&source::Source::File(gltf), Some(&assets), &cancelled).is_err());
    assert_eq!(std::fs::read_dir(assets).unwrap().count(), 0);
}

#[cfg_attr(test, test)]
fn remote_files_follow_safe_redirects_use_mime_and_enforce_size_limits() {
    let directory = tempfile::tempdir().unwrap();
    let bytes = png(&directory.path().join("source.png"));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for response in [format!("HTTP/1.1 302 Found\r\nLocation: http://{address}/download\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes(),
            [format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).into_bytes(), bytes].concat(),
            format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", crate::media_sand::MAX_BYTES + 1).into_bytes()] {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            stream.write_all(&response).unwrap();
        }
    });
    let source = source::Source::parse(&format!("http://{address}/redirect"));
    let cancelled = AtomicBool::new(false);
    let assets = directory.path().join("assets");
    let prepared = worker::prepare(&source, Some(&assets), &cancelled).unwrap();
    assert_eq!(prepared.kind, source::Kind::Image);
    let path = prepared.path.clone();
    assert!(path.is_file());
    drop(prepared);
    assert!(!path.exists());
    assert!(
        worker::prepare(
            &source::Source::parse(&format!("http://{address}/large.png")),
            Some(&assets),
            &cancelled
        )
        .is_err()
    );
    server.join().unwrap();
    assert_eq!(std::fs::read_dir(assets).unwrap().count(), 0);
}

#[cfg_attr(test, test)]
fn stale_choice_buttons_cannot_accept_a_different_url() {
    let (mut app, root, _, _) = fixture();
    begin(
        app.world_mut(),
        root,
        None,
        source::Source::parse("https://example.com/first.pdf"),
        DVec2::ZERO,
        0.0,
        false,
    );
    let old = app
        .world_mut()
        .query::<(&crate::icons::Tooltip, &crate::actions::ActionButton)>()
        .iter(app.world())
        .find(|(tip, action)| tip.0 == "Use Link Sand" && action.target == root)
        .unwrap()
        .1
        .actions
        .clone();
    begin(
        app.world_mut(),
        root,
        None,
        source::Source::parse("https://example.com/second.pdf"),
        DVec2::ZERO,
        0.0,
        false,
    );
    old.run(app.world_mut(), root);
    assert!(crate::media_sand::snapshot(app.world_mut(), root).is_empty());
    assert!(
        app.world()
            .resource::<Sessions>()
            .entries
            .contains_key(&root)
    );
    ui::Control::Link.apply(app.world_mut(), root);
    let sand = app
        .world_mut()
        .query::<&crate::media_sand::MediaSand>()
        .single(app.world())
        .unwrap();
    assert!(
        matches!(sand, crate::media_sand::MediaSand::Link { url } if url.ends_with("/second.pdf"))
    );
}

#[cfg_attr(test, test)]
fn previews_do_not_occupy_sorting_slots_or_receive_area_forces() {
    let (mut app, root, _, directory) = fixture();
    let mut area = crate::area::InfluenceArea::new(
        crate::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(1000.0),
    );
    area.strength = 100.0;
    area.sorting = Some(Default::default());
    area.scale = 2.0;
    crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    let sand = app
        .world_mut()
        .spawn((
            crate::canvas::CanvasItem {
                position: DVec2::new(180.0, 0.0),
                size: Vec2::splat(40.0),
            },
            ChildOf(root),
            crate::workspace::WorkspaceMember(1),
        ))
        .id();
    for spatial in [false, true] {
        if spatial {
            app.world_mut()
                .init_resource::<crate::topology::physics::Runtime>();
        }
        crate::area_effects::update(app.world_mut());
        let before = app
            .world()
            .get::<crate::area::AreaForces>(sand)
            .unwrap()
            .total();
        let path = directory.path().join("picture.png");
        png(&path);
        begin(
            app.world_mut(),
            root,
            None,
            source::Source::File(path),
            DVec2::new(140.0, 0.0),
            0.0,
            false,
        );
        let preview = wait_preview(&mut app, root);
        crate::area_effects::update(app.world_mut());
        assert_eq!(
            app.world()
                .get::<crate::area::AreaForces>(sand)
                .unwrap()
                .total(),
            before
        );
        assert!(
            app.world()
                .get::<crate::area::AreaForces>(preview)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::area_effects::AreaScale>(preview)
                .is_none()
        );
        cancel(app.world_mut(), root);
    }
}

#[cfg_attr(test, test)]
fn document_confirmation_waits_for_rendering_and_rejects_bad_documents() {
    for valid in [true, false] {
        let (mut app, root, _, directory) = fixture();
        let path = directory.path().join("book.pdf");
        std::fs::write(
            &path,
            if valid {
                include_bytes!("../../fixtures/drop.pdf").as_slice()
            } else {
                b"%PDF-not a document"
            },
        )
        .unwrap();
        begin(
            app.world_mut(),
            root,
            None,
            source::Source::File(path),
            DVec2::ZERO,
            0.0,
            false,
        );
        let preview = wait_preview(&mut app, root);
        assert!(!app.world().resource::<Sessions>().entries[&root].ready);
        ui::Control::Show.apply(app.world_mut(), root);
        assert!(app.world().get::<Preview>(preview).is_some());
        assert!(crate::document_viewer::snapshot(app.world_mut(), root).is_empty());
        let viewport = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf, &ScrollPosition), With<crate::scroll_sand::ScrollSand>>()
            .iter(app.world())
            .find(|(_, parent, _)| parent.parent() == preview)
            .unwrap()
            .0;
        app.world_mut().entity_mut(viewport).insert(ComputedNode {
            size: Vec2::new(680.0, 700.0),
            ..default()
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            app.update();
            let session = &app.world().resource::<Sessions>().entries[&root];
            if session.ready || session.prepared.is_none() {
                break;
            }
            assert!(Instant::now() < deadline, "Document render timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        if valid {
            assert!(app.world().resource::<Sessions>().entries[&root].ready);
            assert_eq!(
                app.world()
                    .get::<crate::document_viewer::DocumentViewer>(preview)
                    .unwrap()
                    .position()
                    .section,
                0
            );
            ui::Control::Show.apply(app.world_mut(), root);
            assert_eq!(
                crate::document_viewer::snapshot(app.world_mut(), root).len(),
                1
            );
        } else {
            assert!(
                app.world().resource::<Sessions>().entries[&root]
                    .prepared
                    .is_none()
            );
            assert!(app.world().get_entity(preview).is_err());
            assert!(crate::document_viewer::snapshot(app.world_mut(), root).is_empty());
            assert_eq!(
                std::fs::read_dir(directory.path().join("assets"))
                    .unwrap()
                    .count(),
                0
            );
        }
        no_records(app.world_mut());
    }
}

crate::laboratory_cases! {
    supported_formats_and_urls_have_explicit_viewer_choices,
    hovering_is_translucent_unsaved_and_cancelled_without_creating_records,
    release_requires_a_choice_then_keeps_the_real_image_and_its_file,
    workspace_changes_cancel_pending_previews_and_old_choices,
    model_packages_are_removed_on_cancel_and_retained_on_accept,
    urls_are_not_fetched_until_requested_and_can_be_saved_as_links,
    invalid_large_and_escaping_files_fail_and_leave_no_preview_assets,
    remote_files_follow_safe_redirects_use_mime_and_enforce_size_limits,
    stale_choice_buttons_cannot_accept_a_different_url,
    previews_do_not_occupy_sorting_slots_or_receive_area_forces,
    document_confirmation_waits_for_rendering_and_rejects_bad_documents,
}
