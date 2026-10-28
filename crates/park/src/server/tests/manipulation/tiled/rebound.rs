use super::*;
use crate::presentation::{Frame, Presentation};
use bevy::{
    input::{keyboard::Key, mouse::MouseButton},
    prelude::{
        Children, ComputedNode, Display, Entity, InheritedVisibility, KeyCode, Name, Node, Text,
        UiGlobalTransform, With,
    },
};
use lince_interface::{host::Manipulation, native};
use native::actions::Action;

fn settle(canvas: &mut Presentation) -> Frame {
    let mut frame = canvas.probe_frame().unwrap();
    for _ in 0..3 {
        frame = canvas.probe_frame().unwrap();
    }
    frame.wait_for_probe().unwrap();
    frame
}

fn button(canvas: &mut Presentation, caption: &str) -> Entity {
    let world = canvas.world_mut();
    world
        .query::<(Entity, &Children, &native::actions::ActionButton)>()
        .iter(world)
        .find(|(entity, children, _)| {
            let mut cursor = Some(*entity);
            for _ in 0..32 {
                let Some(entity) = cursor else { break };
                if world
                    .get::<Node>(entity)
                    .is_some_and(|node| node.display == Display::None)
                {
                    return false;
                }
                cursor = world
                    .get::<bevy::prelude::ChildOf>(entity)
                    .map(bevy::prelude::ChildOf::parent);
            }
            world
                .get::<InheritedVisibility>(*entity)
                .is_some_and(|visible| visible.get())
                && world
                    .get::<ComputedNode>(*entity)
                    .is_some_and(|node| node.size().min_element() > 0.0)
                && children.iter().any(|child| {
                    world
                        .get::<Text>(*child)
                        .is_some_and(|text| text.0 == caption)
                })
        })
        .map(|(entity, _, _)| entity)
        .unwrap_or_else(|| panic!("Missing visible control: {caption}"))
}

fn activate(canvas: &mut Presentation, entity: Entity) -> Frame {
    canvas
        .world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    settle(canvas)
}

fn capture(canvas: &mut Presentation, command: &str) -> Entity {
    let world = canvas.world_mut();
    let row = world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, name)| name.as_str() == command)
        .map(|(entity, _)| entity)
        .unwrap();
    world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(entity, name)| {
            if name.as_str() != "shortcut.capture" {
                return false;
            }
            let mut cursor = Some(*entity);
            for _ in 0..32 {
                let Some(entity) = cursor else { break };
                if entity == row {
                    return true;
                }
                cursor = world
                    .get::<bevy::prelude::ChildOf>(entity)
                    .map(bevy::prelude::ChildOf::parent);
            }
            false
        })
        .map(|(entity, _)| entity)
        .unwrap()
}

fn key(canvas: &mut Presentation, pressed: bool) {
    canvas
        .key(KeyCode::ControlLeft, Key::Control, None, pressed, false)
        .unwrap();
    settle(canvas);
}

fn profile(canvas: &mut Presentation) {
    settle(canvas);
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    assert!(native::workspace_config::set_physics(
        canvas.world_mut(),
        root,
        1,
        false
    ));
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    settle(canvas);
    let editor = button(canvas, "Edit all shortcuts");
    assert!(activate(canvas, editor).input.keyboard_modal);
    for (command, mouse) in [
        ("pointer.move", MouseButton::Left),
        ("pointer.resize", MouseButton::Right),
    ] {
        let control = capture(canvas, command);
        assert!(activate(canvas, control).input.keyboard_capture);
        let point = canvas
            .world_mut()
            .get::<UiGlobalTransform>(control)
            .unwrap()
            .transform_point2(bevy::prelude::Vec2::ZERO);
        canvas.pointer(point.to_array()).unwrap();
        key(canvas, true);
        canvas.button(mouse, true).unwrap();
        settle(canvas);
        canvas.button(mouse, false).unwrap();
        key(canvas, false);
        let validate = button(canvas, "Validate combination");
        assert!(!activate(canvas, validate).input.keyboard_capture);
    }
    let save = button(canvas, "Save all changes");
    assert!(
        canvas
            .world_mut()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_none()
    );
    assert!(!activate(canvas, save).input.keyboard_modal);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    settle(canvas);
    canvas
        .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
        .unwrap();
    settle(canvas);
    {
        let mut view = canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasView>(root)
            .unwrap();
        view.center = bevy::math::DVec2::ZERO;
        view.set_zoom(0.3);
    }
    settle(canvas);
}

fn spatial_profile(canvas: &mut Presentation) {
    profile(canvas);
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    settle(canvas);
    assert!(native::topology::view::spatial(canvas.world_mut(), root));
}

fn begin(fixture: &mut Fixture, index: usize, mouse: u32, uv: [f64; 2]) -> [f64; 2] {
    let point = fixture.surface_point(index, uv);
    fixture.harness.client.pointer_buttons.clear();
    fixture.desktop.move_pointer(point, 1).unwrap();
    assert!(
        matches!(fixture.desktop.target(), crate::desktop::input::Target::Application(id, _, _) if id == fixture.clients[index].1)
    );
    fixture.desktop.key(29, true, 2).unwrap();
    fixture.desktop.button(mouse, true, 3).unwrap();
    fixture.desktop.key(29, false, 4).unwrap();
    fixture.flush();
    let active = fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .manipulation
        .unwrap();
    assert_eq!(
        active.target.item,
        ItemId::Application(fixture.clients[index].1)
    );
    point
}

fn end(fixture: &mut Fixture, mouse: u32, commit: bool, capture: &str) {
    if !commit {
        fixture.desktop.key(1, true, 8).unwrap();
        fixture.desktop.key(1, false, 9).unwrap();
    }
    fixture.desktop.button(mouse, false, 10).unwrap();
    fixture.flush();
    fixture.configure();
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .manipulation
            .is_none()
    );
    fixture.sync();
    assert!(fixture.harness.client.pointer_buttons.is_empty());
    assert_eq!(fixture.resizing(), vec![false; 3]);
    fixture.render(capture);
}

fn exercise(spatial: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let data = directory
            .path()
            .join(format!("rebound-{spatial}-{}", size[0]));
        let prepare: fn(&mut Presentation) = if spatial { spatial_profile } else { profile };
        let mut fixture = Fixture::prepared(&runtime, &data, size, Some(prepare));
        let name = |base: &str| {
            if spatial {
                format!("{base}-spatial")
            } else {
                base.to_owned()
            }
        };
        let saved = std::fs::read(data.join("interface.lingua")).unwrap();
        for (operation, button) in [
            (Manipulation::Move, MouseButton::Left),
            (Manipulation::Resize, MouseButton::Right),
        ] {
            assert!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .pointer_bindings
                    .iter()
                    .any(|binding| {
                        binding.operation == operation
                            && binding.chord.button == button
                            && binding.chord.modifiers.control
                            && !binding.chord.modifiers.super_key
                    })
            );
        }
        fixture.action(DesktopAction::SetLayout(WorkspaceLayout::Free));
        let index = 2;
        fixture.activate(index);
        fixture.configure();
        let identities = fixture.identities();
        let sizes = fixture.sizes();
        let initial = fixture.geometry(index);
        assert_eq!(initial.mapping.corners.is_some(), spatial);
        fixture.render(&name("rebound-initial"));
        let point = fixture.surface_point(index, [0.5, 0.5]);
        let anchor = point;
        fixture.desktop.move_pointer(point, 1).unwrap();
        fixture.desktop.key(125, true, 2).unwrap();
        fixture.desktop.button(272, true, 3).unwrap();
        fixture
            .desktop
            .move_pointer([point[0] + 12.0, point[1] + 8.0], 4)
            .unwrap();
        fixture.desktop.button(272, false, 5).unwrap();
        fixture.desktop.key(125, false, 6).unwrap();
        fixture.flush();
        fixture.sync();
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert_eq!(fixture.geometry(index), initial);
        assert_eq!(
            fixture.harness.client.pointer_buttons,
            [(272, true), (272, false)]
        );
        let point = begin(&mut fixture, index, 272, [0.5, 0.5]);
        fixture
            .desktop
            .move_pointer([point[0] + 48.0, point[1] + 24.0], 5)
            .unwrap();
        fixture.flush();
        end(&mut fixture, 272, true, &name("rebound-moved"));
        let moved = fixture.surface_point(index, [0.5, 0.5]);
        for axis in 0..2 {
            assert!((moved[axis] - anchor[axis] - [48.0, 24.0][axis]).abs() < 0.01);
        }
        assert_eq!(fixture.sizes(), sizes);
        let limits = [sizes[index][0] + 24, sizes[index][1] + 16];
        fixture.clients[index]
            .0
            .role
            .set_max_size(limits[0] as i32, limits[1] as i32);
        fixture.clients[index].0.surface.commit();
        fixture.sync();
        fixture.desktop.dispatch().unwrap();
        fixture.flush();
        fixture.configure();
        let point = begin(&mut fixture, index, 273, [0.99, 0.99]);
        let mapping = fixture.geometry(index).mapping;
        let surface = mapping.surface_point(point).unwrap();
        let next = mapping
            .canvas_point(std::array::from_fn(|axis| {
                surface[axis]
                    + [26.0, 18.0][axis] * mapping.surface.size()[axis]
                        / f64::from(sizes[index][axis])
            }))
            .unwrap();
        let corner = mapping.canvas_point(mapping.surface.origin()).unwrap();
        fixture.desktop.move_pointer(next, 5).unwrap();
        fixture.flush();
        fixture.configure();
        assert_eq!(fixture.sizes()[index], limits);
        assert!(fixture.resizing()[index]);
        fixture.render(&name("rebound-maximum-preview"));
        end(&mut fixture, 273, true, &name("rebound-maximum-committed"));
        assert_eq!(fixture.sizes()[index], limits);
        let committed = fixture.geometry(index);
        let after = committed
            .mapping
            .canvas_point(committed.mapping.surface.origin())
            .unwrap();
        assert!(
            (0..2).all(|axis| (after[axis] - corner[axis]).abs() < 0.1),
            "Resize moved the opposite projected corner: {corner:?} -> {after:?}"
        );
        for commit in [false, true] {
            begin(&mut fixture, index, 273, [0.99, 0.99]);
            fixture.desktop.move_pointer([0.0; 2], 5).unwrap();
            fixture.flush();
            fixture.configure();
            assert_eq!(fixture.sizes()[index], [80; 2]);
            fixture.render(&name("rebound-minimum-preview"));
            end(
                &mut fixture,
                273,
                commit,
                &name(if commit {
                    "rebound-minimum-committed"
                } else {
                    "rebound-minimum-cancelled"
                }),
            );
            if !commit {
                assert_eq!(fixture.geometry(index), committed);
            }
        }
        assert_eq!(fixture.sizes()[index], [80; 2]);
        for other in [0, 1] {
            assert_eq!(fixture.sizes()[other], sizes[other]);
        }
        fixture.normal_context_clicks(index);
        assert_eq!(fixture.identities(), identities);
        assert_eq!(std::fs::read(data.join("interface.lingua")).unwrap(), saved);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn saved_pointer_bindings_move_resize_and_clamp_real_clients_after_normal_ui_save_and_restart() {
    exercise(false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn saved_pointer_bindings_keep_projected_grabs_limits_and_cancel_in_spatial_views() {
    exercise(true);
}
