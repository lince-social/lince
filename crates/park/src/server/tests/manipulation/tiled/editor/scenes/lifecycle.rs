use super::*;
use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::ZwpInputMethodV2;

fn source(fixture: &mut Fixture, owner: u64) -> (Vec2, UVec2) {
    super::super::super::recovery::inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let field = field(world, owner);
        let camera = world
            .get::<bevy::ui::ComputedUiTargetCamera>(field)
            .unwrap()
            .get()
            .unwrap();
        let owner = world
            .get::<ui::topology::presentation::VisualOwner>(camera)
            .unwrap()
            .0;
        let surface = world
            .get::<ui::topology::presentation::Surface>(owner)
            .unwrap();
        (surface.size, surface.pixels)
    })
}

pub(super) fn resize(
    fixture: &mut Fixture,
    provider: &mut Harness,
    method: &ZwpInputMethodV2,
    owner: u64,
    expected: &str,
) {
    let serial = provider.client.input_serial;
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(serial);
    wait(fixture, provider, owner, expected, true, serial);
    let old = endpoint(fixture);
    let before = source(fixture, owner);
    let serial = provider.client.input_serial;
    fixture.action(DesktopAction::Resize(
        lince_interface::host::Direction::Right,
    ));
    fixture.configure();
    let after = source(fixture, owner);
    assert_ne!(
        after.0, before.0,
        "The resize did not change the source size"
    );
    assert_ne!(after.1, before.1, "The resize did not change source pixels");
    assert_eq!(value(fixture, owner), (expected.into(), true));
    let current = endpoint(fixture);
    assert_eq!(current.owner, old.owner);
    assert_eq!(current.projection, old.projection);
    assert!(current.same_text(&old));
    client_sync(&mut fixture.desktop, provider);
    client_sync(&mut fixture.desktop, provider);
    assert_eq!(provider.client.input_serial, serial);
    method.commit_string("é".into());
    method.commit(serial);
    wait(
        fixture,
        provider,
        owner,
        &format!("{expected}é"),
        false,
        serial,
    );
    let serial = provider.client.input_serial;
    method.delete_surrounding_text(2, 0);
    method.commit(serial);
    wait(fixture, provider, owner, expected, false, serial);
}

pub(super) fn replace(
    fixture: &mut Fixture,
    provider: &mut Harness,
    method: &ZwpInputMethodV2,
    id: [u8; 16],
    owner: u64,
    note: Entity,
    expected: &str,
) -> ([u8; 16], u64, Entity) {
    super::super::super::recovery::inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let entity = field(world, owner);
        world.get_mut::<Node>(entity).unwrap().display = bevy::ui::Display::Flex;
    });
    assert_eq!(focus(fixture, id).owner, owner);
    keys(
        fixture,
        &[(29, true), (107, true), (107, false), (29, false)],
    );
    let serial = provider.client.input_serial;
    wait(fixture, provider, owner, expected, false, serial);
    let serial = provider.client.input_serial;
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(serial);
    wait(fixture, provider, owner, expected, true, serial);
    let old = endpoint(fixture);
    const REPLACEMENT: &str = "Replacement native editor";
    let (id, replacement, note) = super::super::super::recovery::inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let root = world.get::<ChildOf>(note).unwrap().parent();
        let position = world.get::<ui::canvas::CanvasItem>(note).unwrap().position;
        let editor = field(world, owner);
        assert!(world.despawn(note));
        assert!(world.get::<EditableText>(editor).is_none());
        let note = ui::sand_store::spawn_sand(
            world,
            root,
            1,
            ui::sand_store::SandKind::EditableText,
            "Replacement native note",
            position,
        );
        let size = world.get::<ui::canvas::CanvasItem>(note).unwrap().size;
        ui::layout::configure(world, note, ui::layout::Rules::fixed(size)).unwrap();
        let editor = world
            .get::<ui::sand_store::StoredSand>(note)
            .unwrap()
            .content
            .unwrap();
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text(REPLACEMENT);
        let id = world.get::<ui::layout::LayoutBox>(note).unwrap().id;
        (id, editor.to_bits(), note)
    });
    stale(fixture, old.clone());
    assert_ne!(owner, replacement);
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
    assert_eq!(focus(fixture, id).owner, replacement);
    keys(
        fixture,
        &[(29, true), (107, true), (107, false), (29, false)],
    );
    stale(fixture, old);
    assert_eq!(value(fixture, replacement), (REPLACEMENT.into(), false));
    let serial = provider.client.input_serial;
    wait(fixture, provider, replacement, REPLACEMENT, false, serial);
    let serial = provider.client.input_serial;
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(serial);
    wait(fixture, provider, replacement, REPLACEMENT, true, serial);
    let serial = provider.client.input_serial;
    method.commit_string("é".into());
    method.commit(serial);
    wait(
        fixture,
        provider,
        replacement,
        &format!("{REPLACEMENT}é"),
        false,
        serial,
    );
    (id, replacement, note)
}
