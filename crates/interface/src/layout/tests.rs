use super::*;
use crate::{canvas::CanvasItem, sand_text::SandText, workspace::WorkspaceMember};
use bevy::{math::DVec2, text::TextLayoutInfo, ui::widget::TextScroll};

fn canvas(world: &mut World, root: Entity, size: Vec2) -> Entity {
    world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size,
            },
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id()
}

fn fit(size: Vec2) -> Rules {
    let mut rules = Rules::fixed(size);
    for axis in &mut rules.axes {
        axis.sizing = Sizing::Fit;
        axis.min = 24.0;
    }
    rules
}

fn text(world: &mut World, parent: Entity, editable: bool, measured: Vec2) -> Entity {
    let mut text = SandText::new(editable);
    text.offset = [0.0; 2];
    text.size = [80.0, 24.0];
    world
        .spawn((
            text,
            Text::new("Title"),
            TextScroll::default(),
            TextLayoutInfo {
                size: measured,
                ..default()
            },
            Node::default(),
            ChildOf(parent),
        ))
        .id()
}

#[test]
fn title_growth_reaches_nested_squares_and_scroll_stops_it_on_each_axis() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let outer = canvas(&mut world, root, Vec2::splat(80.0));
    let inner = canvas(&mut world, root, Vec2::splat(80.0));
    configure(&mut world, outer, fit(Vec2::splat(80.0))).unwrap();
    configure(&mut world, inner, fit(Vec2::splat(80.0))).unwrap();
    attach(&mut world, inner, outer).unwrap();
    let title = text(&mut world, inner, true, Vec2::new(320.0, 240.0));
    let mut rules = fit(Vec2::new(80.0, 24.0));
    rules.wrap = false;
    configure(&mut world, title, rules).unwrap();
    engine::resolve(&mut world);
    for entity in [inner, outer] {
        assert_eq!(
            world.get::<CanvasItem>(entity).unwrap().size,
            Vec2::new(320.0, 240.0)
        );
    }
    for axis in 0..2 {
        rules.axes[axis].sizing = Sizing::Fixed;
        rules.axes[axis].overflow = Overflow::Scroll;
    }
    configure(&mut world, title, rules).unwrap();
    engine::resolve(&mut world);
    for entity in [inner, outer] {
        assert_eq!(
            world.get::<CanvasItem>(entity).unwrap().size,
            Vec2::new(80.0, 24.0)
        );
    }
    world.get_mut::<TextLayoutInfo>(title).unwrap().size = Vec2::splat(4000.0);
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(outer).unwrap().size,
        Vec2::new(80.0, 24.0)
    );
    assert_eq!(
        world.get::<LayoutRuntime>(title).unwrap().content,
        Vec2::splat(4000.0)
    );
}

#[test]
fn static_text_fits_shrinks_and_stops_at_maximum_without_moving_top_left() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let square = canvas(&mut world, root, Vec2::splat(80.0));
    configure(&mut world, square, fit(Vec2::splat(80.0))).unwrap();
    let title = text(&mut world, square, false, Vec2::new(400.0, 240.0));
    let mut rules = fit(Vec2::new(80.0, 24.0));
    rules.axes[0].max = 200.0;
    rules.axes[0].overflow = Overflow::Scroll;
    configure(&mut world, title, rules).unwrap();
    let before = *world.get::<CanvasItem>(square).unwrap();
    engine::resolve(&mut world);
    let grown = *world.get::<CanvasItem>(square).unwrap();
    assert_eq!(grown.size, Vec2::new(200.0, 240.0));
    assert_eq!(
        grown.position - grown.size.as_dvec2() * 0.5,
        before.position - before.size.as_dvec2() * 0.5
    );
    world.get_mut::<TextLayoutInfo>(title).unwrap().size = Vec2::splat(12.0);
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(square).unwrap().size,
        Vec2::splat(24.0)
    );
}

#[test]
fn scroll_hands_remaining_distance_to_parent_and_clamps_both_directions() {
    let mut rules = Rules::fixed(Vec2::splat(100.0));
    for axis in &mut rules.axes {
        axis.overflow = Overflow::Scroll;
    }
    let mut child = LayoutRuntime {
        size: Vec2::splat(100.0),
        content: Vec2::new(120.0, 150.0),
        ..default()
    };
    let mut parent = LayoutRuntime {
        size: Vec2::splat(100.0),
        content: Vec2::splat(500.0),
        ..default()
    };
    let remaining = engine::consume_scroll(rules, &mut child, Vec2::new(60.0, 90.0));
    assert_eq!(child.scroll, Vec2::new(20.0, 50.0));
    assert_eq!(
        engine::consume_scroll(rules, &mut parent, remaining),
        Vec2::ZERO
    );
    assert_eq!(parent.scroll, Vec2::splat(40.0));
    let remaining = engine::consume_scroll(rules, &mut child, Vec2::splat(-200.0));
    assert_eq!(child.scroll, Vec2::ZERO);
    assert_eq!(
        engine::consume_scroll(rules, &mut parent, remaining),
        Vec2::new(-140.0, -110.0)
    );
    assert_eq!(parent.scroll, Vec2::ZERO);
}

#[test]
fn property_scroll_consumes_space_before_scrolling_its_area() {
    let mut world = World::new();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.add_observer(engine::scroll);
    let root = world.spawn_empty().id();
    let area = canvas(&mut world, root, Vec2::splat(100.0));
    let mut rules = Rules::fixed(Vec2::splat(100.0));
    rules.axes[1].overflow = Overflow::Scroll;
    configure(&mut world, area, rules).unwrap();
    world.entity_mut(area).insert(LayoutRuntime {
        size: Vec2::splat(100.0),
        content: Vec2::splat(500.0),
        ..default()
    });
    let property = world
        .spawn((
            Node {
                overflow: bevy::ui::Overflow::scroll_y(),
                ..default()
            },
            ComputedNode {
                size: Vec2::splat(100.0),
                content_size: Vec2::new(100.0, 150.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
            ScrollPosition::default(),
            ChildOf(area),
        ))
        .id();
    for (delta, inner, outer) in [(-90.0, 50.0, 40.0), (200.0, 0.0, 0.0)] {
        world.trigger(Pointer::new(
            bevy::picking::pointer::PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: bevy::camera::NormalizedRenderTarget::Image(
                    Handle::<Image>::default().into(),
                ),
                position: Vec2::ZERO,
            },
            bevy::picking::events::Scroll {
                unit: bevy::input::mouse::MouseScrollUnit::Pixel,
                x: 0.0,
                y: delta,
                phase: bevy::input::touch::TouchPhase::Moved,
                hit: bevy::picking::backend::HitData::new(root, 0.0, None, None),
            },
            property,
        ));
        assert_eq!(world.get::<ScrollPosition>(property).unwrap().0.y, inner);
        assert_eq!(world.get::<LayoutRuntime>(area).unwrap().scroll.y, outer);
    }
}

#[test]
fn columns_grids_and_fill_use_visible_sizes_and_do_not_create_growth_loops() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let parent = canvas(&mut world, root, Vec2::new(300.0, 200.0));
    let mut rules = Rules::fixed(Vec2::new(300.0, 200.0));
    rules.arrangement = Arrangement::Column;
    rules.padding = 10.0;
    rules.gap = 8.0;
    configure(&mut world, parent, rules).unwrap();
    let mut children = Vec::new();
    for _ in 0..4 {
        let child = canvas(&mut world, root, Vec2::new(100.0, 40.0));
        let mut rules = Rules::fixed(Vec2::new(100.0, 40.0));
        rules.axes[0].sizing = Sizing::Fill;
        configure(&mut world, child, rules).unwrap();
        attach(&mut world, child, parent).unwrap();
        children.push(child);
    }
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(children[0]).unwrap().size,
        Vec2::new(280.0, 40.0)
    );
    assert_eq!(
        world.get::<CanvasItem>(children[1]).unwrap().position.y
            - world.get::<CanvasItem>(children[0]).unwrap().position.y,
        48.0
    );
    assert_eq!(world.get::<LayoutRuntime>(parent).unwrap().content.y, 204.0);
    rules.arrangement = Arrangement::Grid;
    configure(&mut world, parent, rules).unwrap();
    engine::resolve(&mut world);
    assert_eq!(world.get::<CanvasItem>(children[0]).unwrap().size.x, 136.0);
    assert_eq!(
        world.get::<CanvasItem>(children[0]).unwrap().position.y,
        world.get::<CanvasItem>(children[1]).unwrap().position.y
    );
    assert_eq!(
        world.get::<CanvasItem>(children[2]).unwrap().position.y
            - world.get::<CanvasItem>(children[0]).unwrap().position.y,
        48.0
    );
    rules.axes[0].sizing = Sizing::Fit;
    configure(&mut world, parent, rules).unwrap();
    engine::resolve(&mut world);
    let size = world.get::<CanvasItem>(parent).unwrap().size;
    for _ in 0..20 {
        engine::resolve(&mut world);
        assert_eq!(world.get::<CanvasItem>(parent).unwrap().size, size);
    }
}

#[test]
fn parent_links_reject_cycles_and_cross_workspace_membership() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let a = canvas(&mut world, root, Vec2::splat(80.0));
    let b = canvas(&mut world, root, Vec2::splat(80.0));
    let c = canvas(&mut world, root, Vec2::splat(80.0));
    attach(&mut world, b, a).unwrap();
    attach(&mut world, c, b).unwrap();
    assert!(attach(&mut world, a, c).is_err());
    assert!(attach(&mut world, a, a).is_err());
    world.entity_mut(c).insert(WorkspaceMember(2));
    assert!(attach(&mut world, c, a).is_err());
    engine::resolve(&mut world);
    assert_eq!(world.get::<LayoutRuntime>(c).unwrap().parent, None);
    world.despawn(a);
    engine::resolve(&mut world);
    assert_eq!(world.get::<LayoutRuntime>(b).unwrap().parent, None);
}

#[test]
fn saved_placement_reconnects_layouts_after_entities_change() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let a = canvas(&mut world, root, Vec2::splat(80.0));
    let b = canvas(&mut world, root, Vec2::splat(80.0));
    attach(&mut world, b, a).unwrap();
    let saved: Vec<_> = [a, b]
        .map(|entity| {
            serde_json::to_string(&crate::sand_placement::Placement::capture(&world, entity))
                .unwrap()
        })
        .into();
    world.despawn(a);
    world.despawn(b);
    let restored: Vec<_> = saved
        .into_iter()
        .map(|saved| {
            let entity = canvas(&mut world, root, Vec2::splat(80.0));
            let placement: crate::sand_placement::Placement = serde_json::from_str(&saved).unwrap();
            assert!(placement.valid());
            placement.restore(&mut world, entity);
            entity
        })
        .collect();
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<LayoutRuntime>(restored[1]).unwrap().parent,
        Some(restored[0])
    );
}

#[test]
fn invalid_limits_and_nonfinite_values_are_rejected_before_changes() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let entity = canvas(&mut world, root, Vec2::splat(80.0));
    for value in [f32::NAN, f32::INFINITY, -1.0, 100_001.0] {
        let mut rules = Rules::fixed(Vec2::splat(80.0));
        rules.axes[0].max = value;
        assert!(configure(&mut world, entity, rules).is_err());
        assert!(world.get::<LayoutBox>(entity).is_none());
    }
}

#[test]
fn large_nested_lists_settle_without_repeated_changes() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let parent = canvas(&mut world, root, Vec2::splat(300.0));
    let mut rules = Rules::fixed(Vec2::splat(300.0));
    rules.arrangement = Arrangement::Column;
    rules.axes[1].overflow = Overflow::Scroll;
    configure(&mut world, parent, rules).unwrap();
    for _ in 0..1024 {
        let child = canvas(&mut world, root, Vec2::splat(40.0));
        attach(&mut world, child, parent).unwrap();
    }
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<LayoutRuntime>(parent).unwrap().content.y,
        1024.0 * 48.0 - 8.0
    );
    let before: Vec<_> = world
        .query::<(Entity, &LayoutRuntime)>()
        .iter(&world)
        .map(|(entity, runtime)| (entity, *runtime))
        .collect();
    engine::resolve(&mut world);
    for (entity, runtime) in before {
        assert_eq!(*world.get::<LayoutRuntime>(entity).unwrap(), runtime);
    }
}

#[test]
fn area_containers_preserve_shape_and_fixed_viewports_scroll_their_children() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let area = crate::area::spawn_area(
        &mut world,
        root,
        1,
        crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        ),
    )
    .unwrap();
    let child = canvas(&mut world, root, Vec2::new(300.0, 50.0));
    configure(&mut world, area, fit(Vec2::splat(100.0))).unwrap();
    attach(&mut world, child, area).unwrap();
    world.get_mut::<LayoutBox>(child).unwrap().offset = [0.0; 2];
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(area).unwrap().size,
        Vec2::splat(300.0)
    );
    assert!(
        world
            .get::<crate::area::InfluenceArea>(area)
            .unwrap()
            .validate()
    );
    let mut rules = Rules::fixed(Vec2::splat(100.0));
    rules.axes[0].overflow = Overflow::Scroll;
    configure(&mut world, area, rules).unwrap();
    engine::resolve(&mut world);
    world.get_mut::<LayoutRuntime>(area).unwrap().scroll.x = 100.0;
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<LayoutRuntime>(child).unwrap().visual_offset.x,
        100.0
    );
    assert_eq!(
        world.get::<CanvasItem>(area).unwrap().size,
        Vec2::splat(100.0)
    );
    assert!(
        world
            .get::<crate::area::InfluenceArea>(area)
            .unwrap()
            .validate()
    );
}

#[test]
fn manual_resizing_updates_saved_rules_and_free_placement_offsets() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let parent = canvas(&mut world, root, Vec2::splat(300.0));
    let child = canvas(&mut world, root, Vec2::splat(100.0));
    attach(&mut world, child, parent).unwrap();
    engine::resolve(&mut world);
    let before = *world.get::<CanvasItem>(child).unwrap();
    let offset = world.get::<LayoutBox>(child).unwrap().offset;
    let after = CanvasItem {
        position: before.position + DVec2::new(30.0, 20.0),
        size: Vec2::new(140.0, 100.0),
    };
    edited(&mut world, child, before, after);
    engine::resolve(&mut world);
    assert_eq!(world.get::<CanvasItem>(child).unwrap().size, after.size);
    assert_eq!(
        world.get::<LayoutBox>(child).unwrap().offset,
        [offset[0] + 10.0, offset[1] + 20.0]
    );
}

#[test]
fn ordinary_sand_contents_scroll_and_restore_their_original_transform() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let sand = canvas(&mut world, root, Vec2::splat(100.0));
    world.get_mut::<ComputedNode>(sand).unwrap().content_size = Vec2::splat(400.0);
    let child = world
        .spawn((
            Node::default(),
            UiTransform::from_translation(bevy::ui::Val2::px(10.0, 20.0)),
            ChildOf(sand),
        ))
        .id();
    let mut rules = Rules::fixed(Vec2::splat(100.0));
    for axis in &mut rules.axes {
        axis.overflow = Overflow::Scroll;
    }
    configure(&mut world, sand, rules).unwrap();
    engine::resolve(&mut world);
    world.get_mut::<LayoutRuntime>(sand).unwrap().scroll = Vec2::splat(40.0);
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<UiTransform>(child).unwrap().translation,
        bevy::ui::Val2::px(-30.0, -20.0)
    );
    for _ in 0..5 {
        engine::resolve(&mut world);
    }
    assert_eq!(
        world.get::<UiTransform>(child).unwrap().translation,
        bevy::ui::Val2::px(-30.0, -20.0)
    );
    world.get_mut::<ComputedNode>(sand).unwrap().content_size = Vec2::splat(80.0);
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<UiTransform>(child).unwrap().translation,
        bevy::ui::Val2::px(10.0, 20.0)
    );
}

#[test]
fn protein_rows_restore_layout_by_owner_and_source_when_the_data_returns() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let owner = crate::area::spawn_area(
        &mut world,
        root,
        1,
        crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(300.0),
        ),
    )
    .unwrap();
    let row = canvas(&mut world, root, Vec2::splat(100.0));
    let binding = crate::protein_area::RecordBinding {
        area: owner,
        uid: "record-1".into(),
        source: crate::protein_area::Source::Local,
    };
    world.entity_mut(row).insert(binding.clone());
    attach(&mut world, row, owner).unwrap();
    let saved = records::snapshot(&mut world, root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
    let encoded = serde_json::to_string(&saved).unwrap();
    world.entity_mut(root).insert(records::SavedLayouts(
        serde_json::from_str(&encoded).unwrap(),
    ));
    world.despawn(row);
    let restored = canvas(&mut world, root, Vec2::splat(80.0));
    world.entity_mut(restored).insert(binding.clone());
    let remote = canvas(&mut world, root, Vec2::splat(80.0));
    world
        .entity_mut(remote)
        .insert(crate::protein_area::RecordBinding {
            source: crate::protein_area::Source::Organ("other".into()),
            ..binding
        });
    engine::resolve(&mut world);
    assert_eq!(
        world.get::<LayoutRuntime>(restored).unwrap().parent,
        Some(owner)
    );
    assert!(world.get::<LayoutBox>(remote).is_none());
}
