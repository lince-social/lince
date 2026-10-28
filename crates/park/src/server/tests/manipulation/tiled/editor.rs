use super::*;
use bevy::text::EditableText;

#[path = "editor/scenes.rs"]
mod scenes;

fn value(fixture: &mut Fixture, owner: u64) -> (String, bool) {
    super::recovery::inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let (_, text) = world
            .query::<(bevy::prelude::Entity, &EditableText)>()
            .iter(world)
            .find(|(entity, _)| entity.to_bits() == owner)
            .unwrap();
        (text.value().to_string(), text.is_composing())
    })
}

#[track_caller]
fn wait(
    fixture: &mut Fixture,
    provider: &mut Harness,
    owner: u64,
    expected: &str,
    composing: bool,
    previous_serial: u32,
) {
    let caller = std::panic::Location::caller();
    let stage = caller.line();
    let source = std::path::Path::new(caller.file())
        .file_stem()
        .unwrap()
        .to_string_lossy();
    let scene = fixture
        .directory
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        fixture.desktop.dispatch().unwrap();
        fixture.flush();
        let text = value(fixture, owner);
        client_sync(&mut fixture.desktop, provider);
        client_sync(&mut fixture.desktop, provider);
        let endpoint = fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .editor
            .as_ref();
        let surrounding = if endpoint.is_some_and(|editor| editor.sensitive) {
            ""
        } else {
            expected
        };
        if text == (expected.into(), composing)
            && endpoint.is_some_and(|editor| editor.owner == owner)
            && provider.client.input_active
            && provider.client.input_serial != previous_serial
            && provider
                .client
                .input_surrounding
                .last()
                .is_some_and(|(text, _, _)| text == surrounding)
        {
            fixture.render(&format!("native-ime-{scene}-{source}-stage-{stage}"));
            break;
        }
        if Instant::now() >= deadline {
            let details = super::recovery::inspect(fixture, move |canvas| {
                let world = canvas.world_mut();
                let entity = world
                    .query::<(bevy::prelude::Entity, &EditableText)>()
                    .iter(world)
                    .find(|(entity, _)| entity.to_bits() == owner)
                    .unwrap()
                    .0;
                let camera = world
                    .get::<bevy::ui::ComputedUiTargetCamera>(entity)
                    .and_then(|camera| camera.get());
                let surface = camera
                    .and_then(|camera| {
                        world.get::<lince_interface::native::topology::presentation::VisualOwner>(
                            camera,
                        )
                    })
                    .and_then(|owner| {
                        world.get::<lince_interface::native::topology::presentation::Surface>(
                            owner.0,
                        )
                    });
                format!(
                    "node={:?}, transform={:?}, clip={:?}, scroll={:?}, area={:?}, cursor={:?}, surface={:?}",
                    world
                        .get::<bevy::prelude::ComputedNode>(entity)
                        .map(|node| (node.size(), node.content_box())),
                    world.get::<bevy::prelude::UiGlobalTransform>(entity),
                    world
                        .get::<bevy::ui::CalculatedClip>(entity)
                        .map(|clip| clip.clip),
                    world.get::<bevy::ui::widget::TextScroll>(entity),
                    world
                        .get::<EditableText>(entity)
                        .unwrap()
                        .editor
                        .ime_cursor_area(),
                    world
                        .get::<EditableText>(entity)
                        .unwrap()
                        .editor
                        .cursor_geometry(1.0),
                    surface.map(|surface| (
                        surface.visible,
                        surface.pixels,
                        surface.size,
                        surface.uv
                    ))
                )
            });
            eprintln!("Native IME stage {stage}: {details}");
            fixture.render(&format!(
                "native-ime-{scene}-{source}-missing-stage-{stage}"
            ));
        }
        assert!(
            Instant::now() < deadline,
            "Native IME did not reach {expected:?}/{composing}: {text:?}, surrounding={:?}, endpoint={:?}",
            provider.client.input_surrounding.last(),
            fixture.desktop.current.as_ref().unwrap().frame.input.editor
        );
    }
}

fn connect(
    fixture: &mut Fixture,
) -> (
    Harness,
    wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::ZwpInputMethodV2,
) {
    let server = std::mem::replace(
        &mut fixture.desktop.server,
        Server::new([640, 480]).unwrap(),
    );
    let mut provider =
        Harness::connect_authorized(server, Some(vec![crate::launch::Capability::InputMethod]));
    fixture.desktop.server =
        std::mem::replace(&mut provider.server, Server::new([640, 480]).unwrap());
    let method = provider
        .client
        .input_method
        .as_ref()
        .unwrap()
        .get_input_method(
            provider.client.seat.as_ref().unwrap(),
            &provider.queue.handle(),
            (),
        );
    (provider, method)
}

pub(super) fn compose(fixture: &mut Fixture, owner: u64, expected: &str) {
    let (mut provider, method) = connect(fixture);
    wait(fixture, &mut provider, owner, expected, false, 0);
    let serial = provider.client.input_serial;
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(serial);
    wait(fixture, &mut provider, owner, expected, true, serial);
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
            .unwrap()
            .from_ime
    );
    method.commit_string("é".into());
    let serial = provider.client.input_serial;
    method.commit(serial);
    wait(
        fixture,
        &mut provider,
        owner,
        &format!("{expected}é"),
        false,
        serial,
    );
    method.delete_surrounding_text(2, 0);
    method.commit_string("🌿".into());
    let serial = provider.client.input_serial;
    method.commit(serial);
    wait(
        fixture,
        &mut provider,
        owner,
        &format!("{expected}🌿"),
        false,
        serial,
    );
    method.delete_surrounding_text(4, 0);
    let serial = provider.client.input_serial;
    method.commit(serial);
    wait(fixture, &mut provider, owner, expected, false, serial);
    let stale = fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .editor
        .clone()
        .unwrap();
    let serial = provider.client.input_serial;
    fixture.action(DesktopAction::CycleItemWidth);
    fixture.configure();
    let current = fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .editor
        .as_ref()
        .unwrap();
    assert_eq!(current.owner, stale.owner);
    assert_eq!(current.revision, stale.revision);
    assert_ne!(current.projection, stale.projection);
    let barrier = fixture
        .desktop
        .worker
        .input(Input::NativeEdit(crate::native_input::Edit {
            context: stale,
            commit: Some("stale".into()),
            preedit: None,
            delete: None,
        }))
        .unwrap();
    advance(&mut fixture.desktop, barrier);
    assert_eq!(value(fixture, owner), (expected.into(), false));
    for _ in 0..3 {
        fixture.action(DesktopAction::CycleItemWidth);
    }
    fixture.configure();
    wait(fixture, &mut provider, owner, expected, false, serial);
    let serial = provider.client.input_serial;
    super::recovery::inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let entity = world
            .query::<(bevy::prelude::Entity, &EditableText)>()
            .iter(world)
            .find(|(entity, _)| entity.to_bits() == owner)
            .unwrap()
            .0;
        lince_interface::native::protect_text_input(world, entity);
    });
    wait(fixture, &mut provider, owner, expected, false, serial);
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
            .unwrap()
            .sensitive
    );
    method.commit_string("S".into());
    let serial = provider.client.input_serial;
    method.commit(serial);
    wait(
        fixture,
        &mut provider,
        owner,
        &format!("{expected}S"),
        false,
        serial,
    );
    method.delete_surrounding_text(1, 0);
    method.commit(provider.client.input_serial);
    client_sync(&mut fixture.desktop, &mut provider);
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
    assert_eq!(value(fixture, owner), (format!("{expected}S"), false));
    let serial = provider.client.input_serial;
    for pressed in [true, false] {
        fixture.desktop.key(14, pressed, 25).unwrap();
        for _ in 0..4 {
            fixture.desktop.dispatch().unwrap();
            fixture.flush();
        }
    }
    wait(fixture, &mut provider, owner, expected, false, serial);
    method.destroy();
    client_sync(&mut fixture.desktop, &mut provider);
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
}
