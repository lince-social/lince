use crate::native::wake::WakeSignal;
use bevy::{
    asset::AssetId,
    ecs::{entity_disabling::Disabled, query::{Allow, QueryData}},
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
pub struct StartupStatus(pub Vec<crate::native::laboratory::StartupIssue>);

#[derive(Resource, Clone, Default)]
struct PreparedImages(Arc<RwLock<HashSet<AssetId<Image>>>>);

pub(crate) fn image_ready(world: &World, image: AssetId<Image>) -> bool {
    world
        .get_resource::<PreparedImages>()
        .is_none_or(|prepared| {
            prepared
                .0
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .contains(&image)
        })
}

#[derive(Component)]
struct Presented;

#[derive(Resource, Default)]
struct Layouts(HashMap<Entity, [u32; 5]>);

#[derive(QueryData)]
struct LayoutData {
    node: Ref<'static, Node>,
    children: Option<Ref<'static, Children>>,
    image: Option<Ref<'static, ImageNode>>,
    text: Option<Ref<'static, Text>>,
    editable: Option<Ref<'static, EditableText>>,
}

impl LayoutDataItem<'_, '_> {
    fn stamp(&self) -> [u32; 5] {
        [
            self.node.last_changed().get(),
            self.children.as_ref().map_or(0, |value| value.last_changed().get()),
            self.image.as_ref().map_or(0, |value| value.last_changed().get()),
            self.text.as_ref().map_or(0, |value| value.last_changed().get()),
            self.editable.as_ref().map_or(0, |value| value.last_changed().get()),
        ]
    }
}

#[derive(QueryData)]
struct ReadinessData {
    layout: Option<LayoutData>,
    children: Option<&'static Children>,
    parent: Option<&'static ChildOf>,
    visibility: Option<&'static Visibility>,
    castle: Has<Castle>,
    pending: Has<Pending>,
    failure: Option<&'static crate::native::laboratory::StartupFailure>,
    block: Option<&'static ComputedTextBlock>,
    text_node: Has<Text>,
    text: Option<&'static TextLayoutInfo>,
    image: Option<&'static ImageNode>,
    surface: Option<&'static crate::native::topology::presentation::Surface>,
}

#[derive(Resource)]
struct ReadinessQueries {
    roots: QueryState<Entity, With<Castle>>,
    nodes: QueryState<ReadinessData, Allow<Disabled>>,
}

impl FromWorld for ReadinessQueries {
    fn from_world(world: &mut World) -> Self {
        Self {
            roots: QueryState::new(world),
            nodes: QueryState::new(world),
        }
    }
}

pub struct CastlePlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PresentCastles;

impl Plugin for CastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Layouts>()
            .add_observer(|_: On<bevy::ecs::change_detection::CheckChangeTicks>, mut layouts: ResMut<Layouts>| {
                layouts.0.clear();
            })
            .add_systems(
                PostUpdate,
                remember_layout
                    .after(bevy::ui::UiSystems::PostLayout)
                    .before(crate::native::protein_area::MeasureRecords),
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
    use bevy::ecs::system::RunSystemOnce;

    fn remember_layout(world: &mut World) {
        world.run_system_once(super::remember_layout).unwrap();
    }

    #[cfg_attr(test, test)]
    fn layout_snapshots_track_optional_components_commands_and_removals() {
        #[derive(Resource)]
        struct Spawned(Entity);

        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((MinimalPlugins, CastlePlugin));
        app.add_systems(
            PostUpdate,
            (|mut commands: Commands, mut done: Local<bool>| {
                if !*done {
                    *done = true;
                    let entity = commands.spawn(Node::default()).id();
                    commands.insert_resource(Spawned(entity));
                }
            })
            .in_set(bevy::ui::UiSystems::PostLayout),
        );
        let entity = app.world_mut().spawn(Node::default()).id();
        app.update();
        let spawned = app.world().resource::<Spawned>().0;
        assert_eq!(app.world().resource::<Layouts>().0.len(), 2);
        assert_eq!(
            app.world().resource::<Layouts>().0.get(&spawned),
            stamp(app.world(), spawned).as_ref()
        );
        let changes: [fn(&mut World, Entity); 5] = [
            |world, entity| {
                world.get_mut::<Node>(entity).unwrap().width = px(123);
            },
            |world, entity| {
                world.spawn((Node::default(), ChildOf(entity)));
            },
            |world, entity| {
                world.entity_mut(entity).insert(ImageNode::default());
            },
            |world, entity| {
                world.entity_mut(entity).insert(Text::new("Ready"));
            },
            |world, entity| {
                world.entity_mut(entity).insert(EditableText::new("Ready"));
            },
        ];
        for change in changes {
            let before = app.world().resource::<Layouts>().0[&entity];
            change(app.world_mut(), entity);
            app.update();
            let after = app.world().resource::<Layouts>().0[&entity];
            assert_ne!(before, after);
            assert_eq!(Some(after), stamp(app.world(), entity));
            let layout_tick = app.world().get_resource_ref::<Layouts>().unwrap().last_changed();
            app.update();
            assert_eq!(app.world().resource::<Layouts>().0[&entity], after);
            assert_eq!(app.world().get_resource_ref::<Layouts>().unwrap().last_changed(), layout_tick);
        }
        app.world_mut()
            .entity_mut(entity)
            .remove::<(ImageNode, Text, EditableText)>();
        app.update();
        assert_eq!(app.world().resource::<Layouts>().0[&entity][2..], [0; 3]);
        app.world_mut().resource_mut::<Layouts>().0.clear();
        app.world_mut().get_mut::<Node>(entity).unwrap().width = px(151.0);
        app.update();
        assert_eq!(app.world().resource::<Layouts>().0.get(&entity), stamp(app.world(), entity).as_ref());
        assert_eq!(app.world().resource::<Layouts>().0.get(&spawned), stamp(app.world(), spawned).as_ref());
        app.world_mut().entity_mut(entity).insert(Disabled);
        app.update();
        assert!(!app.world().resource::<Layouts>().0.contains_key(&entity));
        app.world_mut().entity_mut(entity).remove::<Disabled>();
        app.update();
        assert_eq!(app.world().resource::<Layouts>().0.get(&entity), stamp(app.world(), entity).as_ref());
        app.world_mut().entity_mut(entity).remove::<Node>().insert(Node::default());
        app.update();
        assert_eq!(app.world().resource::<Layouts>().0.get(&entity), stamp(app.world(), entity).as_ref());
        app.world_mut().entity_mut(spawned).remove::<Node>();
        app.world_mut().despawn(entity);
        app.update();
        assert!(app.world().resource::<Layouts>().0.is_empty());
    }

    #[cfg_attr(test, test)]
    fn nested_castles_wait_for_every_image_without_hiding_unrelated_sands() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Layouts>();
        world.init_resource::<PreparedImages>();
        let image = Handle::<Image>::default();
        let root = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        let square = world
            .spawn((
                crate::native::sand::Square,
                ChildOf(root),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        let nested = world
            .spawn((Castle, ChildOf(root), InheritedVisibility::VISIBLE))
            .id();
        let picture = world
            .spawn((
                crate::native::sand::ImageSand,
                ImageNode::new(image.clone()),
                ChildOf(nested),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        let other = world
            .spawn((crate::native::sand::Square, InheritedVisibility::VISIBLE))
            .id();
        for entity in [square, picture] {
            world
                .entity_mut(entity)
                .insert(crate::native::canvas::CanvasItem {
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
        let snapshot = crate::native::laboratory::resources::capture(&mut world);
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
            crate::native::laboratory::resources::capture(&mut world)
                .sands
                .iter()
                .all(|row| row.startup.is_empty())
        );
    }

    #[cfg_attr(test, test)]
    fn new_members_and_pending_work_hold_the_whole_group() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
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
            .insert(crate::native::laboratory::StartupFailure(
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
            .remove::<crate::native::laboratory::StartupFailure>();
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
    crate::native::laboratory_cases! {
        layout_snapshots_track_optional_components_commands_and_removals,
        nested_castles_wait_for_every_image_without_hiding_unrelated_sands,
        new_members_and_pending_work_hold_the_whole_group,
        presented_castles_keep_their_surface_while_replacement_content_settles,
        repeated_glyph_pages_still_require_every_atlas_and_follow_removal,
        readiness_follows_reparenting_visibility_and_late_layout_changes,
    }

    #[cfg_attr(test, test)]
    fn readiness_follows_reparenting_visibility_and_late_layout_changes() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Layouts>();
        let first = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        let second = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        let child = world.spawn((Castle, ChildOf(first), InheritedVisibility::VISIBLE)).id();
        let ready = |world: &World, root| world.get::<StartupStatus>(root).unwrap().0.is_empty();
        remember_layout(&mut world);
        present(&mut world);
        present(&mut world);
        assert!(ready(&world, first) && ready(&world, second));
        world.entity_mut(child).insert(Pending);
        present(&mut world);
        assert!(!ready(&world, first) && ready(&world, second));
        world.entity_mut(child).remove::<Pending>();
        present(&mut world);
        assert!(ready(&world, first) && ready(&world, second));
        world.entity_mut(child).insert(ChildOf(second));
        present(&mut world);
        assert!(!ready(&world, first) && !ready(&world, second));
        remember_layout(&mut world);
        present(&mut world);
        assert!(ready(&world, first) && ready(&world, second));
        world.entity_mut(child).insert((Visibility::Hidden, Pending));
        present(&mut world);
        assert!(ready(&world, first) && ready(&world, second));
        world.entity_mut(child).remove::<Visibility>();
        present(&mut world);
        assert!(ready(&world, first) && !ready(&world, second));
        world.entity_mut(child).remove::<Pending>();
        remember_layout(&mut world);
        present(&mut world);
        assert!(ready(&world, second));
        world.get_mut::<Node>(child).unwrap().width = px(137.0);
        present(&mut world);
        assert!(!ready(&world, second));
        remember_layout(&mut world);
        present(&mut world);
        assert!(ready(&world, second));
        world.despawn(child);
        remember_layout(&mut world);
        present(&mut world);
        assert!(ready(&world, first) && ready(&world, second));
    }

    #[cfg_attr(test, test)]
    fn repeated_glyph_pages_still_require_every_atlas_and_follow_removal() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Layouts>();
        world.init_resource::<PreparedImages>();
        let mut images = Assets::<Image>::default();
        let handles: Vec<_> = (0..3).map(|_| images.add(Image::default())).collect();
        let pages: Vec<_> = handles.iter().map(Handle::id).collect();
        let root = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        let label = world.spawn((
            Node::default(),
            TextLayoutInfo {
                glyphs: [0, 0, 1, 1, 0, 2, 2, 0]
                    .into_iter()
                    .map(|index| bevy::text::PositionedGlyph {
                        position: Vec2::ZERO,
                        atlas_info: bevy::text::GlyphAtlasInfo {
                            texture: pages[index],
                            rect: Rect::default(),
                            offset: Vec2::ZERO,
                            is_alpha_mask: true,
                        },
                        section_index: 0,
                        line_index: 0,
                    })
                    .collect(),
                ..default()
            },
            ChildOf(root),
        )).id();
        remember_layout(&mut world);
        for missing in &pages {
            *world.resource::<PreparedImages>().0.write().unwrap() = pages
                .iter()
                .copied()
                .filter(|page| page != missing)
                .collect();
            present(&mut world);
            assert!(
                world
                    .get::<StartupStatus>(root)
                    .unwrap()
                    .0
                    .iter()
                    .any(|issue| issue.reason
                        == "Waiting for text preparation on the graphics device")
            );
            world
                .resource::<PreparedImages>()
                .0
                .write()
                .unwrap()
                .insert(*missing);
            present(&mut world);
            assert!(world.get::<StartupStatus>(root).unwrap().0.is_empty());
        }
        assert!(world.get::<Presented>(root).is_some());
        let missing = images.add(Image::default());
        world.get_mut::<TextLayoutInfo>(label).unwrap().glyphs[0].atlas_info.texture = missing.id();
        present(&mut world);
        assert!(!world.get::<StartupStatus>(root).unwrap().0.is_empty());
        world.get_mut::<TextLayoutInfo>(label).unwrap().glyphs[0].atlas_info.texture = pages[0];
        present(&mut world);
        assert!(world.get::<StartupStatus>(root).unwrap().0.is_empty());
    }

    #[cfg_attr(test, test)]
    fn presented_castles_keep_their_surface_while_replacement_content_settles() {
        let mut world = World::new();
        world.init_resource::<Layouts>();
        let camera = world
            .spawn(crate::native::topology::surface_render::SurfaceCapture(
                Handle::default(),
            ))
            .id();
        let root = world.spawn((Castle, InheritedVisibility::VISIBLE)).id();
        world
            .entity_mut(root)
            .insert(crate::native::topology::presentation::Surface {
                camera,
                image: Handle::default(),
                visual: root,
                body: root,
                face: root,
                size: Vec2::splat(100.0),
                pixels: UVec2::splat(100),
                density: 1.0,
                material: Handle::default(),
                uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE),
                visible: true,
            });
        remember_layout(&mut world);
        present(&mut world);
        assert!(world.get::<Presented>(root).is_some());
        assert!(
            world
                .get::<crate::native::topology::surface_render::CaptureReady>(camera)
                .unwrap()
                .0
        );
        let capture_tick = world.entity(camera).get_ref::<crate::native::topology::surface_render::CaptureReady>().unwrap().last_changed();
        let presented_tick = world.entity(root).get_ref::<Presented>().unwrap().last_changed();
        world.increment_change_tick();
        present(&mut world);
        assert_eq!(world.entity(camera).get_ref::<crate::native::topology::surface_render::CaptureReady>().unwrap().last_changed(), capture_tick);
        assert_eq!(world.entity(root).get_ref::<Presented>().unwrap().last_changed(), presented_tick);
        world.increment_change_tick();
        let child = world
            .spawn((
                Node::default(),
                ChildOf(root),
                InheritedVisibility::VISIBLE,
                Pending,
            ))
            .id();
        present(&mut world);
        assert!(world.get::<InheritedVisibility>(root).unwrap().get());
        assert!(world.get::<InheritedVisibility>(child).unwrap().get());
        assert!(
            !world
                .get::<crate::native::topology::surface_render::CaptureReady>(camera)
                .unwrap()
                .0
        );
        world.entity_mut(child).remove::<Pending>();
        remember_layout(&mut world);
        present(&mut world);
        assert!(world.get::<InheritedVisibility>(root).unwrap().get());
        assert!(
            world
                .get::<crate::native::topology::surface_render::CaptureReady>(camera)
                .unwrap()
                .0
        );
        assert!(world.get::<StartupStatus>(root).unwrap().0.is_empty());
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

fn remember_layout(
    nodes: Query<(Entity, LayoutData)>,
    changed: Query<Entity, (With<Node>, Or<(
        Changed<Node>, Changed<Children>, Changed<ImageNode>, Changed<Text>, Changed<EditableText>,
    )>)>,
    mut removed_nodes: RemovedComponents<Node>,
    mut removed_children: RemovedComponents<Children>,
    mut removed_images: RemovedComponents<ImageNode>,
    mut removed_text: RemovedComponents<Text>,
    mut removed_editors: RemovedComponents<EditableText>,
    disabled: Query<Entity, (With<Node>, Added<Disabled>, Allow<Disabled>)>,
    mut enabled: RemovedComponents<Disabled>,
    mut layouts: ResMut<Layouts>,
) {
    let reset = layouts.0.is_empty();
    for entity in removed_nodes.read().chain(disabled.iter()) {
        layouts.0.remove(&entity);
    }
    for entity in changed.iter()
        .chain(removed_children.read())
        .chain(removed_images.read())
        .chain(removed_text.read())
        .chain(removed_editors.read())
        .chain(enabled.read())
    {
        if !reset && let Ok((_, node)) = nodes.get(entity) {
            layouts.0.insert(entity, node.stamp());
        }
    }
    if reset {
        layouts.0.extend(nodes.iter().map(|(entity, node)| (entity, node.stamp())));
    }
}

fn present(world: &mut World) {
    world.init_resource::<ReadinessQueries>();
    world.resource_scope(|world, mut queries: Mut<ReadinessQueries>| {
        present_with(world, &mut queries);
    });
}

fn present_with(world: &mut World, queries: &mut ReadinessQueries) {
    let roots: Vec<_> = queries.roots.iter(world).collect();
    let roots: Vec<_> = roots.into_iter()
        .filter(|entity| {
            let mut parent = queries.nodes.get(world, *entity).ok().and_then(|node| node.parent.map(ChildOf::parent));
            while let Some(entity) = parent {
                let Ok(node) = queries.nodes.get(world, entity) else { break; };
                if node.castle {
                    return false;
                }
                parent = node.parent.map(ChildOf::parent);
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
        let mut cameras = Vec::new();
        let mut pending = vec![root];
        let mut ready = true;
        let mut layout_pending = false;
        let mut issues = Vec::new();
        while let Some(entity) = pending.pop() {
            let Ok(node) = queries.nodes.get(world, entity) else { continue; };
            if node.layout.as_ref().is_some_and(|layout| layout.node.display == Display::None)
                || node.visibility == Some(&Visibility::Hidden)
            {
                continue;
            }
            members.push(entity);
            if let Some(surface) = node.surface {
                cameras.push(surface.camera);
            }
            if let Some(children) = node.children {
                pending.extend(children.iter());
            }
            let mut issue = |failed, reason: String| {
                issues.push(crate::native::laboratory::StartupIssue {
                    entity: entity.to_string(),
                    failed,
                    reason,
                });
            };
            if let Some(failure) = node.failure {
                ready = false;
                issue(true, failure.0.clone());
            } else if node.pending {
                ready = false;
                issue(false, "Sand initialization is pending".into());
            }
            if let Some(layout) = &node.layout {
                let stamp = layout.stamp();
                let laid_out = world.resource::<Layouts>().0.get(&entity) == Some(&stamp);
                ready &= laid_out;
                layout_pending |= !laid_out;
                if !laid_out {
                    issue(false, "Waiting for layout".into());
                }
            }
            if node.text_node
                && let Some(block) = node.block
            {
                let pending = block.needs_rerender(false, false);
                ready &= !pending;
                if pending {
                    issue(false, "Waiting for text layout".into());
                }
            }
            if let Some(images) = &images {
                if let Some(image) = node.image {
                    let prepared = images.contains(&image.image.id());
                    ready &= prepared;
                    if let Some(problem) = crate::native::laboratory::resources::asset_issue(
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
                if let Some(text) = node.text {
                    let mut previous = None;
                    let prepared = text.glyphs.iter().all(|glyph| {
                        let texture = glyph.atlas_info.texture;
                        if previous == Some(texture) {
                            return true;
                        }
                        previous = Some(texture);
                        images.contains(&texture)
                    });
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
        for &camera in &cameras {
            let value = crate::native::topology::surface_render::CaptureReady(ready);
            if world.get::<crate::native::topology::surface_render::CaptureReady>(camera)
                .is_none_or(|previous| previous.0 != ready) {
                world
                    .entity_mut(camera)
                    .insert(value);
            }
        }
        if ready && world.get::<Presented>(root).is_none() {
            world.entity_mut(root).insert(Presented);
        }
        if !ready && world.get::<Presented>(root).is_none() {
            for entity in members {
                if let Some(mut visibility) = world.get_mut::<InheritedVisibility>(entity) {
                    visibility.set_if_neq(InheritedVisibility::HIDDEN);
                }
            }
            if let Some(mut visibility) = world.get_mut::<Visibility>(root) {
                visibility.set_changed();
            }
        }
        if layout_pending && let Some(wake) = world.get_resource::<WakeSignal>() {
            wake.ring();
        }
    }
}
