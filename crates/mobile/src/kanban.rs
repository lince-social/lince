use bevy::prelude::*;

#[derive(Component)]
pub struct Handle(pub String);

#[derive(Component)]
pub struct Destination(pub usize);

#[derive(Resource, Default)]
pub struct Dragging(pub bool);

pub struct KanbanPlugin;

pub fn changes(
    state: &crate::app::Mobile,
    column: usize,
) -> Result<engine::area_transition::RecordChanges, String> {
    let (_, _, quantity) = lince_interface::records::KANBAN_COLUMNS
        .get(column)
        .ok_or("Choose a column")?;
    let scope = format!("kanban/{column}");
    let names = |field| {
        state
            .draft(&scope, field, "")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let changes = engine::area_transition::RecordChanges {
        quantity: Some(state.draft(&scope, "quantity", &quantity.to_string())),
        assert: names("assert"),
        retract: names("retract"),
        assign: names("assign"),
        unassign: names("unassign"),
    };
    if !changes.validate() {
        return Err("Check the quantity and use up to 16 property changes".into());
    }
    Ok(changes)
}

pub fn settings(world: &mut World, parent: Entity) {
    use crate::app::{Intent, Mobile, button, input, label};
    let column = world.resource::<Mobile>().kanban_column;
    let Some((title, _, quantity)) = lince_interface::records::KANBAN_COLUMNS.get(column) else {
        return;
    };
    button(
        world,
        parent,
        "Column property changes",
        Intent::ColumnSettings,
    );
    if !world.resource::<Mobile>().column_settings {
        return;
    }
    label(world, parent, &format!("When moving to {title}"), 18.0);
    label(
        world,
        parent,
        "Changes apply together. Records immediately appear wherever their Protein query matches. These controls are saved on this profile.",
        14.0,
    );
    let scope = format!("kanban/{column}");
    input(
        world,
        parent,
        &scope,
        "quantity",
        "Quantity",
        &quantity.to_string(),
        false,
    );
    for (field, title) in [
        ("assert", "Add properties (comma separated)"),
        ("retract", "Remove properties"),
        ("assign", "Assign People"),
        ("unassign", "Unassign People"),
    ] {
        input(world, parent, &scope, field, title, "", false);
    }
}

impl Plugin for KanbanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dragging>()
            .add_observer(start)
            .add_observer(drop_card)
            .add_observer(end);
    }
}

fn ancestor<T: Component>(
    mut entity: Entity,
    items: &Query<&T>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    for _ in 0..16 {
        if items.contains(entity) {
            return Some(entity);
        }
        entity = parents.get(entity).ok()?.parent();
    }
    None
}

fn start(
    mut event: On<Pointer<DragStart>>,
    handles: Query<&Handle>,
    parents: Query<&ChildOf>,
    state: Res<crate::app::Mobile>,
    mut dragging: ResMut<Dragging>,
) {
    if let Some(entity) = ancestor(event.entity, &handles, &parents) {
        if state.moving.as_ref() == Some(&handles.get(entity).expect("handle").0) {
            dragging.0 = true;
            event.propagate(false);
        }
    }
}

fn end(_: On<Pointer<DragEnd>>, mut dragging: ResMut<Dragging>) {
    dragging.0 = false;
}

fn drop_card(
    mut event: On<Pointer<DragDrop>>,
    handles: Query<&Handle>,
    destinations: Query<&Destination>,
    parents: Query<&ChildOf>,
    state: Res<crate::app::Mobile>,
    mut commands: Commands,
) {
    let Some(handle) = ancestor(event.dropped, &handles, &parents) else {
        return;
    };
    let Some(destination) = ancestor(event.entity, &destinations, &parents) else {
        return;
    };
    let uid = handles.get(handle).expect("handle").0.clone();
    if state.moving.as_ref() != Some(&uid) {
        return;
    }
    let column = destinations.get(destination).expect("destination").0;
    event.propagate(false);
    commands.queue(move |world: &mut World| {
        crate::app::queue_intent(world, crate::app::Intent::Move(uid, column))
    });
}
