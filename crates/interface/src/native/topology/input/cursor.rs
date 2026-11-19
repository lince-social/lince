use bevy::{
    camera::{NormalizedRenderTarget, RenderTarget},
    picking::pointer::{PointerId, PointerLocation},
    prelude::*,
    ui::{CalculatedClip, ComputedUiTargetCamera, RelativeCursorPosition},
    window::PrimaryWindow,
};

pub(crate) fn relative(
    primary: Query<Entity, With<PrimaryWindow>>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    cameras: Query<(&Camera, &RenderTarget)>,
    mut cursors: Query<(
        &ComputedNode,
        &UiGlobalTransform,
        &ComputedUiTargetCamera,
        &InheritedVisibility,
        Option<&CalculatedClip>,
        &mut RelativeCursorPosition,
    )>,
) {
    let primary = primary.iter().next();
    for (node, transform, camera, visibility, clip, mut cursor) in &mut cursors {
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
                    && clip.is_none_or(|clip| clip.contains_point(position))
            });
        cursor.set_if_neq(RelativeCursorPosition {
            cursor_over,
            normalized,
        });
    }
}
