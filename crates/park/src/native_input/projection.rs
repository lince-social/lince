use super::*;
use bevy::asset::AssetId;
use lince_interface::host::{
    GeometrySnapshot, ItemGeometry, ItemId, OutputId, OutputViewport, Rectangle,
    raster::RasterMapping,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    pub root: Option<Entity>,
    pub output: OutputId,
    pub image: AssetId<Image>,
    pub raster: RasterMapping,
    pub origin: [f64; 2],
    pub viewport: Option<OutputViewport>,
}

impl View {
    fn same_owner(&self, other: &Self) -> bool {
        self.root == other.root && self.output == other.output
            && (self.root.is_some() || self.image == other.image)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Source {
    Final,
    Sand {
        owner: Entity,
        pixels: UVec2,
        size: Vec2,
        workspace: u64,
        item: ItemGeometry,
    },
}

#[derive(Clone, PartialEq)]
struct Context {
    entity: Entity,
    owner: Option<native::accessibility::Context>,
    camera: Entity,
    image: AssetId<Image>,
    view: View,
    local: Option<OutputViewport>,
    workspace: Option<u64>,
    sensitive: bool,
    transform: [f32; 6],
    size: Vec2,
    content_origin: Vec2,
    clip: Option<Rect>,
    caret: [i32; 4],
    source: Source,
}

impl Context {
    fn same_owner(&self, other: &Self) -> bool {
        self.entity == other.entity
            && self.sensitive == other.sensitive
            && self.view.same_owner(&other.view)
            && match (&self.owner, &other.owner) {
                (Some(a), Some(b)) => a.same_owner(b),
                (None, None) => self.camera == other.camera && self.image == other.image
                    && match (&self.source, &other.source) {
                        (Source::Final, Source::Final) => true,
                        (Source::Sand { owner: a, .. }, Source::Sand { owner: b, .. }) => a == b,
                        _ => false,
                    },
                _ => false,
            }
    }

    fn same_session(&self, other: &Self) -> bool {
        if self.owner.is_some() && other.owner.is_some() { self.same_owner(other) } else { self == other }
    }
}

#[derive(Resource, Default)]
pub(super) struct CaptureState {
    view: Option<View>,
    generation: u64,
    context: Option<Context>,
}

pub(crate) fn prepare(world: &mut World, view: Option<View>) {
    let changed = match (&world.resource::<CaptureState>().view, &view) {
        (Some(old), Some(new)) if old.root.is_some() => !old.same_owner(new),
        (old, new) => old != new,
    };
    world.resource_mut::<CaptureState>().view = view;
    if changed {
        let mut state = world.resource_mut::<CaptureState>();
        state.context = None;
        state.generation = state.generation.checked_add(1).expect("native editor session");
        super::cancel(world);
    }
}

fn local_view(world: &World, view: &View) -> Option<OutputViewport> {
    let root = view.root?;
    if let Some(views) = world
        .get_resource::<native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
    {
        return (views.focused == view.output).then(|| views.focused().clone());
    }
    world
        .get::<native::layout::desktop::HostViewport>(root)
        .filter(|viewport| viewport.0.id == view.output)
        .map(|viewport| viewport.0.clone())
}

fn descendant(world: &World, mut entity: Entity, owner: Entity) -> bool {
    for _ in 0..native::layout::MAX_DEPTH {
        if entity == owner {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

fn translated(mut item: ItemGeometry, origin: [f64; 2]) -> Option<ItemGeometry> {
    item.mapping = item.mapping.translated(origin)?;
    if let Some(polygons) = &mut item.clip {
        for point in polygons.iter_mut().flatten() {
            for axis in 0..2 {
                point[axis] += origin[axis];
            }
        }
    }
    Some(item)
}

fn resolve(
    world: &World,
    view: View,
    entity: Entity,
    item: Option<ItemGeometry>,
) -> Option<Context> {
    if !super::allowed(world, entity) || native::shortcuts::keyboard_capturing(world) {
        return None;
    }
    let owner = if let Some(root) = view.root {
        let owner = native::accessibility::owner_context(world, entity, true)?;
        if owner.root != root || owner.output() != Some(view.output) || local_view(world, &view).is_none() { return None; }
        Some(owner)
    } else { None };
    let camera = world
        .get::<bevy::ui::ComputedUiTargetCamera>(entity)?
        .get()?;
    if world
        .get::<Camera>(camera)
        .is_some_and(|camera| !camera.is_active)
    {
        return None;
    }
    let bevy::camera::RenderTarget::Image(target) =
        world.get::<bevy::camera::RenderTarget>(camera)?
    else {
        return None;
    };
    let image = target.handle.id();
    let node = world.get::<ComputedNode>(entity)?;
    if node.size().min_element() <= 0.0 {
        return None;
    }
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let (area, content_origin) = super::caret_area(world, entity)?;
    let clip = world
        .get::<bevy::ui::CalculatedClip>(entity)
        .map(|clip| clip.clip);
    let (source, caret) = if image == view.image {
        (
            Source::Final,
            super::caret_rectangle(transform, area, content_origin, view.raster)?,
        )
    } else {
        let owner = world
            .get::<native::topology::presentation::VisualOwner>(camera)?
            .0;
        let surface = world.get::<native::topology::presentation::Surface>(owner)?;
        let id = world.get::<native::layout::LayoutBox>(owner)?.id;
        let item =
            item.filter(|item| item.id == ItemId::Native(id) && item.output == view.output)?;
        if surface.camera != camera
            || surface.image.id() != image
            || !surface.visible
            || surface.pixels.min_element() == 0
            || surface.size.min_element() <= 0.0
            || !descendant(world, entity, owner)
            || view.root != world.get::<ChildOf>(owner).map(ChildOf::parent)
            || world.get::<Disabled>(owner).is_some()
            || world
                .get::<native::layout::desktop::Suppressed>(owner)
                .is_some()
            || world
                .get::<native::layout::desktop::ApplicationWindow>(owner)
                .is_some()
        {
            return None;
        }
        let corners = super::caret_corners(transform, area, content_origin)?;
        let bounds = Rect::from_corners(Vec2::ZERO, surface.pixels.as_vec2());
        let bounds = clip.map_or(bounds, |clip| Rect {
            min: bounds.min.max(clip.min),
            max: bounds.max.min(clip.max),
        });
        if bounds.size().min_element() <= 0.0 {
            return None;
        }
        let half = node.size() * 0.5;
        let field = [
            -half,
            Vec2::new(half.x, -half.y),
            half,
            Vec2::new(-half.x, half.y),
        ]
        .map(|point| transform.transform_point2(point));
        let field_low = field
            .into_iter()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min)
            .max(bounds.min);
        let field_high = field
            .into_iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max)
            .min(bounds.max);
        if (field_high - field_low).min_element() <= 0.0 {
            return None;
        }
        let caret = projected(
            corners,
            surface.pixels,
            &item,
            bounds,
            view.viewport.as_ref()?.usable?,
        )?;
        (
            Source::Sand {
                owner,
                pixels: surface.pixels,
                size: surface.size,
                workspace: world.get::<native::workspace::WorkspaceMember>(owner)?.0,
                item,
            },
            caret,
        )
    };
    Some(Context {
        entity,
        owner,
        sensitive: native::protected_text_input(world, entity),
        camera,
        image,
        local: local_view(world, &view),
        workspace: view
            .root
            .and_then(|root| world.get::<native::workspace::Workspaces>(root))
            .map(|spaces| spaces.active),
        view,
        transform: transform.to_cols_array(),
        size: node.size(),
        content_origin,
        clip,
        caret,
        source,
    })
}

fn projected(
    corners: [Vec2; 4],
    pixels: UVec2,
    item: &ItemGeometry,
    source_clip: Rect,
    output: Rectangle,
) -> Option<[i32; 4]> {
    if pixels.min_element() == 0 || corners.iter().any(|point| !point.is_finite()) {
        return None;
    }
    if source_clip.size().min_element() <= 0.0 {
        return None;
    }
    let corners: Option<Vec<_>> = corners
        .into_iter()
        .map(|point| {
            let point = point.clamp(source_clip.min, source_clip.max);
            let source = std::array::from_fn(|axis| {
                item.mapping.surface.origin()[axis]
                    + f64::from(point[axis]) / f64::from(pixels[axis])
                        * item.mapping.surface.size()[axis]
            });
            item.mapping.canvas_point(source)
        })
        .collect();
    let corners = corners?;
    let mut low: [f64; 2] = std::array::from_fn(|axis| {
        corners
            .iter()
            .map(|point| point[axis])
            .fold(f64::INFINITY, f64::min)
    });
    let mut high: [f64; 2] = std::array::from_fn(|axis| {
        corners
            .iter()
            .map(|point| point[axis])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let center = std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5);
    let visible = corners
        .iter()
        .chain(std::iter::once(&center))
        .any(|point| item.surface_point(*point).is_some());
    let owner_low: [f64; 2] =
        std::array::from_fn(|axis| item.mapping.canvas.origin()[axis].max(output.origin()[axis]));
    let owner_high: [f64; 2] = std::array::from_fn(|axis| {
        (item.mapping.canvas.origin()[axis] + item.mapping.canvas.size()[axis])
            .min(output.origin()[axis] + output.size()[axis])
    });
    if (0..2).any(|axis| owner_high[axis] <= owner_low[axis]) {
        return None;
    }
    let anchor = if visible {
        center
    } else if let Some(polygons) = &item.clip {
        polygons
            .iter()
            .filter(|polygon| polygon.len() >= 3)
            .flat_map(|polygon| {
                polygon
                    .iter()
                    .zip(polygon.iter().cycle().skip(1))
                    .take(polygon.len())
                    .filter_map(|(a, b)| {
                        let direction = [b[0] - a[0], b[1] - a[1]];
                        let length = direction[0] * direction[0] + direction[1] * direction[1];
                        if !length.is_finite() || length <= f64::EPSILON {
                            return None;
                        }
                        let fraction = (((center[0] - a[0]) * direction[0]
                            + (center[1] - a[1]) * direction[1])
                            / length)
                            .clamp(0.0, 1.0);
                        Some([
                            a[0] + fraction * direction[0],
                            a[1] + fraction * direction[1],
                        ])
                    })
            })
            .min_by(|a, b| {
                let distance =
                    |point: &[f64; 2]| (point[0] - center[0]).hypot(point[1] - center[1]);
                distance(a).total_cmp(&distance(b))
            })?
    } else {
        std::array::from_fn(|axis| center[axis].clamp(owner_low[axis], owner_high[axis]))
    };
    let outside = (0..2).any(|axis| high[axis] <= owner_low[axis] || low[axis] >= owner_high[axis]);
    if !visible && !outside {
        low = anchor;
        high = anchor.map(|value| value + 1.0);
    }
    for axis in 0..2 {
        if high[axis] <= owner_low[axis] || low[axis] >= owner_high[axis] {
            low[axis] = anchor[axis].clamp(
                owner_low[axis],
                (owner_high[axis] - 1.0).max(owner_low[axis]),
            );
            high[axis] = (low[axis] + 1.0).min(owner_high[axis]);
        } else {
            low[axis] = low[axis].max(owner_low[axis]);
            high[axis] = high[axis].min(owner_high[axis]);
            if high[axis] <= low[axis] {
                low[axis] = low[axis].clamp(
                    owner_low[axis],
                    (owner_high[axis] - 1.0).max(owner_low[axis]),
                );
                high[axis] = (low[axis] + 1.0).min(owner_high[axis]);
            }
        }
    }
    if (0..2).any(|axis| {
        !low[axis].is_finite()
            || !high[axis].is_finite()
            || high[axis] <= low[axis]
            || low[axis].abs() > 131072.0
            || high[axis].abs() > 131072.0
    }) {
        return None;
    }
    let low = low.map(f64::floor);
    let high = high.map(f64::ceil);
    Some([
        low[0] as i32,
        low[1] as i32,
        (high[0] - low[0]).max(1.0) as i32,
        (high[1] - low[1]).max(1.0) as i32,
    ])
}

pub(crate) fn accessibility_points(world: &World, view: &View, geometry: Option<&GeometrySnapshot>, entity: Entity, local: Option<Rect>) -> Option<[[f64; 2]; 4]> {
    let node = world.get::<ComputedNode>(entity)?;
    if node.size().min_element() <= 0.0 { return None; }
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let bounds = local.unwrap_or_else(|| Rect::from_center_size(Vec2::ZERO, node.size()));
    let mut corners = [bounds.min, Vec2::new(bounds.max.x, bounds.min.y), bounds.max, Vec2::new(bounds.min.x, bounds.max.y)]
        .map(|point| transform.transform_point2(point));
    if corners.iter().any(|point| !point.is_finite()) { return None; }
    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
        let low = corners.iter().copied().fold(Vec2::splat(f32::INFINITY), Vec2::min).max(clip.clip.min);
        let high = corners.iter().copied().fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max).min(clip.clip.max);
        if (high - low).min_element() < 0.0 { return None; }
        corners = [low, Vec2::new(high.x, low.y), high, Vec2::new(low.x, high.y)];
    }
    let camera = world.get::<bevy::ui::ComputedUiTargetCamera>(entity)?.get()?;
    let bevy::camera::RenderTarget::Image(target) = world.get::<bevy::camera::RenderTarget>(camera)? else { return None; };
    let points = if target.handle.id() == view.image {
        corners.map(|point| view.raster.logical_point(point.as_dvec2().to_array()))
    } else {
        let owner = world.get::<native::topology::presentation::VisualOwner>(camera)?.0;
        let surface = world.get::<native::topology::presentation::Surface>(owner)?;
        if surface.camera != camera || surface.image.id() != target.handle.id() || !surface.visible
            || surface.pixels.min_element() == 0 || !descendant(world, entity, owner)
            || view.root != world.get::<ChildOf>(owner).map(ChildOf::parent)
            || world.get::<native::layout::desktop::ApplicationWindow>(owner).is_some()
        { return None; }
        let id = world.get::<native::layout::LayoutBox>(owner)?.id;
        let item = geometry?.items.iter().find(|item| item.id == ItemId::Native(id) && item.output == view.output)?;
        let points = corners.into_iter().map(|point| {
            let source = std::array::from_fn(|axis| item.mapping.surface.origin()[axis]
                + f64::from(point[axis]) / f64::from(surface.pixels[axis]) * item.mapping.surface.size()[axis]);
            item.mapping.canvas_point(source)
        }).collect::<Option<Vec<_>>>()?;
        <[[f64; 2]; 4]>::try_from(points).ok()?
    };
    points.iter().flatten().all(|value| value.is_finite()).then_some(points)
}

pub(crate) fn accessibility_bounds(world: &World, view: &View, geometry: Option<&GeometrySnapshot>, entity: Entity, local: Option<Rect>) -> Option<Rectangle> {
    let points = accessibility_points(world, view, geometry, entity, local)?;
    let low = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
    let high: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
    Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
}

pub(super) fn capture(world: &mut World, geometry: Option<&GeometrySnapshot>) -> Option<Editor> {
    let context = world
        .resource::<CaptureState>()
        .view
        .clone()
        .and_then(|view| {
            let entity = world
                .get_resource::<bevy::input_focus::InputFocus>()?
                .get()?;
            let camera = world
                .get::<bevy::ui::ComputedUiTargetCamera>(entity)?
                .get()?;
            let item = world
                .get::<native::topology::presentation::VisualOwner>(camera)
                .and_then(|owner| world.get::<native::layout::LayoutBox>(owner.0))
                .and_then(|layout| {
                    geometry?.items.iter().find(|item| {
                        item.id == ItemId::Native(layout.id) && item.output == view.output
                    })
                })
                .cloned();
            resolve(world, view, entity, item)
        });
    let cancelled = world
        .resource::<CaptureState>()
        .context
        .as_ref()
        .is_some_and(|old| context.as_ref().is_none_or(|new| !old.same_owner(new)));
    if cancelled {
        super::cancel(world);
    }
    let mut state = world.resource_mut::<CaptureState>();
    let same_session = match (&state.context, &context) {
        (Some(old), Some(new)) => old.same_session(new),
        (None, None) => true,
        _ => false,
    };
    if !same_session {
        state.generation = state.generation.checked_add(1).expect("native editor session");
    }
    state.context = context.clone();
    let generation = state.generation;
    drop(state);
    let context = context?;
    let mut editor = super::text_state(world, context.entity, context.caret)?;
    editor.projection = generation;
    Some(editor)
}

pub(super) fn current(world: &mut World, requested: &Editor) -> Option<Editor> {
    let Some(state) = world.get_resource::<CaptureState>() else {
        let entity = world
            .get_resource::<bevy::input_focus::InputFocus>()?
            .get()?;
        return super::text_state(world, entity, requested.caret);
    };
    if state.generation != requested.projection {
        return None;
    }
    let context = state.context.clone()?;
    if world.get_resource::<bevy::input_focus::InputFocus>()?.get() != Some(context.entity) { return None; }
    let view = state.view.clone()?;
    let local = local_view(world, &view);
    if context.owner.is_some() {
        if !view.same_owner(&context.view) || local.is_none() { return None; }
    } else if view != context.view || local != context.local { return None; }
    let camera = world.get::<bevy::ui::ComputedUiTargetCamera>(context.entity)?.get()?;
    let item = match world.get::<native::topology::presentation::VisualOwner>(camera) {
        Some(owner) => {
            let id = world.get::<native::layout::LayoutBox>(owner.0)?.id;
            let root = view.root?;
            let local = local.as_ref()?;
            let current = native::topology::output_views::geometry(world, root, local)
                .into_iter()
                .find(|candidate| candidate.id == ItemId::Native(id) && candidate.output == view.output)?;
            Some(translated(current, view.origin)?)
        },
        None => None,
    };
    let live = resolve(world, view, context.entity, item)?;
    if !context.same_session(&live) {
        return None;
    }
    let mut editor = super::text_state(world, context.entity, live.caret)?;
    editor.projection = requested.projection;
    Some(editor)
}

#[cfg(test)]
#[path = "projection/lifetime.rs"]
mod lifetime;

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::ContentMapping;

    fn item() -> ItemGeometry {
        ItemGeometry {
            id: ItemId::Native([1; 16]),
            output: OutputId(2),
            floating: true,
            clip: None,
            mapping: ContentMapping {
                canvas: Rectangle::new([640.0, 30.0], [200.0, 100.0]).unwrap(),
                surface: Rectangle::new([0.0; 2], [100.0, 50.0]).unwrap(),
                corners: None,
            },
        }
    }

    #[test]
    fn texture_density_and_output_translation_are_applied_once() {
        let corners = [
            Vec2::new(80.0, 40.0),
            Vec2::new(120.0, 40.0),
            Vec2::new(120.0, 80.0),
            Vec2::new(80.0, 80.0),
        ];
        let pixels = UVec2::new(400, 200);
        let source = Rect::from_corners(Vec2::ZERO, pixels.as_vec2());
        let output = Rectangle::new([640.0, 30.0], [200.0, 100.0]).unwrap();
        assert_eq!(
            projected(corners, pixels, &item(), source, output),
            Some([680, 50, 20, 20])
        );
        assert_eq!(
            projected(
                [
                    Vec2::new(300.0, 40.0),
                    Vec2::new(320.0, 40.0),
                    Vec2::new(320.0, 80.0),
                    Vec2::new(300.0, 80.0)
                ],
                pixels,
                &item(),
                source,
                Rectangle::new([640.0, 30.0], [100.0, 100.0]).unwrap(),
            ),
            Some([739, 50, 1, 20])
        );
        assert!(projected(corners, UVec2::ZERO, &item(), source, output).is_none());
        assert!(projected([Vec2::NAN; 4], pixels, &item(), source, output).is_none());
        assert_eq!(
            projected(
                corners,
                pixels,
                &item(),
                Rect::from_corners(Vec2::new(0.0, 100.0), pixels.as_vec2()),
                output
            ),
            Some([680, 80, 20, 1])
        );
        assert!(
            projected(
                corners,
                pixels,
                &item(),
                source,
                Rectangle::new([0.0; 2], [100.0; 2]).unwrap()
            )
            .is_none()
        );
    }

    #[test]
    fn perspective_bounds_use_every_rotated_caret_corner() {
        let mut item = item();
        item.mapping.canvas = Rectangle::new([640.0, 30.0], [160.0, 120.0]).unwrap();
        item.mapping.surface = Rectangle::new([0.0; 2], [200.0, 100.0]).unwrap();
        item.mapping.corners = Some([
            [640.0, 30.0],
            [940.0 / 1.2, 50.0 / 1.2],
            [800.0, 170.0 / 1.2],
            [660.0, 150.0],
        ]);
        let corners = [
            Vec2::new(40.0, 30.0),
            Vec2::new(60.0, 50.0),
            Vec2::new(40.0, 70.0),
            Vec2::new(20.0, 50.0),
        ];
        assert_eq!(
            projected(
                corners,
                UVec2::new(400, 200),
                &item,
                Rect::from_corners(Vec2::ZERO, Vec2::new(400.0, 200.0)),
                Rectangle::new([640.0, 30.0], [200.0, 180.0]).unwrap()
            ),
            Some([653, 49, 17, 24])
        );
    }
}
