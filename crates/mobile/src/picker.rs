use crate::app::{Intent, Mobile, button, input, label};
use bevy::prelude::*;

#[derive(Clone)]
pub enum Kind {
    Record,
    Person,
    Concept,
}

#[derive(Clone)]
pub struct Picker {
    pub scope: String,
    pub field: String,
    pub kind: Kind,
}

pub fn search(world: &mut World) -> Result<(), String> {
    let state = world.resource::<Mobile>();
    let picker = state.picker.as_ref().ok_or("Open a selector first")?;
    let text = state.draft("picker", "search", "");
    let mut query = crate::record::query(None, 31, &text);
    match picker.kind {
        Kind::Record => {}
        Kind::Person => query
            .filter
            .push(protein::Predicate::KindEq("person".into())),
        Kind::Concept => {
            query.source = protein::Source::Concept;
            query.fields = None;
            query.order.clear();
        }
    }
    crate::app::subscribe(world, "picker", query)
}

pub fn close(world: &mut World) -> Result<(), String> {
    world.resource_mut::<Mobile>().picker = None;
    world.resource_mut::<Mobile>().rows.remove("picker");
    world
        .resource_mut::<Mobile>()
        .drafts
        .remove("picker/search");
    crate::app::subscriptions(world)
}

pub fn render(world: &mut World, parent: Entity) {
    label(world, parent, "Choose a related item", 22.0);
    input(
        world,
        parent,
        "picker",
        "search",
        "Search by name",
        "",
        false,
    );
    button(world, parent, "Search", Intent::SearchPicker);
    button(world, parent, "Cancel", Intent::ClosePicker);
    let rows = world
        .resource::<Mobile>()
        .rows
        .get("picker")
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        label(world, parent, "No matching items", 16.0);
    }
    for row in rows.iter().take(30) {
        if let Some(uid) = row["uid"].as_str() {
            let name = row["head"]
                .as_str()
                .or_else(|| row["name"].as_str())
                .unwrap_or(uid);
            button(world, parent, name, Intent::SelectItem(uid.into()));
        }
    }
    if rows.len() > 30 {
        label(
            world,
            parent,
            "More matches exist. Narrow the search.",
            14.0,
        );
    }
}
