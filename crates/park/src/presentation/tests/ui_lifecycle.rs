use super::*;
use bevy::ui::{ContentSize, FixedMeasure, NodeMeasure, experimental::GhostNode};
use lince_interface::host::{DesktopAction, WorkspaceLayout};
use native::{canvas::CanvasItem, layout::LayoutBox, workspace::WorkspaceMember};
use std::time::{Duration, Instant};

fn card(canvas: &mut Presentation, root: Entity, x: f64) -> Entity {
    let size = Vec2::new(400.0, 260.0);
    canvas
        .world_mut()
        .spawn((
            CanvasItem {
                position: bevy::math::DVec2::new(x, 0.0),
                size,
            },
            LayoutBox::new(size),
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id()
}

fn label(canvas: &mut Presentation, parent: Entity, text: &str, color: Color) -> Entity {
    let font = canvas
        .app
        .world()
        .resource::<native::theme::Typography>()
        .text(24.0);
    canvas
        .world_mut()
        .spawn((
            Text::new(text),
            font,
            TextColor(color),
            TextLayout::no_wrap(),
            Node {
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn ink(pixels: &[u8]) -> [usize; 2] {
    [false, true].map(|cyan| {
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[0] < 40 && pixel[1] > 220 && if cyan { pixel[2] > 220 } else { pixel[2] < 40 }
            })
            .count()
    })
}

fn assert_content(canvas: &mut Presentation, entities: [Entity; 3], expected: [Vec2; 3]) {
    let frame = canvas.probe_frame().unwrap();
    let root = canvas.canvas_root().unwrap();
    assert!(canvas.app.world().get::<GhostNode>(root).is_some());
    assert!(canvas.app.world().get::<Node>(root).is_none());
    for (entity, size) in entities.into_iter().zip(expected) {
        assert_eq!(
            canvas
                .app
                .world()
                .get::<ComputedNode>(entity)
                .unwrap()
                .size(),
            size,
            "An unrelated UI lifecycle change discarded a retained content measurement"
        );
    }
    for entity in &entities[..2] {
        assert!(
            !canvas
                .app
                .world()
                .get::<bevy::text::TextLayoutInfo>(*entity)
                .unwrap()
                .glyphs
                .is_empty()
        );
    }
    assert!(
        ink(&probe_pixels(frame.dmabuf().unwrap()).unwrap())
            .iter()
            .all(|pixels| *pixels > 100),
        "Existing label or button text disappeared from the rendered desktop"
    );
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn unrelated_ui_replacements_and_ghost_transitions_preserve_static_text_and_intrinsic_measurements()
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new(
        [1000, 720],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas
        .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
        .unwrap();
    canvas.world_mut().entity_mut(root).insert((
        native::canvas::CanvasView::default(),
        native::topology::view::View::default(),
    ));
    let stable = card(&mut canvas, root, -230.0);
    let text = label(
        &mut canvas,
        stable,
        "Persistent Castle text",
        Color::srgb(0.0, 1.0, 0.0),
    );
    let button = canvas
        .world_mut()
        .spawn((
            Node {
                border: UiRect::all(px(2)),
                padding: UiRect::all(px(6)),
                ..default()
            },
            BorderColor::all(Color::WHITE),
            ChildOf(stable),
        ))
        .id();
    let button_text = label(
        &mut canvas,
        button,
        "Persistent button",
        Color::srgb(0.0, 1.0, 1.0),
    );
    let mut intrinsic = ContentSize::default();
    intrinsic.set(NodeMeasure::Fixed(FixedMeasure {
        size: Vec2::new(96.0, 24.0),
    }));
    let measured = canvas
        .world_mut()
        .spawn((
            Node {
                align_self: AlignSelf::Start,
                flex_shrink: 0.0,
                ..default()
            },
            intrinsic,
            BackgroundColor(Color::WHITE),
            ChildOf(stable),
        ))
        .id();
    let mut changing = card(&mut canvas, root, 230.0);
    label(&mut canvas, changing, "Refresh 0", Color::WHITE);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let frame = canvas.probe_frame().unwrap();
        if ink(&probe_pixels(frame.dmabuf().unwrap()).unwrap())
            .iter()
            .all(|pixels| *pixels > 100)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The initial real font rendering did not become visible"
        );
    }
    let entities = [text, button_text, measured];
    let expected = entities.map(|entity| {
        canvas
            .app
            .world()
            .get::<ComputedNode>(entity)
            .unwrap()
            .size()
    });
    assert_eq!(expected[2], Vec2::new(96.0, 24.0));
    for refresh in 1..=8 {
        let view = *canvas.app.world().get::<native::canvas::CanvasView>(root).unwrap();
        canvas.world_mut().entity_mut(root).insert(view);
        canvas.world_mut().despawn(changing);
        changing = card(&mut canvas, root, 230.0);
        label(
            &mut canvas,
            changing,
            &format!("Refresh {refresh}"),
            Color::WHITE,
        );
        for _ in 0..4 {
            assert_content(&mut canvas, entities, expected);
        }
    }
    let wrapper = canvas
        .world_mut()
        .spawn((Node::default(), ChildOf(changing)))
        .id();
    label(&mut canvas, wrapper, "Changing hierarchy", Color::WHITE);
    assert_content(&mut canvas, entities, expected);
    for _ in 0..4 {
        canvas
            .world_mut()
            .entity_mut(wrapper)
            .remove::<Node>()
            .insert(GhostNode);
        assert_content(&mut canvas, entities, expected);
        canvas
            .world_mut()
            .entity_mut(wrapper)
            .remove::<GhostNode>()
            .insert(Node::default());
        assert_content(&mut canvas, entities, expected);
    }
    canvas.finish().unwrap();
}
