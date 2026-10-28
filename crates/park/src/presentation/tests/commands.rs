use super::shortcuts::{capture, click, key, settle};
use super::*;
use lince_interface::{
    host::{DesktopAction, HostRequest},
    native::operation::OpenOperation,
};
use native::actions::Action;

pub(super) fn named(canvas: &mut Presentation, value: &str) -> Entity {
    canvas
        .world_mut()
        .query::<(Entity, &Name)>()
        .iter(canvas.app.world())
        .find(|(_, name)| name.as_str() == value)
        .map(|(entity, _)| entity)
        .unwrap()
}

pub(super) fn type_command(canvas: &mut Presentation, text: &str) {
    for character in text.chars() {
        let code = match character {
            '/' => KeyCode::Slash,
            'a' => KeyCode::KeyA,
            'c' => KeyCode::KeyC,
            'd' => KeyCode::KeyD,
            'e' => KeyCode::KeyE,
            'g' => KeyCode::KeyG,
            'h' => KeyCode::KeyH,
            'i' => KeyCode::KeyI,
            'k' => KeyCode::KeyK,
            'l' => KeyCode::KeyL,
            'm' => KeyCode::KeyM,
            'n' => KeyCode::KeyN,
            'o' => KeyCode::KeyO,
            'p' => KeyCode::KeyP,
            'r' => KeyCode::KeyR,
            's' => KeyCode::KeyS,
            't' => KeyCode::KeyT,
            'u' => KeyCode::KeyU,
            'v' => KeyCode::KeyV,
            'w' => KeyCode::KeyW,
            'x' => KeyCode::KeyX,
            '-' => KeyCode::Minus,
            _ => unreachable!(),
        };
        let text = character.to_string();
        for pressed in [true, false] {
            canvas
                .key(
                    code,
                    Key::Character(text.clone().into()),
                    pressed.then(|| text.clone()),
                    pressed,
                    false,
                )
                .unwrap();
        }
        canvas.probe_frame().unwrap();
    }
    settle(canvas);
}

fn row_fits(canvas: &Presentation, button: Entity, size: [u32; 2]) -> Vec2 {
    let world = canvas.app.world();
    let node = world.get::<ComputedNode>(button).unwrap();
    let center = world.get::<UiGlobalTransform>(button).unwrap().translation;
    assert!(center.x - node.size().x * 0.5 >= -1.0);
    assert!(center.x + node.size().x * 0.5 <= size[0] as f32 + 1.0);
    let label = world.get::<Children>(button).unwrap()[0];
    let layout = world.get::<bevy::text::TextLayoutInfo>(label).unwrap();
    let text = world.get::<ComputedNode>(label).unwrap();
    assert!(
        layout.size.x <= text.size().x + 1.0,
        "clipped text: {:?} / {:?}",
        layout.size,
        text.size()
    );
    assert!(layout.size.y <= text.size().y + 1.0);
    if size[0] == 360 {
        assert!(layout.glyphs.iter().any(|glyph| glyph.line_index > 0));
    }
    node.size()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn operation_shares_command_metadata_and_explains_unavailable_actions_at_narrow_widths() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut canvas = Presentation::new(
            size,
            directory.path().join(format!("assets-{}", size[0])),
            WakeSignal::new(|| {}),
        )
        .unwrap();
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        OpenOperation.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let input = named(&mut canvas, "operation.input");
        type_command(&mut canvas, "/launch/terminal");
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(input)
                .unwrap()
                .value(),
            "/launch/terminal"
        );
        let suggestions = named(&mut canvas, "operation.suggestions");
        let button = canvas.app.world().get::<Children>(suggestions).unwrap()[0];
        let row_size = row_fits(&canvas, button, size);
        click(&mut canvas, button, size);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let frame = settle(&mut canvas);
        assert_eq!(row_fits(&canvas, button, size), row_size);
        let status = named(&mut canvas, "operation.status");
        assert!(
            canvas
                .app
                .world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("requires a Park session")
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(input)
        );
        capture(&format!("commands-unavailable-{}", size[0]), size, &frame);
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        canvas.host_requests(sender);
        let frame = settle(&mut canvas);
        assert_eq!(row_fits(&canvas, button, size), row_size);
        assert!(canvas.app.world().get::<Text>(status).unwrap().0.is_empty());
        assert_eq!(
            canvas.app.world().get::<Children>(suggestions).unwrap()[0],
            button
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(input)
        );
        capture(&format!("commands-available-{}", size[0]), size, &frame);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        settle(&mut canvas);
        assert!(matches!(
            receiver.try_recv().unwrap(),
            HostRequest::Action(DesktopAction::OpenTerminal)
        ));
        assert!(canvas.app.world().get_entity(input).is_err());
        let view = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        OpenOperation.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        type_command(&mut canvas, "/item/width");
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let frame = settle(&mut canvas);
        let status = named(&mut canvas, "operation.status");
        assert!(
            canvas
                .app
                .world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("No eligible item")
        );
        let current = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        assert_eq!(current.center, view.center);
        assert_eq!(current.zoom, view.zoom);
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        capture(&format!("commands-no-target-{}", size[0]), size, &frame);
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        settle(&mut canvas);
        let workspace = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let target = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            workspace,
            native::sand_store::SandKind::Square,
            "",
            bevy::math::DVec2::ZERO,
        );
        settle(&mut canvas);
        native::layout::desktop::focus_item(canvas.world_mut(), target);
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        settle(&mut canvas);
        let before = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        OpenOperation.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        type_command(&mut canvas, "/item/width");
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let frame = settle(&mut canvas);
        let status = named(&mut canvas, "operation.status");
        assert!(
            canvas
                .app
                .world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("No eligible item")
        );
        let spaces = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap();
        assert!(
            spaces
                .entries
                .iter()
                .find(|space| space.id == spaces.active)
                .unwrap()
                .desktop
                .focused()
                .is_none()
        );
        let current = canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        assert_eq!(current.size, before.size);
        assert_eq!(current.position, before.position);
        capture(
            &format!("commands-unfocused-existing-{}", size[0]),
            size,
            &frame,
        );
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        OpenOperation.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let input = named(&mut canvas, "operation.input");
        type_command(&mut canvas, "/");
        let suggestions = named(&mut canvas, "operation.suggestions");
        let rows: Vec<_> = canvas
            .app
            .world()
            .get::<Children>(suggestions)
            .unwrap()
            .iter()
            .collect();
        assert!(rows.len() > 20);
        for _ in 1..rows.len() {
            key(&mut canvas, KeyCode::ArrowDown, true);
            key(&mut canvas, KeyCode::ArrowDown, false);
        }
        let frame = settle(&mut canvas);
        let target = *rows.last().unwrap();
        let world = canvas.app.world();
        assert_eq!(
            world.resource::<bevy::input_focus::InputFocus>().get(),
            Some(input)
        );
        assert_eq!(
            world
                .get::<bevy::a11y::AccessibilityNode>(target)
                .unwrap()
                .is_selected(),
            Some(true)
        );
        let node = world.get::<ComputedNode>(target).unwrap();
        let center = world.get::<UiGlobalTransform>(target).unwrap().translation;
        let clip = world.get::<bevy::ui::CalculatedClip>(target).unwrap().clip;
        assert!(center.y - node.size().y * 0.5 >= clip.min.y - 1.0);
        assert!(center.y + node.size().y * 0.5 <= clip.max.y + 1.0);
        let feedback = world.get::<ChildOf>(suggestions).unwrap().parent();
        assert!(world.get::<ScrollPosition>(feedback).unwrap().0.y > 0.0);
        capture(
            &format!("commands-keyboard-scroll-{}", size[0]),
            size,
            &frame,
        );
        key(&mut canvas, KeyCode::ArrowDown, true);
        key(&mut canvas, KeyCode::ArrowDown, false);
        settle(&mut canvas);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::a11y::AccessibilityNode>(rows[0])
                .unwrap()
                .is_selected(),
            Some(true)
        );
        let world = canvas.app.world();
        let node = world.get::<ComputedNode>(rows[0]).unwrap();
        let center = world.get::<UiGlobalTransform>(rows[0]).unwrap().translation;
        let clip = world.get::<bevy::ui::CalculatedClip>(rows[0]).unwrap().clip;
        assert!(center.y - node.size().y * 0.5 >= clip.min.y - 1.0);
        assert!(center.y + node.size().y * 0.5 <= clip.max.y + 1.0);
    }
}
