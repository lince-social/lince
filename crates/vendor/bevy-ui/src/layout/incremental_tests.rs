use super::*;
use crate::{
    CalculatedClip, ComputedStackIndex, GlobalZIndex, OverrideClip, UiRect, UiStack, Val, ZIndex,
};
use bevy_app::{App, PostUpdate, TaskPoolPlugin};
use bevy_ecs::prelude::*;
use bevy_math::{BVec2, UVec2};

struct Scene {
    app: App,
    roots: Vec<Entity>,
    leaves: Vec<Entity>,
    transient: Option<Entity>,
}

fn target() -> ComputedUiRenderTargetInfo {
    ComputedUiRenderTargetInfo {
        physical_size: UVec2::new(1280, 720),
        scale_factor: 1.25,
    }
}

fn scene(reference: bool, roots: usize, children: usize) -> Scene {
    let mut app = App::new();
    app.add_plugins(TaskPoolPlugin::default())
        .init_resource::<UiSurface>()
        .init_resource::<UiStack>()
        .init_resource::<FontCx>();
    if reference {
        app.add_systems(
            PostUpdate,
            (
                reference::ui_layout_system,
                crate::stack::reference_ui_stack_system,
                crate::update::reference_clipping_system,
            )
                .chain(),
        );
    } else {
        app.add_systems(
            PostUpdate,
            (
                ui_layout_system,
                crate::stack::ui_stack_system,
                crate::update::update_clipping_system,
            )
                .chain(),
        );
    }
    let mut scene = Scene {
        app,
        roots: Vec::new(),
        leaves: Vec::new(),
        transient: None,
    };
    for root in 0..roots {
        let entity = scene
            .app
            .world_mut()
            .spawn((
                Node {
                    width: Val::Px(300.0 + root as f32),
                    height: Val::Px(220.0),
                    flex_direction: crate::FlexDirection::Column,
                    overflow: crate::Overflow::scroll(),
                    padding: UiRect::all(Val::Px(7.5)),
                    row_gap: Val::Px(2.5),
                    ..Default::default()
                },
                target(),
                GlobalZIndex(root as i32),
            ))
            .id();
        scene.roots.push(entity);
        for child in 0..children {
            let leaf = scene
                .app
                .world_mut()
                .spawn((
                    Node {
                        min_width: Val::Px(20.0),
                        height: Val::Px(30.0 + (child % 3) as f32),
                        flex_shrink: 0.0,
                        margin: UiRect::left(Val::Px((child % 5) as f32)),
                        ..Default::default()
                    },
                    target(),
                    ContentSize::fixed_size(Vec2::new(80.0, 22.0)),
                    ChildOf(entity),
                ))
                .id();
            scene.leaves.push(leaf);
        }
    }
    scene
}

fn equivalent(candidate: &mut Scene, reference: &mut Scene, step: usize) {
    candidate.app.update();
    let world = reference.app.world_mut();
    for mut node in world.query::<&mut Node>().iter_mut(world) {
        node.set_changed();
    }
    for mut children in world.query::<&mut Children>().iter_mut(world) {
        children.set_changed();
    }
    reference.app.update();
    let expected: Vec<_> = reference
        .app
        .world_mut()
        .query::<(Entity, &ComputedNode, &UiGlobalTransform)>()
        .iter(reference.app.world())
        .map(|(entity, node, transform)| (entity, *node, *transform))
        .collect();
    for (entity, node, transform) in expected {
        assert_eq!(
            candidate.app.world().get::<ComputedNode>(entity),
            Some(&node),
            "Layout differs at step {step}, entity {entity:?}"
        );
        assert_eq!(
            candidate.app.world().get::<UiGlobalTransform>(entity),
            Some(&transform),
            "Transform differs at step {step}, entity {entity:?}"
        );
        assert_eq!(
            candidate
                .app
                .world()
                .get::<CalculatedClip>(entity)
                .map(|clip| clip.clip),
            reference
                .app
                .world()
                .get::<CalculatedClip>(entity)
                .map(|clip| clip.clip),
            "Clip differs at step {step}, entity {entity:?}"
        );
        assert_eq!(
            candidate
                .app
                .world()
                .get::<ComputedStackIndex>(entity)
                .map(|index| index.0),
            reference
                .app
                .world()
                .get::<ComputedStackIndex>(entity)
                .map(|index| index.0),
            "Stack index differs at step {step}, entity {entity:?}"
        );
    }
    assert_eq!(
        candidate.app.world().resource::<UiStack>().uinodes,
        reference.app.world().resource::<UiStack>().uinodes,
        "Stack differs at step {step}"
    );
}

#[test]
fn unchanged_trees_do_no_geometry_work_and_one_changed_tree_does_not_walk_its_neighbours() {
    let mut scene = scene(false, 32, 64);
    scene.app.update();
    let count = scene.app.world().resource::<UiSurface>().geometry_visits;
    let stack_tick = scene
        .app
        .world()
        .get_resource_ref::<UiStack>()
        .unwrap()
        .last_changed();
    assert_eq!(count, 32 * 65);
    for _ in 0..20 {
        scene.app.update();
    }
    assert_eq!(
        scene.app.world().resource::<UiSurface>().geometry_visits,
        count
    );
    assert_eq!(
        scene
            .app
            .world()
            .get_resource_ref::<UiStack>()
            .unwrap()
            .last_changed(),
        stack_tick
    );
    scene
        .app
        .world_mut()
        .get_mut::<Node>(scene.leaves[2])
        .unwrap()
        .height = Val::Px(77.0);
    scene.app.update();
    assert_eq!(
        scene.app.world().resource::<UiSurface>().geometry_visits,
        count + 65
    );
    assert_eq!(
        scene
            .app
            .world()
            .get::<ComputedNode>(scene.leaves[2])
            .unwrap()
            .size
            .y,
        96.0
    );
    scene.app.update();
    assert_eq!(
        scene.app.world().resource::<UiSurface>().geometry_visits,
        count + 65
    );
}

#[test]
fn geometry_changes_match_full_traversal_inside_nested_branches() {
    let mut candidate = scene(false, 1, 8);
    let mut reference = scene(true, 1, 8);
    let mut nested = Vec::new();
    for scene in [&mut candidate, &mut reference] {
        let mut children = Vec::new();
        for &branch in &scene.leaves {
            for _ in 0..32 {
                children.push(
                    scene
                        .app
                        .world_mut()
                        .spawn((
                            Node {
                                width: Val::Px(13.25),
                                height: Val::Px(12.75),
                                flex_shrink: 0.0,
                                ..Default::default()
                            },
                            target(),
                            ChildOf(branch),
                        ))
                        .id(),
                );
            }
        }
        nested.push(children);
    }
    equivalent(&mut candidate, &mut reference, 0);
    for step in 0..9 {
        for (scene, children) in [&mut candidate, &mut reference].into_iter().zip(&nested) {
            let world = scene.app.world_mut();
            let branch = scene.leaves[2];
            let leaf = children[65];
            match step {
                0 => {
                    world.entity_mut(leaf).insert(Outline {
                        width: Val::Px(3.5),
                        ..Default::default()
                    });
                }
                1 => {
                    world
                        .entity_mut(branch)
                        .insert(UiTransform::from_translation(crate::Val2::px(2.25, -3.5)));
                }
                2 => {
                    world.entity_mut(branch).insert(LayoutConfig {
                        use_rounding: false,
                    });
                }
                3 => {
                    world.entity_mut(branch).remove::<LayoutConfig>();
                }
                4 => {
                    world.get_mut::<ComputedNode>(leaf).unwrap().size = Vec2::splat(800.0);
                }
                5 => {
                    world.entity_mut(leaf).insert(UiGlobalTransform::from(
                        Affine2::from_translation(Vec2::splat(900.0)),
                    ));
                }
                6 => {
                    world
                        .entity_mut(scene.roots[0])
                        .insert(ScrollPosition(Vec2::new(0.0, 31.5)));
                }
                7 => {
                    world.get_mut::<Node>(leaf).unwrap().height = Val::Px(27.25);
                }
                8 => {
                    world.entity_mut(leaf).insert(ChildOf(scene.leaves[3]));
                }
                _ => unreachable!(),
            }
        }
        equivalent(&mut candidate, &mut reference, step + 1);
        let surface = candidate.app.world().resource::<UiSurface>();
        let stable = surface.geometry_visits;
        equivalent(&mut candidate, &mut reference, step + 1);
        assert_eq!(
            candidate
                .app
                .world()
                .resource::<UiSurface>()
                .geometry_visits,
            stable
        );
    }
}

#[test]
fn layout_diagnostics_count_work_without_changing_geometry() {
    static READINGS: std::sync::Mutex<Vec<UiLayoutReading>> = std::sync::Mutex::new(Vec::new());
    let mut candidate = scene(false, 2, 3);
    let mut reference = scene(true, 2, 3);
    candidate
        .app
        .insert_resource(UiLayoutDiagnostics(|reading| {
            READINGS.lock().unwrap().push(reading);
        }));
    equivalent(&mut candidate, &mut reference, 0);
    equivalent(&mut candidate, &mut reference, 1);
    for scene in [&mut candidate, &mut reference] {
        scene
            .app
            .world_mut()
            .get_mut::<Node>(scene.leaves[1])
            .unwrap()
            .height = Val::Px(51.0);
    }
    equivalent(&mut candidate, &mut reference, 2);
    let readings = READINGS.lock().unwrap();
    assert_eq!(readings.len(), 3);
    assert_eq!(readings[0].nodes, 8);
    assert_eq!(readings[0].styles, 8);
    assert_eq!(readings[0].targets, 8);
    assert_eq!(readings[0].roots, 2);
    assert_eq!(readings[0].geometry_nodes, 8);
    assert_eq!(readings[1].nodes, 8);
    assert_eq!(readings[1].styles, 0);
    assert_eq!(readings[1].roots, 0);
    assert_eq!(readings[1].geometry_nodes, 0);
    assert_eq!(readings[1].computation, std::time::Duration::ZERO);
    assert_eq!(readings[1].geometry, std::time::Duration::ZERO);
    assert_eq!(readings[2].styles, 1);
    assert_eq!(readings[2].roots, 1);
    assert_eq!(readings[2].geometry_nodes, 4);
}

#[test]
fn equal_intrinsic_measurements_do_no_layout_work_and_real_changes_match_full_traversal() {
    let mut candidate = scene(false, 1, 1);
    let mut reference = scene(true, 1, 1);
    for scene in [&mut candidate, &mut reference] {
        let mut node = scene
            .app
            .world_mut()
            .get_mut::<Node>(scene.leaves[0])
            .unwrap();
        node.height = Val::Auto;
        node.align_self = crate::AlignSelf::Start;
    }
    equivalent(&mut candidate, &mut reference, 0);
    for step in 0..21 {
        let visits = candidate
            .app
            .world()
            .resource::<UiSurface>()
            .geometry_visits;
        for scene in [&mut candidate, &mut reference] {
            let leaf = scene.leaves[0];
            let world = scene.app.world_mut();
            let fixed = |size| crate::NodeMeasure::Fixed(crate::FixedMeasure { size });
            let image = |size, visual_box| {
                crate::NodeMeasure::Image(crate::widget::ImageMeasure { size, visual_box })
            };
            match step {
                0 => world
                    .get_mut::<ContentSize>(leaf)
                    .unwrap()
                    .set(fixed(Vec2::new(80.0, 22.0))),
                1 => world
                    .get_mut::<ContentSize>(leaf)
                    .unwrap()
                    .set(fixed(Vec2::new(90.0, 25.0))),
                2 | 3 => world
                    .get_mut::<ContentSize>(leaf)
                    .unwrap()
                    .set(image(Vec2::new(80.0, 22.0), crate::VisualBox::ContentBox)),
                4 => world
                    .get_mut::<ContentSize>(leaf)
                    .unwrap()
                    .set(image(Vec2::new(80.0, 22.0), crate::VisualBox::PaddingBox)),
                5 => world
                    .get_mut::<ContentSize>(leaf)
                    .unwrap()
                    .set(image(Vec2::new(95.0, 34.0), crate::VisualBox::PaddingBox)),
                6 | 7 | 14 | 15 => world.get_mut::<ContentSize>(leaf).unwrap().clear(),
                8 => world
                    .get_mut::<ContentSize>(leaf)
                    .unwrap()
                    .set(fixed(Vec2::new(80.0, 22.0))),
                9 | 10 => {
                    world
                        .get_mut::<ContentSize>(leaf)
                        .unwrap()
                        .set(crate::NodeMeasure::Custom(Box::new(crate::FixedMeasure {
                            size: Vec2::new(83.0, 27.0),
                        })))
                }
                11 => world.get_mut::<Node>(leaf).unwrap().margin = UiRect::all(Val::Px(2.5)),
                12 => {
                    for mut target in world
                        .query::<&mut ComputedUiRenderTargetInfo>()
                        .iter_mut(world)
                    {
                        target.scale_factor = 1.5;
                    }
                }
                13 => {
                    world.get_mut::<Node>(leaf).unwrap().border_radius =
                        crate::BorderRadius::all(Val::Px(3.0))
                }
                16..=20 => {
                    world
                        .get_mut::<ContentSize>(leaf)
                        .unwrap()
                        .set(crate::NodeMeasure::TextInput(
                            crate::widget::TextInputMeasure {
                                width: (step >= 18).then_some(80.0),
                                height: Some(if step == 20 { 24.0 } else { 12.0 }),
                            },
                        ))
                }
                _ => unreachable!(),
            }
        }
        equivalent(&mut candidate, &mut reference, step + 1);
        assert_eq!(
            candidate
                .app
                .world()
                .resource::<UiSurface>()
                .geometry_visits
                - visits,
            if matches!(step, 0 | 3 | 7 | 15 | 17 | 19) {
                0
            } else {
                2
            },
            "Unexpected work for intrinsic measurement change {step}"
        );
        equivalent(&mut candidate, &mut reference, step + 1);
    }
}

#[test]
fn incremental_geometry_matches_the_original_across_layout_and_hierarchy_transitions() {
    let mut candidate = scene(false, 4, 12);
    let mut reference = scene(true, 4, 12);
    for scene in [&mut candidate, &mut reference] {
        for index in 0..3 {
            let world = scene.app.world_mut();
            let parent = world
                .spawn((
                    Node {
                        width: Val::Percent(72.5),
                        min_height: Val::Px(40.0),
                        padding: UiRect::all(Val::Px(3.25)),
                        flex_direction: crate::FlexDirection::Column,
                        ..Default::default()
                    },
                    target(),
                    ChildOf(scene.leaves[3 + index]),
                ))
                .id();
            world
                .entity_mut(scene.leaves[20 + index])
                .insert(ChildOf(parent));
        }
    }
    equivalent(&mut candidate, &mut reference, 0);
    for step in 0..320 {
        for scene in [&mut candidate, &mut reference] {
            let world = scene.app.world_mut();
            let root = scene.roots[0];
            let leaf = scene.leaves[1];
            let other = scene.roots[1];
            match step % 40 {
                0 => world.get_mut::<Node>(root).unwrap().width = Val::Px(281.0 + step as f32),
                1 => {
                    world
                        .entity_mut(leaf)
                        .insert(ContentSize::fixed_size(Vec2::new(
                            390.0 + step as f32,
                            10.0,
                        )));
                }
                2 => {
                    world
                        .entity_mut(root)
                        .insert(ScrollPosition(Vec2::new(17.5, 35.0)));
                }
                3 => {
                    world.entity_mut(root).remove::<ScrollPosition>();
                }
                4 => {
                    world
                        .entity_mut(leaf)
                        .insert(UiTransform::from_translation(crate::Val2::px(7.0, -13.0)));
                }
                5 => {
                    world.entity_mut(root).insert(LayoutConfig {
                        use_rounding: false,
                    });
                }
                6 => {
                    world.entity_mut(leaf).insert(Outline {
                        width: Val::Px(4.0),
                        offset: Val::Px(2.0),
                        ..Default::default()
                    });
                }
                7 => {
                    world.entity_mut(leaf).remove::<Outline>();
                }
                8 => {
                    world.entity_mut(leaf).insert(ChildOf(other));
                }
                9 => {
                    world.entity_mut(leaf).insert(ChildOf(root));
                }
                10 => {
                    world.entity_mut(leaf).remove::<Node>();
                }
                11 => {
                    world.entity_mut(leaf).insert(Node {
                        width: Val::Percent(50.0),
                        height: Val::Px(29.5),
                        ..Default::default()
                    });
                }
                12 => {
                    world.entity_mut(leaf).remove::<ChildOf>();
                }
                13 => {
                    world.entity_mut(leaf).insert(ChildOf(root));
                }
                14 => {
                    for mut target in world
                        .query::<&mut ComputedUiRenderTargetInfo>()
                        .iter_mut(world)
                    {
                        target.scale_factor += 0.125;
                        target.physical_size.x += 16;
                    }
                }
                15 => world.get_mut::<Node>(root).unwrap().display = Display::None,
                16 => world.get_mut::<Node>(root).unwrap().display = Display::Flex,
                17 => {
                    scene.transient = Some(
                        world
                            .spawn((
                                Node {
                                    width: Val::Percent(65.0),
                                    height: Val::Px(60.0),
                                    ..Default::default()
                                },
                                target(),
                                ChildOf(root),
                            ))
                            .id(),
                    );
                }
                18 => {
                    world.despawn(scene.transient.take().unwrap());
                }
                19 => {
                    world.entity_mut(leaf).insert(IgnoreScroll(BVec2::TRUE));
                }
                20 => {
                    world.entity_mut(leaf).remove::<IgnoreScroll>();
                }
                21 => world.get_mut::<Node>(root).unwrap().padding = UiRect::all(Val::Px(13.0)),
                22 => world.get_mut::<ComputedNode>(leaf).unwrap().size = Vec2::splat(1234.0),
                23 => {
                    world.entity_mut(root).remove::<LayoutConfig>();
                }
                24 => {
                    world.entity_mut(leaf).insert(UiGlobalTransform::from(
                        Affine2::from_translation(Vec2::splat(818.0)),
                    ));
                }
                25 => {
                    world.get_mut::<Node>(root).unwrap().flex_direction = crate::FlexDirection::Row
                }
                26 => {
                    world.entity_mut(root).insert(ChildOf(scene.roots[3]));
                }
                27 => {
                    world.entity_mut(root).remove::<ChildOf>();
                }
                28 => {
                    let mut children: Vec<_> =
                        world.get::<Children>(root).unwrap().iter().collect();
                    children.reverse();
                    world.entity_mut(root).replace_children(&children);
                }
                29 => {
                    world.get_mut::<Node>(root).unwrap().flex_direction =
                        crate::FlexDirection::Column
                }
                30 => world.get_mut::<Node>(root).unwrap().border = UiRect::all(Val::Px(3.75)),
                31 => world.get_mut::<Node>(leaf).unwrap().width = Val::Percent(34.5),
                32 => {
                    world.entity_mut(leaf).insert(GlobalZIndex(100));
                }
                33 => {
                    world.entity_mut(leaf).remove::<GlobalZIndex>();
                }
                34 => {
                    world.entity_mut(leaf).insert(ZIndex(-10));
                }
                35 => {
                    world.entity_mut(leaf).remove::<ZIndex>();
                }
                36 => {
                    world.entity_mut(leaf).insert(OverrideClip);
                }
                37 => {
                    world.entity_mut(leaf).remove::<OverrideClip>();
                }
                38 => {
                    world.entity_mut(leaf).insert(CalculatedClip {
                        clip: bevy_math::Rect::from_corners(Vec2::ZERO, Vec2::splat(12.0)),
                    });
                }
                39 => {
                    world.entity_mut(leaf).remove::<CalculatedClip>();
                }
                _ => unreachable!(),
            };
        }
        equivalent(&mut candidate, &mut reference, step + 1);
        equivalent(&mut candidate, &mut reference, step + 1);
    }
}

#[test]
fn disabled_tree_resumes_with_current_sizes_and_a_replaced_surface_rebuilds() {
    let mut scene = scene(false, 2, 8);
    scene.app.update();
    let root = scene.roots[0];
    let leaf = scene.leaves[0];
    scene.app.world_mut().entity_mut(root).insert(Disabled);
    scene.app.update();
    scene.app.world_mut().get_mut::<Node>(leaf).unwrap().height = Val::Px(64.0);
    for _ in 0..3 {
        scene.app.update();
    }
    scene.app.world_mut().entity_mut(root).remove::<Disabled>();
    scene.app.update();
    assert_eq!(
        scene.app.world().get::<ComputedNode>(leaf).unwrap().size.y,
        80.0
    );
    scene.app.world_mut().insert_resource(UiSurface::default());
    scene.app.update();
    assert_eq!(
        scene.app.world().resource::<UiSurface>().geometry_visits,
        18
    );
    assert_eq!(
        scene.app.world().get::<ComputedNode>(leaf).unwrap().size.y,
        80.0
    );
}

#[cfg(feature = "ghost_nodes")]
#[test]
fn ghost_branch_insertion_removal_and_reparenting_match_the_original() {
    let mut candidate = scene(false, 3, 8);
    let mut reference = scene(true, 3, 8);
    let mut ghosts = Vec::new();
    for scene in [&mut candidate, &mut reference] {
        let world = scene.app.world_mut();
        let ghost = world.spawn((GhostNode, ChildOf(scene.roots[0]))).id();
        let nested = world.spawn((GhostNode, ChildOf(ghost))).id();
        world.entity_mut(scene.leaves[0]).insert(ChildOf(nested));
        ghosts.push((ghost, nested));
    }
    equivalent(&mut candidate, &mut reference, 0);
    for step in 0..15 {
        for (scene, (ghost, nested)) in [&mut candidate, &mut reference].into_iter().zip(&ghosts) {
            let world = scene.app.world_mut();
            match step {
                0 => {
                    world.entity_mut(*ghost).insert(ChildOf(scene.roots[1]));
                }
                1 => {
                    world.entity_mut(*nested).remove::<GhostNode>();
                }
                2 => {
                    world.entity_mut(*nested).insert(GhostNode);
                }
                3 => {
                    world.entity_mut(scene.leaves[1]).insert(ChildOf(*nested));
                }
                4 => {
                    world.entity_mut(*ghost).remove::<ChildOf>();
                }
                5 => {
                    world
                        .entity_mut(*nested)
                        .insert((Node::default(), target()))
                        .remove::<GhostNode>();
                }
                6 => {
                    world.entity_mut(*ghost).insert(ChildOf(scene.roots[2]));
                }
                7 => {
                    world.entity_mut(*nested).remove::<Node>().insert(GhostNode);
                }
                8 => {
                    world.entity_mut(*ghost).remove::<ChildOf>().insert((
                        Node {
                            width: Val::Px(200.0),
                            height: Val::Px(130.0),
                            ..Default::default()
                        },
                        target(),
                    ));
                }
                9 => {
                    world.get_mut::<Node>(*ghost).unwrap().height = Val::Px(250.0);
                }
                10 => {
                    world.entity_mut(*ghost).remove::<Node>();
                }
                11 => {
                    world
                        .entity_mut(scene.leaves[0])
                        .insert(UiTransform::from_translation(crate::Val2::px(3.25, -5.75)));
                }
                12 => {
                    world.entity_mut(scene.leaves[1]).insert(LayoutConfig {
                        use_rounding: false,
                    });
                }
                13 => {
                    world.entity_mut(scene.leaves[1]).remove::<LayoutConfig>();
                }
                14 => {
                    world.get_mut::<ComputedNode>(scene.leaves[0]).unwrap().size =
                        Vec2::splat(712.0);
                }
                _ => unreachable!(),
            }
        }
        equivalent(&mut candidate, &mut reference, step + 1);
    }
}
