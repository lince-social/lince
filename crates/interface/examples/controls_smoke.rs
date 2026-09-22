use bevy::{
    a11y::AccessibilityNode,
    diagnostic::FrameCount,
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    container::BoxRoot,
    icons::{Icon, IconButton},
    sand_store::{SandKind, spawn_sand},
    topology::{presentation::bounds, view::View},
};

#[derive(Component)]
struct Probe;

fn rect(world: &World, entity: Entity) -> Rect {
    let node = world.get::<ComputedNode>(entity).unwrap();
    let scale = node.inverse_scale_factor() * world.resource::<UiScale>().0;
    Rect::from_center_size(
        world.get::<UiGlobalTransform>(entity).unwrap().translation * scale,
        node.size() * scale,
    )
}

fn pointer(world: &mut World, position: Vec2) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(position));
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
}

fn info(world: &mut World, source: Entity) -> Entity {
    world
        .query::<(Entity, &lince_interface::icons::TooltipIcon)>()
        .iter(world)
        .find(|(_, icon)| icon.source == source)
        .unwrap()
        .0
}

fn tooltip(world: &mut World) -> Entity {
    world
        .query::<(Entity, &GlobalZIndex, &Text)>()
        .iter(world)
        .find(|(_, z, _)| z.0 == 100)
        .unwrap()
        .0
}

fn check_tip(world: &mut World, button: Rect) {
    let tip = tooltip(world);
    assert_ne!(
        *world.get::<Visibility>(tip).unwrap(),
        Visibility::Hidden,
        "frame {}, button {button:?}, tooltip {}",
        world.resource::<FrameCount>().0,
        world.get::<Text>(tip).unwrap().0
    );
    let bounds = rect(world, tip);
    assert!(
        (bounds.center().x - button.center().x).abs() < 2.0,
        "{bounds:?} is not centered on {button:?}"
    );
    assert!(
        (bounds.max.y - (button.min.y - 8.0 * world.resource::<UiScale>().0)).abs() < 2.0,
        "{bounds:?} is not above {button:?}"
    );
}

fn capture(world: &mut World, name: &str) {
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("/tmp/lince-controls-{name}.png")));
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame < 20 {
        return;
    }
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    let zoom = world
        .query::<(Entity, &IconButton)>()
        .iter(world)
        .find(|(_, icon)| icon.label == "Zoom in")
        .unwrap()
        .0;
    let bar = world.get::<ChildOf>(zoom).unwrap().parent();
    let corner = world
        .query::<(Entity, &AccessibilityNode)>()
        .iter(world)
        .find(|(_, node)| node.label() == Some("Show controls"))
        .unwrap()
        .0;
    match frame {
        20 => {
            assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
            capture(world, "corner");
            pointer(world, rect(world, corner).center());
        }
        30 => {
            assert_eq!(
                *world.get::<Visibility>(bar).unwrap(),
                Visibility::Inherited
            );
            let icon = info(world, zoom);
            pointer(world, rect(world, icon).center());
        }
        40 => {
            let icon = info(world, zoom);
            check_tip(world, rect(world, icon));
            capture(world, "toolbar");
        }
        50 => pointer(world, Vec2::splat(100.0)),
        60 => {
            assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
            world
                .resource_mut::<InputFocus>()
                .set(corner, FocusCause::Navigated);
        }
        65 => {
            assert_eq!(
                *world.get::<Visibility>(bar).unwrap(),
                Visibility::Inherited
            );
            world
                .resource_mut::<InputFocus>()
                .set(zoom, FocusCause::Navigated);
        }
        70 => {
            assert_eq!(
                *world.get::<Visibility>(bar).unwrap(),
                Visibility::Inherited
            );
            world.resource_mut::<InputFocus>().clear();
            let workspace = world
                .get::<lince_interface::workspace::Workspaces>(root)
                .unwrap()
                .active;
            let sand = spawn_sand(world, root, workspace, SandKind::Square, "", DVec2::ZERO);
            world.spawn((
                Probe,
                IconButton::new(Icon::Info, "Inspect this Sand"),
                ChildOf(sand),
            ));
        }
        90 => {
            let probe = world
                .query_filtered::<Entity, With<Probe>>()
                .single(world)
                .unwrap();
            let icon = info(world, probe);
            pointer(world, bounds(world, icon).unwrap().center());
        }
        105 => {
            let probe = world
                .query_filtered::<Entity, With<Probe>>()
                .single(world)
                .unwrap();
            let icon = info(world, probe);
            check_tip(world, bounds(world, icon).unwrap());
            capture(world, "sand");
            world.spawn((Text::new("Inspect this Sand"), ChildOf(probe)));
        }
        115 => {
            let tip = tooltip(world);
            assert_ne!(*world.get::<Visibility>(tip).unwrap(), Visibility::Hidden);
            let probe = world
                .query_filtered::<Entity, With<Probe>>()
                .single(world)
                .unwrap();
            let label = world
                .get::<Children>(probe)
                .unwrap()
                .iter()
                .find(|child| world.get::<Text>(*child).is_some())
                .unwrap();
            world.despawn(label);
            world.get_mut::<View>(root).unwrap().spatial = true;
        }
        135 => {
            let probe = world
                .query_filtered::<Entity, With<Probe>>()
                .single(world)
                .unwrap();
            let icon = info(world, probe);
            pointer(world, bounds(world, icon).unwrap().center());
        }
        150 => {
            let probe = world
                .query_filtered::<Entity, With<Probe>>()
                .single(world)
                .unwrap();
            let icon = info(world, probe);
            check_tip(world, bounds(world, icon).unwrap());
            capture(world, "spatial");
        }
        165 => {
            println!(
                "Controls passed: corner hover and focus, toolbar hiding, info icons with duplicate labels, screen and Sand tooltip placement in 2D and 3D."
            );
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    interface_app()
        .insert_resource(lince_interface::icons::TooltipSettings { enabled: true })
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands,
             mut windows: Query<&mut Window>,
             mut scale: ResMut<UiScale>| {
                if std::env::args().any(|arg| arg == "--hidpi") {
                    windows
                        .single_mut()
                        .unwrap()
                        .resolution
                        .set_scale_factor_override(Some(2.0));
                }
                if std::env::args().any(|arg| arg == "--ui-scale") {
                    scale.0 = 1.25;
                }
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
