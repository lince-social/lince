use super::{
    schedules::{Clock, frame_pixels, summary},
    *,
};
use bevy::{
    text::{EditableText, TextLayoutInfo},
    ui::ContentSize,
};

#[test]
#[ignore = "private hardware idle-editor measurement and UI transition checks; no concurrent compilation"]
fn idle_editors_record_measurement_and_exact_pixels() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(checks.first().is_some_and(|check| {
        check
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    }));
    assert!(
        checks.iter().all(|check| check.error.is_none()),
        "{checks:?}"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut canvas, _) = super::offscreen::fixture(directory.path(), "visible-only", 0);
    let root = canvas.canvas_root().unwrap();
    let owner = super::super::appearance::item(
        &mut canvas,
        root,
        Vec2::splat(400.0),
        [10000.0, 0.0],
        Color::BLACK,
        0,
        false,
    );
    let editors = (0..64)
        .map(|index| {
            let editor = native::sand_text::spawn(
                canvas.world_mut(),
                owner,
                native::sand_text::SavedText {
                    area: native::sand_text::SandText::new(true),
                    text: format!("Idle field {index}"),
                },
            );
            if index % 2 == 0 {
                canvas
                    .world_mut()
                    .get_mut::<EditableText>(editor)
                    .unwrap()
                    .visible_lines = Some(1.0);
            }
            editor
        })
        .collect::<Vec<_>>();
    for _ in 0..8 {
        drop(canvas.probe_frame().unwrap());
    }
    let reference = frame_pixels(&canvas.probe_frame().unwrap());
    let geometry = editors
        .iter()
        .map(|entity| {
            canvas
                .app
                .world()
                .get::<ComputedNode>(*entity)
                .unwrap()
                .size()
        })
        .collect::<Vec<_>>();
    let mut updates = [0; 3];
    let mut readings = Vec::new();
    lince_interface::host::performance::flush();
    for _ in 0..24 {
        let ticks = editors
            .iter()
            .map(|entity| {
                let entity = canvas.app.world().entity(*entity);
                [
                    entity.get_ref::<EditableText>().unwrap().last_changed(),
                    entity.get_ref::<ContentSize>().unwrap().last_changed(),
                    entity.get_ref::<TextLayoutInfo>().unwrap().last_changed(),
                ]
            })
            .collect::<Vec<_>>();
        let clock = Clock::start();
        drop(canvas.probe_frame().unwrap());
        readings.push(clock.finish());
        lince_interface::host::performance::flush_if_due();
        for (entity, previous) in editors.iter().zip(ticks) {
            let entity = canvas.app.world().entity(*entity);
            let current = [
                entity.get_ref::<EditableText>().unwrap().last_changed(),
                entity.get_ref::<ContentSize>().unwrap().last_changed(),
                entity.get_ref::<TextLayoutInfo>().unwrap().last_changed(),
            ];
            for index in 0..3 {
                updates[index] += usize::from(previous[index] != current[index]);
            }
        }
    }
    lince_interface::host::performance::flush();
    for (index, (entity, size)) in editors.iter().zip(geometry).enumerate() {
        assert_eq!(
            canvas
                .app
                .world()
                .get::<EditableText>(*entity)
                .unwrap()
                .value(),
            format!("Idle field {index}").as_str()
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<ComputedNode>(*entity)
                .unwrap()
                .size(),
            size
        );
        assert!(
            !canvas
                .app
                .world()
                .get::<TextLayoutInfo>(*entity)
                .unwrap()
                .glyphs
                .is_empty()
        );
    }
    assert_eq!(frame_pixels(&canvas.probe_frame().unwrap()), reference);
    let report = serde_json::json!({"scope":"private unoptimized idle editor measurement; no physical latency claim",
        "editors":editors.len(),"updates":24,"component_tick_changes":updates,
        "frames":summary(&readings),"readings":readings});
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for (index, pixels) in reference.iter().enumerate() {
            std::fs::write(directory.join(format!("output-{index}.rgba")), pixels).unwrap();
        }
        std::fs::write(
            directory.join("idle-editors.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    eprintln!("idle-editors: {report}");
    transitions(&mut canvas, &reference);
    canvas.finish().unwrap();
}

fn transition_pixels(canvas: &mut Presentation) -> Vec<Vec<u8>> {
    let pixels = frame_pixels(&super::super::shortcuts::settle(canvas));
    assert_eq!(frame_pixels(&canvas.probe_frame().unwrap()), pixels);
    pixels
}

fn transitions(canvas: &mut Presentation, reference: &[Vec<u8>]) {
    let label = canvas
        .world_mut()
        .query::<(Entity, &Text)>()
        .iter(canvas.app.world())
        .find_map(|(entity, text)| (text.0 == "Item 0").then_some(entity))
        .unwrap();
    let parent = canvas.app.world().get::<ChildOf>(label).unwrap().parent();
    canvas.world_mut().get_mut::<Text>(label).unwrap().0 = "WWWW".into();
    assert_ne!(transition_pixels(canvas), reference);
    canvas.world_mut().get_mut::<Text>(label).unwrap().0 = "Item 0".into();
    assert_eq!(transition_pixels(canvas), reference);

    let node = canvas.app.world().get::<Node>(label).unwrap().clone();
    canvas.world_mut().get_mut::<Node>(label).unwrap().left = px(12);
    assert_ne!(transition_pixels(canvas), reference);
    canvas.world_mut().entity_mut(label).insert(node);
    assert_eq!(transition_pixels(canvas), reference);

    let transform = *canvas.app.world().get::<UiTransform>(label).unwrap();
    canvas
        .world_mut()
        .get_mut::<UiTransform>(label)
        .unwrap()
        .translation = Val2::px(6, 0);
    assert_ne!(transition_pixels(canvas), reference);
    canvas.world_mut().entity_mut(label).insert(transform);
    assert_eq!(transition_pixels(canvas), reference);

    let font = canvas.app.world().get::<TextFont>(label).unwrap().clone();
    canvas
        .world_mut()
        .get_mut::<TextFont>(label)
        .unwrap()
        .font_size = bevy::text::FontSize::Px(18.0);
    assert_ne!(transition_pixels(canvas), reference);
    canvas.world_mut().entity_mut(label).insert(font);
    assert_eq!(transition_pixels(canvas), reference);

    let marker = canvas
        .world_mut()
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(10),
                top: px(40),
                width: px(20),
                height: px(20),
                ..default()
            },
            BackgroundColor(Color::srgb(1.0, 0.0, 0.0)),
            ChildOf(parent),
        ))
        .id();
    let inserted = transition_pixels(canvas);
    assert_ne!(inserted, reference);
    let other = canvas
        .app
        .world()
        .get::<Children>(parent)
        .unwrap()
        .iter()
        .find(|entity| {
            *entity != marker
                && *entity != label
                && canvas.app.world().get::<Text>(*entity).is_none()
                && canvas
                    .app
                    .world()
                    .get::<Node>(*entity)
                    .is_some_and(|node| node.left == px(6))
        })
        .unwrap();
    canvas.world_mut().entity_mut(marker).insert(ChildOf(other));
    let moved = transition_pixels(canvas);
    assert_ne!(moved, reference);
    assert_ne!(moved, inserted);
    canvas.world_mut().despawn(marker);
    assert_eq!(transition_pixels(canvas), reference);

    let color = *canvas.app.world().get::<BackgroundColor>(parent).unwrap();
    canvas
        .world_mut()
        .get_mut::<BackgroundColor>(parent)
        .unwrap()
        .0 = Color::srgb(1.0, 0.0, 0.0);
    let painted = frame_pixels(&canvas.probe_frame().unwrap());
    assert_ne!(painted, reference);
    canvas.world_mut().entity_mut(parent).insert(color);
    assert_eq!(transition_pixels(canvas), reference);
}
