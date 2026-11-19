use crate::native::canvas::CanvasView;
use bevy::{
    ecs::entity::EntityHashMap,
    math::Affine2,
    prelude::*,
    ui::{FixedNode, OverrideClip, UiClippingBoundary, UiSystems, experimental::UiChildren},
};

pub struct CanvasClipPlugin;

impl Plugin for CanvasClipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                clipping_boundaries.before(UiSystems::Prepare),
                canvas_clips.after(UiSystems::PostLayout),
            ),
        );
    }
}

fn clipping_boundaries(
    missing: Query<Entity, (With<CanvasView>, Without<UiClippingBoundary>)>,
    canvases: Query<(), With<CanvasView>>,
    mut removed: RemovedComponents<CanvasView>,
    mut commands: Commands,
) {
    for entity in &missing {
        commands
            .entity(entity)
            .insert(UiClippingBoundary::default());
    }
    for entity in removed.read() {
        if canvases.get(entity).is_err() {
            commands.entity(entity).try_remove::<UiClippingBoundary>();
        }
    }
}

#[derive(Clone, Copy)]
struct ParentClip {
    bounds: Option<(Rect, Option<Affine2>)>,
    parent: Option<Entity>,
}

#[derive(Default)]
struct ClipPreparation {
    parents: EntityHashMap<ParentClip>,
    pending: Vec<(Entity, Option<CalculatedClip>)>,
}

pub fn clipped_rect(rect: Rect, clip: &CalculatedClip) -> Rect {
    let points = [
        (rect.min, ()),
        (Vec2::new(rect.max.x, rect.min.y), ()),
        (rect.max, ()),
        (Vec2::new(rect.min.x, rect.max.y), ()),
    ];
    let polygon = bevy::ui_render::clipping::clip_polygon(Some(clip), &points, |(), (), _| ());
    let Some((first, ())) = polygon.first().copied() else {
        return Rect::default();
    };
    polygon.iter().fold(Rect::from_corners(first, first), |rect, (point, ())| {
        Rect::from_corners(rect.min.min(*point), rect.max.max(*point))
    })
}

fn add_clip(clip: &mut Option<CalculatedClip>, rect: Rect, world_to_clip: Option<Affine2>) {
    if let Some(world_to_clip) = world_to_clip {
        clip.get_or_insert_with(CalculatedClip::default)
            .push_rect(rect, world_to_clip);
    } else {
        *clip = Some(CalculatedClip::FullyClipped);
    }
}

fn canvas_clips(
    roots: Query<Entity, With<CanvasView>>,
    nodes: Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        Has<OverrideClip>,
        Has<FixedNode>,
    )>,
    children: UiChildren,
    layouts: Query<&crate::native::layout::LayoutRuntime>,
    mut boundaries: Query<&mut UiClippingBoundary>,
    mut clips: Query<&mut CalculatedClip>,
    mut commands: Commands,
    mut preparation: Local<ClipPreparation>,
) {
    let _preparation = engine::performance::Span::new("native-canvas-clipping");
    let ClipPreparation { parents, pending } = &mut *preparation;
    parents.clear();
    pending.clear();
    let mut visited = 0;
    let mut ancestor_visits = 0;
    'roots: for root in &roots {
        let mut ancestor = root;
        while let Some(parent) = children.get_parent(ancestor) {
            if !nodes.contains(parent) {
                break;
            }
            if roots.contains(parent) {
                continue 'roots;
            }
            if boundaries.contains(parent) {
                break;
            }
            ancestor = parent;
        }
        let initial = boundaries
            .get(root)
            .map(|boundary| boundary.inherited_clip.clone())
            .unwrap_or_else(|_| clips.get(root).ok().cloned());
        pending.push((root, initial));
        while let Some((entity, mut clip)) = pending.pop() {
            let Ok((node, computed, transform, override_clip, fixed)) = nodes.get(entity) else {
                continue;
            };
            visited += 1;
            if entity != root
                && let Ok(mut boundary) = boundaries.get_mut(entity)
            {
                if boundary.inherited_clip != clip {
                    boundary.inherited_clip = clip.clone();
                }
                if !roots.contains(entity) {
                    continue;
                }
            }
            if override_clip || fixed {
                clip = None;
            }
            if node.display == Display::None {
                clip = Some(CalculatedClip::FullyClipped);
            }
            let mut container = layouts.get(entity).ok().and_then(|layout| layout.parent);
            for _ in 0..crate::native::layout::MAX_DEPTH {
                let Some(parent) = container else { break };
                ancestor_visits += 1;
                let prepared = parents.entry(parent).or_insert_with(|| ParentClip {
                    bounds: nodes
                        .get(parent)
                        .ok()
                        .map(|(node, computed, transform, _, _)| {
                            let inverse = (node.display != Display::None)
                                .then(|| transform.try_inverse())
                                .flatten();
                            (computed.content_box(), inverse)
                        }),
                    parent: layouts.get(parent).ok().and_then(|layout| layout.parent),
                });
                if let Some((rect, world_to_clip)) = prepared.bounds {
                    add_clip(&mut clip, rect, world_to_clip);
                }
                container = prepared.parent;
            }
            match (clips.get_mut(entity), clip.as_ref()) {
                (Ok(mut current), Some(clip)) => {
                    if *current != *clip {
                        *current = clip.clone();
                    }
                }
                (Err(_), Some(clip)) => {
                    commands.entity(entity).insert(clip.clone());
                }
                (Ok(_), None) => {
                    commands.entity(entity).remove::<CalculatedClip>();
                }
                (Err(_), None) => {}
            }
            if !node.overflow.is_visible() {
                add_clip(
                    &mut clip,
                    computed.resolve_clip_rect(node.overflow, node.overflow_clip_margin),
                    transform.try_inverse(),
                );
            }
            pending.extend(
                children
                    .iter_ui_children(entity)
                    .map(|child| (child, clip.clone())),
            );
        }
    }
    engine::performance::add("native-canvas-clip-nodes", visited);
    engine::performance::add("native-canvas-clip-ancestor-visits", ancestor_visits);
    engine::performance::add("native-canvas-clip-parents-prepared", parents.len() as u64);
    if parents.capacity() > (parents.len().saturating_mul(4)).max(256) {
        parents.shrink_to_fit();
    }
    if pending.capacity() > (visited as usize).saturating_mul(4).max(256) {
        pending.shrink_to_fit();
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn scaled_editor_clip_contains_its_full_content_and_keeps_open_axes() {
        for zoom in [0.1, 0.5, 1.2, 3.0] {
            let transform = Affine2::from_scale_angle_translation(
                Vec2::splat(zoom),
                0.0,
                Vec2::new(400.0, 200.0),
            );
            let content = Rect::from_center_size(Vec2::new(4.0, 8.0), Vec2::new(180.0, 80.0));
            let mut clip = None;
            add_clip(&mut clip, content, Some(transform.inverse()));
            let clip = clip.unwrap();
            assert!(clip.contains_point(transform.transform_point2(content.center())));
            assert!(!clip.contains_point(transform.transform_point2(content.max + Vec2::ONE)));

            let mut open = None;
            add_clip(
                &mut open,
                Rect {
                    min: Vec2::new(f32::NEG_INFINITY, -40.0),
                    max: Vec2::new(f32::INFINITY, 40.0),
                },
                Some(transform.inverse()),
            );
            let open = open.unwrap();
            assert!(open.contains_point(transform.transform_point2(Vec2::new(10_000.0, 0.0))));
            assert!(!open.contains_point(transform.transform_point2(Vec2::new(0.0, 41.0))));
        }
    }

    crate::native::laboratory_cases! {
        scaled_editor_clip_contains_its_full_content_and_keeps_open_axes,
    }
}
