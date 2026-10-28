use super::shortcuts::{capture, click, key, settle};
use super::transfers::output;
use super::*;
use lince_interface::host::{DesktopAction, HostRequest, OutputId, SurfaceId};
use std::sync::mpsc::Receiver;

fn gesture(canvas: &mut Presentation) {
    for (physical, pressed) in [
        (KeyCode::AltLeft, true),
        (KeyCode::Tab, true),
        (KeyCode::Tab, false),
        (KeyCode::AltLeft, false),
    ] {
        key(canvas, physical, pressed);
    }
}

pub(super) fn map(canvas: &mut Presentation, surface: SurfaceId, color: [u8; 4]) {
    let extent = wgpu::Extent3d {
        width: 64,
        height: 48,
        depth_or_array_layers: 1,
    };
    let texture = canvas
        .renderer
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("recent-window-fixture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
    canvas.renderer.queue.write_texture(
        texture.as_image_copy(),
        &color.repeat(64 * 48),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(64 * 4),
            rows_per_image: Some(48),
        },
        extent,
    );
    canvas.surface_texture(surface, &texture).unwrap();
}

fn receive(canvas: &mut Presentation, receiver: &Receiver<HostRequest>) -> Frame {
    let HostRequest::Action(action @ DesktopAction::CommitRecent { .. }) =
        receiver.try_recv().unwrap()
    else {
        panic!("expected a recent-window commit");
    };
    canvas.desktop_action(action).unwrap();
    settle(canvas)
}

fn focused(canvas: &Presentation, root: Entity) -> Option<[u8; 16]> {
    let spaces = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap();
    spaces
        .entries
        .iter()
        .find(|space| space.id == spaces.active)
        .unwrap()
        .desktop
        .focused()
        .map(|id| id.0)
}

fn choice(canvas: &mut Presentation, target: Entity) -> Entity {
    let name = format!("recent.target.{}", target.to_bits());
    let world = canvas.world_mut();
    let found = world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, value)| value.as_str() == name)
        .map(|(entity, _)| entity);
    found.unwrap_or_else(|| {
        let rows: Vec<_> = world
            .query::<(Entity, &Name)>()
            .iter(world)
            .filter(|(_, name)| name.as_str().starts_with("recent."))
            .map(|(entity, name)| (entity, name.as_str().to_owned()))
            .collect();
        let items: Vec<_> = world
            .query::<(Entity, &native::canvas::CanvasItem)>()
            .iter(world)
            .map(|(entity, item)| {
                (
                    entity,
                    world
                        .get::<native::workspace::WorkspaceMember>(entity)
                        .map(|member| member.0),
                    item.size,
                    world
                        .get::<native::layout::LayoutBox>(entity)
                        .map(|layout| layout.id),
                    world
                        .get::<bevy::ecs::entity_disabling::Disabled>(entity)
                        .is_some(),
                    world
                        .get::<native::layout::desktop::TransientParent>(entity)
                        .is_some(),
                )
            })
            .collect();
        panic!("missing {name}; rows={rows:?}; items={items:?}")
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn recent_window_keys_pointer_and_queued_monitor_commits_keep_native_drafts_and_client_sizes() {
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
        canvas
            .outputs(vec![output(1, 0.0, size), output(2, 1000.0, size)])
            .unwrap();
        let root = canvas.canvas_root().unwrap();
        let spaces = native::workspace::numbers(canvas.app.world(), root);
        canvas
            .activate_workspace(root.to_bits(), spaces[0])
            .unwrap();
        settle(&mut canvas);
        let note = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            spaces[0],
            native::sand_store::SandKind::EditableText,
            "Retained note",
            bevy::math::DVec2::ZERO,
        );
        let editor = canvas
            .app
            .world()
            .get::<native::sand_store::StoredSand>(note)
            .unwrap()
            .content
            .unwrap();
        settle(&mut canvas);
        native::layout::desktop::focus_item(canvas.world_mut(), note);
        canvas
            .world_mut()
            .get_mut::<bevy::text::EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Retained draft");
        canvas
            .world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        settle(&mut canvas);
        canvas
            .open_window(SurfaceId(81), "First application", "test.generic")
            .unwrap();
        map(&mut canvas, SurfaceId(81), [180, 80, 30, 255]);
        settle(&mut canvas);
        let first = canvas.windows[&SurfaceId(81)].entity;
        native::layout::desktop::focus_item(canvas.world_mut(), first);
        settle(&mut canvas);
        canvas
            .activate_workspace(root.to_bits(), spaces[1])
            .unwrap();
        canvas
            .open_window(SurfaceId(82), "Second application", "test.generic")
            .unwrap();
        map(&mut canvas, SurfaceId(82), [70, 130, 40, 255]);
        settle(&mut canvas);
        let second = canvas.windows[&SurfaceId(82)].entity;
        native::layout::desktop::focus_item(canvas.world_mut(), second);
        let before = settle(&mut canvas);
        let dimensions: Vec<_> = canvas
            .windows
            .values()
            .map(|window| {
                (
                    window.entity,
                    canvas
                        .app
                        .world()
                        .get::<native::canvas::CanvasItem>(window.entity)
                        .unwrap()
                        .size,
                )
            })
            .collect();
        let second_id = canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(second)
            .unwrap()
            .id;
        let first_id = canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(first)
            .unwrap()
            .id;
        let note_id = canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(note)
            .unwrap()
            .id;
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        canvas.host_requests(sender);
        key(&mut canvas, KeyCode::AltLeft, true);
        key(&mut canvas, KeyCode::Tab, true);
        key(&mut canvas, KeyCode::Tab, false);
        let frame = settle(&mut canvas);
        assert!(frame.input.keyboard_modal);
        assert_eq!(canvas.render_size, size);
        assert_eq!(frame.input.bindings.len(), 2);
        assert!(
            frame
                .input
                .bindings
                .iter()
                .all(|binding| binding.action.is_recent())
        );
        assert_eq!(focused(&canvas, root), Some(second_id));
        assert_eq!(frame.output, before.output);
        for (entity, size) in &dimensions {
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::canvas::CanvasItem>(*entity)
                    .unwrap()
                    .size,
                *size
            );
        }
        let selected = choice(&mut canvas, first);
        let caption = canvas.app.world().get::<Children>(selected).unwrap()[0];
        let node = canvas.app.world().get::<ComputedNode>(selected).unwrap();
        assert!(node.size().min_element() > 0.0);
        let text = canvas.app.world().get::<ComputedNode>(caption).unwrap();
        assert!(text.size().min_element() > 0.0);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::a11y::AccessibilityNode>(selected)
                .unwrap()
                .is_selected(),
            Some(true)
        );
        capture(&format!("recent-window-{}", size[0]), size, &frame);
        key(&mut canvas, KeyCode::AltLeft, false);
        settle(&mut canvas);
        let frame = receive(&mut canvas, &receiver);
        assert!(!frame.input.keyboard_modal);
        assert_eq!(frame.output, Some(OutputId(1)));
        assert_eq!(focused(&canvas, root), Some(first_id));
        key(&mut canvas, KeyCode::AltLeft, true);
        key(&mut canvas, KeyCode::Tab, true);
        key(&mut canvas, KeyCode::Tab, false);
        settle(&mut canvas);
        let target = choice(&mut canvas, note);
        click(&mut canvas, target, size);
        key(&mut canvas, KeyCode::AltLeft, false);
        settle(&mut canvas);
        receive(&mut canvas, &receiver);
        assert_eq!(focused(&canvas, root), Some(note_id));
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value(),
            "Retained draft"
        );
        key(&mut canvas, KeyCode::AltLeft, true);
        key(&mut canvas, KeyCode::Tab, true);
        key(&mut canvas, KeyCode::Tab, false);
        settle(&mut canvas);
        for (physical, logical, text, pressed) in [
            (KeyCode::Escape, Key::Escape, None, true),
            (
                KeyCode::KeyX,
                Key::Character("x".into()),
                Some("x".into()),
                true,
            ),
            (KeyCode::KeyX, Key::Character("x".into()), None, false),
            (KeyCode::Escape, Key::Escape, None, false),
            (KeyCode::AltLeft, Key::Alt, None, false),
        ] {
            canvas.key(physical, logical, text, pressed, false).unwrap();
        }
        settle(&mut canvas);
        assert!(!native::layout::desktop::recent::active(canvas.app.world()));
        assert!(receiver.try_recv().is_err());
        assert_eq!(focused(&canvas, root), Some(note_id));
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value(),
            "Retained draft"
        );
        canvas
            .key(
                KeyCode::KeyY,
                Key::Character("y".into()),
                Some("y".into()),
                true,
                false,
            )
            .unwrap();
        canvas
            .key(
                KeyCode::KeyY,
                Key::Character("y".into()),
                None,
                false,
                false,
            )
            .unwrap();
        settle(&mut canvas);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string()
                .contains('y')
        );
        canvas.focus_output(OutputId(2)).unwrap();
        settle(&mut canvas);
        native::layout::desktop::focus_item(canvas.world_mut(), second);
        settle(&mut canvas);
        gesture(&mut canvas);
        key(&mut canvas, KeyCode::AltLeft, true);
        key(&mut canvas, KeyCode::Tab, true);
        key(&mut canvas, KeyCode::Tab, false);
        settle(&mut canvas);
        assert!(native::layout::desktop::recent::active(canvas.app.world()));
        receive(&mut canvas, &receiver);
        assert!(native::layout::desktop::recent::active(canvas.app.world()));
        key(&mut canvas, KeyCode::AltLeft, false);
        settle(&mut canvas);
        receive(&mut canvas, &receiver);
        assert_eq!(focused(&canvas, root), Some(second_id));
        assert_eq!(canvas.output_state.assignments.focused(), Some(OutputId(2)));
        assert!(receiver.try_recv().is_err());
        canvas
            .activate_workspace(root.to_bits(), spaces[0])
            .unwrap();
        settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(note_id));
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        assert_eq!(canvas.windows[&SurfaceId(81)].entity, first);
        assert_eq!(canvas.windows[&SurfaceId(82)].entity, second);
        for index in 0..24 {
            canvas.world_mut().spawn((
                native::canvas::CanvasItem {
                    position: bevy::math::DVec2::new(4000.0 + index as f64 * 400.0, 0.0),
                    size: Vec2::new(250.0, 180.0),
                },
                native::layout::LayoutBox::new(Vec2::new(250.0, 180.0)),
                native::workspace::WorkspaceMember(spaces[0]),
                Name::new(format!(
                    "Additional window {index}: a long title that wraps in the compact chooser"
                )),
                ChildOf(root),
            ));
        }
        settle(&mut canvas);
        key(&mut canvas, KeyCode::AltLeft, true);
        key(&mut canvas, KeyCode::Tab, true);
        key(&mut canvas, KeyCode::Tab, false);
        settle(&mut canvas);
        for _ in 0..23 {
            key(&mut canvas, KeyCode::ArrowDown, true);
            key(&mut canvas, KeyCode::ArrowDown, false);
        }
        let frame = settle(&mut canvas);
        let world = canvas.app.world();
        let selected = world
            .resource::<bevy::input_focus::InputFocus>()
            .get()
            .unwrap();
        let node = world.get::<ComputedNode>(selected).unwrap();
        let center = world
            .get::<UiGlobalTransform>(selected)
            .unwrap()
            .translation;
        let clip = world
            .get::<bevy::ui::CalculatedClip>(selected)
            .unwrap()
            .clip;
        assert!(center.y - node.size().y * 0.5 >= clip.min.y - 1.0);
        assert!(center.y + node.size().y * 0.5 <= clip.max.y + 1.0);
        assert!(
            world
                .get::<ScrollPosition>(world.get::<ChildOf>(selected).unwrap().parent())
                .unwrap()
                .0
                .y
                > 0.0
        );
        capture(&format!("recent-window-scroll-{}", size[0]), size, &frame);
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        key(&mut canvas, KeyCode::AltLeft, false);
        settle(&mut canvas);
        assert!(receiver.try_recv().is_err());
    }
}
