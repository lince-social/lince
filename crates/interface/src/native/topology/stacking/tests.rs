use super::*;
use crate::{
    host::{Rectangle, SurfaceId},
    layout::{
        Rules,
        desktop::{Geometry, Item, ItemKind, PlacementId},
    },
    native::{
        canvas::CanvasItem,
        layout::desktop::{ApplicationWindow, Managed, TransientParent},
        workspace::{WorkspaceMember, Workspaces},
    },
};

fn window(world: &mut World, root: Entity, index: u8) -> Entity {
    let id = PlacementId([index; 16]);
    let entity = world
        .spawn((
            ApplicationWindow {
                surface: SurfaceId(u64::from(index)),
                content_size: [320, 240],
            },
            Managed { id, floating: true },
            CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::new(320.0, 240.0),
            },
            WorkspaceMember(1),
            ZIndex(i32::from(index)),
            ChildOf(root),
        ))
        .id();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .insert_inactive(Item {
            id,
            kind: ItemKind::Application,
            rules: Rules::fixed([320.0, 240.0]),
            free: Geometry {
                origin: [-160.0, -120.0],
                size: [320.0, 240.0],
            },
            free_width: None,
            floating: false,
        })
        .unwrap();
    entity
}

fn focus(world: &mut World, root: Entity, entity: Entity) {
    let id = world.get::<Managed>(entity).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .focus(id, Rectangle::new([0.0; 2], [1000.0, 700.0]).unwrap())
        .unwrap();
}

#[test]
fn application_selection_changes_coplanar_order_without_rewriting_authored_placement() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let first = window(&mut world, root, 1);
    let second = window(&mut world, root, 2);
    let geometry = [first, second].map(|entity| *world.get::<CanvasItem>(entity).unwrap());
    assert!(key(&world, first) < key(&world, second));
    focus(&mut world, root, first);
    assert!(key(&world, second) < key(&world, first));
    focus(&mut world, root, second);
    assert!(key(&world, first) < key(&world, second));
    for (index, entity) in [first, second].into_iter().enumerate() {
        assert_eq!(world.get::<ZIndex>(entity).unwrap().0, index as i32 + 1);
        let actual = world.get::<CanvasItem>(entity).unwrap();
        assert_eq!(actual.position, geometry[index].position);
        assert_eq!(actual.size, geometry[index].size);
    }
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .clear_focus();
    assert!(key(&world, first) < key(&world, second));
    world.entity_mut(first).remove::<ApplicationWindow>();
    focus(&mut world, root, first);
    assert!(key(&world, first) < key(&world, second));
}

#[test]
fn selecting_a_transient_raises_its_family_and_keeps_children_above_the_parent() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let parent = window(&mut world, root, 1);
    let child = window(&mut world, root, 2);
    let grandchild = window(&mut world, root, 3);
    let other = window(&mut world, root, 4);
    world.entity_mut(child).insert(TransientParent(parent));
    world
        .entity_mut(grandchild)
        .insert((TransientParent(child), ZIndex(-20)));
    for selected in [parent, child, grandchild] {
        focus(&mut world, root, selected);
        assert!(key(&world, other) < key(&world, parent));
        assert!(key(&world, parent) < key(&world, child));
        assert!(key(&world, child) < key(&world, grandchild));
    }
    focus(&mut world, root, other);
    assert!(key(&world, grandchild) < key(&world, other));
}

#[test]
fn invalid_transient_chains_and_workspace_changes_do_not_raise_unrelated_windows() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let first = window(&mut world, root, 1);
    let second = window(&mut world, root, 2);
    focus(&mut world, root, first);
    world.entity_mut(first).insert(TransientParent(second));
    world.entity_mut(second).insert(TransientParent(first));
    assert!(key(&world, first) < key(&world, second));
    world.entity_mut(second).remove::<TransientParent>();
    world.entity_mut(second).insert(WorkspaceMember(2));
    assert!(key(&world, first) < key(&world, second));
    world.despawn(second);
    assert_eq!(key(&world, first).active, (false, 0));
}

#[test]
fn decorations_stay_with_their_actual_face_without_reordering_unrelated_transparent_content() {
    use bevy::{
        core_pipeline::core_3d::TransparentSortingInfo3d,
        render::{render_phase::{DrawFunctionId, PhaseItemExtraIndex, SortedRenderPhase}, render_resource::CachedRenderPipelineId},
    };
    let mut world = World::new();
    let keys: Vec<_> = (0..8).map(|_| {
        let entity = world.spawn_empty().id();
        (entity, MainEntity::from(entity))
    }).collect();
    let mut phase = SortedRenderPhase::<Transparent3d>::default();
    for (index, key) in keys.iter().enumerate() {
        phase.add_retained(Transparent3d {
            sorting_info: TransparentSortingInfo3d::AlwaysOnTop, distance: index as f32,
            pipeline: CachedRenderPipelineId::INVALID, entity: *key, draw_function: DrawFunctionId(index as u32),
            batch_range: index as u32..index as u32 + 1, extra_index: PhaseItemExtraIndex::None, indexed: false,
        });
    }
    let anchors = HashMap::from([
        (keys[0].1.id(), (keys[4].1.id(), -1)),
        (keys[3].1.id(), (keys[4].1.id(), 1)),
        (keys[5].1.id(), (keys[2].1.id(), -1)),
        (keys[7].1.id(), (keys[2].1.id(), 1)),
    ]);
    let originals = vec![keys[1], keys[2], keys[4], keys[6]];
    group_decorations(&mut phase, &anchors);
    assert_eq!(phase.items.keys().copied().collect::<Vec<_>>(), [1, 5, 2, 7, 0, 4, 3, 6].map(|index| keys[index]).to_vec());
    assert_eq!(phase.items.keys().filter(|key| !anchors.contains_key(&key.1.id())).copied().collect::<Vec<_>>(), originals);
    let once = phase.items.keys().copied().collect::<Vec<_>>();
    group_decorations(&mut phase, &anchors);
    assert_eq!(phase.items.keys().copied().collect::<Vec<_>>(), once);
    group_decorations(&mut phase, &HashMap::new());
    assert_eq!(phase.items.keys().copied().collect::<Vec<_>>(), once);
}
