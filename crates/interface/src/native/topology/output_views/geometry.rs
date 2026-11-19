use super::{CanvasItem, OutputViews, Surface, WorkspaceMember};
use crate::{
    host::{ContentMapping, ItemGeometry, ItemId, OutputViewport, Rectangle},
    native::topology::{self, holes},
};
use bevy::{ecs::query::QueryState, prelude::*};
use std::cmp::Ordering;

#[derive(Resource)]
struct Members(
    QueryState<(
        Entity,
        &'static CanvasItem,
        &'static ChildOf,
        &'static WorkspaceMember,
        &'static crate::native::layout::LayoutBox,
        &'static Surface,
    )>,
);

impl FromWorld for Members {
    fn from_world(world: &mut World) -> Self {
        nucleus::performance::add("native-output-geometry-query-plans", 1);
        Self(world.query())
    }
}

type Ranked = (usize, f64, topology::stacking::Order, ItemGeometry);

fn compare(a: &Ranked, b: &Ranked) -> Ordering {
    let (a_depth, a_height, a_order, a) = a;
    let (b_depth, b_height, b_order, b) = b;
    b_depth
        .cmp(a_depth)
        .then_with(|| a_height.total_cmp(b_height))
        .then_with(|| a.floating.cmp(&b.floating))
        .then_with(|| a_order.cmp(b_order))
        .then_with(|| a.id.cmp(&b.id))
}

pub fn geometry(world: &mut World, root: Entity, output: &OutputViewport) -> Vec<ItemGeometry> {
    matching_geometry(world, root, output, |_| true)
}

pub(crate) fn matching_geometry(
    world: &mut World,
    root: Entity,
    output: &OutputViewport,
    include: impl Fn(ItemId) -> bool,
) -> Vec<ItemGeometry> {
    let _preparation = nucleus::performance::Span::new("native-output-geometry");
    let mut result = Vec::new();
    visit(world, root, output, include, |item| result.push(item));
    result.sort_by(compare);
    result.into_iter().map(|(_, _, _, item)| item).collect()
}

pub fn item_geometry(
    world: &mut World,
    root: Entity,
    output: &OutputViewport,
    id: ItemId,
) -> Option<ItemGeometry> {
    let _preparation = nucleus::performance::Span::new("native-output-item-geometry");
    let mut first: Option<Ranked> = None;
    visit(
        world,
        root,
        output,
        |candidate| candidate == id,
        |candidate| {
            if first
                .as_ref()
                .is_none_or(|old| compare(&candidate, old).is_lt())
            {
                first = Some(candidate);
            }
        },
    );
    first.map(|(_, _, _, item)| item)
}

fn visit(
    world: &mut World,
    root: Entity,
    output: &OutputViewport,
    include: impl Fn(ItemId) -> bool,
    mut publish: impl FnMut(Ranked),
) {
    world.init_resource::<Members>();
    world.resource_scope(|world, mut members: Mut<Members>| {
        let canvas = crate::native::output_views::canvas(world, root, output.workspace);
        let Some(children) = world.get::<Children>(root) else {
            return;
        };
        let layers = holes::layers_for(world, root, output.workspace);
        let focused = world
            .get_resource::<OutputViews>()
            .is_none_or(|views| views.focused == output.id);
        let spatial = focused && topology::view::spatial(world, root);
        let extent = bevy::math::DVec2::from_array(output.bounds.size());
        let origin = bevy::math::DVec2::from_array(output.bounds.origin());
        let viewport = [
            origin,
            origin + bevy::math::DVec2::new(extent.x, 0.0),
            origin + extent,
            origin + bevy::math::DVec2::new(0.0, extent.y),
        ];
        let screen = |point: bevy::math::DVec2| {
            (point - canvas.center) * canvas.zoom + extent * 0.5 + origin
        };
        let mut candidates = 0_u64;
        let mut projections = 0_u64;
        let mut ordering = topology::stacking::Keys::new(world);
        for (entity, item, parent, member, layout, surface) in
            members.0.iter_many(world, children.iter()).matched()
        {
            candidates += 1;
            if parent.parent() != root
                || !surface.visible
                || !topology::presentation::displayed(world, entity)
                || member.0 != output.workspace
                    && !layers.is_some_and(|layers| {
                        layers.0.iter().any(|layer| layer.workspace == member.0)
                    })
            {
                continue;
            }
            let application =
                world.get::<crate::native::layout::desktop::ApplicationWindow>(entity);
            let identity = application.map_or(ItemId::Native(layout.id), |window| {
                ItemId::Application(window.surface)
            });
            if !include(identity) {
                continue;
            }
            projections += 1;
            let item = world
                .get::<crate::native::canvas::ItemPreview>(entity)
                .map_or(*item, |preview| preview.0);
            let workspace = member.0;
            let id = layout.id;
            let scale = world
                .get::<crate::native::area_effects::AreaScale>(entity)
                .map_or(1.0, |scale| f64::from(scale.0));
            let size = item.size.as_dvec2() * scale;
            let Some(mut rectangle) = Rectangle::new(
                screen(item.position - size * 0.5).to_array(),
                (size * canvas.zoom).to_array(),
            ) else {
                continue;
            };
            let (id, surface) = if let Some(application) = application {
                (
                    ItemId::Application(application.surface),
                    Rectangle::new([0.0; 2], application.content_size.map(f64::from)),
                )
            } else {
                (
                    ItemId::Native(id),
                    Rectangle::new([0.0; 2], item.size.as_dvec2().to_array()),
                )
            };
            let Some(surface) = surface else {
                continue;
            };
            let placement = crate::native::canvas::presented_spatial(world, entity);
            let projected =
                spatial || focused && placement.rotation() != bevy::math::DQuat::IDENTITY;
            let corners = if projected {
                let Some(mut corners) = topology::presentation::projected_corners(world, entity)
                else {
                    continue;
                };
                for point in &mut corners {
                    for axis in 0..2 {
                        point[axis] += output.raster.origin[axis];
                    }
                }
                let low = std::array::from_fn(|axis| {
                    corners
                        .iter()
                        .map(|point| point[axis])
                        .fold(f64::INFINITY, f64::min)
                });
                let high: [f64; 2] = std::array::from_fn(|axis| {
                    corners
                        .iter()
                        .map(|point| point[axis])
                        .fold(f64::NEG_INFINITY, f64::max)
                });
                let Some(bounds) =
                    Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
                else {
                    continue;
                };
                rectangle = bounds;
                Some(corners)
            } else {
                None
            };
            let mapping = ContentMapping {
                canvas: rectangle,
                surface,
                corners,
            };
            let (polygons, center, basis) =
                holes::surface_pieces_in(world, root, output.workspace, entity, &item);
            if projected && basis.determinant().abs() < 1e-10 {
                continue;
            }
            let inverse = if projected {
                basis.inverse()
            } else {
                bevy::math::DMat2::IDENTITY
            };
            let screen = |point: bevy::math::DVec2| {
                if projected {
                    let uv = inverse * (point - center) / item.size.as_dvec2()
                        + bevy::math::DVec2::splat(0.5);
                    mapping
                        .canvas_point(std::array::from_fn(|axis| {
                            surface.origin()[axis] + uv[axis] * surface.size()[axis]
                        }))
                        .map(bevy::math::DVec2::from_array)
                } else {
                    Some(screen(point))
                }
            };
            let clip: Vec<_> = polygons
                .into_iter()
                .map(|polygon| {
                    let polygon: Vec<_> = polygon.into_iter().filter_map(screen).collect();
                    let winding: f64 = polygon
                        .iter()
                        .zip(polygon.iter().cycle().skip(1))
                        .take(polygon.len())
                        .map(|(a, b)| a.x * b.y - b.x * a.y)
                        .sum();
                    let mut polygon = polygon;
                    if winding < 0.0 {
                        polygon.reverse();
                    }
                    holes::intersect(&polygon, &viewport)
                })
                .filter(|polygon| polygon.len() >= 3)
                .map(|polygon| polygon.into_iter().map(|point| point.to_array()).collect())
                .collect();
            if clip.is_empty() {
                continue;
            }
            let depth = layers
                .and_then(|layers| layers.0.iter().find(|layer| layer.workspace == workspace))
                .map_or(0, |layer| layer.depth);
            publish((
                depth,
                placement.elevation,
                ordering.key(entity),
                ItemGeometry {
                    id,
                    output: output.id,
                    mapping,
                    floating: world
                        .get::<crate::native::layout::desktop::Managed>(entity)
                        .is_none_or(|managed| managed.floating),
                    clip: Some(clip),
                },
            ));
        }
        nucleus::performance::add("native-output-geometry-candidates", candidates);
        nucleus::performance::add("native-output-geometry-projections", projections);
    });
}
