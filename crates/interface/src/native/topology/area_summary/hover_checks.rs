use super::*;
use bevy::picking::{backend::HitData, hover::HoverMap, pointer::PointerId};

fn unchanged(world: &mut World, rows: &[(Entity, Entity)]) {
    let ticks = |world: &World, (row, button): (Entity, Entity)| [
        world.entity(row).get_ref::<Pickable>().unwrap().last_changed().get(),
        world.entity(button).get_ref::<Node>().unwrap().last_changed().get(),
    ];
    let before = rows.iter().map(|row| ticks(world, *row)).collect::<Vec<_>>();
    let popup = world.get_resource::<HoverPreview>().and_then(|preview| preview.popup);
    for _ in 0..8 {
        world.increment_change_tick();
        hover(world);
        for (row, expected) in rows.iter().zip(&before) {
            assert_eq!(ticks(world, *row), *expected, "Stable Area hover dirtied its retained interface");
        }
        assert_eq!(world.get_resource::<HoverPreview>().and_then(|preview| preview.popup), popup);
    }
}

#[cfg_attr(test, test)]
fn stable_behavior_hover_retains_layout_and_follows_pointer_and_edit_changes() {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins((
            crate::native::workspace::WorkspacePlugin,
            crate::native::edit_mode::EditModePlugin,
        ));
    let root = app.world_mut().spawn(crate::native::container::BoxRoot).id();
    app.update();
    let world = app.world_mut();
    world.insert_resource(HoverMap::default());
    for _ in 0..2 {
        let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(300.0));
        area.changes.enter.quantity = Some("+1".into());
        area.changes.leave.quantity = Some("-1".into());
        world.spawn((area, ChildOf(root)));
    }
    update(world);
    let rows = world.query::<(Entity, &BehaviorRow)>().iter(world)
        .map(|(entity, row)| (entity, row.button)).collect::<Vec<_>>();
    assert_eq!(rows.len(), 4);
    hover(world);
    unchanged(world, &rows);
    world.get_mut::<crate::native::edit_mode::EditMode>(root).unwrap().enabled = true;
    hover(world);
    for (row, button) in &rows {
        assert_eq!(world.get::<Pickable>(*row), Some(&Pickable::default()));
        assert_eq!(world.get::<Node>(*button).unwrap().display, Display::None);
    }
    unchanged(world, &rows);
    for (pointer, target) in [(PointerId::Mouse, 0), (super::super::input::CONTENT_POINTER, 1)] {
        world.resource_mut::<HoverMap>().clear();
        world.resource_mut::<HoverMap>().entry(pointer).or_default()
            .insert(rows[target].1, HitData::new(root, 0.0, None, None));
        hover(world);
        assert!(over_behavior(world));
        assert_eq!(world.resource::<HoverPreview>().row, Some(rows[target].0));
        for (index, (_, button)) in rows.iter().enumerate() {
            assert_eq!(world.get::<Node>(*button).unwrap().display,
                if index == target { Display::Flex } else { Display::None });
        }
        unchanged(world, &rows);
        let popup = world.resource::<HoverPreview>().popup.unwrap();
        world.despawn(popup);
        hover(world);
        assert_ne!(world.resource::<HoverPreview>().popup, Some(popup));
        unchanged(world, &rows);
    }
    world.get_mut::<crate::native::edit_mode::EditMode>(root).unwrap().enabled = false;
    let popup = world.resource::<HoverPreview>().popup.unwrap();
    hover(world);
    assert!(!over_behavior(world));
    assert!(world.get_entity(popup).is_err());
    for (row, button) in &rows {
        assert_eq!(world.get::<Pickable>(*row), Some(&Pickable::IGNORE));
        assert_eq!(world.get::<Node>(*button).unwrap().display, Display::None);
    }
    unchanged(world, &rows);
    world.get_mut::<crate::native::edit_mode::EditMode>(root).unwrap().enabled = true;
    hover(world);
    assert!(over_behavior(world));
    world.resource_mut::<HoverMap>().clear();
    hover(world);
    assert!(!over_behavior(world));
    assert!(world.resource::<HoverPreview>().popup.is_none());
    unchanged(world, &rows);
}

crate::native::laboratory_cases! {
    stable_behavior_hover_retains_layout_and_follows_pointer_and_edit_changes,
}
