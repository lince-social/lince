use crate::native::canvas::CanvasView;
use bevy::{
    ecs::entity::EntityHashMap,
    math::Affine2,
    prelude::*,
    render::{Extract, ExtractSchedule, RenderApp},
    ui::{
        OverrideClip, UiClippingBoundary, UiSystems, experimental::UiChildren, widget::TextScroll,
    },
    ui_render::{ExtractedUiNodes, RenderUiSystems},
};

pub struct CanvasClipPlugin;

#[derive(Resource, Default)]
struct TextStart(usize);

impl Plugin for CanvasClipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                clipping_boundaries.before(UiSystems::Prepare),
                canvas_clips.after(UiSystems::PostLayout),
            ),
        );
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.init_resource::<TextStart>().add_systems(
                ExtractSchedule,
                (
                    text_start
                        .after(RenderUiSystems::ExtractBorders)
                        .before(RenderUiSystems::ExtractTextBackgrounds),
                    text_clips
                        .after(RenderUiSystems::ExtractCursor)
                        .before(RenderUiSystems::ExtractDebug),
                ),
            );
        }
    }
}

// Install ownership only when the canvas clip pass is present. CanvasView can
// also be used by small hosts that rely entirely on Bevy's ordinary clip pass.
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
    bounds: Option<Rect>,
    parent: Option<Entity>,
}

#[derive(Default)]
struct ClipPreparation {
    parents: EntityHashMap<ParentClip>,
    pending: Vec<(Entity, Option<Rect>)>,
}

fn bounds(rect: Rect, transform: &Affine2) -> Rect {
    let scale = Vec2::new(
        transform.matrix2.x_axis.length(),
        transform.matrix2.y_axis.length(),
    );
    Rect {
        min: rect.min * scale + transform.translation,
        max: rect.max * scale + transform.translation,
    }
}

fn canvas_clips(
    roots: Query<Entity, With<CanvasView>>,
    nodes: Query<(&Node, &ComputedNode, &UiGlobalTransform, Has<OverrideClip>)>,
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
        // An enclosing canvas visits nested canvas roots with their current
        // incoming clip. Starting them independently would repeat the subtree
        // and could queue conflicting component insertions/removals before
        // commands are applied. Stop at a gap or another owner's boundary;
        // neither is traversed by an enclosing native canvas.
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
            .map(|boundary| boundary.inherited_clip)
            .unwrap_or_else(|_| clips.get(root).ok().map(|clip| clip.clip));
        pending.push((root, initial));
        while let Some((entity, mut clip)) = pending.pop() {
            let Ok((node, computed, transform, override_clip)) = nodes.get(entity) else {
                continue;
            };
            visited += 1;
            if entity != root
                && let Ok(mut boundary) = boundaries.get_mut(entity)
            {
                if boundary.inherited_clip != clip {
                    boundary.inherited_clip = clip;
                }
                // Another owner may delegate a nested subtree. Native canvas
                // roots use this same pass; leave other owners their input.
                if !roots.contains(entity) {
                    continue;
                }
            }
            if override_clip {
                clip = None;
            }
            if node.display == Display::None {
                clip = Some(Rect::default());
            }
            let mut container = layouts.get(entity).ok().and_then(|layout| layout.parent);
            for _ in 0..crate::native::layout::MAX_DEPTH {
                let Some(parent) = container else { break };
                ancestor_visits += 1;
                let prepared = parents.entry(parent).or_insert_with(|| ParentClip {
                    bounds: nodes
                        .get(parent)
                        .ok()
                        .map(|(node, computed, transform, _)| {
                            if node.display == Display::None {
                                Rect::default()
                            } else {
                                bounds(computed.content_box(), &transform.affine())
                            }
                        }),
                    parent: layouts.get(parent).ok().and_then(|layout| layout.parent),
                });
                if let Some(own) = prepared.bounds {
                    clip = Some(clip.map_or(own, |inherited| inherited.intersect(own)));
                }
                container = prepared.parent;
            }
            match (clips.get_mut(entity), clip) {
                (Ok(mut current), Some(clip)) => {
                    if current.clip != clip {
                        current.clip = clip;
                    }
                }
                (Err(_), Some(clip)) => {
                    commands.entity(entity).insert(CalculatedClip { clip });
                }
                (Ok(_), None) => {
                    commands.entity(entity).remove::<CalculatedClip>();
                }
                (Err(_), None) => {}
            }
            if !node.overflow.is_visible() {
                let own = bounds(
                    computed.resolve_clip_rect(node.overflow, node.overflow_clip_margin),
                    &transform.affine(),
                );
                clip = Some(clip.map_or(own, |parent| parent.intersect(own)));
            }
            pending.extend(children.iter_ui_children(entity).map(|child| (child, clip)));
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

fn text_start(nodes: Res<ExtractedUiNodes>, mut start: ResMut<TextStart>) {
    start.0 = nodes.uinodes.len();
}

fn text_clips(
    mut extracted: ResMut<ExtractedUiNodes>,
    start: Res<TextStart>,
    texts: Extract<
        Query<
            (&ComputedNode, &UiGlobalTransform, Option<&CalculatedClip>),
            Or<(With<TextScroll>, With<crate::native::sand_text::SandText>)>,
        >,
    >,
    mut prepared: Local<EntityHashMap<Rect>>,
) {
    let _clipping = engine::performance::Span::new("native-text-clipping");
    prepared.clear();
    let mut visited = 0;
    for node in extracted.uinodes.iter_mut().skip(start.0) {
        visited += 1;
        let clip = if let Some(clip) = prepared.get(&*node.main_entity) {
            *clip
        } else {
            let Ok((computed, transform, inherited)) = texts.get(*node.main_entity) else {
                continue;
            };
            let clip = bounds(computed.content_box(), &transform.affine());
            let clip = inherited.map_or(clip, |parent| clip.intersect(parent.clip));
            prepared.insert(*node.main_entity, clip);
            clip
        };
        node.clip = Some(clip);
    }
    engine::performance::add("native-text-clip-nodes", visited);
    engine::performance::add("native-text-clips-prepared", prepared.len() as u64);
    if prepared.capacity() > (prepared.len().saturating_mul(4)).max(256) {
        prepared.shrink_to_fit();
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
            let clip = bounds(content, &transform);
            assert_eq!(clip.min, transform.transform_point2(content.min));
            assert_eq!(clip.max, transform.transform_point2(content.max));
            let open = bounds(
                Rect {
                    min: Vec2::new(f32::NEG_INFINITY, -40.0),
                    max: Vec2::new(f32::INFINITY, 40.0),
                },
                &transform,
            );
            assert_eq!(open.min.x, f32::NEG_INFINITY);
            assert_eq!(open.max.x, f32::INFINITY);
            assert!(open.min.y.is_finite() && open.max.y.is_finite());
        }
    }

    crate::native::laboratory_cases! {
        scaled_editor_clip_contains_its_full_content_and_keeps_open_axes,
    }
}
