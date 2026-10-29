use super::*;
use crate::host::{ItemGeometry, ItemId};

mod native;
pub(crate) use native::input;
pub(in crate::native) use native::keyboard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub root: Entity,
    pub entity: Entity,
    pub workspace: u64,
    pub output: OutputId,
    pub placement: PlacementId,
    pub item: ItemId,
}

struct Hit {
    target: Target,
    geometry: ItemGeometry,
}

struct Projection {
    target: Target,
    operation: Manipulation,
    start: Vec2,
    point: [f64; 2],
    mapping: ContentMapping,
    offset: [f64; 2],
    size: [f64; 2],
    zoom: f64,
    plane: Option<(f64, bevy::math::DVec3)>,
    basis: Workspace,
    hits: Vec<Hit>,
}

#[derive(Resource, Default)]
struct Drag(Option<Projection>);

fn viewport(world: &World, root: Entity) -> Option<OutputViewport> {
    if let Some(viewport) = world.get::<HostViewport>(root) {
        return Some(viewport.0.clone());
    }
    let output = world
        .get::<Output>(root)
        .map_or(OutputId(root.to_bits()), |output| output.0);
    world
        .get_resource::<PublishedGeometry>()?
        .0
        .as_ref()?
        .outputs
        .iter()
        .find(|viewport| viewport.id == output)
        .cloned()
}

pub fn targets(world: &World, root: Entity) -> Vec<Target> {
    let Some(output) = viewport(world, root) else {
        return Vec::new();
    };
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return Vec::new();
    };
    world
        .get::<Children>(root)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|entity| {
            if !focus::eligible(world, root, spaces.active, entity) {
                return None;
            }
            let id = world.get::<Managed>(entity)?.id;
            Some(Target {
                root,
                entity,
                workspace: spaces.active,
                output: output.id,
                placement: id,
                item: world
                    .get::<ApplicationWindow>(entity)
                    .map_or(ItemId::Native(id.0), |window| {
                        ItemId::Application(window.surface)
                    }),
            })
        })
        .collect()
}

fn hits(world: &mut World, root: Entity) -> Vec<Hit> {
    let Some(output) = viewport(world, root) else {
        return Vec::new();
    };
    let targets = targets(world, root);
    let geometry =
        if world.contains_resource::<crate::native::topology::presentation::SceneCamera>() {
            crate::native::topology::output_views::geometry(world, root, &output)
        } else {
            world
                .get_resource::<PublishedGeometry>()
                .and_then(|published| published.0.as_ref())
                .map(|geometry| {
                    geometry
                        .items
                        .iter()
                        .filter(|item| item.output == output.id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
    geometry
        .into_iter()
        .filter_map(|geometry| {
            let target = targets
                .iter()
                .find(|target| target.item == geometry.id)
                .copied()?;
            Some(Hit { target, geometry })
        })
        .collect()
}

fn blocked(world: &World, root: Entity, window: Entity, point: Vec2, target: Entity) -> bool {
    for hit in crate::native::pointer_hit::nodes(world, window, point) {
        let mut next = Some(hit);
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(entity) = next else {
                break;
            };
            if world
                .get::<crate::native::inspection::InspectionExcluded>(entity)
                .is_some()
            {
                return true;
            }
            if world.get::<Managed>(entity).is_some() {
                return entity != target;
            }
            if world.get::<CanvasItem>(entity).is_some() {
                return true;
            }
            if entity == root {
                return false;
            }
            next = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        if world
            .get::<Pickable>(hit)
            .is_none_or(|pickable| pickable.should_block_lower)
        {
            return true;
        }
    }
    false
}

pub fn press(
    world: &mut World,
    root: Entity,
    window: Entity,
    point: Vec2,
    operation: Manipulation,
    expected: Option<Target>,
) -> bool {
    press_with_edges(world, root, window, point, operation, expected, None)
}

pub fn press_with_edges(
    world: &mut World,
    root: Entity,
    window: Entity,
    point: Vec2,
    operation: Manipulation,
    expected: Option<Target>,
    edges: Option<[i8; 2]>,
) -> bool {
    if super::active(world)
        || !point.is_finite()
        || world
            .get::<Window>(window)
            .is_some_and(|window| !window.focused)
    {
        return false;
    }
    focus::ensure(world, root);
    let Some(output) = viewport(world, root) else {
        return false;
    };
    let offset = output.raster.origin;
    let logical = std::array::from_fn(|axis| f64::from(point[axis]) + offset[axis]);
    let hits = hits(world, root);
    let owner = (expected.is_none()
        && world
            .get::<Window>(window)
            .and_then(Window::cursor_position)
            == Some(point))
    .then(|| {
        world
            .get_resource::<crate::native::topology::input::PointerState>()
            .and_then(|state| state.hit)
            .map(|(owner, _)| owner)
    })
    .flatten();
    let Some(hit) = hits.iter().rev().find(|hit| {
        owner.is_none_or(|owner| owner == hit.target.entity)
            && hit.geometry.surface_point(logical).is_some()
    }) else {
        return false;
    };
    if expected.is_some_and(|expected| expected != hit.target)
        || blocked(world, root, window, point, hit.target.entity)
    {
        return false;
    }
    let mapping = hit.geometry.mapping;
    let surface = mapping.surface_point(logical).unwrap();
    let uv = std::array::from_fn(|axis| {
        (surface[axis] - mapping.surface.origin()[axis]) / mapping.surface.size()[axis]
    });
    let target = hit.target;
    let item = *world.get::<CanvasItem>(target.entity).unwrap();
    let view = *world.get::<CanvasView>(root).unwrap();
    let spatial = crate::native::topology::spatial(world, target.entity);
    let scale = world
        .get::<crate::native::area_effects::AreaScale>(target.entity)
        .map_or(1.0, |scale| f64::from(scale.0));
    let plane = if crate::native::topology::view::spatial(world, root) {
        let hit = spatial.position(item.position)
            + spatial.rotation()
                * bevy::math::DVec3::new(
                    (uv[0] - 0.5) * f64::from(item.size.x) * scale,
                    0.0,
                    (uv[1] - 0.5) * f64::from(item.size.y) * scale,
                );
        let Some(start) = crate::native::topology::input::plane_point(world, root, point, hit.y)
        else {
            return false;
        };
        Some((hit.y, start))
    } else {
        None
    };
    if !super::begin_with_edges(world, root, target.entity, operation, uv, edges) {
        return false;
    }
    let basis = model(world, root, target.workspace).unwrap().clone();
    world.init_resource::<Drag>();
    world.resource_mut::<Drag>().0 = Some(Projection {
        target,
        operation,
        start: point,
        point: surface,
        mapping,
        offset,
        size: item.size.as_dvec2().to_array(),
        zoom: view.zoom,
        plane,
        basis,
        hits,
    });
    true
}

pub fn motion(world: &mut World, point: Vec2) -> bool {
    let Some(mut projection) = world
        .get_resource_mut::<Drag>()
        .and_then(|mut drag| drag.0.take())
    else {
        return false;
    };
    if !super::active(world) || !point.is_finite() {
        super::cancel(world);
        return false;
    }
    let logical = std::array::from_fn(|axis| f64::from(point[axis]) + projection.offset[axis]);
    let tiled = world
        .get_resource::<State>()
        .and_then(|state| state.0.as_ref())
        .is_some_and(|session| session.gesture.tiled());
    let delta = if tiled && projection.operation == Manipulation::Move {
        (point - projection.start).as_dvec2().to_array()
    } else if projection.operation == Manipulation::Resize {
        let Some(current) = projection.mapping.surface_point_unbounded(logical) else {
            super::cancel(world);
            return false;
        };
        std::array::from_fn(|axis| {
            (current[axis] - projection.point[axis]) / projection.mapping.surface.size()[axis]
                * projection.size[axis]
        })
    } else if let Some((height, start)) = projection.plane {
        let Some(current) = crate::native::topology::input::plane_point(
            world,
            projection.target.root,
            point,
            height,
        ) else {
            super::cancel(world);
            return false;
        };
        [current.x - start.x, current.z - start.z]
    } else {
        ((point - projection.start).as_dvec2() / projection.zoom).to_array()
    };
    let mut slot = None;
    if tiled && projection.operation == Manipulation::Move {
        if model(world, projection.target.root, projection.target.workspace)
            != Some(&projection.basis)
        {
            projection.hits = hits(world, projection.target.root);
            if let Some(model) = model(world, projection.target.root, projection.target.workspace) {
                projection.basis = model.clone();
            }
        }
        slot = projection
            .hits
            .iter()
            .rev()
            .filter(|hit| hit.target != projection.target)
            .find_map(|hit| {
                let point = hit.geometry.surface_point(logical)?;
                let uv: [f64; 2] = std::array::from_fn(|axis| {
                    (point[axis] - hit.geometry.mapping.surface.origin()[axis])
                        / hit.geometry.mapping.surface.size()[axis]
                });
                let side = if uv[1] < 0.2 {
                    crate::host::Direction::Up
                } else if uv[1] > 0.8 {
                    crate::host::Direction::Down
                } else if uv[0] < 0.5 {
                    crate::host::Direction::Left
                } else {
                    crate::host::Direction::Right
                };
                Some(DropSlot {
                    anchor: hit.target.placement,
                    side,
                })
            });
    }
    if !super::propose(world, delta, slot) {
        return false;
    }
    world.resource_mut::<Drag>().0 = Some(projection);
    true
}

pub fn release(world: &mut World, point: Vec2) -> bool {
    if !motion(world, point) {
        return false;
    }
    if let Some(mut drag) = world.get_resource_mut::<Drag>() {
        drag.0 = None;
    }
    super::commit(world)
}

pub fn cancel(world: &mut World) {
    if let Some(mut drag) = world.get_resource_mut::<Drag>() {
        drag.0 = None;
    }
    super::cancel(world);
}
