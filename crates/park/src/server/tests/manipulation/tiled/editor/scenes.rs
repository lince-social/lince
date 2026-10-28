use super::*;
use bevy::prelude::*;
use lince_interface::native as ui;
use smithay::backend::winit::WinitEvent;

#[path = "scenes/handoff.rs"]
mod handoff;
#[path = "scenes/lifecycle.rs"]
mod lifecycle;

fn field(world: &mut World, owner: u64) -> Entity {
    world
        .query::<(Entity, &EditableText)>()
        .iter(world)
        .find(|(entity, _)| entity.to_bits() == owner)
        .unwrap()
        .0
}

fn configure(fixture: &mut Fixture, scale: f64, origin: [i32; 2]) {
    fixture
        .desktop
        .server
        .configure_scaled_outputs(
            &[Monitor {
                id: OutputId(1),
                name: "Native editor output".into(),
                origin,
                size: fixture.size.map(|value| value as i32),
            }],
            &BTreeMap::from([(OutputId(1), scale)]),
        )
        .unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
    fixture.desktop.event(WinitEvent::Focus(true)).unwrap();
    fixture.flush();
    fixture.configure();
}

fn focus(fixture: &mut Fixture, id: [u8; 16]) -> crate::native_input::Editor {
    fixture.action(DesktopAction::SelectPlacement {
        workspace: 1,
        item: Some(id),
    });
    fixture.configure();
    let pixels = fixture.render("free-editor-ready");
    let point = super::super::native::text_point(fixture, ItemId::Native(id), &pixels);
    fixture.desktop.move_pointer(point, 30).unwrap();
    fixture.flush();
    fixture.render("free-editor-hover");
    for pressed in [true, false] {
        fixture.desktop.button(272, pressed, 31).unwrap();
        fixture.flush();
    }
    fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .editor
        .clone()
        .unwrap()
}

fn keys(fixture: &mut Fixture, events: &[(u32, bool)]) {
    for &(key, pressed) in events {
        fixture.desktop.key(key, pressed, 32).unwrap();
        for _ in 0..4 {
            fixture.desktop.dispatch().unwrap();
            fixture.flush();
        }
    }
}

fn endpoint(fixture: &Fixture) -> crate::native_input::Editor {
    fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .editor
        .clone()
        .unwrap()
}

fn stale(fixture: &mut Fixture, context: crate::native_input::Editor) {
    let serial = fixture
        .desktop
        .worker
        .input(Input::NativeEdit(crate::native_input::Edit {
            context,
            commit: Some("stale".into()),
            preedit: None,
            delete: None,
        }))
        .unwrap();
    advance(&mut fixture.desktop, serial);
}

fn exercise(spatial: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale, origin) in [
        ([1000, 700], 1.25, [1000, 48]),
        ([360, 700], 1.5, [-640, 80]),
    ] {
        let mut fixture = Fixture::prepared(
            &runtime,
            &directory.path().join(format!("free-{spatial}-{}", size[0])),
            size,
            Some(if spatial {
                super::super::native::spatial
            } else {
                super::super::native::flat
            }),
        );
        configure(&mut fixture, scale, origin);
        fixture.action(DesktopAction::SetLayout(WorkspaceLayout::Free));
        let app = fixture.clients[0].1;
        let text = if spatial {
            (0..32)
                .map(|index| format!("Native line {index:02}\n"))
                .collect::<String>()
        } else {
            "An editable horizontal source line ".repeat(20)
        };
        let base = text.clone();
        let (id, owner, note) = super::super::recovery::inspect(&mut fixture, move |canvas| {
            let world = canvas.world_mut();
            let (note, editor) = world
                .query::<(Entity, &ui::sand_store::StoredSand)>()
                .iter(world)
                .find(|(_, stored)| stored.kind == ui::sand_store::SandKind::EditableText)
                .map(|(entity, stored)| (entity, stored.content.unwrap()))
                .unwrap();
            let id = world.get::<ui::layout::LayoutBox>(note).unwrap().id;
            let position = bevy::math::DVec2::new(-2000.0, -300.0);
            world.entity_mut(note).insert(ui::canvas::CanvasItem {
                position,
                size: Vec2::new(360.0, 240.0),
            });
            world.get_mut::<ui::layout::LayoutBox>(note).unwrap().rules =
                lince_interface::layout::Rules::fixed([360.0, 240.0]);
            let application = world
                .query::<(Entity, &ui::layout::desktop::ApplicationWindow)>()
                .iter(world)
                .find(|(_, window)| window.surface == app)
                .unwrap()
                .0;
            world
                .entity_mut(application)
                .insert(ui::canvas::CanvasItem {
                    position: position + bevy::math::DVec2::new(0.0, 200.0),
                    size: Vec2::splat(80.0),
                });
            world
                .get_mut::<ui::layout::LayoutBox>(application)
                .unwrap()
                .rules = lince_interface::layout::Rules::fixed([80.0; 2]);
            let mut input = world.get_mut::<EditableText>(editor).unwrap();
            input.editor.set_text(&base);
            input.allow_newlines = spatial;
            input.visible_lines = Some(if spatial { 3.0 } else { 1.0 });
            drop(input);
            let text = world.get::<ui::sand_text::SandText>(editor).unwrap();
            let mut rules = text
                .layout
                .unwrap_or_else(|| ui::layout::Rules::fixed(text.size));
            rules.wrap = spatial;
            rules.axes[usize::from(spatial)].overflow = ui::layout::Overflow::Scroll;
            ui::layout::configure(world, editor, rules).unwrap();
            (id, editor.to_bits(), note)
        });
        let identities = fixture.identities();
        assert_eq!(focus(&mut fixture, id).owner, owner);
        keys(
            &mut fixture,
            &[
                (29, true),
                (107, true),
                (107, false),
                (29, false),
                (45, true),
                (45, false),
            ],
        );
        let expected = format!("{text}x");
        assert_eq!(value(&mut fixture, owner), (expected.clone(), false));
        let scroll = super::super::recovery::inspect(&mut fixture, move |canvas| {
            let world = canvas.world_mut();
            let field = field(world, owner);
            world.get::<bevy::ui::widget::TextScroll>(field).unwrap().0
        });
        assert!(
            scroll[usize::from(spatial)] > 0.0,
            "The native field did not scroll: {scroll:?}"
        );
        let (mut provider, method) = connect(&mut fixture);
        wait(&mut fixture, &mut provider, owner, &expected, false, 0);
        let serial = provider.client.input_serial;
        method.set_preedit_string("界".into(), 0, 3);
        method.commit(serial);
        wait(&mut fixture, &mut provider, owner, &expected, true, serial);
        let before = endpoint(&fixture);
        let serial = provider.client.input_serial;
        let geometry_revision = fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .revision;
        let motion_frames = if size[0] == 1000 && !spatial { 120 } else { 8 };
        for _ in 0..motion_frames {
            super::super::recovery::inspect(&mut fixture, move |canvas| {
                let world = canvas.world_mut();
                world
                    .get_mut::<ui::canvas::CanvasItem>(note)
                    .unwrap()
                    .position += bevy::math::DVec2::new(16.0, -8.0);
                let application = world
                    .query::<(Entity, &ui::layout::desktop::ApplicationWindow)>()
                    .iter(world)
                    .find(|(_, window)| window.surface == app)
                    .unwrap()
                    .0;
                world
                    .get_mut::<ui::canvas::CanvasItem>(application)
                    .unwrap()
                    .position += bevy::math::DVec2::new(20.0, -6.0);
            });
            fixture.desktop.dispatch().unwrap();
            fixture.flush();
            client_sync(&mut fixture.desktop, &mut provider);
            client_sync(&mut fixture.desktop, &mut provider);
            let current = endpoint(&fixture);
            assert_eq!(current.owner, before.owner);
            assert_eq!(current.revision, before.revision);
            assert_eq!(
                current.caret, before.caret,
                "A followed caret moved relative to its editor"
            );
            assert_eq!(
                current.projection, before.projection,
                "Unrelated scene motion retired the editor"
            );
            assert!(value(&mut fixture, owner).1);
        }
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .revision
                > geometry_revision
        );
        assert_eq!(provider.client.input_serial, serial);
        method.commit_string("é".into());
        method.commit(serial);
        wait(
            &mut fixture,
            &mut provider,
            owner,
            &format!("{expected}é"),
            false,
            serial,
        );
        let serial = provider.client.input_serial;
        method.delete_surrounding_text(2, 0);
        method.commit(serial);
        wait(&mut fixture, &mut provider, owner, &expected, false, serial);
        lifecycle::resize(&mut fixture, &mut provider, &method, owner, &expected);
        let serial = provider.client.input_serial;
        method.set_preedit_string("界".into(), 0, 3);
        method.commit(serial);
        wait(&mut fixture, &mut provider, owner, &expected, true, serial);
        let old = endpoint(&fixture);
        fixture.action(DesktopAction::Workspace(2));
        stale(&mut fixture, old);
        assert_eq!(value(&mut fixture, owner), (expected.clone(), false));
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .as_ref()
                .is_none_or(|editor| editor.owner != owner)
        );
        assert_eq!(focus(&mut fixture, id).owner, owner);
        keys(
            &mut fixture,
            &[(29, true), (107, true), (107, false), (29, false)],
        );
        let serial = provider.client.input_serial;
        wait(&mut fixture, &mut provider, owner, &expected, false, serial);
        let serial = provider.client.input_serial;
        method.set_preedit_string("界".into(), 0, 3);
        method.commit(serial);
        wait(&mut fixture, &mut provider, owner, &expected, true, serial);
        let before_output = endpoint(&fixture);
        configure(
            &mut fixture,
            scale + 0.25,
            [origin[0] + 160, origin[1] - 40],
        );
        stale(&mut fixture, before_output);
        assert_eq!(value(&mut fixture, owner), (expected.clone(), false));
        assert_eq!(focus(&mut fixture, id).owner, owner);
        keys(
            &mut fixture,
            &[(29, true), (107, true), (107, false), (29, false)],
        );
        client_sync(&mut fixture.desktop, &mut provider);
        client_sync(&mut fixture.desktop, &mut provider);
        let serial = provider.client.input_serial;
        method.set_preedit_string("界".into(), 0, 3);
        method.commit(serial);
        wait(&mut fixture, &mut provider, owner, &expected, true, serial);
        #[cfg(feature = "session")]
        {
            let before_headless = endpoint(&fixture);
            fixture.desktop.server.disconnect_outputs();
            fixture.desktop.server.output_catalog(Vec::new()).unwrap();
            fixture.desktop.invalidate_outputs().unwrap();
            fixture.desktop.dispatch().unwrap();
            fixture.flush();
            assert!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .geometry
                    .as_ref()
                    .unwrap()
                    .outputs
                    .is_empty()
            );
            assert!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .editor
                    .is_none()
            );
            stale(&mut fixture, before_headless.clone());
            assert_eq!(value(&mut fixture, owner), (expected.clone(), false));
            configure(&mut fixture, scale, origin);
            stale(&mut fixture, before_headless);
            assert_eq!(value(&mut fixture, owner), (expected.clone(), false));
        }
        assert_eq!(focus(&mut fixture, id).owner, owner);
        keys(
            &mut fixture,
            &[(29, true), (107, true), (107, false), (29, false)],
        );
        client_sync(&mut fixture.desktop, &mut provider);
        client_sync(&mut fixture.desktop, &mut provider);
        let serial = provider.client.input_serial;
        method.set_preedit_string("界".into(), 0, 3);
        method.commit(serial);
        wait(&mut fixture, &mut provider, owner, &expected, true, serial);
        let old = endpoint(&fixture);
        super::super::recovery::inspect(&mut fixture, move |canvas| {
            let world = canvas.world_mut();
            let entity = field(world, owner);
            world.get_mut::<Node>(entity).unwrap().display = bevy::ui::Display::None;
        });
        stale(&mut fixture, old);
        assert_eq!(value(&mut fixture, owner), (expected.clone(), false));
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .is_none()
        );
        let (id, owner, note) = lifecycle::replace(
            &mut fixture,
            &mut provider,
            &method,
            id,
            owner,
            note,
            &expected,
        );
        handoff::exercise(&mut fixture, &mut provider, &method, id, owner, note);
        assert_eq!(fixture.identities(), identities);
        assert_eq!(fixture.desktop.server.windows().len(), 3);
        method.destroy();
        client_sync(&mut fixture.desktop, &mut provider);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn flat_free_editors_scroll_follow_and_reject_stale_contexts_on_scaled_outputs() {
    exercise(false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn spatial_free_editors_scroll_follow_and_reject_stale_contexts_on_scaled_outputs() {
    exercise(true);
}
