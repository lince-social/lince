use super::*;
use smithay::{backend::input::Axis, input::pointer::AxisFrame};

pub(super) fn panel_visible(fixture: &mut Fixture) {
    let size = Vec2::from_array(fixture.size.map(|value| value as f32));
    inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        for mode in world
            .query::<&ui::edit_mode::EditMode>()
            .iter(world)
            .filter(|mode| mode.enabled)
        {
            let node = world.get::<ComputedNode>(mode.panel).unwrap();
            let transform = world.get::<UiGlobalTransform>(mode.panel).unwrap();
            let low = transform.transform_point2(-node.size() * 0.5);
            let high = transform.transform_point2(node.size() * 0.5);
            assert!(
                low.min_element() >= -1.0 && (high - size).max_element() <= 1.0,
                "The edit panel escaped its output: {low:?}..{high:?}, {size:?}"
            );
        }
    });
}

pub(super) fn modal_usable(fixture: &mut Fixture) {
    let size = Vec2::from_array(fixture.size.map(|value| value as f32));
    inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let mut footer = false;
        let mut body = false;
        for (entity, name, node, transform) in world
            .query::<(Entity, &Name, &ComputedNode, &UiGlobalTransform)>()
            .iter(world)
        {
            if name.as_str() == "shortcuts.footer" {
                footer = true;
                let low = transform.transform_point2(-node.size() * 0.5);
                let high = transform.transform_point2(node.size() * 0.5);
                assert!(
                    low.min_element() >= -1.0 && (high - size).max_element() <= 1.0,
                    "The modal footer escaped its output: {low:?}..{high:?}, {size:?}"
                );
                assert!(node.size().min_element() > 0.0);
            } else if name.as_str() == "shortcuts.body" {
                body = true;
                assert!(node.size().min_element() > 0.0, "The modal body collapsed");
                assert_eq!(
                    world.get::<Node>(entity).unwrap().overflow.y,
                    OverflowAxis::Scroll
                );
            }
        }
        assert!(
            body && footer,
            "The modal is missing its scroll body or fixed footer"
        );
    });
}

pub(super) fn frame(fixture: &mut Fixture, stage: &str) {
    let scene = fixture.directory.file_name().unwrap().to_string_lossy();
    fixture.render(&format!("native-ime-{scene}-{stage}"));
}

pub(super) fn filter_unchanged(fixture: &mut Fixture) {
    inspect(fixture, |canvas| {
        let world = canvas.world_mut();
        let (_, input) = world
            .query::<(&bevy::a11y::AccessibilityNode, &EditableText)>()
            .iter(world)
            .find(|(node, _)| {
                node.label() == Some("Search command names, scopes or key combinations")
            })
            .unwrap();
        assert_eq!(input.value().to_string(), "");
        assert!(!input.is_composing());
    });
}

pub(super) fn click(fixture: &mut Fixture, point: [f64; 2]) {
    fixture.desktop.move_pointer(point, 40).unwrap();
    fixture.flush();
    frame(fixture, "control-motion");
    for pressed in [true, false] {
        fixture.desktop.button(272, pressed, 41).unwrap();
        fixture.flush();
        frame(
            fixture,
            if pressed {
                "control-press"
            } else {
                "control-release"
            },
        );
    }
}

pub(super) fn name(fixture: &mut Fixture, name: &str) -> [f64; 2] {
    let name = name.to_string();
    let entity = inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, candidate)| candidate.as_str() == name)
            .unwrap()
            .0
    });
    reveal(fixture, entity)
}

pub(super) fn caption(fixture: &mut Fixture, caption: &str) -> [f64; 2] {
    let caption = caption.to_string();
    let entity = inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        world
            .query::<(Entity, &Children, &ui::actions::ActionButton)>()
            .iter(world)
            .find(|(entity, children, _)| {
                world
                    .get::<ComputedNode>(*entity)
                    .is_some_and(|node| node.size().min_element() > 0.0)
                    && children.iter().any(|child| {
                        world
                            .get::<Text>(child)
                            .is_some_and(|text| text.0 == caption)
                    })
            })
            .unwrap()
            .0
    });
    reveal(fixture, entity)
}

fn reveal(fixture: &mut Fixture, entity: Entity) -> [f64; 2] {
    let size = Vec2::from_array(fixture.size.map(|value| value as f32));
    for _ in 0..20 {
        let (point, wheel) = inspect(fixture, move |canvas| {
            let world = canvas.world_mut();
            let point = world
                .get::<UiGlobalTransform>(entity)
                .unwrap()
                .transform_point2(Vec2::ZERO);
            if Rect::from_corners(Vec2::ZERO, size).contains(point)
                && world
                    .get::<bevy::ui::CalculatedClip>(entity)
                    .is_none_or(|clip| clip.clip.contains(point))
            {
                return (point, None);
            }
            let mut ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            for _ in 0..ui::layout::MAX_DEPTH {
                let Some(current) = ancestor else { break };
                let node = world.get::<Node>(current);
                if node.is_some_and(|node| node.overflow.y == OverflowAxis::Scroll)
                    && let (Some(node), Some(transform)) = (
                        world.get::<ComputedNode>(current),
                        world.get::<UiGlobalTransform>(current),
                    )
                {
                    let center = transform.transform_point2(Vec2::ZERO);
                    let mut bounds = Rect::from_center_size(center, node.size());
                    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(current) {
                        bounds = bounds.intersect(clip.clip);
                    }
                    bounds = bounds.intersect(Rect::from_corners(Vec2::ZERO, size));
                    if bounds.size().min_element() > 0.0 {
                        return (
                            point,
                            Some((
                                Vec2::new(bounds.max.x - 3.0, bounds.center().y),
                                f64::from((point.y - center.y).clamp(-512.0, 512.0)),
                            )),
                        );
                    }
                }
                ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
            }
            panic!("The native control has no visible scroll owner");
        });
        let Some((position, delta)) = wheel else {
            return fixture
                .viewport()
                .raster
                .logical_point(point.as_dvec2().to_array());
        };
        fixture
            .desktop
            .move_pointer(
                fixture
                    .viewport()
                    .raster
                    .logical_point(position.as_dvec2().to_array()),
                42,
            )
            .unwrap();
        fixture.flush();
        fixture
            .desktop
            .axis(
                [0.0, delta],
                AxisFrame::new(43).value(Axis::Vertical, delta),
            )
            .unwrap();
        fixture.flush();
    }
    fixture.render("native-ime-control-unreachable");
    panic!("The native control did not become reachable through scrolling");
}
