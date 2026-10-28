use bevy::prelude::*;

#[derive(Component)]
pub struct ScrollSand;

const CREDITS: &[crate::native::credits::Attribution] = &[crate::native::credits::Attribution {
    name: "Bevy",
    author: "Bevy contributors",
    license: crate::native::credits::BEVY_LICENSE,
}];

pub fn attach(world: &mut World, entity: Entity) {
    if world.get::<ScrollSand>(entity).is_none() {
        world
            .entity_mut(entity)
            .insert((ScrollSand, ScrollPosition::default()));
    }
    world.spawn((
        crate::native::sand_store::SandCredits(CREDITS),
        ChildOf(entity),
    ));
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.overflow = Overflow::scroll_y();
    }
}

pub(crate) fn reveal(
    world: &mut World,
    viewport: Entity,
    target: Entity,
    inset: f32,
) -> Option<bool> {
    let panel = world.get::<ComputedNode>(viewport)?;
    let node = world.get::<ComputedNode>(target)?;
    if panel.size().y <= 0.0 || node.size().y <= 0.0 {
        return None;
    }
    let mut current = target;
    let mut contained = false;
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(parent) = world.get::<ChildOf>(current) else {
            break;
        };
        current = parent.parent();
        if current == viewport {
            contained = true;
            break;
        }
    }
    if !contained || world.get::<Node>(viewport)?.display == Display::None {
        return None;
    }
    let scale = panel.inverse_scale_factor();
    if !scale.is_finite() || scale <= 0.0 || !inset.is_finite() {
        return None;
    }
    let inset = inset.max(0.0) / scale;
    let center = world.get::<UiGlobalTransform>(viewport)?.translation.y;
    let start = center - panel.size().y * 0.5 + inset;
    let end = center + panel.size().y * 0.5 - inset;
    let top = world.get::<UiGlobalTransform>(target)?.translation.y - node.size().y * 0.5;
    let bottom = top + node.size().y;
    let delta = if top < start || node.size().y > end - start {
        top - start
    } else if bottom > end {
        bottom - end
    } else {
        0.0
    };
    let max = (panel.content_size.y - panel.size.y + panel.scrollbar_size.y).max(0.0) * scale;
    let mut scroll = world.get_mut::<ScrollPosition>(viewport)?;
    let next = (scroll.0.y + delta * scale).clamp(0.0, max);
    let changed = (next - scroll.0.y).abs() > 0.5;
    if changed {
        scroll.0.y = next;
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    }
    Some(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine2;

    #[test]
    fn revealing_a_selected_descendant_scrolls_once_without_changing_input_focus() {
        let mut world = World::new();
        world.init_resource::<bevy::input_focus::InputFocus>();
        let input = world.spawn_empty().id();
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(input, bevy::input_focus::FocusCause::Navigated);
        let viewport = world
            .spawn((
                Node::default(),
                ComputedNode {
                    size: Vec2::new(300.0, 100.0),
                    content_size: Vec2::new(300.0, 500.0),
                    inverse_scale_factor: 0.5,
                    ..default()
                },
                UiGlobalTransform::default(),
                ScrollPosition::default(),
            ))
            .id();
        let target = world
            .spawn((
                ComputedNode {
                    size: Vec2::new(280.0, 20.0),
                    ..default()
                },
                UiGlobalTransform::from(Affine2::from_translation(Vec2::new(0.0, 180.0))),
                ChildOf(viewport),
            ))
            .id();
        assert_eq!(reveal(&mut world, viewport, target, 8.0), Some(true));
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 78.0);
        world
            .entity_mut(target)
            .insert(UiGlobalTransform::from(Affine2::from_translation(
                Vec2::new(0.0, 24.0),
            )));
        assert_eq!(reveal(&mut world, viewport, target, 8.0), Some(false));
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 78.0);
        world.entity_mut(target).remove::<ChildOf>();
        assert_eq!(reveal(&mut world, viewport, target, 8.0), None);
        assert_eq!(
            world.resource::<bevy::input_focus::InputFocus>().get(),
            Some(input)
        );
    }

    #[test]
    fn wheel_over_content_scrolls_once_and_stops_at_both_ends() {
        let mut app = App::new();
        app.add_plugins(crate::native::layout::LayoutPlugin);
        let world = app.world_mut();
        let viewport = world
            .spawn((
                Node::default(),
                ComputedNode {
                    size: Vec2::new(520.0, 640.0),
                    content_size: Vec2::new(520.0, 1600.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
            ))
            .id();
        attach(world, viewport);
        let content = world.spawn((Node::default(), ChildOf(viewport))).id();
        for (delta, expected) in [(-90.0, 90.0), (-2000.0, 960.0), (2000.0, 0.0)] {
            world.trigger(Pointer::new(
                bevy::picking::pointer::PointerId::Mouse,
                bevy::picking::pointer::Location {
                    target: bevy::camera::NormalizedRenderTarget::Image(
                        Handle::<Image>::default().into(),
                    ),
                    position: Vec2::ZERO,
                },
                bevy::picking::events::Scroll {
                    unit: bevy::input::mouse::MouseScrollUnit::Pixel,
                    x: 0.0,
                    y: delta,
                    phase: bevy::input::touch::TouchPhase::Moved,
                    hit: bevy::picking::backend::HitData::new(viewport, 0.0, None, None),
                },
                content,
            ));
            assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, expected);
        }
    }
}
