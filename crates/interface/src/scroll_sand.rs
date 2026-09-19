use bevy::prelude::*;

#[derive(Component)]
pub struct ScrollSand;

const CREDITS: &[crate::credits::Attribution] = &[crate::credits::Attribution {
    name: "Bevy",
    author: "Bevy contributors",
    license: crate::credits::BEVY_LICENSE,
}];

pub fn attach(world: &mut World, entity: Entity) {
    if world.get::<ScrollSand>(entity).is_none() {
        world
            .entity_mut(entity)
            .insert((ScrollSand, ScrollPosition::default()));
    }
    world.spawn((crate::sand_store::SandCredits(CREDITS), ChildOf(entity)));
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.overflow = Overflow::scroll_y();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_over_content_scrolls_once_and_stops_at_both_ends() {
        let mut app = App::new();
        app.add_plugins(crate::layout::LayoutPlugin);
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
