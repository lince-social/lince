use crate::wake::WakeSignal;
use bevy::{
    asset::AssetId,
    prelude::*,
    render::{Render, RenderApp, RenderSystems, render_asset::RenderAssets, texture::GpuImage},
    text::{ComputedTextBlock, EditableText, TextLayoutInfo},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

#[derive(Component, Default)]
#[require(Node)]
pub struct Castle;

#[derive(Component, Default)]
pub struct Pending;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct StartupStatus(pub Vec<crate::laboratory::StartupIssue>);

#[derive(Resource, Clone, Default)]
struct PreparedImages(Arc<RwLock<HashSet<AssetId<Image>>>>);

#[derive(Resource, Default)]
struct Layouts(HashMap<Entity, [u32; 5]>);

pub struct CastlePlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PresentCastles;

impl Plugin for CastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Layouts>()
            .add_systems(
                PostUpdate,
                remember_layout
                    .after(bevy::ui::UiSystems::PostLayout)
                    .before(crate::actions::ApplyActions),
            )
            .add_systems(Last, present.in_set(PresentCastles));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            let images = PreparedImages::default();
            render
                .insert_resource(images.clone())
                .add_systems(Render, prepared_images.after(RenderSystems::PrepareAssets));
            app.insert_resource(images);
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(wake) = app.world().get_resource::<WakeSignal>().cloned()
            && let Some(render) = app.get_sub_app_mut(RenderApp)
        {
            render.insert_resource(wake);
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn nested_castles_wait_for_every_image_without_hiding_unrelated_sands() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<Layouts>();
        world.init_resource::<PreparedImages>();
        let image = Handle::<Image>::default();
        let root = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        let square = world
            .spawn((
                crate::sand::Square,
                ChildOf(root),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        let nested = world
            .spawn((Castle, ChildOf(root), InheritedVisibility::VISIBLE))
            .id();
        let picture = world
            .spawn((
                crate::sand::ImageSand,
                ImageNode::new(image.clone()),
                ChildOf(nested),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        let other = world
            .spawn((crate::sand::Square, InheritedVisibility::VISIBLE))
            .id();
        for entity in [square, picture] {
            world.entity_mut(entity).insert(crate::canvas::CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::splat(100.0),
            });
        }
        remember_layout(&mut world);
        present(&mut world);
        for entity in [root, square, nested, picture] {
            assert!(!world.get::<InheritedVisibility>(entity).unwrap().get());
        }
        assert!(world.get::<InheritedVisibility>(other).unwrap().get());
        let status = world.get::<StartupStatus>(root).unwrap();
        assert!(
            status
                .0
                .iter()
                .any(|issue| issue.entity == picture.to_string()
                    && !issue.failed
                    && issue.reason.contains("image"))
        );
        let snapshot = crate::laboratory::resources::capture(&mut world);
        let sibling = snapshot
            .sands
            .iter()
            .find(|row| row.entity == square.to_string())
            .unwrap();
        assert!(
            sibling
                .startup
                .iter()
                .any(|issue| issue.entity == picture.to_string())
        );
        world
            .resource::<PreparedImages>()
            .0
            .write()
            .unwrap()
            .insert(image.id());
        for entity in [root, square, nested, picture] {
            world
                .entity_mut(entity)
                .insert(InheritedVisibility::VISIBLE);
        }
        present(&mut world);
        for entity in [root, square, nested, picture] {
            assert!(world.get::<InheritedVisibility>(entity).unwrap().get());
        }
        assert!(world.get::<StartupStatus>(root).unwrap().0.is_empty());
        assert!(
            crate::laboratory::resources::capture(&mut world)
                .sands
                .iter()
                .all(|row| row.startup.is_empty())
        );
    }

    #[cfg_attr(test, test)]
    fn new_members_and_pending_work_hold_the_whole_group() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<Layouts>();
        let root = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        remember_layout(&mut world);
        world.increment_change_tick();
        let child = world
            .spawn((Node::default(), ChildOf(root), InheritedVisibility::VISIBLE))
            .id();
        present(&mut world);
        assert!(!world.get::<InheritedVisibility>(root).unwrap().get());
        assert!(
            world
                .get::<StartupStatus>(root)
                .unwrap()
                .0
                .iter()
                .any(|issue| issue.reason == "Waiting for layout")
        );
        world.entity_mut(child).insert(Pending);
        remember_layout(&mut world);
        world.entity_mut(root).insert(InheritedVisibility::VISIBLE);
        present(&mut world);
        assert!(!world.get::<InheritedVisibility>(root).unwrap().get());
        assert!(
            world
                .get::<StartupStatus>(root)
                .unwrap()
                .0
                .iter()
                .any(|issue| !issue.failed && issue.reason == "Sand initialization is pending")
        );
        world
            .entity_mut(child)
            .insert(crate::laboratory::StartupFailure(
                "Cannot open document".into(),
            ));
        present(&mut world);
        assert!(
            world
                .get::<StartupStatus>(root)
                .unwrap()
                .0
                .iter()
                .any(|issue| issue.failed
                    && issue.entity == child.to_string()
                    && issue.reason == "Cannot open document")
        );
        world
            .entity_mut(child)
            .remove::<crate::laboratory::StartupFailure>();
        world.entity_mut(child).remove::<Pending>();
        for entity in [root, child] {
            world
                .entity_mut(entity)
                .insert(InheritedVisibility::VISIBLE);
        }
        present(&mut world);
        assert!(world.get::<InheritedVisibility>(root).unwrap().get());
        assert!(world.get::<StartupStatus>(root).unwrap().0.is_empty());
    }
    crate::laboratory_cases! {
        nested_castles_wait_for_every_image_without_hiding_unrelated_sands,
        new_members_and_pending_work_hold_the_whole_group,
    }
}

fn prepared_images(
    images: Res<RenderAssets<GpuImage>>,
    prepared: Res<PreparedImages>,
    wake: Option<Res<WakeSignal>>,
) {
    let next: HashSet<_> = images.iter().map(|(id, _)| id).collect();
    let mut current = prepared
        .0
        .write()
        .unwrap_or_else(|error| error.into_inner());
    if *current != next {
        *current = next;
        if let Some(wake) = wake {
            wake.ring();
        }
    }
}

pub(crate) fn image_prepared(world: &World, image: AssetId<Image>) -> bool {
    world
        .get_resource::<PreparedImages>()
        .is_some_and(|images| {
            images
                .0
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .contains(&image)
        })
}

fn stamp(world: &World, entity: Entity) -> Option<[u32; 5]> {
    let entity = world.get_entity(entity).ok()?;
    Some([
        entity.get_ref::<Node>()?.last_changed().get(),
        entity
            .get_ref::<Children>()
            .map_or(0, |value| value.last_changed().get()),
        entity
            .get_ref::<ImageNode>()
            .map_or(0, |value| value.last_changed().get()),
        entity
            .get_ref::<Text>()
            .map_or(0, |value| value.last_changed().get()),
        entity
            .get_ref::<EditableText>()
            .map_or(0, |value| value.last_changed().get()),
    ])
}

fn remember_layout(world: &mut World) {
    let layouts = world
        .query_filtered::<Entity, With<Node>>()
        .iter(world)
        .filter_map(|entity| stamp(world, entity).map(|stamp| (entity, stamp)))
        .collect();
    world.resource_mut::<Layouts>().0 = layouts;
}

fn present(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Castle>>()
        .iter(world)
        .filter(|entity| {
            let mut parent = world.get::<ChildOf>(*entity).map(ChildOf::parent);
            while let Some(entity) = parent {
                if world.get::<Castle>(entity).is_some() {
                    return false;
                }
                parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            true
        })
        .collect();
    let images = world.get_resource::<PreparedImages>().cloned();
    let images = images
        .as_ref()
        .map(|images| images.0.read().unwrap_or_else(|error| error.into_inner()));
    for root in roots {
        let mut members = Vec::new();
        let mut pending = vec![root];
        let mut ready = true;
        let mut layout_pending = false;
        let mut issues = Vec::new();
        while let Some(entity) = pending.pop() {
            if world
                .get::<Node>(entity)
                .is_some_and(|node| node.display == Display::None)
                || world.get::<Visibility>(entity) == Some(&Visibility::Hidden)
            {
                continue;
            }
            members.push(entity);
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
            let mut issue = |failed, reason: String| {
                issues.push(crate::laboratory::StartupIssue {
                    entity: entity.to_string(),
                    failed,
                    reason,
                });
            };
            if let Some(failure) = world.get::<crate::laboratory::StartupFailure>(entity) {
                ready = false;
                issue(true, failure.0.clone());
            } else if world.get::<Pending>(entity).is_some() {
                ready = false;
                issue(false, "Sand initialization is pending".into());
            }
            if let Some(stamp) = stamp(world, entity) {
                let laid_out = world.resource::<Layouts>().0.get(&entity) == Some(&stamp);
                ready &= laid_out;
                layout_pending |= !laid_out;
                if !laid_out {
                    issue(false, "Waiting for layout".into());
                }
            }
            if world.get::<Text>(entity).is_some()
                && let Some(block) = world.get::<ComputedTextBlock>(entity)
            {
                let pending = block.needs_rerender(false, false);
                ready &= !pending;
                if pending {
                    issue(false, "Waiting for text layout".into());
                }
            }
            if let Some(images) = &images {
                if let Some(image) = world.get::<ImageNode>(entity) {
                    let prepared = images.contains(&image.image.id());
                    ready &= prepared;
                    if let Some(problem) = crate::laboratory::resources::asset_issue(
                        world,
                        entity,
                        image.image.id().untyped(),
                        "image",
                        prepared
                            || world
                                .get_resource::<Assets<Image>>()
                                .is_some_and(|assets| assets.contains(image.image.id())),
                    ) {
                        ready &= !problem.failed;
                        issue(problem.failed, problem.reason);
                    } else if !prepared {
                        issue(
                            false,
                            "Waiting for image preparation on the graphics device".into(),
                        );
                    }
                }
                if let Some(text) = world.get::<TextLayoutInfo>(entity) {
                    let prepared = text
                        .glyphs
                        .iter()
                        .all(|glyph| images.contains(&glyph.atlas_info.texture));
                    ready &= prepared;
                    if !prepared {
                        issue(
                            false,
                            "Waiting for text preparation on the graphics device".into(),
                        );
                    }
                }
            }
        }
        let status = StartupStatus(issues);
        if world.get::<StartupStatus>(root) != Some(&status) {
            world.entity_mut(root).insert(status);
        }
        if !ready {
            for entity in members {
                if let Some(mut visibility) = world.get_mut::<InheritedVisibility>(entity) {
                    visibility.set_if_neq(InheritedVisibility::HIDDEN);
                }
            }
            if let Some(mut visibility) = world.get_mut::<Visibility>(root) {
                visibility.set_changed();
            }
            if layout_pending && let Some(wake) = world.get_resource::<WakeSignal>() {
                wake.ring();
            }
        }
    }
}
