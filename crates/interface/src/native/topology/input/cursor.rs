use bevy::{
    camera::{NormalizedRenderTarget, RenderTarget},
    picking::pointer::{PointerId, PointerLocation},
    prelude::*,
    ui::{ComputedUiTargetCamera, OverrideClip, RelativeCursorPosition, clip_check_recursive},
    window::PrimaryWindow,
};

pub(crate) fn relative(
    primary: Query<Entity, With<PrimaryWindow>>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    cameras: Query<(&Camera, &RenderTarget)>,
    mut cursors: Query<(
        Entity,
        &ComputedNode,
        &UiGlobalTransform,
        &ComputedUiTargetCamera,
        &InheritedVisibility,
        &mut RelativeCursorPosition,
    )>,
    clipping: Query<(&ComputedNode, &UiGlobalTransform, &Node)>,
    parents: Query<&ChildOf, Without<OverrideClip>>,
) {
    let primary = primary.iter().next();
    for (entity, node, transform, camera, visibility, mut cursor) in &mut cursors {
        let Some((camera, target)) = camera.get().and_then(|camera| cameras.get(camera).ok())
        else {
            continue;
        };
        let Some(target @ NormalizedRenderTarget::Image(_)) = target.normalize(primary) else {
            continue;
        };
        let position = pointers
            .iter()
            .filter_map(|(id, pointer)| Some((id, pointer.location.as_ref()?)))
            .filter(|(_, location)| location.target == target)
            .min_by_key(|(id, _)| {
                if **id == super::CONTENT_POINTER {
                    0
                } else if **id == PointerId::Mouse {
                    1
                } else {
                    2
                }
            })
            .map(|(_, location)| {
                let NormalizedRenderTarget::Image(image) = &target else {
                    unreachable!()
                };
                let origin = camera
                    .physical_viewport_rect()
                    .map_or(Vec2::ZERO, |rect| rect.min.as_vec2());
                location.position * image.scale_factor - origin
            });
        let normalized = position.and_then(|position| node.normalize_point(*transform, position));
        let cursor_over = visibility.get()
            && position.is_some_and(|position| {
                node.contains_point(*transform, position)
                    && clip_check_recursive(position, entity, &clipping, &parents)
            });
        cursor.set_if_neq(RelativeCursorPosition {
            cursor_over,
            normalized,
        });
    }
}
