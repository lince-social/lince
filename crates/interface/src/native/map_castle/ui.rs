use super::*;
use bevy::{
    picking::events::{Click, Drag, DragEnd, DragStart, Scroll},
    render::render_resource::TextureFormat,
    ui::RelativeCursorPosition,
};
use lince_map::scene::Palette;

pub(super) fn mount(
    world: &mut World,
    owner: Entity,
    camera: Camera,
    records: Vec<String>,
    person: String,
) {
    if !world.contains_resource::<Assets<Image>>() {
        world.init_resource::<Assets<Image>>();
    }
    let controls = crate::native::sand_panel::row(world, owner);
    for (caption, command) in [
        ("−", Command::Zoom(-1.0)),
        ("+", Command::Zoom(1.0)),
        ("World", Command::Center),
        ("Go to Record", Command::RecordCenter),
        ("Protein source", Command::SourcePicker),
        ("Shared location", Command::SharedPicker),
        ("Draw area", Command::Select),
        ("Retry", Command::Retry),
    ] {
        crate::native::sand_panel::button(world, controls, owner, caption, command);
    }
    settings_button(world, controls, owner);
    let status = crate::native::edit_mode::label(world, owner, "World overview · offline", 13.0);
    let image = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            800,
            360,
            TextureFormat::Rgba8Unorm,
            None,
        ));
    let viewport = world
        .spawn((
            ImageNode::new(image.clone()),
            Node {
                width: percent(100),
                height: px(360),
                min_height: px(180),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            ChildOf(owner),
            Viewport(owner),
            RelativeCursorPosition::default(),
        ))
        .observe(scroll)
        .observe(drag_start)
        .observe(drag)
        .observe(drag_end)
        .observe(click)
        .id();
    crate::native::accessibility::input(
        world,
        viewport,
        "Map. Drag to pan, use plus and minus to zoom, or Draw area to select an offline download.",
        false,
    );
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ChildOf(viewport),
            Pickable::IGNORE,
        ))
        .id();
    let selected_text = crate::native::edit_mode::label(
        world,
        owner,
        "Click to choose a reference point. Draw an area to download more detail.",
        13.0,
    );
    let download = crate::native::sand_panel::row(world, owner);
    let zoom_field = crate::native::sand_panel::field(world, download, "Area detail (0–14)", "14");
    crate::native::sand_panel::button(world, download, owner, "Download area", Command::Download);
    crate::native::sand_panel::button(world, download, owner, "Cancel download", Command::Cancel);
    let records: Vec<_> = records
        .into_iter()
        .filter(|record| nucleus::valid_uid(record, "r"))
        .take(32)
        .collect();
    crate::native::sand_panel::button(
        world,
        controls,
        owner,
        "Save point on Record",
        Command::SavePoint,
    );
    crate::native::edit_mode::label(
        world,
        owner,
        "Made with Natural Earth · © OpenStreetMap contributors · © ESA WorldCover project 2021 / Contains modified Copernicus Sentinel data (2021) processed by ESA WorldCover consortium",
        11.0,
    );
    crate::native::sand_panel::credits(world, controls, owner, CREDITS);
    let auto_center = !records.is_empty();
    world.entity_mut(owner).insert(MapSand {
        camera: if camera.valid() {
            camera
        } else {
            Camera::default()
        },
        records,
        person,
        area: None,
        authority_node_id: None,
        source_picker: None,
        auto_center,
        viewport,
        status,
        overlay,
        selected_text,
        zoom_field,
        size: [800, 360],
        image,
        scene: None,
        source_revision: 0,
        rendered_camera: None,
        select_area: false,
        selection: None,
        drag_start: None,
        picked: None,
        markers: Vec::new(),
        marker_stamp: String::new(),
        job: None,
        notice: String::new(),
        last_status: String::new(),
        next_wake: std::time::Instant::now(),
    });
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Map",
        "Offline world overview, Record locations and downloadable detail.",
        Command::Create,
        |world, root| {
            spawn(
                world,
                root,
                1,
                DVec2::ZERO,
                Camera::default(),
                Vec::new(),
                String::new(),
            )
        },
    );
}

pub(super) fn command(world: &mut World, target: Entity, command: Command) {
    if matches!(command, Command::Create) {
        let workspace = world
            .get::<crate::native::workspace::Workspaces>(target)
            .map_or(1, |spaces| spaces.active);
        spawn(
            world,
            target,
            workspace,
            DVec2::ZERO,
            Camera::default(),
            Vec::new(),
            String::new(),
        );
        return;
    }
    if matches!(command, Command::Download) {
        download(world, target);
        return;
    }
    if matches!(command, Command::SavePoint) {
        feed::save_point(world, target);
        return;
    }
    if matches!(command, Command::SourcePicker) {
        source_picker(world, target);
        return;
    }
    if matches!(command, Command::SharedPicker) {
        shared_picker(world, target);
        return;
    }
    if let Command::SharedReference(input) = command {
        let invitation = crate::native::sand_panel::value(world, input)
            .and_then(|value| nucleus::location::Invitation::parse(&value).map_err(str::to_owned));
        match invitation {
            Ok(invitation) => {
                let picker = world
                    .get::<MapSand>(target)
                    .and_then(|sand| sand.source_picker);
                if let Some(picker) = picker {
                    world.despawn(picker);
                }
                feed::reset(world, target);
                let mut sand = world.get_mut::<MapSand>(target).unwrap();
                sand.source_picker = None;
                sand.area = None;
                sand.records = vec![invitation.record_uid];
                sand.authority_node_id = Some(invitation.authority_node_id);
                sand.camera = Camera::default();
                sand.auto_center = true;
                sand.notice = "Viewing permitted saved and live locations".into();
            }
            Err(error) => world.get_mut::<MapSand>(target).unwrap().notice = error,
        }
        return;
    }
    if matches!(&command, Command::Source(_))
        && let Some(picker) = world
            .get::<MapSand>(target)
            .and_then(|sand| sand.source_picker)
    {
        world.despawn(picker);
        world.get_mut::<MapSand>(target).unwrap().source_picker = None;
    }
    let Some(mut sand) = world.get_mut::<MapSand>(target) else {
        return;
    };
    match command {
        Command::Zoom(amount) => {
            let size = sand.size.map(f64::from);
            sand.camera
                .zoom_at(amount, [size[0] / 2.0, size[1] / 2.0], size);
        }
        Command::Center => {
            sand.camera = Camera::default();
            sand.auto_center = false;
        }
        Command::RecordCenter => {
            if let Some(marker) = sand.markers.first() {
                sand.camera = Camera {
                    center: marker.coordinate,
                    zoom: marker.radius_metres.map_or(16.0, |radius| {
                        let scale = marker
                            .coordinate
                            .latitude
                            .clamp(-85.0, 85.0)
                            .to_radians()
                            .cos()
                            * 6_371_000.0
                            / 6_378_137.0;
                        let metres_per_pixel = f64::from(radius) * 2.5
                            / f64::from(sand.size[0].min(sand.size[1]).max(1));
                        (lince_map::geometry::WORLD_METERS * scale / (256.0 * metres_per_pixel))
                            .log2()
                            .clamp(0.0, 20.0)
                    }),
                };
            } else {
                sand.auto_center = true;
            }
        }
        Command::Source(area) => {
            sand.area = area;
            sand.authority_node_id = None;
            sand.records.clear();
            sand.markers.clear();
            sand.auto_center = true;
        }
        Command::Select => {
            sand.select_area = !sand.select_area;
            sand.notice = if sand.select_area {
                "Drag to select an area. Set its detail level below.".into()
            } else {
                "Drag to pan the map".into()
            };
        }
        Command::Cancel => {
            sand.job = None;
            sand.notice = "Area download cancelled. Completed tiles remain available.".into();
        }
        Command::Retry => {
            sand.scene = None;
            sand.notice.clear();
        }
        _ => {}
    }
}

fn download(world: &mut World, owner: Entity) {
    let Some(sand) = world.get::<MapSand>(owner) else {
        return;
    };
    let field = sand.zoom_field;
    let selection = sand.selection;
    let store = world.resource::<settings::Sources>().store.clone();
    let result = (|| {
        let store = store.ok_or("Configure a map source in Map Settings")?;
        let zoom: u8 = crate::native::sand_panel::value(world, field)?
            .trim()
            .parse()
            .map_err(|_| "Choose a detail level between 0 and 14")?;
        if zoom > store.settings().max_zoom {
            return Err("The selected detail exceeds the source maximum".into());
        }
        let tiles: Vec<_> = selection
            .ok_or("Draw an area first")?
            .pyramid(zoom)?
            .into_iter()
            .filter(|tile| tile.z >= 3)
            .collect();
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| "The download runtime is unavailable")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(Mutex::new(None));
        let job = Job {
            tiles: tiles.clone(),
            cancelled: cancelled.clone(),
            finished: finished.clone(),
        };
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
        runtime.spawn(async move {
            let progress = store.download(tiles, cancelled).await;
            *finished.lock().unwrap_or_else(|error| error.into_inner()) = Some(progress);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        Ok(job)
    })();
    let mut sand = world.get_mut::<MapSand>(owner).unwrap();
    match result {
        Ok(job) => {
            sand.job = Some(job);
            sand.notice.clear();
        }
        Err(error) => sand.notice = error,
    }
}

pub(super) fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<MapSand>>()
        .iter(world)
        .collect();
    world
        .resource_mut::<render::Scenes>()
        .0
        .retain(|entity, _| owners.contains(entity));
    let sources = world.resource::<settings::Sources>();
    let (store, source_revision, source_error) = (
        sources.store.clone(),
        sources.revision,
        sources.error.clone(),
    );
    let frame = world
        .get_resource::<Time>()
        .map_or(0, |time| (time.elapsed_secs() * 4.0) as usize % 4);
    for owner in owners {
        if !visible(world, owner) {
            world.resource_mut::<render::Scenes>().0.remove(&owner);
            continue;
        }
        source_records(world, owner);
        let (viewport, status, selected_text) = {
            let sand = world.get::<MapSand>(owner).unwrap();
            (sand.viewport, sand.status, sand.selected_text)
        };
        let size = world
            .get::<ComputedNode>(viewport)
            .map(|node| node.size() * node.inverse_scale_factor())
            .filter(|size| size.min_element() > 0.0)
            .map_or([800, 360], |size| {
                [
                    size.x.clamp(1.0, 2048.0) as u32,
                    size.y.clamp(1.0, 2048.0) as u32,
                ]
            });
        let palette = palette(world, owner);
        let resized = world.get::<MapSand>(owner).unwrap().size != size;
        if resized {
            let image = world
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    size[0],
                    size[1],
                    TextureFormat::Rgba8Unorm,
                    None,
                ));
            world.get_mut::<ImageNode>(viewport).unwrap().image = image.clone();
            let mut sand = world.get_mut::<MapSand>(owner).unwrap();
            sand.image = image;
            sand.size = size;
        }
        let sand = world.get::<MapSand>(owner).unwrap();
        let new_scene = sand.scene.is_none()
            || sand.source_revision != source_revision
            || sand.scene.as_ref().is_some_and(|scene| {
                scene
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .palette
                    != palette
            });
        if new_scene {
            if let Some(store) = &store {
                let wake = world
                    .get_resource::<crate::native::wake::WakeSignal>()
                    .cloned()
                    .map(|wake| Arc::new(move || wake.ring()) as Arc<dyn Fn() + Send + Sync>);
                match Scene::new(store.clone(), sand.camera, size, palette, wake) {
                    Ok(scene) => {
                        let previous = sand.scene.as_ref().map_or(1, |scene| {
                            scene
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .revision
                                .load(Ordering::Relaxed)
                                + 1
                        });
                        scene.revision.store(previous, Ordering::Relaxed);
                        let mut sand = world.get_mut::<MapSand>(owner).unwrap();
                        sand.scene = Some(Arc::new(Mutex::new(scene)));
                        if sand.source_revision != source_revision {
                            sand.job = None;
                        }
                        sand.source_revision = source_revision;
                        sand.rendered_camera = None;
                    }
                    Err(error) => world.get_mut::<MapSand>(owner).unwrap().notice = error,
                }
            }
        }
        let (scene, camera, image, changed) = {
            let sand = world.get::<MapSand>(owner).unwrap();
            (
                sand.scene.clone(),
                sand.camera,
                sand.image.id(),
                resized || sand.rendered_camera != Some(sand.camera),
            )
        };
        if let Some(scene) = scene {
            if changed {
                scene
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .set_camera(camera, size);
                world.get_mut::<MapSand>(owner).unwrap().rendered_camera = Some(camera);
            }
            if tokio::runtime::Handle::try_current().is_ok() {
                scene
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .load();
            }
            world
                .resource_mut::<render::Scenes>()
                .0
                .insert(owner, render::Target { scene, image, size });
        }
        {
            let now = chrono::Utc::now().timestamp_millis();
            let mut sand = world.get_mut::<MapSand>(owner).unwrap();
            sand.markers.retain(|marker| {
                !marker.live
                    || (marker.observed.elapsed().as_secs() < 60
                        && marker.expires.is_none_or(|expires| expires > now))
            });
            if sand.auto_center
                && let Some(marker) = sand.markers.first()
            {
                sand.camera = Camera {
                    center: marker.coordinate,
                    zoom: 16.0,
                };
                sand.auto_center = false;
            }
        }
        overlays(world, owner);
        let sand = world.get::<MapSand>(owner).unwrap();
        let mut caption = format!("Zoom {:.1} · ", sand.camera.zoom);
        if let Some(error) = &source_error {
            caption.push_str(error);
        } else if let Some(store) = &store {
            let tiles = if sand.camera.data_zoom(store.settings().max_zoom) < 3 {
                Vec::new()
            } else {
                sand.camera
                    .visible_tiles(size.map(f64::from), store.settings().max_zoom)
            };
            let progress = store.progress(&tiles);
            caption.push_str(&progress_caption(&progress, frame));
            if let Some(job) = &sand.job {
                let finished = job
                    .finished
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone();
                let progress = finished.unwrap_or_else(|| store.progress(&job.tiles));
                caption.push_str(&format!(" · Area: {}", progress_caption(&progress, frame)));
            }
        } else {
            caption.push_str("Preparing local map");
        }
        if !sand.notice.is_empty() {
            caption.push_str(&format!(" · {}", sand.notice));
        }
        if caption != sand.last_status {
            crate::native::sand_panel::status(world, status, caption.clone());
            world.get_mut::<MapSand>(owner).unwrap().last_status = caption;
        }
        let sand = world.get::<MapSand>(owner).unwrap();
        let caption = if let Some(point) = sand.picked {
            format!(
                "Reference point: {:.6}, {:.6}",
                point.latitude, point.longitude
            )
        } else if let Some(bounds) = sand.selection {
            format!(
                "Selected area: {:.4}…{:.4} longitude, {:.4}…{:.4} latitude",
                bounds.west, bounds.east, bounds.south, bounds.north
            )
        } else {
            "Click to choose a reference point. Draw an area to download more detail.".into()
        };
        let pending = store.as_ref().is_some_and(|store| {
            sand.camera.data_zoom(store.settings().max_zoom) >= 3
                && !store
                    .progress(
                        &sand
                            .camera
                            .visible_tiles(size.map(f64::from), store.settings().max_zoom),
                    )
                    .finished()
        }) || sand.job.as_ref().is_some_and(|job| {
            job.finished
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_none()
        });
        if world
            .get::<Text>(selected_text)
            .is_some_and(|text| text.0 != caption)
        {
            crate::native::sand_panel::status(world, selected_text, caption);
        }
        if pending && world.get::<MapSand>(owner).unwrap().next_wake <= std::time::Instant::now() {
            if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                wake.after(std::time::Duration::from_millis(250));
            }
            world.get_mut::<MapSand>(owner).unwrap().next_wake =
                std::time::Instant::now() + std::time::Duration::from_millis(250);
        }
    }
}

fn progress_caption(progress: &Progress, frame: usize) -> String {
    if progress.total == 0 {
        return "World overview · offline".into();
    }
    if progress.failed > 0 {
        return format!(
            "{} / {} tiles · {}",
            progress.completed,
            progress.total,
            progress
                .errors
                .first()
                .map_or("Some detail unavailable", String::as_str)
        );
    }
    if progress.finished() {
        return format!("{} tiles available locally", progress.completed);
    }
    let received = progress.downloaded_bytes as f64 / 1024.0;
    let bytes = if progress.expected_bytes > 0 {
        format!(
            "{received:.0} / {:.0} KiB",
            progress.expected_bytes as f64 / 1024.0
        )
    } else {
        format!("{received:.0} KiB downloaded")
    };
    format!(
        "{} {} / {} tiles · {bytes}",
        ["◐", "◓", "◑", "◒"][frame % 4],
        progress.completed,
        progress.total
    )
}

pub(super) fn palette(world: &World, owner: Entity) -> Palette {
    let resolve = |token| {
        let linear = crate::native::token_style::resolve(world, owner, token)
            .0
            .color()
            .to_linear();
        [linear.red, linear.green, linear.blue, linear.alpha]
            .map(|value| (value * 255.0).round() as u8)
    };
    let mix = |a: [u8; 4], b: [u8; 4], amount: f64| {
        std::array::from_fn(|i| {
            if i == 3 {
                255
            } else {
                (f64::from(a[i]) * (1.0 - amount) + f64::from(b[i]) * amount).round() as u8
            }
        })
    };
    let land = resolve(crate::native::tokens::Token::Surface);
    let ink = resolve(crate::native::tokens::Token::Ink);
    let accent = resolve(crate::native::tokens::Token::Accent);
    Palette {
        land,
        water: mix(land, accent, 0.35),
        roads: mix(land, ink, 0.7),
        nature: mix(land, accent, 0.15),
        ink,
    }
}

fn overlays(world: &mut World, owner: Entity) {
    let sand = world.get::<MapSand>(owner).unwrap();
    let stamp = format!(
        "{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
        sand.camera,
        sand.size,
        sand.selection,
        sand.picked,
        sand.markers
            .iter()
            .map(|marker| (
                &marker.record,
                marker.coordinate,
                marker.live,
                marker.radius_metres
            ))
            .collect::<Vec<_>>(),
        palette(world, owner)
    );
    if sand.marker_stamp == stamp {
        return;
    }
    let (overlay, camera, size, picked, selection, markers) = (
        sand.overlay,
        sand.camera,
        sand.size.map(f64::from),
        sand.picked,
        sand.selection,
        sand.markers.clone(),
    );
    crate::native::sand_panel::clear(world, overlay);
    if let Some(bounds) = selection {
        let [a, b] = camera.area_screen(bounds, size);
        let node = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(a[0].min(b[0]) as f32),
                    top: px(a[1].min(b[1]) as f32),
                    width: px((b[0] - a[0]).abs() as f32),
                    height: px((b[1] - a[1]).abs() as f32),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                ChildOf(overlay),
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
                Pickable::IGNORE,
            ))
            .id();
        let color =
            crate::native::token_style::resolve(world, owner, crate::native::tokens::Token::Accent)
                .0
                .color()
                .with_alpha(0.15);
        world.entity_mut(node).insert(BackgroundColor(color));
    }
    for (coordinate, label, live, record, radius) in markers
        .iter()
        .map(|marker| {
            (
                marker.coordinate,
                format!(
                    "{} · {}{}",
                    marker.record,
                    if marker.live { "live" } else { "saved" },
                    marker
                        .radius_metres
                        .map_or_else(String::new, |radius| format!(
                            " · approximate area, radius {radius} m"
                        ))
                ),
                marker.live,
                Some(marker.record.clone()),
                marker.radius_metres,
            )
        })
        .chain(
            picked
                .into_iter()
                .map(|point| (point, "Reference point".into(), false, None, None)),
        )
    {
        let point = camera.screen(coordinate, size);
        let pixels = radius.map_or(6.0, |radius| {
            (f64::from(radius)
                / (camera.resolution()
                    * coordinate.latitude.clamp(-85.0, 85.0).to_radians().cos()
                    * 6_371_000.0
                    / 6_378_137.0))
                .clamp(4.0, 8192.0)
        });
        if point[0] + pixels < 0.0
            || point[1] + pixels < 0.0
            || point[0] - pixels > size[0]
            || point[1] - pixels > size[1]
        {
            continue;
        }
        let dot = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px((point[0] - pixels) as f32),
                    top: px((point[1] - pixels) as f32),
                    width: px((pixels * 2.0) as f32),
                    height: px((pixels * 2.0) as f32),
                    border: if radius.is_some() {
                        UiRect::all(px(2))
                    } else {
                        UiRect::ZERO
                    },
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                ChildOf(overlay),
                crate::native::token_style::background(if live {
                    crate::native::tokens::Token::Accent
                } else {
                    crate::native::tokens::Token::Ink
                }),
                crate::native::icons::Tooltip(label.clone()),
                Pickable::IGNORE,
            ))
            .id();
        world.entity_mut(dot).insert((
            crate::native::sand::Square,
            crate::native::sand::FixedGeometry,
        ));
        if radius.is_some() {
            let token = if live {
                crate::native::tokens::Token::Accent
            } else {
                crate::native::tokens::Token::Ink
            };
            let fill = crate::native::token_style::resolve(world, owner, token)
                .0
                .color()
                .with_alpha(0.12);
            world
                .entity_mut(dot)
                .remove::<crate::native::token_style::BackgroundToken>()
                .insert((
                    BackgroundColor(fill),
                    crate::native::token_style::border(token),
                ));
        }
        if let Some(record) = record {
            world.entity_mut(dot).insert((
                crate::native::sand::button(0),
                Pickable::default(),
                crate::native::actions::ActionButton::new(
                    owner,
                    crate::native::actions![crate::native::full_record::Open(
                        crate::native::protein_area::RecordBinding {
                            area: owner,
                            uid: record,
                            source: crate::native::protein_area::Source::Local
                        }
                    )],
                ),
            ));
            crate::native::accessibility::input(world, dot, &label, false);
        }
    }
    world.get_mut::<MapSand>(owner).unwrap().marker_stamp = stamp;
}

fn pointer(world: &World, viewport: Entity) -> Option<[f64; 2]> {
    let cursor = world.get::<RelativeCursorPosition>(viewport)?.normalized?;
    let owner = world.get::<Viewport>(viewport)?.0;
    let size = world.get::<MapSand>(owner)?.size;
    Some([
        f64::from(cursor.x + 0.5) * f64::from(size[0]),
        f64::from(cursor.y + 0.5) * f64::from(size[1]),
    ])
}

fn source_picker(world: &mut World, owner: Entity) {
    let Some(picker) = picker(world, owner) else {
        return;
    };
    let ancestor = scope(world, owner);
    let areas: Vec<_> = world
        .query::<(Entity, &crate::native::area::InfluenceArea)>()
        .iter(world)
        .filter(|(entity, area)| {
            area.protein.is_some()
                && ancestor.is_some_and(|root| scope(world, *entity) == Some(root))
        })
        .map(|(_, area)| (area.id.clone(), area.name.clone()))
        .collect();
    crate::native::sand_panel::button(
        world,
        picker,
        owner,
        "No Protein source",
        Command::Source(None),
    );
    for (id, name) in areas {
        crate::native::sand_panel::button(world, picker, owner, &name, Command::Source(Some(id)));
    }
}

fn shared_picker(world: &mut World, owner: Entity) {
    let Some(picker) = picker(world, owner) else {
        return;
    };
    let input = crate::native::sand_panel::field(world, picker, "Shared location reference", "");
    crate::native::sand_panel::button(
        world,
        picker,
        owner,
        "View permitted locations",
        Command::SharedReference(input),
    );
    crate::native::sand_panel::button(world, picker, owner, "Close", Command::SharedPicker);
}

fn picker(world: &mut World, owner: Entity) -> Option<Entity> {
    let Some(sand) = world.get::<MapSand>(owner) else {
        return None;
    };
    if let Some(picker) = sand.source_picker {
        world.despawn(picker);
        world.get_mut::<MapSand>(owner).unwrap().source_picker = None;
        return None;
    }
    let picker = crate::native::sand_panel::column(world, owner);
    world.entity_mut(picker).insert((
        Node {
            position_type: PositionType::Absolute,
            left: px(8),
            top: px(72),
            width: percent(95),
            max_height: px(240),
            padding: UiRect::all(px(8)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ZIndex(5),
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    crate::native::scroll_sand::attach(world, picker);
    world.get_mut::<MapSand>(owner).unwrap().source_picker = Some(picker);
    Some(picker)
}

fn source_records(world: &mut World, owner: Entity) {
    let Some(id) = world
        .get::<MapSand>(owner)
        .and_then(|sand| sand.area.clone())
    else {
        return;
    };
    let owner_scope = scope(world, owner);
    let source = world
        .query::<(Entity, &crate::native::area::InfluenceArea)>()
        .iter(world)
        .find(|(entity, area)| area.id == id && scope(world, *entity) == owner_scope)
        .map(|(entity, _)| entity);
    let records: Vec<String> = source
        .and_then(|source| crate::native::protein_area::calendar_feed(world, source))
        .map_or(Vec::new(), |(rows, _)| {
            rows.iter()
                .filter_map(|row| {
                    row["uid"]
                        .as_str()
                        .filter(|uid| nucleus::valid_uid(uid, "r"))
                        .map(str::to_string)
                })
                .take(32)
                .collect()
        });
    if world.get::<MapSand>(owner).unwrap().records != records {
        let mut sand = world.get_mut::<MapSand>(owner).unwrap();
        sand.records = records;
        sand.markers.clear();
    }
}

fn scope(world: &World, mut entity: Entity) -> Option<(Entity, u64)> {
    let mut workspace = None;
    loop {
        workspace = workspace.or_else(|| {
            world
                .get::<crate::native::workspace::WorkspaceMember>(entity)
                .map(|member| member.0)
        });
        if world
            .get::<crate::native::workspace::Workspaces>(entity)
            .is_some()
        {
            return workspace.map(|workspace| (entity, workspace));
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

fn scroll(mut event: On<Pointer<Scroll>>, mut commands: Commands) {
    let viewport = event.entity;
    let amount = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
        f64::from(event.y)
    } else {
        f64::from(event.y) / 100.0
    };
    if !amount.is_finite() {
        return;
    }
    commands.queue(move |world: &mut World| {
        let Some(point) = pointer(world, viewport) else {
            return;
        };
        let Some(owner) = world.get::<Viewport>(viewport).map(|viewport| viewport.0) else {
            return;
        };
        let Some(mut sand) = world.get_mut::<MapSand>(owner) else {
            return;
        };
        let size = sand.size.map(f64::from);
        sand.camera.zoom_at(amount.clamp(-3.0, 3.0), point, size);
    });
    event.propagate(false);
}

fn drag_start(mut event: On<Pointer<DragStart>>, mut commands: Commands) {
    let viewport = event.entity;
    commands.queue(move |world: &mut World| {
        let Some(point) = pointer(world, viewport) else {
            return;
        };
        if let Some(owner) = world.get::<Viewport>(viewport).map(|viewport| viewport.0) {
            world.get_mut::<MapSand>(owner).unwrap().drag_start = Some(point);
        }
    });
    event.propagate(false);
}

fn drag(mut event: On<Pointer<Drag>>, mut commands: Commands) {
    let viewport = event.entity;
    let delta = event.delta;
    commands.queue(move |world: &mut World| {
        let point = pointer(world, viewport);
        let Some(owner) = world.get::<Viewport>(viewport).map(|viewport| viewport.0) else {
            return;
        };
        let Some(mut sand) = world.get_mut::<MapSand>(owner) else {
            return;
        };
        if sand.select_area {
            if let (Some(start), Some(point)) = (sand.drag_start, point) {
                sand.selection = Some(sand.camera.area(start, point, sand.size.map(f64::from)));
            }
        } else {
            sand.camera.pan([f64::from(delta.x), f64::from(delta.y)]);
        }
    });
    event.propagate(false);
}

fn drag_end(mut event: On<Pointer<DragEnd>>) {
    event.propagate(false);
}

fn click(mut event: On<Pointer<Click>>, mut commands: Commands) {
    let viewport = event.entity;
    commands.queue(move |world: &mut World| {
        let Some(point) = pointer(world, viewport) else {
            return;
        };
        let Some(owner) = world.get::<Viewport>(viewport).map(|viewport| viewport.0) else {
            return;
        };
        let Some(mut sand) = world.get_mut::<MapSand>(owner) else {
            return;
        };
        if !sand.select_area {
            sand.picked = Some(sand.camera.coordinate(point, sand.size.map(f64::from)));
        }
    });
    event.propagate(false);
}
