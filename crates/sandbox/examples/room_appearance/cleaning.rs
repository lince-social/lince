use super::*;
use lince_sandbox::{actions::ActionButton, shader_effects::Surface};
use std::sync::{Arc, Mutex, mpsc};

#[derive(Resource)]
pub(super) struct Replay {
    engine: Arc<engine::Engine>,
    record: String,
    unavailable: String,
    editor: Option<Entity>,
    edit: Option<Edit>,
    ambient: Option<bool>,
    next: u32,
    waiting: bool,
    images: Vec<Vec<u8>>,
    size: UVec2,
    pending: Option<Mutex<mpsc::Receiver<Result<(), String>>>>,
    readback: Option<Entity>,
    pixel: Option<[u8; 4]>,
    change: Option<usize>,
}

#[derive(Clone, Copy)]
enum Edit {
    Setup,
    Invalid,
    Reset,
}

fn settled(world: &World, frame: u32) -> u32 {
    frame + if connected::active(world) { 8 } else { 36 }
}

pub(super) async fn install(app: &mut App) {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let organ = store::organs::ensure_local(&engine.store.pool, "http://room-cleaning-replay")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(engine::trust::Signer::generate(&organ, "key"))
        .await
        .unwrap();
    let record = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Clean the floor".into(),
                body: "Room cleaning replay".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    app.insert_resource(lince_sandbox::app::CellHandle(cell::CellRuntime {
        commands: default(),
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: default(),
        fiote: None,
        speech: None,
        information: None,
    }))
    .add_plugins(lince_sandbox::cell_bridge::CellBridgePlugin)
    .insert_resource(Replay {
        engine,
        record,
        unavailable: nucleus::new_uid("r"),
        editor: None,
        edit: Some(Edit::Setup),
        ambient: None,
        next: 0,
        waiting: false,
        images: Vec::new(),
        size: UVec2::ZERO,
        pending: None,
        readback: None,
        pixel: None,
        change: None,
    });
}

pub(super) fn engine(world: &World) -> Arc<engine::Engine> {
    world.resource::<Replay>().engine.clone()
}

pub(super) fn clear_readback(world: &mut World) {
    if let Some(entity) = world.resource_mut::<Replay>().readback.take() {
        world.despawn(entity);
    }
    world.resource_mut::<Replay>().pixel = None;
}

pub(super) fn images(world: &World) -> (&[Vec<u8>], UVec2) {
    let replay = world.resource::<Replay>();
    (&replay.images, replay.size)
}

fn button(world: &mut World, target: Option<Entity>, caption: &str) -> Option<ActionButton> {
    let labels: Vec<_> = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .filter(|(_, text)| text.0 == caption)
        .map(|(entity, _)| entity)
        .collect();
    for label in labels {
        let mut current = Some(label);
        while let Some(entity) = current {
            if let Some(action) = world.get::<ActionButton>(entity)
                && target.is_none_or(|target| action.target == target)
            {
                return Some(action.clone());
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
    None
}

fn press(world: &mut World, target: Option<Entity>, caption: &str) -> Option<Entity> {
    let action = button(world, target, caption)?;
    action.actions.run(world, action.target);
    Some(action.target)
}

fn field(world: &mut World, caption: &str, value: &str) {
    let entity = world
        .query::<(
            Entity,
            &bevy::a11y::AccessibilityNode,
            &bevy::text::EditableText,
        )>()
        .iter(world)
        .find(|(_, node, _)| node.label() == Some(caption))
        .map(|(entity, _, _)| entity)
        .unwrap_or_else(|| panic!("Missing cleaning field: {caption}"));
    world
        .get_mut::<bevy::text::EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

fn editor_action(world: &mut World, floor: Entity, caption: &str) {
    press(world, Some(floor), "Configure cleaning effect").expect("Cleaning floor inspector");
    let editor = button(world, None, "Save / Apply").unwrap().target;
    press(world, Some(editor), caption).unwrap();
    press(world, Some(editor), "Close").unwrap();
}

fn quantity(world: &mut World, value: f64) {
    let state = world.resource::<Replay>();
    let engine = state.engine.clone();
    let target = state.record.clone();
    let (tx, rx) = mpsc::channel();
    tokio::spawn(async move {
        let result = engine
            .act(engine::actions::Action::SetQuantity { target, value }, None)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string());
        tx.send(result).unwrap();
    });
    world.resource_mut::<Replay>().pending = Some(Mutex::new(rx));
}

fn edit(world: &mut World, floor: Entity, frame: u32, kind: Edit) -> bool {
    if world.resource::<Replay>().editor.is_none() {
        if press(world, Some(floor), "Configure cleaning effect").is_none() {
            return false;
        }
        let editor = button(world, None, "Save / Apply").unwrap().target;
        match kind {
            Edit::Setup => {
                press(
                    world,
                    Some(editor),
                    "Task state: unmapped (configure quantity range)",
                )
                .unwrap();
                press(
                    world,
                    Some(editor),
                    "Progress: unmapped (configure quantity range)",
                )
                .unwrap();
                let record = world.resource::<Replay>().record.clone();
                field(world, "Record UID (empty uses local WGSL)", &record);
                field(world, "Source Organ UID (empty for local)", "");
                field(world, "Preview / static quantity", "0");
                for caption in ["Task state start quantity", "Progress start quantity"] {
                    field(world, caption, "0");
                }
                for caption in ["Task state end quantity", "Progress end quantity"] {
                    field(world, caption, "10");
                }
            }
            Edit::Invalid => field(world, "WGSL: fn shade(uv, inputs) → vec4<f32>", "fn shade("),
            Edit::Reset => {
                press(world, Some(editor), "Reset").unwrap();
            }
        }
        let next = settled(world, frame);
        let mut state = world.resource_mut::<Replay>();
        state.editor = Some(editor);
        state.next = next;
        return false;
    }
    if frame < world.resource::<Replay>().next {
        return false;
    }
    let editor = world.resource::<Replay>().editor.unwrap();
    press(world, Some(editor), "Save / Apply").unwrap();
    let surface = world.get::<Surface>(floor).unwrap();
    let ready = match kind {
        Edit::Invalid => {
            surface
                .draft
                .as_ref()
                .is_some_and(|draft| draft.validate().is_err())
                && surface.effect.validate().is_ok()
        }
        Edit::Setup | Edit::Reset => {
            surface.draft.is_none()
                && surface
                    .effect
                    .inputs
                    .task
                    .as_ref()
                    .is_some_and(|range| range.end == "10")
                && surface.effect.programs[&surface.effect.fallback]
                    == nucleus::shader_effect::CLEANING
        }
    };
    if !ready {
        world.resource_mut::<Replay>().next = frame + 12;
        return false;
    }
    press(world, Some(editor), "Close").unwrap();
    let next = settled(world, frame);
    let mut state = world.resource_mut::<Replay>();
    state.edit = None;
    state.editor = None;
    state.next = next;
    true
}

fn change(world: &mut World, root: Entity, floor: Entity, stage: usize) {
    let room = world.resource::<Demo>().room.unwrap();
    match stage {
        0 | 4 => quantity(world, 5.0),
        1 | 5 => quantity(world, 10.0),
        2 | 6 => {
            let missing = world.resource::<Replay>().unavailable.clone();
            world.get_mut::<Surface>(floor).unwrap().record = Some(missing);
        }
        3 | 7 => {
            let record = world.resource::<Replay>().record.clone();
            world.get_mut::<Surface>(floor).unwrap().record = Some(record);
            quantity(world, 0.0);
            if stage == 3 {
                world.get_mut::<View>(root).unwrap().spatial = true;
                assets::frame(world, world.resource::<Demo>().cloud.unwrap());
            }
        }
        8 => {
            editor_action(world, floor, "Freeze snapshot / Resume");
            assert!(world.get::<Surface>(floor).unwrap().frozen);
            quantity(world, 10.0);
        }
        9 => editor_action(world, floor, "Freeze snapshot / Resume"),
        10 => {
            quantity(world, 0.0);
            world.resource_mut::<Replay>().edit = Some(Edit::Invalid);
        }
        11 => world.resource_mut::<Replay>().edit = Some(Edit::Reset),
        12 | 13 => {
            world
                .get_mut::<lince_sandbox::inspection::Inspection>(root)
                .unwrap()
                .selected = Some(room);
            world.resource_mut::<Replay>().ambient = Some(stage == 13);
        }
        14 | 15 => editor_action(world, floor, "Disable / Enable"),
        16 if !world.contains_resource::<connected::Replay>() => {
            if let Some(readback) = world.resource_mut::<Replay>().readback.take() {
                world.despawn(readback);
            }
            assets::discard(world, world.resource::<Demo>().cloud.unwrap());
            for entity in world.resource::<Demo>().proxies.clone() {
                world.despawn(entity);
            }
            world.despawn(room);
        }
        _ => {}
    }
}

fn verify(world: &mut World) {
    let state = world.resource::<Replay>();
    let width = state.size.x as usize;
    let height = state.size.y as usize;
    let darkness = |dirty: usize, clean: usize| {
        let mut sum = 0u64;
        let mut count = 0;
        for y in height * 38 / 100..height * 66 / 100 {
            for x in width * 37 / 100..width * 63 / 100 {
                let offset = (y * width + x) * 4;
                let a: u32 = state.images[dirty][offset..offset + 3]
                    .iter()
                    .map(|v| u32::from(*v))
                    .sum();
                let b: u32 = state.images[clean][offset..offset + 3]
                    .iter()
                    .map(|v| u32::from(*v))
                    .sum();
                sum += u64::from(b.saturating_sub(a));
                count += usize::from(b > a + 24);
            }
        }
        (sum, count)
    };
    for (dirty, partial, clean, missing) in [(0, 1, 2, 3), (4, 5, 6, 7)] {
        let (full, pixels) = darkness(dirty, clean);
        let (half, half_pixels) = darkness(partial, clean);
        println!("Cleaning view {dirty}: dirty {full}/{pixels}px, partial {half}/{half_pixels}px");
        assert!(
            pixels > 300 && half_pixels > 100 && full > half * 12 / 10,
            "Cleaning overlay is absent or does not fade with real progress"
        );
        assert!(
            darkness(missing, clean).0 < full / 5,
            "Missing source invented dirt"
        );
    }
    for dirty in [8, 9, 11, 12, 14, 16] {
        assert!(
            darkness(dirty, 10).1 > 300,
            "Cleaning/recovery/freeze lost dirt at stage {dirty}"
        );
    }
    for clean in [13, 15] {
        assert!(
            darkness(clean, 10).0 < darkness(8, 10).0 / 5,
            "Disabled cleaning still visible at stage {clean}"
        );
    }
    assert_eq!(world.query::<&Surface>().iter(world).count(), 0);
    assert_eq!(world.query::<&rooms::Member>().iter(world).count(), 0);
    assert_eq!(
        world.query::<&assets::ImportedAsset>().iter(world).count(),
        0
    );
    println!(
        "Connected room cleaning pixels, native source/mapping editor, Freeze, recovery, Reset/Disable, ambient and cleanup passed"
    );
    world.write_message(AppExit::Success);
}

pub(super) fn exercise(world: &mut World, root: Entity) {
    let frame = world.resource::<FrameCount>().0;
    if let Some(stage) = world.resource::<Replay>().change {
        if frame < world.resource::<Replay>().next {
            return;
        }
        world.resource_mut::<Replay>().change = None;
        let floor = world.resource::<Demo>().proxies[0];
        change(world, root, floor, stage);
        let next = settled(world, frame);
        world.resource_mut::<Replay>().next = next;
        return;
    }
    let stage = world.resource::<Replay>().images.len();
    if stage == 17 && connected::active(world) {
        connected::exercise(world, root);
        return;
    }
    let floor = world.resource::<Demo>().proxies[0];
    if let Some(enabled) = world.resource::<Replay>().ambient {
        let room = world.resource::<Demo>().room.unwrap();
        if press(
            world,
            Some(room),
            if enabled {
                "Enable room ambient effects"
            } else {
                "Disable room ambient effects"
            },
        )
        .is_none()
        {
            return;
        }
        assert_eq!(
            world.get::<rooms::Room>(room).unwrap().0.ambient_enabled,
            enabled
        );
        world
            .get_mut::<lince_sandbox::inspection::Inspection>(root)
            .unwrap()
            .selected = Some(floor);
        let next = settled(world, frame);
        let mut state = world.resource_mut::<Replay>();
        state.ambient = None;
        state.next = next;
        return;
    }
    if stage < 17
        && !connected::presenting(world)
        && world
            .get::<lince_sandbox::inspection::Inspection>(root)
            .unwrap()
            .selected
            != Some(floor)
    {
        world
            .get_mut::<lince_sandbox::inspection::Inspection>(root)
            .unwrap()
            .selected = Some(floor);
        return;
    }
    if stage < 17 {
        if world.resource::<Demo>().cloud.is_none() {
            let Some(cloud) = world
                .query_filtered::<Entity, (With<assets::ImportedAsset>, With<assets::Ready>)>()
                .iter(world)
                .next()
            else {
                return;
            };
            world.resource_mut::<Demo>().cloud = Some(cloud);
            world
                .get_mut::<lince_sandbox::inspection::Inspection>(root)
                .unwrap()
                .selected = Some(floor);
            assets::frame(world, cloud);
        }
        if let Some(kind) = world.resource::<Replay>().edit {
            edit(world, floor, frame, kind);
            return;
        }
    }
    let pending = world
        .resource::<Replay>()
        .pending
        .as_ref()
        .map(|pending| pending.lock().unwrap().try_recv());
    if let Some(pending) = pending {
        match pending {
            Ok(result) => {
                result.unwrap();
                world.resource_mut::<Replay>().pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => return,
            Err(error) => panic!("Cleaning backend action failed: {error}"),
        }
    }
    if world.resource::<Replay>().waiting || frame < world.resource::<Replay>().next {
        return;
    }
    if stage == 0 && !connected::prepare(world, root) {
        return;
    }
    lince_sandbox::canvas_host::capture(world, root)
        .unwrap()
        .validate()
        .unwrap();
    if stage == 18 {
        verify(world);
        return;
    }
    if stage < 17 {
        let Some(reading) = lince_sandbox::shader_effects::surface_reading(world, floor) else {
            return;
        };
        let expected = [
            Some("0"),
            Some("5"),
            Some("10"),
            None,
            Some("0"),
            Some("5"),
            Some("10"),
            None,
            Some("0"),
            Some("10"),
            Some("10"),
            Some("0"),
            Some("0"),
            Some("0"),
            Some("0"),
            Some("0"),
            Some("0"),
        ][stage];
        let enabled = ![3, 7, 13, 15].contains(&stage);
        if !enabled {
            let mut descendants = vec![floor];
            while let Some(entity) = descendants.pop() {
                if let Some(children) = world.get::<Children>(entity) {
                    descendants.extend(children.iter());
                }
                if let Some(text) = world.get::<Text>(entity) {
                    if text.0.contains("Task: completed") {
                        return;
                    }
                }
            }
        }
        if reading.quantity.as_deref() != expected
            || reading.enabled != enabled
            || enabled && !reading.image_available
        {
            if frame.is_multiple_of(120) {
                eprintln!("Waiting for cleaning stage {stage}: {reading:?}");
            }
            return;
        }
        if world.resource::<Replay>().readback.is_none() {
            let surface = world.get::<topology::presentation::Surface>(floor).unwrap();
            let children: Vec<_> = world
                .get::<Children>(surface.visual)
                .unwrap()
                .iter()
                .collect();
            for child in children {
                if let Some(material) = world
                    .get::<MeshMaterial3d<StandardMaterial>>(child)
                    .and_then(|handle| world.resource::<Assets<StandardMaterial>>().get(&handle.0))
                {
                    let image = material.base_color_texture.clone().filter(|image| {
                        world
                            .resource::<Assets<Image>>()
                            .get(image)
                            .is_some_and(|image| image.width() == 256 && image.height() == 256)
                    });
                    if world.resource::<Replay>().readback.is_none()
                        && let Some(image) = image
                    {
                        let entity = world
                            .spawn(bevy::render::gpu_readback::Readback::texture(image))
                            .observe(
                                |event: On<bevy::render::gpu_readback::ReadbackComplete>,
                                 mut state: ResMut<Replay>| {
                                    let offset = (128 * 256 + 128) * 4;
                                    state.pixel =
                                        Some(event.data[offset..offset + 4].try_into().unwrap());
                                },
                            )
                            .id();
                        world.resource_mut::<Replay>().readback = Some(entity);
                    }
                }
            }
            return;
        }
        let Some(pixel) = world.resource::<Replay>().pixel else {
            return;
        };
        let expected_alpha = if [1, 5].contains(&stage) {
            83
        } else if [2, 6, 10].contains(&stage) {
            0
        } else {
            166
        };
        if enabled && pixel[3].abs_diff(expected_alpha) > 5 {
            if frame.is_multiple_of(120) {
                eprintln!("Waiting for cleaning GPU stage {stage}: {pixel:?}");
            }
            return;
        }
        eprintln!("Cleaning stage {stage}: {reading:?}; GPU pixel {pixel:?}");
    }
    let names = [
        "dirty-flat",
        "partial-flat",
        "clean-flat",
        "unavailable-flat",
        "dirty-spatial",
        "partial-spatial",
        "clean-spatial",
        "unavailable-spatial",
        "dirty-restored",
        "frozen-dirty",
        "resumed-clean",
        "invalid-working",
        "reset-working",
        "ambient-off",
        "ambient-on",
        "disabled",
        "enabled",
        "removed",
    ];
    if !connected::present(world, root) {
        return;
    }
    let output = world
        .resource::<Demo>()
        .output
        .join(format!("cleaning-{}.png", names[stage]));
    connected::before_capture(world, root, stage);
    world.resource_mut::<Replay>().waiting = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(output))
        .observe(
            move |event: On<ScreenshotCaptured>, mut commands: Commands| {
                let pixels = capture_pixels(&event.image);
                let size = UVec2::new(event.image.width(), event.image.height());
                commands.queue(move |world: &mut World| {
                    world.resource_mut::<Replay>().images.push(pixels);
                    world.resource_mut::<Replay>().size = size;
                    let next = if connected::active(world) {
                        connected::after_capture(world, root);
                        world.resource_mut::<Replay>().change = Some(stage);
                        world.resource::<FrameCount>().0 + 3
                    } else {
                        change(world, root, floor, stage);
                        settled(world, world.resource::<FrameCount>().0)
                    };
                    let mut state = world.resource_mut::<Replay>();
                    state.next = next;
                    state.waiting = false;
                });
            },
        );
}
