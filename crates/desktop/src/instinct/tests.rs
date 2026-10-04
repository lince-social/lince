use super::*;
use crate::container::BoxRoot;
#[cfg(feature = "instinct")]
use crate::{actions::ActionButton, icons::Tooltip};

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>();
    let root = app
        .world_mut()
        .spawn((BoxRoot, crate::workspace::Workspaces::default()))
        .id();
    (app, root)
}

#[cfg(feature = "instinct")]
fn click(world: &mut World, owner: Entity, tip: &str) {
    let action = world
        .query::<(&Tooltip, &ActionButton)>()
        .iter(world)
        .find(|(tooltip, button)| tooltip.0 == tip && button.target == owner)
        .map(|(_, button)| button.actions.clone())
        .unwrap();
    action.run(world, owner);
    world.flush();
}

#[cfg_attr(test, test)]
fn load_errors_are_shown_without_panicking() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let error = "institute/anicca/Broken.lingua: malformed Record";
    world.insert_resource(Book(Arc::from([]), Arc::from([]), Some(error.into())));
    spawn(world, root, 1, DVec2::ZERO, Instinct::default());
    world.flush();
    let text: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.as_str())
        .collect();
    assert!(text.contains(&"Instinct could not be loaded."));
    assert!(text.contains(&error));
}

#[cfg(feature = "instinct")]
#[cfg_attr(test, test)]
fn pages_embed_records_and_navigation_is_independent_and_bounded() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let first = spawn(world, root, 1, DVec2::ZERO, Instinct::default());
    let other = spawn(world, root, 1, DVec2::new(900.0, 0.0), Instinct::default());
    world.flush();
    let book = world.resource::<Book>().0.clone();
    assert_eq!(book[0].id, "learn-philosophy");
    assert_eq!(book[1].id, "learn-installation");
    assert_eq!(
        book.iter()
            .take(65)
            .map(|page| page.id.as_str())
            .collect::<Vec<_>>(),
        lince_interface::handbook::PAGES
            .iter()
            .map(|page| page.slug)
            .collect::<Vec<_>>()
    );
    assert!(!book.iter().any(|page| page.id == "ailuros"));
    let before = world.entities().count_spawned();
    for _ in 0..8 {
        click(world, first, "Next chapter");
        assert_eq!(
            world.get::<Instinct>(first).unwrap().page.as_deref(),
            Some("learn-installation")
        );
        assert_eq!(
            world.get::<Instinct>(other).unwrap().page.as_deref(),
            Some("learn-philosophy")
        );
        click(world, first, "Previous chapter");
    }
    assert_eq!(world.entities().count_spawned(), before);
    assert!(Arc::ptr_eq(&book, &world.resource::<Book>().0));
    Command::Page("missing".into()).apply(world, first);
    assert_eq!(
        world.get::<Instinct>(first).unwrap().page.as_deref(),
        Some("learn-philosophy")
    );
    click(world, first, &book.last().unwrap().title);
    let disabled = world
        .query::<(&Tooltip, &ActionButton, &bevy::ui::InteractionDisabled)>()
        .iter(world)
        .any(|(tip, button, _)| tip.0 == "Next chapter" && button.target == first);
    assert!(disabled);
    assert!(
        world
            .get::<crate::sand_store::SandCredits>(first)
            .unwrap()
            .0
            .iter()
            .all(|credit| !credit.license.is_empty() && !credit.author.is_empty())
    );
}

#[cfg(feature = "instinct")]
#[cfg_attr(test, test)]
fn saved_reader_restores_geometry_and_selection_without_copying_the_book() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let entity = spawn(
        world,
        root,
        1,
        DVec2::new(123.0, -45.0),
        Instinct {
            page: Some("tool".into()),
        },
    );
    world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::new(850.0, 640.0);
    let saved = snapshot(world, root);
    let encoded = serde_json::to_string(&saved).unwrap();
    assert!(!encoded.contains("Everything is a Record"));
    assert!(!encoded.contains("It is possible to find"));
    let saved: Vec<SavedInstinct> = serde_json::from_str(&encoded).unwrap();
    assert!(saved[0].valid());
    world.despawn(entity);
    saved.into_iter().next().unwrap().restore(world, root);
    let (item, state) = world
        .query::<(&CanvasItem, &Instinct)>()
        .single(world)
        .unwrap();
    assert_eq!(item.position, DVec2::new(123.0, -45.0));
    assert_eq!(item.size, Vec2::new(850.0, 640.0));
    assert_eq!(state.page.as_deref(), Some("tool"));
    let entity = spawn(
        world,
        root,
        1,
        DVec2::ZERO,
        Instinct {
            page: Some("removed".into()),
        },
    );
    assert_eq!(
        world.get::<Instinct>(entity).unwrap().page.as_deref(),
        Some("learn-philosophy")
    );
}

crate::laboratory_cases! {
    load_errors_are_shown_without_panicking,
    #[cfg(feature = "instinct")]
    child_records_are_ordered_indented_and_restore_their_section,
    #[cfg(feature = "instinct")]
    pages_embed_records_and_navigation_is_independent_and_bounded,
    #[cfg(feature = "instinct")]
    saved_reader_restores_geometry_and_selection_without_copying_the_book,
}

#[cfg(feature = "instinct")]
#[cfg_attr(test, test)]
fn child_records_are_ordered_indented_and_restore_their_section() {
    let (mut app, root) = fixture();
    let world = app.world_mut();
    let reader = spawn(world, root, 1, DVec2::ZERO, Instinct::default());
    let entries = world.resource::<Book>().1.clone();
    let expected = entries
        .iter()
        .find(|entry| entry.id == "area-record-actions")
        .unwrap();
    Command::Page(expected.id.clone()).apply(world, reader);
    let nav = world.get::<View>(reader).unwrap().nav.unwrap();
    world.get_mut::<ScrollPosition>(nav).unwrap().0.y = 200.0;
    let saved = snapshot(world, root).pop().unwrap();
    world.despawn(reader);
    saved.restore(world, root);
    let restored = world
        .query_filtered::<Entity, With<Instinct>>()
        .single(world)
        .unwrap();
    assert_eq!(
        world.get::<Instinct>(restored).unwrap().page.as_deref(),
        Some("area-record-actions")
    );
    assert_eq!(
        world
            .resource::<Book>()
            .1
            .iter()
            .filter(|entry| entry.id == expected.id)
            .count(),
        1
    );
}
