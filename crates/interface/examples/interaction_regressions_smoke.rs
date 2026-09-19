use bevy::{
    a11y::AccessibilityNode,
    diagnostic::FrameCount,
    input::{
        ButtonState,
        mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
    },
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    canvas_controls::{CanvasAction, CanvasControl},
    canvas_selection::SandSelection,
    container::BoxRoot,
    edit_mode::EditAction,
    inspection::{Inspection, InspectionOverlay},
    sand_store::{SandKind, spawn_sand},
    topology::{input::PointerState, presentation::bounds},
};

#[derive(Resource)]
struct Fixture {
    sand: Entity,
    scrolls: usize,
}

fn window(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap()
}

fn pointer(world: &mut World, position: Vec2) {
    let window = window(world);
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

fn press(world: &mut World, down: bool) {
    let window = window(world);
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
}

fn wheel(world: &mut World) {
    let window = window(world);
    world.write_message(WindowEvent::MouseWheel(MouseWheel {
        window,
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: -1.0,
        phase: TouchPhase::Moved,
    }));
}

fn toggle_click(world: &mut World, label: &str) {
    let entity = world
        .query::<(&Text, &ChildOf)>()
        .iter(world)
        .find(|(text, _)| text.0 == label)
        .unwrap()
        .1
        .parent();
    world.trigger(bevy::ui_widgets::Activate { entity });
}

fn overlay_count(world: &mut World) -> usize {
    world
        .query_filtered::<Entity, With<InspectionOverlay>>()
        .iter(world)
        .count()
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
    if frame == 20 {
        let workspace = world
            .get::<lince_interface::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let sand = spawn_sand(
            world,
            root,
            workspace,
            SandKind::Square,
            "",
            DVec2::new(300.0, 0.0),
        );
        world
            .entity_mut(sand)
            .insert(lince_interface::icons::Tooltip("Inspect this Sand".into()))
            .observe(
                |mut event: On<Pointer<bevy::picking::events::Scroll>>,
                 mut fixture: ResMut<Fixture>| {
                    fixture.scrolls += 1;
                    event.propagate(false);
                },
            );
        world.insert_resource(Fixture { sand, scrolls: 0 });
        return;
    }
    let sand = world.resource::<Fixture>().sand;
    match frame {
        25 => {
            let window = window(world);
            let size = world.get::<Window>(window).unwrap().size();
            pointer(world, size * 0.5 + Vec2::new(150.0, 0.0));
        }
        30 | 33 | 36 | 39 => wheel(world),
        43 => {
            assert_eq!(
                world
                    .resource::<PointerState>()
                    .hit
                    .map(|(entity, _)| entity),
                Some(sand)
            );
            assert!((world.get::<CanvasView>(root).unwrap().zoom - (-0.4_f64).exp()).abs() < 1e-6);
            assert_eq!(world.resource::<Fixture>().scrolls, 0);
            pointer(world, bounds(world, sand).unwrap().center());
        }
        46 => wheel(world),
        50 => {
            assert!((world.get::<CanvasView>(root).unwrap().zoom - (-0.4_f64).exp()).abs() < 1e-6);
            assert!(world.resource::<Fixture>().scrolls > 0);
            EditAction::Open.apply(world, root);
            EditAction::General.apply(world, root);
            world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(-200.0, 0.0);
        }
        65 | 85 | 105 => pointer(world, bounds(world, sand).unwrap().center()),
        70 | 90 | 110 => press(world, true),
        72 | 92 | 112 => press(world, false),
        80 => {
            assert!(!world.get::<Inspection>(root).unwrap().click);
            assert!(world.get::<SandSelection>(root).unwrap().0.contains(&sand));
            assert_eq!(overlay_count(world), 0);
            let roundness = world
                .query::<(&CanvasControl, &Node)>()
                .iter(world)
                .find(|(control, _)| control.action == CanvasAction::ResetZoom)
                .unwrap()
                .1
                .border_radius;
            for (node, accessibility) in world.query_filtered::<(&Node, Option<&AccessibilityNode>), With<bevy::ui_widgets::Button>>().iter(world) {
                if accessibility.and_then(|node| node.label()) != Some("Show controls") {
                    assert_eq!(node.border_radius, roundness);
                }
            }
            toggle_click(world, "Click: off");
        }
        100 => {
            assert!(overlay_count(world) > 0);
            toggle_click(world, "Click: on");
        }
        120 => {
            assert!(!world.get::<Inspection>(root).unwrap().click);
            assert_eq!(overlay_count(world), 0);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-interaction-regressions.png"));
        }
        135 => {
            println!(
                "Interactions passed: background zoom continues over Sands without scrolling their contents; pointer movement restores Sand scrolling; button corners match; Click: off suppresses inspection after repeated selections."
            );
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
