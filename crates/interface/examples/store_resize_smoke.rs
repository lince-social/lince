use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorIcon, CursorMoved, PrimaryWindow, SystemCursorIcon, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::dispatch,
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    edit_mode::{EditAction, EditMode},
    sand_store::{SandKind, SandPreview, StoreEntry, StoredSand, spawn_sand},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    sand: Entity,
    cursor: Vec2,
    original: CanvasItem,
    captured: usize,
    path: String,
}

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    world.insert_resource(Fixture {
        root,
        sand: Entity::PLACEHOLDER,
        cursor: Vec2::ZERO,
        original: CanvasItem {
            position: DVec2::ZERO,
            size: Vec2::ZERO,
        },
        captured: 0,
        path: std::env::args().nth(1).expect("provide screenshot path"),
    });
}

fn window(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap()
}

fn pointer(world: &mut World, position: Vec2) {
    let window = window(world);
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
}

fn press(world: &mut World, down: bool) {
    mouse_button(world, down, MouseButton::Left);
}

fn mouse_button(world: &mut World, down: bool, button: MouseButton) {
    let window = window(world);
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
}

fn bounds(world: &World, sand: Entity, root: Entity) -> Rect {
    let center = world.get::<UiGlobalTransform>(sand).unwrap().translation
        * world
            .get::<ComputedNode>(sand)
            .unwrap()
            .inverse_scale_factor();
    let size = world.get::<CanvasItem>(sand).unwrap().size
        * world.get::<CanvasView>(root).unwrap().zoom as f32;
    Rect::from_center_size(center, size)
}

fn capture(world: &mut World, suffix: &str) {
    let panel = world
        .get::<EditMode>(world.resource::<Fixture>().root)
        .unwrap()
        .panel;
    assert!(world.get::<InheritedVisibility>(panel).unwrap().get());
    assert!(
        world
            .query_filtered::<&InheritedVisibility, With<StoreEntry>>()
            .iter(world)
            .all(|visibility| visibility.get())
    );
    let path = format!("{}.{}.png", world.resource::<Fixture>().path, suffix);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut fixture: ResMut<Fixture>| fixture.captured += 1);
}

fn check_position_controls(world: &mut World, sand: Entity, root: Entity) {
    let panel = world
        .query::<(&lince_interface::icons::IconButton, &ChildOf)>()
        .iter(world)
        .find(|(icon, _)| icon.label == "Pin to screen")
        .map(|(_, parent)| parent.parent())
        .expect("position icons");
    assert!(
        world
            .get::<lince_interface::inspection::InspectionExcluded>(panel)
            .is_some()
    );
    assert_eq!(world.get::<Children>(panel).unwrap().len(), 5);
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Sand position")
    );
    let node = world.get::<ComputedNode>(panel).unwrap();
    let size = node.size() * node.inverse_scale_factor();
    let center =
        world.get::<UiGlobalTransform>(panel).unwrap().translation * node.inverse_scale_factor();
    let rect = Rect::from_center_size(center, size);
    let sand_bounds = bounds(world, sand, root);
    assert!(rect.min.y >= sand_bounds.max.y || rect.max.y <= sand_bounds.min.y);
    assert!(size.x > size.y);
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world.resource::<Fixture>().root;
    let sand = world.resource::<Fixture>().sand;
    match frame {
        4 => {
            let sand = spawn_sand(
                world,
                root,
                1,
                SandKind::EditableText,
                "Retain this text",
                DVec2::new(-200.0, 0.0),
            );
            world.get_mut::<CanvasItem>(sand).unwrap().size = Vec2::new(120.0, 100.0);
            world.resource_mut::<Fixture>().sand = sand;
            dispatch(
                world,
                root,
                lince_interface::actions![EditAction::Open, EditAction::Store],
            );
        }
        12 => {
            let previews: Vec<_> = world
                .query_filtered::<&ComputedNode, With<SandPreview>>()
                .iter(world)
                .map(|node| node.size() * node.inverse_scale_factor())
                .collect();
            assert_eq!(previews.len(), 4);
            assert!(previews.iter().all(|size| *size == Vec2::new(72.0, 64.0)));
            assert_eq!(world.query::<&StoreEntry>().iter(world).count(), 4);
            capture(world, "store");
        }
        14 => {
            let rect = bounds(world, sand, root);
            let position = Vec2::new(rect.max.x - 2.0, rect.center().y);
            world.resource_mut::<Fixture>().cursor = position;
            pointer(world, position);
        }
        16 => {
            let window = window(world);
            assert_eq!(
                world.get::<CursorIcon>(window),
                Some(&CursorIcon::System(SystemCursorIcon::EwResize))
            );
            press(world, true);
        }
        18 => pointer(
            world,
            world.resource::<Fixture>().cursor + Vec2::new(40.0, 0.0),
        ),
        20 => press(world, false),
        22 => {
            let item = world.get::<CanvasItem>(sand).unwrap();
            assert_eq!(item.size, Vec2::new(160.0, 100.0));
            assert_eq!(item.position, DVec2::new(-180.0, 0.0));
            assert_eq!(world.get::<CanvasView>(root).unwrap().center, DVec2::ZERO);
            check_position_controls(world, sand, root);
            let path = format!("{}.controls.png", world.resource::<Fixture>().path);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
        24 => {
            let item = *world.get::<CanvasItem>(sand).unwrap();
            world.resource_mut::<Fixture>().original = item;
            *world.get_mut::<CanvasView>(root).unwrap() = CanvasView {
                center: item.position + DVec2::new(100.0, 0.0),
                zoom: 2.0,
            };
        }
        26 => {
            let position = bounds(world, sand, root).min + Vec2::splat(2.0);
            world.resource_mut::<Fixture>().cursor = position;
            pointer(world, position);
        }
        28 => {
            let window = window(world);
            assert_eq!(
                world.get::<CursorIcon>(window),
                Some(&CursorIcon::System(SystemCursorIcon::NwseResize))
            );
            press(world, true);
        }
        30 => pointer(
            world,
            world.resource::<Fixture>().cursor - Vec2::splat(20.0),
        ),
        32 => press(world, false),
        34 => {
            let item = world.get::<CanvasItem>(sand).unwrap();
            let original = world.resource::<Fixture>().original;
            assert_eq!(item.size, Vec2::new(170.0, 110.0));
            assert_eq!(
                item.position + item.size.as_dvec2() * 0.5,
                original.position + original.size.as_dvec2() * 0.5
            );
            let text = world.get::<StoredSand>(sand).unwrap().content.unwrap();
            assert_eq!(
                lince_interface::sand_text::value(world, text),
                "Retain this text"
            );
            assert_eq!(world.get::<Node>(sand).unwrap().overflow, Overflow::clip());
        }
        36 => dispatch(world, root, lince_interface::actions![EditAction::Close]),
        38 => {
            let window = window(world);
            assert!(
                world
                    .get::<CursorIcon>(window)
                    .is_none_or(|icon| *icon == CursorIcon::default())
            );
        }
        40 => {
            let rect = bounds(world, sand, root);
            let position = Vec2::new(rect.max.x - 2.0, rect.center().y);
            world.resource_mut::<Fixture>().cursor = position;
            pointer(world, position);
        }
        42 => press(world, true),
        44 => pointer(
            world,
            world.resource::<Fixture>().cursor + Vec2::new(10.0, 0.0),
        ),
        46 => press(world, false),
        48 => assert_eq!(
            world.get::<CanvasItem>(sand).unwrap().size,
            Vec2::new(170.0, 110.0)
        ),
        50 => dispatch(
            world,
            root,
            lince_interface::actions![EditAction::Open, EditAction::Store],
        ),
        54 => {
            let panel = world.get::<EditMode>(root).unwrap().panel;
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 10000.0;
        }
        58 => capture(world, "resized"),
        64 => {
            dispatch(world, root, lince_interface::actions![EditAction::Close]);
        }
        68 => {
            let text = world.get::<StoredSand>(sand).unwrap().content.unwrap();
            assert_eq!(world.get::<BackgroundColor>(sand).unwrap().0.alpha(), 0.0);
            assert!(world.get::<Outline>(sand).is_none());
            assert_eq!(world.get::<Outline>(text).unwrap().width, px(0));
            let position = bounds(world, sand, root).center();
            world.resource_mut::<Fixture>().cursor = position;
            world.resource_mut::<Fixture>().original = *world.get::<CanvasItem>(sand).unwrap();
            pointer(world, position);
        }
        70 => mouse_button(world, true, MouseButton::Right),
        72 => pointer(
            world,
            world.resource::<Fixture>().cursor + Vec2::new(40.0, 20.0),
        ),
        74 => mouse_button(world, false, MouseButton::Right),
        76 => {
            let original = world.resource::<Fixture>().original;
            let item = world.get::<CanvasItem>(sand).unwrap();
            assert_eq!(item.position, original.position + DVec2::new(20.0, 10.0));
            assert_eq!(item.size, original.size);
            dispatch(
                world,
                root,
                lince_interface::actions![EditAction::Open, EditAction::Store],
            );
        }
        80 => {
            let row = world
                .query::<(Entity, &StoreEntry)>()
                .iter(world)
                .find(|(_, entry)| entry.0 == SandKind::Text)
                .unwrap()
                .0;
            let position = world.get::<UiGlobalTransform>(row).unwrap().translation
                * world
                    .get::<ComputedNode>(row)
                    .unwrap()
                    .inverse_scale_factor();
            pointer(world, position);
        }
        82 => press(world, true),
        84 => press(world, false),
        88 => {
            assert_eq!(world.query::<&StoredSand>().iter(world).count(), 2);
            let text = world.get::<StoredSand>(sand).unwrap().content.unwrap();
            assert_eq!(world.get::<Outline>(text).unwrap().width, px(1));
            assert_eq!(world.resource::<Fixture>().captured, 2);
            dispatch(
                world,
                root,
                lince_interface::actions![EditAction::Workspaces, EditAction::CreateWorkspace],
            );
        }
        92 => {
            let path = format!("{}.workspaces.png", world.resource::<Fixture>().path);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(|_: On<ScreenshotCaptured>, mut fixture: ResMut<Fixture>| {
                    fixture.captured += 1
                });
        }
        98 => {
            assert_eq!(world.resource::<Fixture>().captured, 3);
            println!(
                "Store and resize smoke passed: equal previews, resizing at two zooms, preserved text, transparent text Sands, Edit mode boundaries, right-button movement outside Edit mode, whole-row adding and workspace layout."
            );
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(Update, exercise)
        .run();
}
