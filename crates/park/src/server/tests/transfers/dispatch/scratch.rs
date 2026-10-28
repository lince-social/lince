use super::*;
use super::compact::{action, application, absent, spawn_note, select_native, state, unique};
use super::opening::{activate, click_entity, persisted, released};
use lince_interface::layout::desktop::{PlacementId, Scratch};

mod workflow;
mod clients;
mod lifetime;

fn association(fixture: &mut Fixture, id: [u8; 16]) -> Option<(u64, Scratch)> {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<&native::workspace::Workspaces>().iter(world).find_map(|spaces|
            spaces.entries.iter().find_map(|workspace| workspace.desktop.scratch(PlacementId(id)).cloned().map(|scratch| (workspace.id, scratch))))
    })
}

fn choose(fixture: &mut Fixture, target: Entity) -> Entity {
    let choice = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &Name, &native::topology::presentation::VisualOwner)>().iter(world)
            .find_map(|(entity, name, owner)| (name.as_str().starts_with("scratch.choice.") && owner.0 == target).then_some(entity)).unwrap()
    });
    click_entity(fixture, choice, "Choose existing scratch item");
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::a11y::AccessibilityNode>(choice).unwrap().toggled()), Some(accesskit::Toggled::True));
    choice
}

fn confirm(fixture: &mut Fixture) {
    activate(fixture, "Show selected item here");
    assert!(absent(fixture, "park.scratch-chooser"));
}

fn property(fixture: &mut Fixture, inspector: Entity, caption: &str) {
    let button = windowed_fullscreen::button(fixture, inspector, caption);
    click_entity(fixture, button, caption);
}

fn native_entity(fixture: &mut Fixture, id: [u8; 16]) -> Entity {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let entity = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            world.query::<(Entity, &native::layout::LayoutBox, &native::sand_store::StoredSand)>().iter(world)
                .find_map(|(entity, layout, _)| (layout.id == id).then_some(entity))
        });
        if let Some(entity) = entity { return entity; }
        assert!(Instant::now() < deadline, "The selected ordinary workspace did not materialize its retained native item");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn content(fixture: &mut Fixture, entity: Entity) -> String {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let field = world.get::<native::sand_store::StoredSand>(entity).unwrap().content.unwrap();
        world.get::<bevy::text::EditableText>(field).unwrap().value().to_string()
    })
}
