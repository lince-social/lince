use super::*;
use super::opening::{activate, click_entity, persisted, released};
use lince_interface::layout::desktop::{CompactGroup, PlacementId};

mod workflow;
mod clients;
mod lifetime;

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

pub(super) fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap();
    fixture.inspect(|_| ());
}

pub(super) fn named(fixture: &mut Fixture, name: &str) -> Entity {
    let name = name.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &Name)>().iter(world).find_map(|(entity, current)| (current.as_str() == name.as_str()).then_some(entity)).unwrap()
    })
}

pub(super) fn absent(fixture: &mut Fixture, name: &str) -> bool {
    let name = name.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        !world.query::<&Name>().iter(world).any(|current| current.as_str() == name.as_str())
    })
}

pub(super) fn spawn_note(fixture: &mut Fixture, title: &'static str, text: &'static str) -> (Entity, [u8; 16], Entity) {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        let sand = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, title, bevy::math::DVec2::ZERO);
        let field = world.get::<native::sand_store::StoredSand>(sand).unwrap().content.unwrap();
        world.get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text(text);
        if world.get::<native::layout::LayoutBox>(sand).is_none() {
            let size = world.get::<native::canvas::CanvasItem>(sand).unwrap().size;
            world.entity_mut(sand).insert(native::layout::LayoutBox::new(size));
        }
        (sand, world.get::<native::layout::LayoutBox>(sand).unwrap().id, field)
    })
}

pub(super) fn application(fixture: &mut Fixture, surface: SurfaceId) -> (Entity, [u8; 16]) {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &native::layout::desktop::ApplicationWindow, &native::layout::LayoutBox)>().iter(world)
            .find_map(|(entity, window, layout)| (window.surface == surface).then_some((entity, layout.id))).unwrap()
    })
}

fn group(fixture: &mut Fixture, placement: [u8; 16]) -> Option<CompactGroup> {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<&native::workspace::Workspaces>().iter(world).find_map(|spaces|
            spaces.entries.iter().find_map(|space| space.desktop.compact_group(PlacementId(placement)).cloned()))
    })
}

pub(super) fn state(fixture: &mut Fixture) -> serde_json::Value {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let spaces = world.query::<&native::workspace::Workspaces>().single(world).unwrap();
        serde_json::to_value(&spaces.entries).unwrap()
    })
}

pub(super) fn unique(fixture: &mut Fixture) {
    assert!(fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let mut identities = std::collections::BTreeSet::new();
        world.query::<&native::workspace::Workspaces>().iter(world).all(|spaces|
            spaces.entries.iter().all(|space| space.desktop.valid() && space.desktop.items().iter().all(|item| identities.insert(item.id))))
    }));
}

fn choose(fixture: &mut Fixture, entity: Entity) -> Entity {
    let choice = named(fixture, &format!("compact.choice.{}", entity.to_bits()));
    click_entity(fixture, choice, "Select compact member");
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::a11y::AccessibilityNode>(choice).unwrap().toggled()), Some(accesskit::Toggled::True));
    choice
}

fn confirm(fixture: &mut Fixture, add: bool) {
    activate(fixture, if add { "Add selected items" } else { "Create selected group" });
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal));
    assert!(absent(fixture, "park.compact-chooser"));
}

fn selector(fixture: &mut Fixture, entity: Entity) {
    let selector = named(fixture, &format!("compact.member.{}", entity.to_bits()));
    click_entity(fixture, selector, "Select visible compact header member");
}

fn property(fixture: &mut Fixture, inspector: Entity, caption: &str) {
    let button = windowed_fullscreen::button(fixture, inspector, caption);
    click_entity(fixture, button, caption);
}

pub(super) fn select_native(fixture: &mut Fixture, placement: [u8; 16]) {
    let workspace = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(&native::layout::LayoutBox, &native::workspace::WorkspaceMember)>().iter(world)
            .find_map(|(layout, member)| (layout.id == placement).then_some(member.0)).unwrap()
    });
    action(fixture, DesktopAction::SelectPlacement { workspace, item: Some(placement) });
}
