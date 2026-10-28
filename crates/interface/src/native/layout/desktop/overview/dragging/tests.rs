use super::*;
use bevy::picking::{backend::HitData, pointer::Location};

fn geometry(world: &mut World, entity: Entity, bounds: Rect, scale: f32) {
    world.entity_mut(entity).insert((
        ComputedNode {
            size: bounds.size() * scale,
            content_size: bounds.size() * scale,
            inverse_scale_factor: scale.recip(),
            ..default()
        },
        UiGlobalTransform::from(bevy::math::Affine2::from_translation(
            bounds.center() * scale,
        )),
        InheritedVisibility::VISIBLE,
    ));
}

fn fixture() -> (App, Entity, Entity, Entity, Entity, Entity) {
    let (mut app, root, item, editor) = super::super::tests::fixture();
    let other = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: bevy::math::DVec2::new(800.0, -200.0),
                size: Vec2::new(300.0, 200.0),
            },
            LayoutBox::new(Vec2::new(300.0, 200.0)),
            WorkspaceMember(1),
            Name::new("Neighbour"),
            ChildOf(root),
        ))
        .id();
    let mut second = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 2;
    second.name = "Destination".into();
    second.desktop = crate::layout::desktop::Workspace::new(WorkspaceLayout::Tiled);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(WorkspaceLayout::Tiled);
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(second);
    crate::native::layout::engine::resolve(app.world_mut());
    toggle(app.world_mut(), root);
    let state = app.world().resource::<Overview>().0.as_ref().unwrap();
    let id = app.world().get::<LayoutBox>(item).unwrap().id;
    let other_id = app.world().get::<LayoutBox>(other).unwrap().id;
    let (overlay, list, first, first_preview, second, second_preview, button, neighbour) = (
        state.overlay,
        state.list,
        state.cards[0].entity,
        state.cards[0].preview,
        state.cards[1].entity,
        state.cards[1].preview,
        state.cards[0].items[&id].0,
        state.cards[0].items[&other_id].0,
    );
    for (entity, bounds) in [
        (overlay, Rect::new(0.0, 0.0, 1000.0, 700.0)),
        (list, Rect::new(20.0, 100.0, 980.0, 620.0)),
        (first, Rect::new(20.0, 100.0, 980.0, 300.0)),
        (first_preview, Rect::new(20.0, 120.0, 980.0, 300.0)),
        (second, Rect::new(20.0, 320.0, 980.0, 520.0)),
        (second_preview, Rect::new(20.0, 340.0, 980.0, 520.0)),
        (button, Rect::new(100.0, 150.0, 300.0, 250.0)),
        (neighbour, Rect::new(400.0, 150.0, 600.0, 250.0)),
    ] {
        geometry(app.world_mut(), entity, bounds, 1.0);
    }
    (app, root, item, editor, button, neighbour)
}

fn pointer<E: std::fmt::Debug + Clone + bevy::reflect::Reflect>(
    world: &mut World,
    entity: Entity,
    point: Vec2,
    event: E,
) {
    world.trigger(Pointer::new(
        PointerId::Mouse,
        Location {
            target: bevy::camera::NormalizedRenderTarget::None {
                width: 1000,
                height: 700,
            },
            position: point,
        },
        event,
        entity,
    ));
    world.flush();
}

fn start(world: &mut World, root: Entity, button: Entity) {
    world.entity_mut(button).insert(bevy::ui::Pressed);
    pointer(
        world,
        button,
        Vec2::new(210.0, 200.0),
        DragStart {
            button: PointerButton::Primary,
            hit: HitData::new(root, 0.0, None, None),
        },
    );
}

#[test]
fn destination_feedback_stays_in_the_floating_preview_without_replacing_navigation_help() {
    let (mut app, root, _, _, button, _) = fixture();
    let help = app.world().resource::<Overview>().0.as_ref().unwrap().help;
    let navigation = app.world().get::<Text>(help).unwrap().0.clone();
    start(app.world_mut(), root, button);
    motion(app.world_mut(), PointerId::Mouse, Vec2::new(350.0, 420.0));
    let drag = app
        .world()
        .resource::<Overview>()
        .0
        .as_ref()
        .unwrap()
        .drag
        .as_ref()
        .unwrap();
    assert!(
        app.world()
            .get::<Text>(drag.caption)
            .unwrap()
            .0
            .contains("Destination")
    );
    assert_eq!(app.world().get::<Text>(help).unwrap().0, navigation);
    assert_eq!(
        app.world().get::<ChildOf>(drag.caption).unwrap().parent(),
        drag.ghost
    );
    assert_eq!(
        app.world().get::<Node>(drag.ghost).unwrap().position_type,
        PositionType::Absolute
    );
}

#[test]
fn pointer_drag_moves_the_captured_item_once_without_activating_the_workspace_or_losing_its_draft()
{
    let (mut app, root, item, editor, button, _) = fixture();
    let id = app.world().get::<LayoutBox>(item).unwrap().id;
    let layout = *app.world().get::<LayoutBox>(item).unwrap();
    start(app.world_mut(), root, button);
    assert!(app.world().get::<bevy::ui::Pressed>(button).is_none());
    let ghost = app
        .world()
        .resource::<Overview>()
        .0
        .as_ref()
        .unwrap()
        .drag
        .as_ref()
        .unwrap()
        .ghost;
    let end = Vec2::new(350.0, 420.0);
    pointer(
        app.world_mut(),
        button,
        end,
        Drag {
            button: PointerButton::Primary,
            distance: Vec2::new(140.0, 220.0),
            delta: Vec2::new(140.0, 220.0),
        },
    );
    assert_eq!(app.world().get::<WorkspaceMember>(item).unwrap().0, 1);
    pointer(
        app.world_mut(),
        button,
        end,
        DragEnd {
            button: PointerButton::Primary,
            distance: Vec2::new(140.0, 220.0),
        },
    );
    assert!(active(app.world()));
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
    assert_eq!(app.world().get::<WorkspaceMember>(item).unwrap().0, 2);
    assert_eq!(*app.world().get::<LayoutBox>(item).unwrap(), layout);
    assert_eq!(
        app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .selected,
        (2, Some(id))
    );
    assert!(app.world().get_entity(ghost).is_err());
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Retained draft"
    );
    super::super::update(app.world_mut());
    let state = app.world().resource::<Overview>().0.as_ref().unwrap();
    assert_eq!(
        app.world().resource::<InputFocus>().get(),
        Some(state.cards[1].items[&id].0)
    );
    pointer(
        app.world_mut(),
        button,
        end,
        DragEnd {
            button: PointerButton::Primary,
            distance: Vec2::new(140.0, 220.0),
        },
    );
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().entries[1]
            .desktop
            .items()
            .len(),
        1
    );
}

#[test]
fn row_destination_uses_rendered_edges_and_preserves_the_other_item_and_authored_home() {
    let (mut app, root, item, _, button, neighbour) = fixture();
    let id = PlacementId(app.world().get::<LayoutBox>(item).unwrap().id);
    let state = app.world().resource::<Overview>().0.as_ref().unwrap();
    let anchor = PlacementId(
        *state.cards[0]
            .items
            .iter()
            .find(|(_, (candidate, _))| *candidate == neighbour)
            .unwrap()
            .0,
    );
    let original = app.world().get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .items()
        .to_vec();
    start(app.world_mut(), root, button);
    let end = Vec2::new(500.0, 245.0);
    pointer(
        app.world_mut(),
        button,
        end,
        Drag {
            button: PointerButton::Primary,
            distance: Vec2::new(290.0, 45.0),
            delta: Vec2::new(290.0, 45.0),
        },
    );
    pointer(
        app.world_mut(),
        button,
        end,
        DragEnd {
            button: PointerButton::Primary,
            distance: Vec2::new(290.0, 45.0),
        },
    );
    let model = &app.world().get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.tiled_rows(), vec![vec![anchor], vec![id]]);
    assert_eq!(model.items(), original);
    assert_eq!(app.world().get::<WorkspaceMember>(item).unwrap().0, 1);
}

#[test]
fn cancellation_and_lost_context_retire_the_preview_without_a_late_transfer() {
    for loss in 0..5 {
        let (mut app, root, item, _, button, _) = fixture();
        start(app.world_mut(), root, button);
        let ghost = app
            .world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .drag
            .as_ref()
            .unwrap()
            .ghost;
        match loss {
            0 => pointer(
                app.world_mut(),
                button,
                Vec2::new(200.0, 200.0),
                Cancel {
                    hit: HitData::new(root, 0.0, None, None),
                },
            ),
            1 => {
                app.world_mut()
                    .get_mut::<ComputedNode>(root)
                    .unwrap()
                    .size
                    .x += 10.0;
            }
            2 => {
                app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 2;
            }
            3 => {
                let owner = app.world_mut().spawn_empty().id();
                crate::native::shortcuts::own_keyboard(app.world_mut(), owner);
            }
            _ => {
                app.world_mut().despawn(item);
            }
        }
        motion(app.world_mut(), PointerId::Mouse, Vec2::new(350.0, 420.0));
        finish(app.world_mut(), PointerId::Mouse, Vec2::new(350.0, 420.0));
        assert!(app.world().get_entity(ghost).is_err());
        assert!(
            app.world()
                .resource::<Overview>()
                .0
                .as_ref()
                .unwrap()
                .drag
                .is_none()
        );
        if loss != 4 {
            assert_eq!(app.world().get::<WorkspaceMember>(item).unwrap().0, 1);
        }
        assert!(
            app.world().get::<Workspaces>(root).unwrap().entries[1]
                .desktop
                .items()
                .is_empty()
        );
    }
}

#[test]
fn a_disappearing_drop_anchor_cancels_instead_of_becoming_a_workspace_background_drop() {
    let (mut app, root, item, _, button, neighbour) = fixture();
    start(app.world_mut(), root, button);
    motion(app.world_mut(), PointerId::Mouse, Vec2::new(500.0, 245.0));
    app.world_mut().despawn(neighbour);
    finish(app.world_mut(), PointerId::Mouse, Vec2::new(500.0, 245.0));
    assert_eq!(app.world().get::<WorkspaceMember>(item).unwrap().0, 1);
    assert!(
        app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .drag
            .is_none()
    );
    assert!(
        !app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .focus_selection
    );
}

#[test]
fn an_individually_floating_item_keeps_its_state_and_uses_the_workspace_destination() {
    let (mut app, root, item, _, button, _) = fixture();
    let id = PlacementId(app.world().get::<LayoutBox>(item).unwrap().id);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .toggle_floating(id, Rectangle::new([30.0, 40.0], [300.0, 200.0]).unwrap())
        .unwrap();
    start(app.world_mut(), root, button);
    motion(app.world_mut(), PointerId::Mouse, Vec2::new(500.0, 245.0));
    let target = app
        .world()
        .resource::<Overview>()
        .0
        .as_ref()
        .unwrap()
        .drag
        .as_ref()
        .unwrap()
        .destination
        .unwrap();
    assert!(target.slot.is_none());
    motion(app.world_mut(), PointerId::Mouse, Vec2::new(350.0, 420.0));
    finish(app.world_mut(), PointerId::Mouse, Vec2::new(350.0, 420.0));
    let spaces = app.world().get::<Workspaces>(root).unwrap();
    assert_eq!(app.world().get::<WorkspaceMember>(item).unwrap().0, 2);
    assert!(
        spaces.entries[1]
            .desktop
            .items()
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .floating
    );
}

#[test]
fn edge_scroll_clamps_scaled_content_and_returns_quietly_at_either_extent() {
    let mut world = World::new();
    let list = world
        .spawn((Node::default(), ScrollPosition::default()))
        .id();
    geometry(&mut world, list, Rect::new(10.0, 20.0, 510.0, 420.0), 2.0);
    world.get_mut::<ComputedNode>(list).unwrap().content_size.y = 1600.0;
    let settings = OverviewSettings::default();
    for _ in 0..30 {
        scroll(
            &mut world,
            list,
            Vec2::new(250.0, 419.0),
            Duration::from_millis(100),
            settings,
        );
    }
    assert_eq!(world.get::<ScrollPosition>(list).unwrap().0.y, 400.0);
    assert!(!scroll(
        &mut world,
        list,
        Vec2::new(250.0, 419.0),
        Duration::from_millis(100),
        settings
    ));
    assert!(!scroll(
        &mut world,
        list,
        Vec2::new(250.0, 200.0),
        Duration::from_millis(100),
        settings
    ));
    for _ in 0..30 {
        scroll(
            &mut world,
            list,
            Vec2::new(250.0, 21.0),
            Duration::from_millis(100),
            settings,
        );
    }
    assert_eq!(world.get::<ScrollPosition>(list).unwrap().0.y, 0.0);
    assert!(!scroll(
        &mut world,
        list,
        Vec2::new(250.0, 21.0),
        Duration::from_millis(100),
        settings
    ));
}

#[test]
fn clipped_preview_bounds_use_logical_pixels_and_never_target_invisible_content() {
    let mut world = World::new();
    let target = world.spawn(Node::default()).id();
    geometry(&mut world, target, Rect::new(20.0, 30.0, 220.0, 130.0), 1.5);
    world.entity_mut(target).insert(bevy::ui::CalculatedClip {
        clip: Rect::new(60.0, 60.0, 180.0, 180.0),
    });
    assert_eq!(
        rectangle(&world, target),
        Some(Rect::new(40.0, 40.0, 120.0, 120.0))
    );
    world.entity_mut(target).insert(InheritedVisibility::HIDDEN);
    assert_eq!(rectangle(&world, target), None);
}

#[test]
fn elapsed_scroll_keeps_the_same_velocity_across_different_render_rates() {
    for rate in [30, 60, 144] {
        let mut world = World::new();
        let list = world
            .spawn((Node::default(), ScrollPosition::default()))
            .id();
        geometry(&mut world, list, Rect::new(10.0, 20.0, 510.0, 420.0), 2.0);
        world.get_mut::<ComputedNode>(list).unwrap().content_size.y = 2400.0;
        for _ in 0..rate {
            assert!(scroll(
                &mut world,
                list,
                Vec2::new(250.0, 419.0),
                Duration::from_secs_f64(1.0 / f64::from(rate)),
                OverviewSettings::default()
            ));
        }
        assert!((world.get::<ScrollPosition>(list).unwrap().0.y - 387.5).abs() < 0.1);
    }
}
