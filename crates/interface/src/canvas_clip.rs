use crate::canvas::CanvasView;
use bevy::{
    math::Affine2,
    prelude::*,
    render::{Extract, ExtractSchedule, RenderApp},
    ui::{OverrideClip, UiSystems, widget::TextScroll},
    ui_render::{ExtractedUiNodes, RenderUiSystems},
};

pub struct CanvasClipPlugin;

#[derive(Resource, Default)]
struct TextStart(usize);

impl Plugin for CanvasClipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, canvas_clips.after(UiSystems::PostLayout));
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
    children: Query<&Children>,
    mut clips: Query<&mut CalculatedClip>,
    mut commands: Commands,
) {
    for root in &roots {
        let initial = clips.get(root).ok().map(|clip| clip.clip);
        let mut pending = vec![(root, initial)];
        while let Some((entity, mut clip)) = pending.pop() {
            let Ok((node, computed, transform, override_clip)) = nodes.get(entity) else {
                continue;
            };
            if override_clip {
                clip = None;
            }
            if node.display == Display::None {
                clip = Some(Rect::default());
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
            if let Ok(children) = children.get(entity) {
                pending.extend(children.iter().map(|child| (child, clip)));
            }
        }
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
            Or<(With<TextScroll>, With<crate::sand_text::SandText>)>,
        >,
    >,
) {
    for node in extracted.uinodes.iter_mut().skip(start.0) {
        let Ok((computed, transform, inherited)) = texts.get(*node.main_entity) else {
            continue;
        };
        let clip = bounds(computed.content_box(), &transform.affine());
        node.clip = Some(inherited.map_or(clip, |parent| clip.intersect(parent.clip)));
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

    crate::laboratory_cases! {
        scaled_editor_clip_contains_its_full_content_and_keeps_open_axes,
    }
}
