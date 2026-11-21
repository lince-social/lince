// non-lean-yet
// Scope: fn fixture, fn column_labels_follow_late_creation_reparenting_and_removed_roles, fn
//   columns_with_geometry_keep_their_headers_when_another_column_loses_its_geometry.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 34, 63.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (23, 34, 35, 37, 62, 63, 64, 69, 78, 79, 80, 81, 82, 87, 94, 101, 104, 105, 106,
//   114, 115, 119).

use super::*;

pub(super) fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins((
            crate::native::protein_area::ProteinAreaPlugin,
            crate::native::area_mutation::AreaMutationPlugin,
            crate::native::layout::LayoutPlugin,
            KanbanPlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            crate::native::container::BoxRoot,
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let board = spawn(app.world_mut(), root, 1, DVec2::ZERO).unwrap();
    (app, root, board)
}

#[cfg_attr(test, test)]
fn column_labels_follow_late_creation_reparenting_and_removed_roles() {
    #[derive(Component)]
    struct Late;

    let (mut app, root, owner) = fixture();
    app.update();
    let first = app.world().get::<Kanban>(owner).unwrap().clone();
    let column = area(app.world(), owner, &first.columns[0].area).unwrap();
    {
        let mut column = app.world_mut().get_mut::<InfluenceArea>(column).unwrap();
        column.name = "First".into();
        column.changes.enter.quantity = Some("7".into());
    }
    let direct = app
        .world_mut()
        .spawn((Text::new("pending"), ColumnTitle, ChildOf(owner), Late))
        .id();
    let row = app
        .world_mut()
        .spawn((Node::default(), ChildOf(owner)))
        .id();
    let nested = app
        .world_mut()
        .spawn((Text::new("pending"), Status, ChildOf(row), Late))
        .id();
    let ordinary = app
        .world_mut()
        .spawn((Text::new("ordinary"), ChildOf(row)))
        .id();
    let inner = app.world_mut().spawn((Node::default(), ChildOf(row))).id();
    let deep = app
        .world_mut()
        .spawn((Text::new("deep"), ColumnTitle, ChildOf(inner)))
        .id();
    let other = spawn(app.world_mut(), root, 1, DVec2::new(0.0, 1200.0)).unwrap();
    let second = app.world().get::<Kanban>(other).unwrap().clone();
    let other_column = area(app.world(), other, &second.columns[0].area).unwrap();
    {
        let mut column = app
            .world_mut()
            .get_mut::<InfluenceArea>(other_column)
            .unwrap();
        column.name = "Other".into();
        column.changes.enter.quantity = Some("9".into());
    }
    let other_title = app
        .world_mut()
        .spawn((Text::new("pending"), ColumnTitle, ChildOf(other), Late))
        .id();
    app.update();
    assert_eq!(app.world().get::<Text>(direct).unwrap().0, "First: 7");
    assert_eq!(app.world().get::<Text>(nested).unwrap().0, "0 records");
    assert_eq!(app.world().get::<Text>(other_title).unwrap().0, "Other: 9");
    assert_eq!(app.world().get::<Text>(ordinary).unwrap().0, "ordinary");
    assert_eq!(app.world().get::<Text>(deep).unwrap().0, "deep");
    let changed = app
        .world()
        .entity(direct)
        .get_ref::<Text>()
        .unwrap()
        .last_changed();
    app.update();
    assert_eq!(
        app.world()
            .entity(direct)
            .get_ref::<Text>()
            .unwrap()
            .last_changed(),
        changed
    );

    app.world_mut().entity_mut(direct).insert(ChildOf(other));
    app.world_mut().entity_mut(nested).remove::<Status>();
    app.world_mut().get_mut::<Text>(nested).unwrap().0 = "removed role".into();
    app.world_mut().entity_mut(ordinary).insert(ColumnTitle);
    app.update();
    assert_eq!(app.world().get::<Text>(direct).unwrap().0, "Other: 9");
    assert_eq!(app.world().get::<Text>(nested).unwrap().0, "removed role");
    assert_eq!(app.world().get::<Text>(ordinary).unwrap().0, "First: 7");

    app.world_mut().entity_mut(direct).insert(Status);
    app.world_mut()
        .entity_mut(nested)
        .insert((Status, ChildOf(root)));
    app.world_mut().despawn(other_title);
    app.update();
    assert_eq!(app.world().get::<Text>(direct).unwrap().0, "0 records");
    assert_eq!(app.world().get::<Text>(nested).unwrap().0, "removed role");
    assert!(app.world().get_entity(other_title).is_err());
    app.world_mut().entity_mut(direct).remove::<Status>();
    app.update();
    assert_eq!(app.world().get::<Text>(direct).unwrap().0, "Other: 9");
}

crate::native::laboratory_cases! {
    column_labels_follow_late_creation_reparenting_and_removed_roles,
}

#[test]
fn columns_with_geometry_keep_their_headers_when_another_column_loses_its_geometry() {
    let (mut app, _, owner) = fixture();
    let world = app.world_mut();
    let column = column_entity(world, owner, 0).expect("the prepared board has its first column");
    world.entity_mut(column).remove::<CanvasItem>();

    maintain(world, owner);

    assert!(world.get::<CanvasItem>(column).is_none());
    assert!(
        world
            .query::<&Part>()
            .iter(world)
            .any(|part| part.owner == owner && part.column == 1)
    );
}
