use bevy::prelude::*;

#[derive(Resource, Default)]
struct Insets {
    physical: [i32; 4],
    applied: Option<[f32; 4]>,
}

#[derive(Component)]
pub(crate) struct InsetModal;

fn logical(world: &mut World) -> [f32; 4] {
    let physical = world
        .get_resource::<Insets>()
        .map_or([0; 4], |insets| insets.physical);
    let scale = world
        .query::<&Window>()
        .iter(world)
        .next()
        .map_or(1.0, Window::scale_factor);
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    physical.map(|value| value.max(0) as f32 / scale)
}

pub(crate) fn modal(world: &mut World) -> (crate::app::Modal, InsetModal, Node) {
    let [left, top, right, bottom] = logical(world).map(px);
    (
        crate::app::Modal,
        InsetModal,
        Node {
            position_type: PositionType::Absolute,
            left,
            top,
            right,
            bottom,
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(),
            padding: UiRect::all(px(18)),
            row_gap: px(12),
            ..default()
        },
    )
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn update(world: &mut World, physical: [i32; 4]) {
    world.init_resource::<Insets>();
    world.resource_mut::<Insets>().physical = physical;
    refresh(world);
}

pub(crate) fn refresh(world: &mut World) {
    let Some(insets) = world.get_resource::<Insets>() else {
        return;
    };
    let applied = insets.applied;
    let values = logical(world);
    if applied == Some(values) {
        return;
    }
    world.resource_mut::<Insets>().applied = Some(values);
    let [left, top, right, bottom] = values.map(|value| px(12.0 + value));
    for mut node in world
        .query_filtered::<&mut Node, With<crate::app::Shell>>()
        .iter_mut(world)
    {
        node.padding = UiRect {
            left,
            top,
            right,
            bottom,
        };
    }
    let [left, top, right, bottom] = values.map(px);
    for mut node in world
        .query_filtered::<&mut Node, With<InsetModal>>()
        .iter_mut(world)
    {
        node.left = left;
        node.top = top;
        node.right = right;
        node.bottom = bottom;
    }
}
