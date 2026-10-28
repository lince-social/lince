use super::*;
use crate::native::{
    canvas::{CanvasItem, CanvasView},
    workspace::WorkspaceMember,
};

#[derive(Clone)]
pub(crate) struct Preview {
    pub id: Entity,
    pub root: Entity,
    pub workspace: u64,
    pub view: CanvasView,
    pub pixels: UVec2,
    pub image: Handle<Image>,
}

#[derive(Resource, Default)]
pub(crate) struct Previews(pub Vec<Preview>);

pub(crate) const LIMIT: usize = crate::native::render_layers::OVERVIEW.len;

pub(crate) fn bounds(
    world: &World,
    root: Entity,
    workspace: u64,
    entity: Entity,
    item: &CanvasItem,
) -> Option<(DVec2, DVec2)> {
    if !item.position.is_finite()
        || !item.size.is_finite()
        || item.size.min_element() <= 0.0
        || world
            .get::<bevy::ecs::entity_disabling::Disabled>(root)
            .is_some()
        || world
            .get::<bevy::ecs::entity_disabling::Disabled>(entity)
            .is_some()
        || world
            .get::<crate::native::layout::desktop::Suppressed>(entity)
            .is_some()
        || world
            .get::<crate::native::protein_area::placement::Pending>(entity)
            .is_some()
    {
        return None;
    }
    let (pieces, _, _) = super::holes::surface_pieces_in(world, root, workspace, entity, item);
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    for point in pieces.into_iter().flatten() {
        min = min.min(point);
        max = max.max(point);
    }
    (min.is_finite() && max.is_finite() && (max - min).min_element() > 0.0).then_some((min, max))
}

pub(crate) fn fit(world: &World, root: Entity, workspace: u64, pixels: Vec2) -> CanvasView {
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    if let Some(children) = world.get::<Children>(root) {
        for entity in children.iter() {
            if world
                .get::<WorkspaceMember>(entity)
                .is_none_or(|member| member.0 != workspace)
                || crate::native::inspection::excluded(world, entity)
                || world
                    .get::<crate::native::area::InfluenceArea>(entity)
                    .is_some()
                || world
                    .get::<crate::native::sand_placement::Pinned>(entity)
                    .is_some()
                || world
                    .get::<crate::native::layout::desktop::Suppressed>(entity)
                    .is_some()
            {
                continue;
            }
            let Some(item) = world.get::<CanvasItem>(entity) else {
                continue;
            };
            if !item.position.is_finite()
                || !item.size.is_finite()
                || item.size.min_element() <= 0.0
            {
                continue;
            }
            let scale = f64::from(
                world
                    .get::<crate::native::area_effects::AreaScale>(entity)
                    .map_or(1.0, |scale| scale.0),
            );
            let placement = spatial(world, entity);
            for x in [-0.5, 0.5] {
                for z in [-0.5, 0.5] {
                    for y in [0.0, -placement.depth(item.size)] {
                        let point = placement.position(item.position)
                            + placement.rotation()
                                * DVec3::new(
                                    f64::from(item.size.x) * x,
                                    y,
                                    f64::from(item.size.y) * z,
                                )
                                * scale;
                        let point = DVec2::new(point.x, point.z);
                        min = min.min(point);
                        max = max.max(point);
                    }
                }
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return crate::native::output_views::canvas(world, root, workspace);
    }
    let size = (max - min).max(DVec2::ONE);
    CanvasView {
        center: (min + max) * 0.5,
        zoom: ((f64::from(pixels.x) - 24.0).max(1.0) / size.x)
            .min((f64::from(pixels.y) - 24.0).max(1.0) / size.y)
            .clamp(1e-9, 3.0),
    }
}

pub(crate) fn demand(
    world: &World,
    root: Entity,
    entity: Entity,
    item: &CanvasItem,
) -> Option<f32> {
    let workspace = world.get::<WorkspaceMember>(entity)?.0;
    let scale = world
        .get::<crate::native::area_effects::AreaScale>(entity)
        .map_or(1.0, |scale| scale.0);
    world
        .get_resource::<Previews>()?
        .0
        .iter()
        .filter(|preview| {
            preview.root == root && super::holes::live_in(world, root, preview.workspace, workspace)
        })
        .filter_map(|preview| {
            let (min, max) = bounds(world, root, preview.workspace, entity, item)?;
            let min =
                (min - preview.view.center) * preview.view.zoom + preview.pixels.as_dvec2() * 0.5;
            let max =
                (max - preview.view.center) * preview.view.zoom + preview.pixels.as_dvec2() * 0.5;
            (max.x >= 0.0
                && max.y >= 0.0
                && min.x < f64::from(preview.pixels.x)
                && min.y < f64::from(preview.pixels.y))
            .then_some(preview.view.zoom as f32 * scale)
        })
        .max_by(f32::total_cmp)
}
