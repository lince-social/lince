use super::*;

fn fixture(width: f32) -> (World, Entity, Entity, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    let root = world
        .spawn((
            Node::default(),
            ComputedNode {
                size: Vec2::new(width, 800.0),
                ..default()
            },
            UiGlobalTransform::from_xy(width * 0.5, 400.0),
        ))
        .id();
    let mut spawn = |kind, width| {
        let panel = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(width),
                    ..default()
                },
                ComputedNode {
                    size: Vec2::new(width, 400.0),
                    unrounded_size: Vec2::new(width, 400.0),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        attach(&mut world, panel, root, kind);
        panel
    };
    let edit = spawn(Kind::Edit, 376.0);
    let item = spawn(Kind::Item, 300.0);
    (world, root, edit, item)
}

fn rectangle(world: &World, entity: Entity) -> Rect {
    let node = world.get::<Node>(entity).unwrap();
    let (Val::Px(x), Val::Px(y), Val::Px(width), Val::Px(maximum)) =
        (node.left, node.top, node.width, node.max_width)
    else {
        panic!("positioned panel")
    };
    Rect::from_corners(Vec2::new(x, y), Vec2::new(x + width.min(maximum), y + 32.0))
}

#[test]
fn default_panels_start_apart_on_wide_and_narrow_screens_and_respect_reserved_space() {
    use crate::host::{OutputViewport, Rectangle};
    for width in [360.0, 800.0, 1280.0] {
        let (mut world, root, edit, item) = fixture(width);
        world
            .entity_mut(root)
            .insert(crate::native::layout::desktop::HostViewport(
                OutputViewport {
                    id: crate::host::OutputId(1),
                    bounds: Rectangle::new([0.0, 0.0], [f64::from(width), 800.0]).unwrap(),
                    usable: Some(Rectangle::new([0.0, 40.0], [f64::from(width), 720.0]).unwrap()),
                    physical_size: [width as u32, 800],
                    raster: crate::host::raster::RasterMapping {
                        origin: [0.0; 2],
                        scale: [1.0; 2],
                    },
                    workspace: 1,
                    rotation: Default::default(),
                },
            ));
        layout(&mut world);
        let left = rectangle(&world, edit);
        let right = rectangle(&world, item);
        assert!(left.max.x + 11.9 <= right.min.x);
        assert!(left.min.x >= 0.0 && right.max.x <= width);
        assert!(left.min.y >= 40.0 && right.min.y >= 40.0);
        if width >= 800.0 {
            assert_eq!(left.width(), 376.0);
            assert_eq!(right.width(), 300.0);
        }
        assert!(
            world
                .get::<crate::native::canvas_item::CanvasItem>(edit)
                .is_none()
        );
    }
}

#[test]
fn dragging_panels_keeps_them_screen_fixed_clamps_edges_and_does_not_move_the_other_panel() {
    let (mut world, root, edit, item) = fixture(1280.0);
    layout(&mut world);
    let before = rectangle(&world, item);
    move_panel(&mut world, edit, Vec2::new(300.0, 60.0));
    layout(&mut world);
    let moved = rectangle(&world, edit);
    assert_eq!(rectangle(&world, item), before);
    assert!(moved.min.x < 700.0 && moved.min.y > 60.0);
    move_panel(&mut world, edit, Vec2::splat(100_000.0));
    layout(&mut world);
    assert!(rectangle(&world, edit).max.x <= 1280.0);
    world.get_mut::<ComputedNode>(root).unwrap().size = Vec2::new(420.0, 500.0);
    layout(&mut world);
    let resized = rectangle(&world, edit);
    assert!(resized.min.x >= 0.0 && resized.max.x <= 420.0);
    assert!(resized.min.y >= 0.0 && resized.max.y <= 500.0);
    let stable = rectangle(&world, edit);
    layout(&mut world);
    assert_eq!(rectangle(&world, edit), stable);
}

#[test]
fn minimizing_preserves_content_scroll_and_position_until_restored() {
    let (mut world, _, _, item) = fixture(1280.0);
    layout(&mut world);
    move_panel(&mut world, item, Vec2::new(100.0, 80.0));
    layout(&mut world);
    let before = rectangle(&world, item).min;
    let body = world.get::<Panel>(item).unwrap().body.unwrap();
    world.get_mut::<ScrollPosition>(body).unwrap().0.y = 72.0;
    let field = world
        .spawn((Text::new("Retained item"), ChildOf(body)))
        .id();
    Minimize.apply(&mut world, item);
    layout(&mut world);
    assert_eq!(world.get::<Node>(body).unwrap().display, Display::None);
    assert!(rectangle(&world, item).width() < 300.0);
    assert_eq!(rectangle(&world, item).min, before);
    Minimize.apply(&mut world, item);
    layout(&mut world);
    assert_eq!(world.get::<Node>(body).unwrap().display, Display::Flex);
    assert_eq!(world.get::<ScrollPosition>(body).unwrap().0.y, 72.0);
    assert_eq!(world.get::<Text>(field).unwrap().0, "Retained item");
    assert_eq!(rectangle(&world, item).width(), 300.0);
    assert_eq!(rectangle(&world, item).min, before);
}

#[test]
fn settled_panels_do_not_keep_invalidating_layout() {
    let (mut world, _, _, _) = fixture(1280.0);
    layout(&mut world);
    world.clear_trackers();
    layout(&mut world);
    assert_eq!(
        world
            .query_filtered::<Entity, Changed<Node>>()
            .iter(&world)
            .count(),
        0
    );
}

#[test]
fn title_bar_pointer_drag_moves_the_panel_without_recreating_its_contents() {
    use bevy::picking::pointer::{Location, PointerId};
    let (mut world, _, edit, _) = fixture(1280.0);
    layout(&mut world);
    let before = rectangle(&world, edit).min;
    let header = world
        .query::<(Entity, &Header, &ChildOf)>()
        .iter(&world)
        .find(|(_, _, parent)| parent.parent() == edit)
        .unwrap()
        .0;
    world.trigger(Pointer::new(
        PointerId::Mouse,
        Location {
            target: bevy::camera::NormalizedRenderTarget::None {
                width: 1280,
                height: 800,
            },
            position: before,
        },
        Drag {
            button: PointerButton::Primary,
            distance: Vec2::new(100.0, 40.0),
            delta: Vec2::new(100.0, 40.0),
        },
        header,
    ));
    world.flush();
    layout(&mut world);
    assert_eq!(rectangle(&world, edit).min, before + Vec2::new(100.0, 40.0));
    assert_eq!(world.get::<ChildOf>(header).unwrap().parent(), edit);
}

#[test]
fn real_ui_layout_keeps_panel_headers_and_restore_controls_inside_the_viewport() {
    for width in [360, 1000] {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::input::InputPlugin,
            bevy::picking::DefaultPickingPlugins,
            bevy::window::WindowPlugin {
                primary_window: None,
                ..default()
            },
            bevy::image::ImagePlugin::default(),
            bevy::text::TextPlugin,
            bevy::transform::TransformPlugin,
            bevy::input_focus::InputFocusPlugin,
            bevy::ui::UiPlugin,
            crate::native::theme::ThemePlugin,
            crate::native::sand::SandPlugin,
        ))
        .init_resource::<Assets<TextureAtlasLayout>>()
        .add_systems(
            PostUpdate,
            (layout, crate::native::scroll_sand::update_bars)
                .chain()
                .after(bevy::ui::UiSystems::Layout),
        );
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: bevy::camera::ComputedCameraValues {
                    target_info: Some(bevy::camera::RenderTargetInfo {
                        physical_size: UVec2::new(width, 720),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
        ));
        let root = app
            .world_mut()
            .spawn(Node {
                width: px(width),
                height: px(720),
                ..default()
            })
            .id();
        let mut panels = Vec::new();
        for kind in [Kind::Edit, Kind::Item] {
            let panel = app
                .world_mut()
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(if kind == Kind::Edit { 376 } else { 300 }),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(8)),
                        border: UiRect::all(px(1)),
                        row_gap: px(4),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    ChildOf(root),
                ))
                .id();
            let body = attach(app.world_mut(), panel, root, kind);
            if kind == Kind::Edit {
                crate::native::scroll_sand::decorate(app.world_mut(), body);
            }
            for index in 0..40 {
                super::super::label(app.world_mut(), body, &format!("Control {index}"), 14.0);
            }
            panels.push(panel);
        }
        for _ in 0..20 {
            app.update();
        }
        let bounds =
            |app: &App, entity| crate::native::inspection::bounds(app.world(), entity).unwrap();
        let edit = bounds(&app, panels[0]);
        let item = bounds(&app, panels[1]);
        assert!(edit.max.x + 11.0 <= item.min.x, "{item:?} {edit:?}");
        assert!(edit.min.x >= 0.0 && item.max.x <= width as f32);
        let track = app
            .world_mut()
            .query::<(&bevy::a11y::AccessibilityNode, &ChildOf)>()
            .iter(app.world())
            .filter(|(node, _)| node.label() == Some("Scroll content"))
            .map(|(_, parent)| parent.parent())
            .find(|track| app.world().get::<ChildOf>(*track).unwrap().parent() == panels[0])
            .unwrap();
        let before_scroll = bounds(&app, track);
        assert!((before_scroll.width() - 6.0).abs() < 0.1);
        app.world_mut()
            .get_mut::<ScrollPosition>(panels[0])
            .unwrap()
            .0
            .y = 80.0;
        for _ in 0..4 {
            app.update();
        }
        let after_scroll = bounds(&app, track);
        assert!((after_scroll.min - before_scroll.min).length() < 0.1);
        Minimize.apply(app.world_mut(), panels[1]);
        for _ in 0..8 {
            app.update();
        }
        let compact = bounds(&app, panels[1]);
        let (button, _) = app.world().get::<Panel>(panels[1]).unwrap().toggle.unwrap();
        let restore = bounds(&app, button);
        assert!(compact.height() <= 57.0, "{compact:?}");
        assert!(
            restore.min.x >= compact.min.x && restore.max.x <= compact.max.x + 0.1,
            "{restore:?} {compact:?}"
        );
        let remove = app
            .world()
            .get::<Children>(panels[0])
            .unwrap()
            .iter()
            .filter(|entity| {
                !crate::native::scroll_sand::decoration(app.world(), *entity)
                    && app.world().get::<Header>(*entity).is_none()
            })
            .collect::<Vec<_>>();
        for entity in remove {
            app.world_mut().despawn(entity);
        }
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(
            app.world().get::<Node>(track).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<ScrollPosition>(panels[0]).unwrap().0.y,
            0.0
        );
    }
}
