use super::{native, projection::View};
use bevy::{a11y::AccessibilityNode, asset::AssetId, ecs::change_detection::DetectChanges, prelude::*};
use bevy::{camera::{ImageRenderTarget, NormalizedRenderTarget}, picking::pointer::Location};
use lince_interface::host::{GeometrySnapshot, ItemGeometry, ItemId, OutputId, Rectangle, raster::RasterMapping};
use std::collections::{HashMap, HashSet};

const MAX_TARGETS: usize = 16_384;

#[derive(Clone, Debug, PartialEq)]
enum Projection {
    Final(RasterMapping),
    Sand { owner: Entity, pixels: UVec2, item: ItemGeometry },
}

#[derive(Clone, PartialEq)]
struct Geometry {
    entity: Entity,
    output: OutputId,
    bounds: Option<Rectangle>,
    image: AssetId<Image>,
    lineage: Vec<Entity>,
    actions: Vec<(Entity, Entity, u32)>,
    owner: Option<native::accessibility::Context>,
    modal: Option<Entity>,
    eligible: bool,
    blocks: bool,
    node: ComputedNode,
    transform: UiGlobalTransform,
    clip: Option<Rect>,
    projection: Projection,
}

impl Geometry {
    fn same_owner(&self, other: &Self) -> bool {
        self.entity == other.entity && self.output == other.output && self.lineage == other.lineage
            && self.actions == other.actions && self.modal == other.modal && self.eligible && other.eligible
            && match (&self.owner, &other.owner) {
                (Some(a), Some(b)) => a.same_owner(b),
                (None, None) => self.image == other.image,
                _ => false,
            }
    }

    fn point(&self, point: [f64; 2]) -> Option<Vec2> {
        if self.bounds.is_some_and(|bounds| !bounds.contains(point)) { return None; }
        let point = match &self.projection {
            Projection::Final(raster) => Vec2::from_array(raster.point(point).map(|value| value as f32)),
            Projection::Sand { pixels, item, .. } => {
                let point = item.surface_point(point)?;
                Vec2::from_array(std::array::from_fn(|axis| ((point[axis] - item.mapping.surface.origin()[axis])
                    / item.mapping.surface.size()[axis] * f64::from(pixels[axis])) as f32))
            },
        };
        point.is_finite().then_some(point)
    }

    fn hit(&self, point: [f64; 2]) -> bool {
        self.point(point).is_some_and(|point| self.node.contains_point(self.transform, point)
            && self.clip.is_none_or(|clip| clip.contains(point)))
    }
}

#[derive(Clone, PartialEq)]
struct Background {
    root: Entity,
    output: OutputId,
    image: AssetId<Image>,
    bounds: Rectangle,
    owner: native::accessibility::Context,
}

#[derive(Clone, PartialEq)]
pub struct Target {
    point: [f64; 2],
    geometry: Vec<Geometry>,
    background: Option<Background>,
}

impl Target {
    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.geometry.iter().any(|geometry| geometry.lineage.contains(&entity))
    }

    pub(crate) fn output(&self) -> Option<OutputId> {
        self.geometry.first().map(|geometry| geometry.output).or_else(|| self.background.as_ref().map(|background| background.output))
    }

    #[cfg(feature = "session")]
    pub(crate) fn owner_current(&self, world: &World) -> bool {
        if let Some(background) = &self.background {
            return native::accessibility::owner_context(world, background.root, true)
                .is_some_and(|owner| background.owner.same_owner(&owner));
        }
        !self.geometry.is_empty() && self.geometry.iter().all(|geometry| geometry.eligible
            && geometry.owner.as_ref().is_some_and(|owner| native::accessibility::owner_context(world, geometry.entity, true)
                .is_some_and(|current| owner.same_owner(&current))))
    }
}

#[derive(Resource, Default)]
pub(crate) struct Held {
    targets: HashMap<bevy::input::mouse::MouseButton, Target>,
    cancelled: HashSet<bevy::input::mouse::MouseButton>,
    overflowed: bool,
}

impl Held {
    pub(crate) fn press(&mut self, button: bevy::input::mouse::MouseButton, target: Target) -> bool {
        if !self.cancelled.is_empty() || self.targets.contains_key(&button) || self.targets.len() >= 32 { return false; }
        self.targets.insert(button, target);
        true
    }

    pub(crate) fn release(&mut self, button: bevy::input::mouse::MouseButton) -> Result<Option<Target>, ()> {
        if self.cancelled.remove(&button) { Err(()) } else { Ok(self.targets.remove(&button)) }
    }

    pub(crate) fn cancel(&mut self) { self.cancelled.extend(self.targets.drain().map(|(button, _)| button)); }
}

#[derive(Default)]
pub(crate) struct Snapshot {
    targets: Vec<Geometry>,
    backgrounds: Vec<Background>,
    overflowed: bool,
}

impl Snapshot {
    pub(crate) fn editor_point(&self, entity: Entity, point: [f64; 2]) -> Option<Vec2> {
        let target = self.target(point)?;
        let hit = target.geometry.first()?;
        if !hit.lineage.contains(&entity) { return None; }
        let editor = self.targets.iter().find(|candidate| candidate.entity == entity
            && candidate.output == hit.output && candidate.image == hit.image && candidate.eligible)?;
        editor.hit(point).then(|| editor.point(point)).flatten()
    }

    pub(crate) fn target(&self, point: [f64; 2]) -> Option<Target> {
        if self.overflowed || point.iter().any(|value| !value.is_finite()) { return None; }
        let mut hits = Vec::new();
        for geometry in self.targets.iter().filter(|geometry| geometry.hit(point)) {
            if !geometry.eligible { return None; }
            hits.push(geometry.clone());
            if geometry.blocks { break; }
        }
        if !hits.is_empty() { return Some(Target { point, geometry: hits, background: None }); }
        let background = self.backgrounds.iter().find(|background| background.bounds.contains(point))?.clone();
        Some(Target { point, geometry: Vec::new(), background: Some(background) })
    }
}

fn candidate(world: &World, view: &View, geometry: Option<&GeometrySnapshot>, entity: Entity) -> Option<Geometry> {
    let node = *world.get::<ComputedNode>(entity)?;
    if node.size().min_element() <= 0.0 || world.get::<InheritedVisibility>(entity).is_none_or(|visible| !visible.get())
        || world.get::<Pickable>(entity).is_some_and(|pickable| !pickable.should_block_lower && !pickable.is_hoverable)
    { return None; }
    let clip = world.get::<bevy::ui::CalculatedClip>(entity).map(|clip| clip.clip);
    if clip.is_some_and(|clip| clip.size().min_element() <= 0.0) { return None; }
    let camera = world.get::<bevy::ui::ComputedUiTargetCamera>(entity)?.get()?;
    let active = world.get::<Camera>(camera)?.is_active;
    let bevy::camera::RenderTarget::Image(target) = world.get::<bevy::camera::RenderTarget>(camera)? else { return None; };
    let image = target.handle.id();
    let projection = if image == view.image {
        if !active { return None; }
        Projection::Final(view.raster)
    } else {
        let owner = world.get::<native::topology::presentation::VisualOwner>(camera)?.0;
        let surface = world.get::<native::topology::presentation::Surface>(owner)?;
        let id = world.get::<native::layout::LayoutBox>(owner)?.id;
        if !surface.visible || surface.camera != camera || surface.image.id() != image || surface.pixels.min_element() == 0
            || world.get::<native::layout::desktop::ApplicationWindow>(owner).is_some()
            || !active && !native::topology::presentation::displayed(world, owner)
        { return None; }
        let item = geometry?.items.iter().find(|item| item.id == ItemId::Native(id) && item.output == view.output)?.clone();
        Projection::Sand { owner, pixels: surface.pixels, item }
    };
    let owner = native::accessibility::owner_context(world, entity, false);
    if owner.as_ref().is_some_and(|owner| owner.output().is_some_and(|output| output != view.output)) { return None; }
    let mut lineage = Vec::new();
    let mut actions = Vec::new();
    let mut eligible = view.root.is_none() || owner.as_ref().is_some_and(|owner| Some(owner.root) == view.root);
    let mut next = Some(entity);
    for _ in 0..native::layout::MAX_DEPTH {
        let Some(current) = next else { break; };
        if world.get::<bevy::ecs::entity_disabling::Disabled>(current).is_some()
            || world.get::<Node>(current).is_some_and(|node| node.display == Display::None)
            || world.get::<Visibility>(current).is_some_and(|visible| *visible == Visibility::Hidden)
            || world.get::<InheritedVisibility>(current).is_some_and(|visible| !visible.get())
        { return None; }
        eligible &= world.get::<bevy::ui::InteractionDisabled>(current).is_none()
            && world.get::<AccessibilityNode>(current).is_none_or(|node| !node.is_disabled());
        lineage.push(current);
        if let Some(action) = world.get_entity(current).ok().and_then(|entity| entity.get_ref::<native::actions::ActionButton>()) {
            actions.push((current, action.target, action.last_changed().get()));
        }
        next = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    eligible &= next.is_none();
    if let Projection::Sand { owner, .. } = &projection {
        eligible &= lineage.contains(owner) && world.get::<ChildOf>(*owner).map(ChildOf::parent) == view.root;
    }
    Some(Geometry { entity, output: view.output, bounds: view.viewport.as_ref().map(|viewport| viewport.bounds), image, lineage, actions, owner, eligible,
        blocks: world.get::<Pickable>(entity).is_none_or(|pickable| pickable.should_block_lower),
        modal: native::shortcuts::keyboard_owner(world), node, transform: *world.get::<UiGlobalTransform>(entity)?,
        clip, projection })
}

pub(crate) fn capture(world: &World, views: &[View], geometry: Option<&GeometrySnapshot>) -> Snapshot {
    let Some(stack) = world.get_resource::<bevy::ui::UiStack>() else { return Snapshot::default(); };
    let mut snapshot = Snapshot::default();
    for view in views {
        if let Some(root) = view.root
            && let Some(viewport) = &view.viewport
            && let Some(owner) = native::accessibility::owner_context(world, root, true)
        {
            snapshot.backgrounds.push(Background { root, output: view.output, image: view.image, bounds: viewport.bounds, owner });
        }
        for entity in stack.uinodes.iter().rev() {
            let Some(target) = candidate(world, view, geometry, *entity) else { continue; };
            if snapshot.targets.len() == MAX_TARGETS { snapshot.overflowed = true; return snapshot; }
            snapshot.targets.push(target);
        }
    }
    snapshot
}

pub(crate) fn publish_status(world: &mut World, snapshot: &Snapshot) {
    if world.get_resource::<Held>().is_some_and(|held| held.overflowed != snapshot.overflowed) {
        world.resource_mut::<Held>().overflowed = snapshot.overflowed;
        if snapshot.overflowed {
            native::notifications::report(world, "Park input", "The visible native interface exceeds the captured-input node limit. Captured actions are refused until the interface is reduced.");
        }
    }
}

pub(crate) fn current(world: &World, views: &[View], geometry: Option<&GeometrySnapshot>, captured: &Target) -> bool {
    let Some(output) = captured.geometry.first().map(|geometry| geometry.output).or_else(|| captured.background.as_ref().map(|background| background.output)) else { return false; };
    let Some(view) = views.iter().find(|view| view.output == output) else { return false; };
    let current = capture(world, std::slice::from_ref(view), geometry).target(captured.point);
    #[cfg(test)]
    if current.as_ref() != Some(captured) {
        let describe = |target: &Target| target.geometry.iter().map(|item| (item.entity, item.output, item.modal,
            item.eligible, item.image, item.bounds, item.node.size(), item.owner.as_ref().map(|owner| (owner.root, owner.output())),
            format!("{:?}", item.projection))).collect::<Vec<_>>();
        eprintln!("Captured native pointer changed at {:?}: captured={:?}, current={:?}, backgrounds={:?}", captured.point,
            describe(captured), current.as_ref().map(describe), (captured.background.as_ref().map(|background| background.root),
                current.as_ref().and_then(|target| target.background.as_ref().map(|background| background.root))));
        if let Some(current) = &current {
            eprintln!("Captured native pointer equality: {:?}", captured.geometry.iter().zip(&current.geometry).map(|(a, b)|
                (a.entity, b.entity, vec![("owner", a.owner == b.owner), ("node", a.node == b.node), ("transform", a.transform == b.transform),
                    ("clip", a.clip == b.clip), ("projection", a.projection == b.projection), ("actions", a.actions == b.actions),
                    ("lineage", a.lineage == b.lineage), ("bounds", a.bounds == b.bounds), ("same_owner", a.same_owner(b))])).collect::<Vec<_>>());
        }
        for original in &captured.geometry {
            let camera = world.get::<bevy::ui::ComputedUiTargetCamera>(original.entity).and_then(|camera| camera.get());
            let surface = camera.and_then(|camera| world.get::<native::topology::presentation::VisualOwner>(camera))
                .and_then(|owner| world.get::<native::topology::presentation::Surface>(owner.0).map(|surface| (owner.0, surface)));
            eprintln!("Captured native source: surface={:?}, item={:?}", surface.map(|(owner, surface)|
                (owner, surface.visible, surface.camera, surface.image.id(), surface.pixels, native::topology::presentation::displayed(world, owner))),
                surface.and_then(|(owner, _)| world.get::<native::layout::LayoutBox>(owner)).map(|layout|
                    (layout.id, geometry.map(|geometry| geometry.items.iter().map(|item| (item.id, item.output)).collect::<Vec<_>>()))));
            let current = candidate(world, view, geometry, original.entity);
            eprintln!("Captured native candidate {:?}: inherited={:?}, camera={:?}, active={:?}, image={:?}, owner_current={}, geometry={:?}",
                original.entity, world.get::<InheritedVisibility>(original.entity).map(|visible| visible.get()), camera,
                camera.and_then(|camera| world.get::<Camera>(camera).map(|camera| camera.is_active)),
                camera.and_then(|camera| match world.get::<bevy::camera::RenderTarget>(camera) {
                    Some(bevy::camera::RenderTarget::Image(target)) => Some(target.handle.id()), _ => None,
                }), original.owner.as_ref().is_some_and(|owner| native::accessibility::owner_context(world, original.entity, true)
                    .is_some_and(|current| owner.same_owner(&current))), current.as_ref().map(|current|
                    (current.hit(captured.point), current.eligible, current.node.size(), current.transform, current.clip, format!("{:?}", current.projection))));
        }
    }
    current.as_ref() == Some(captured)
}

pub(crate) fn location(world: &mut World, views: &[View], geometry: Option<&GeometrySnapshot>, captured: &Target, point: [f64; 2]) -> Option<Location> {
    let output = captured.output()?;
    let view = views.iter().find(|view| view.output == output)?;
    let (image, scale, position) = if let Some(original) = captured.geometry.first() {
        let current = candidate(world, view, geometry, original.entity)?;
        if !original.same_owner(&current) { return None; }
        match current.projection {
            Projection::Final(raster) => (current.image, raster.scale[0], raster.point(point)),
            Projection::Sand { owner, pixels, item } => {
                let surface = world.get::<native::topology::presentation::Surface>(owner)?;
                let local = item.mapping.surface_point_unbounded(point)?;
                let position = std::array::from_fn(|axis| (local[axis] - item.mapping.surface.origin()[axis])
                    / item.mapping.surface.size()[axis] * f64::from(pixels[axis]));
                (current.image, f64::from(surface.density), position)
            }
        }
    } else { (view.image, view.raster.scale[0], view.raster.point(point)) };
    if !scale.is_finite() || scale <= 0.0 || position.iter().any(|value| !value.is_finite()) { return None; }
    let image = world.get_resource_mut::<Assets<Image>>()?.get_strong_handle(image)?;
    let position = Vec2::from_array(position.map(|value| (value / scale) as f32));
    if !position.is_finite() { return None; }
    Some(Location { target: NormalizedRenderTarget::Image(ImageRenderTarget { handle: image, scale_factor: scale as f32 }),
        position })
}

pub(crate) fn held_current(world: &World, views: &[View], geometry: Option<&GeometrySnapshot>, captured: &Target) -> bool {
    if let Some(background) = &captured.background {
        return views.iter().any(|view| view.output == background.output && view.root == Some(background.root))
            && native::accessibility::owner_context(world, background.root, true).is_some_and(|owner| background.owner.same_owner(&owner));
    }
    if captured.geometry.is_empty() { return false; }
    captured.geometry.iter().all(|original| {
        views.iter().find(|view| view.output == original.output)
            .and_then(|view| candidate(world, view, geometry, original.entity))
            .is_some_and(|current| original.same_owner(&current))
    })
}

#[cfg(test)]
mod tests;
