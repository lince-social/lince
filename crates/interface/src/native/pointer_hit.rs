// non-lean-yet
// Scope: fn extend_ancestors, fn nodes, struct Nodes.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 33.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   39.

use bevy::{
    camera::{NormalizedRenderTarget, RenderTarget},
    prelude::*,
};
use std::collections::HashSet;

pub(crate) fn extend_ancestors(
    world: &World,
    visited: &mut HashSet<Entity>,
    entities: impl IntoIterator<Item = Entity>,
) {
    for entity in entities {
        let mut current = Some(entity);
        while let Some(entity) = current {
            if !visited.insert(entity) {
                break;
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

pub(crate) fn nodes(
    world: &World,
    window: Entity,
    position: Vec2,
) -> impl Iterator<Item = Entity> + '_ {
    let query = nucleus::performance::Span::new("pointer-hit-query");
    let target = world
        .get_resource::<crate::native::app::OutputTarget>()
        .map_or_else(
            || RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
            |target| target.0.clone(),
        )
        .normalize(Some(window));
    let entries = target
        .as_ref()
        .and_then(|_| world.get_resource::<bevy::ui::UiStack>())
        .map_or(&[][..], |stack| stack.uinodes.as_slice());
    Nodes {
        world,
        window,
        position,
        target,
        entries: entries.iter().rev(),
        camera_point: None,
        inspected: 0,
        matched: 0,
        mappings: 0,
        _query: query,
    }
}

struct Nodes<'a> {
    world: &'a World,
    window: Entity,
    position: Vec2,
    target: Option<NormalizedRenderTarget>,
    entries: std::iter::Rev<std::slice::Iter<'a, Entity>>,
    camera_point: Option<(Entity, Option<Vec2>)>,
    inspected: u64,
    matched: u64,
    mappings: u64,
    _query: nucleus::performance::Span,
}

impl Iterator for Nodes<'_> {
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let target = self.target.as_ref()?;
        for &entity in self.entries.by_ref() {
            self.inspected += 1;
            let Some(node) = self.world.get::<ComputedNode>(entity) else {
                continue;
            };
            if node.size().min_element() <= 0.0
                || self
                    .world
                    .get::<InheritedVisibility>(entity)
                    .is_none_or(|visible| !visible.get())
                || self
                    .world
                    .get::<Pickable>(entity)
                    .is_some_and(|pickable| !pickable.should_block_lower && !pickable.is_hoverable)
            {
                continue;
            }
            let Some(camera) = self
                .world
                .get::<bevy::ui::ComputedUiTargetCamera>(entity)
                .and_then(|camera| camera.get())
            else {
                continue;
            };
            let point = match self.camera_point {
                Some((previous, point)) if previous == camera => point,
                _ => {
                    self.mappings += 1;
                    let point = self
                        .world
                        .get::<RenderTarget>(camera)
                        .and_then(|target| target.normalize(Some(self.window)))
                        .filter(|actual| actual == target)
                        .and_then(|_| self.world.get::<Camera>(camera))
                        .map(|camera| {
                            self.position * camera.target_scaling_factor().unwrap_or(1.0)
                                - camera
                                    .physical_viewport_rect()
                                    .map_or(Vec2::ZERO, |viewport| viewport.min.as_vec2())
                        });
                    self.camera_point = Some((camera, point));
                    point
                }
            };
            let Some(point) = point else {
                continue;
            };
            if self
                .world
                .get::<UiGlobalTransform>(entity)
                .is_some_and(|transform| node.contains_point(*transform, point))
                && self
                    .world
                    .get::<bevy::ui::CalculatedClip>(entity)
                    .is_none_or(|clip| clip.contains_point(point))
            {
                self.matched += 1;
                return Some(entity);
            }
        }
        None
    }
}

impl Drop for Nodes<'_> {
    fn drop(&mut self) {
        nucleus::performance::add("pointer-hit-nodes-inspected", self.inspected);
        nucleus::performance::add("pointer-hit-nodes-returned", self.matched);
        nucleus::performance::add("pointer-hit-camera-mappings", self.mappings);
    }
}
