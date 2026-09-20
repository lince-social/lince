use super::*;
use crate::actions::{Action, ActionButton};

#[derive(Component, Clone)]
pub(super) struct Sections {
    title: Entity,
    filled: Entity,
    toggle: Entity,
    arrow: Entity,
    empty: Entity,
    body: Entity,
    dates: Entity,
    threads: Entity,
    fiote: bool,
    observed: Value,
}

fn section(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn create(world: &mut World, row: Entity) -> Sections {
    let fiote = world
        .get::<RecordBinding>(row)
        .and_then(|binding| world.get::<crate::area::InfluenceArea>(binding.area))
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.fiote);
    let title = section(world, row);
    let filled = section(world, row);
    let toggle = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Borderless,
            crate::icons::Tooltip("Empty properties".into()),
            Node {
                width: percent(100),
                min_height: px(28),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            },
            ChildOf(row),
        ))
        .id();
    divider(world, toggle);
    let arrow = crate::edit_mode::label(world, toggle, "⌄", 18.0);
    divider(world, toggle);
    let empty = section(world, row);
    world.get_mut::<Node>(empty).unwrap().display = Display::None;
    let body = section(world, row);
    let threads = section(world, row);
    let dates = section(world, empty);
    world.get_mut::<Node>(dates).unwrap().flex_direction = FlexDirection::Row;
    world.get_mut::<Node>(dates).unwrap().column_gap = px(8);
    world
        .entity_mut(toggle)
        .insert(ActionButton::new(row, crate::actions![Toggle]));
    let sections = Sections {
        title,
        filled,
        toggle,
        arrow,
        empty,
        body,
        dates,
        threads,
        fiote,
        observed: Value::Null,
    };
    world.entity_mut(row).insert(sections.clone());
    sections
}

fn divider(world: &mut World, parent: Entity) {
    world.spawn((
        Node {
            height: px(1),
            flex_grow: 1.0,
            flex_basis: px(0),
            ..default()
        },
        crate::token_style::background(crate::tokens::Token::Accent),
        ChildOf(parent),
    ));
}

fn empty(property: &str, data: &Value) -> bool {
    let value = &data[property];
    match property {
        "start_date" | "due_date" => {
            empty("date", &serde_json::json!({"date":data["start_date"]}))
                && empty("date", &serde_json::json!({"date":data["due_date"]}))
        }
        "work_timer" => data["work_logs"].as_array().is_none_or(Vec::is_empty),
        "quantity_exact" | "spent_seconds" => {
            value.is_null() || value.as_str() == Some("0") || value.as_f64() == Some(0.0)
        }
        _ => {
            value.is_null()
                || value.as_str().is_some_and(|value| value.trim().is_empty())
                || value.as_array().is_some_and(Vec::is_empty)
        }
    }
}

impl Sections {
    pub(super) fn parent(&self, property: &str, data: &Value) -> Entity {
        match property {
            "head" => self.title,
            "body" => self.body,
            "threads" => self.threads,
            "start_date" | "due_date" => self.dates,
            _ if self.fiote => self.empty,
            _ if empty(property, data) => self.empty,
            _ => self.filled,
        }
    }
}

#[derive(Clone)]
struct Toggle;
impl Action for Toggle {
    fn apply(&self, world: &mut World, row: Entity) {
        let Some(sections) = world.get::<Sections>(row).cloned() else {
            return;
        };
        let opened = world.get::<Node>(sections.empty).unwrap().display == Display::None;
        world.get_mut::<Node>(sections.empty).unwrap().display =
            if opened { Display::Flex } else { Display::None };
        world.get_mut::<Text>(sections.arrow).unwrap().0 = if opened { "⌃" } else { "⌄" }.into();
    }
}

pub(super) fn update(world: &mut World) {
    let cards: Vec<_> = world
        .query::<(Entity, &Sections, &RecordBinding)>()
        .iter(world)
        .filter_map(|(row, sections, binding)| {
            let data = world
                .get_resource::<Runtime>()?
                .areas
                .get(&binding.area)?
                .data
                .iter()
                .find(|data| data["uid"].as_str() == Some(&binding.uid))?;
            (data != &sections.observed).then(|| (row, sections.clone(), data.clone()))
        })
        .collect();
    for (row, sections, data) in cards {
        arrange(world, row, &sections, &data);
    }
}

pub(super) fn arrange(world: &mut World, row: Entity, sections: &Sections, data: &Value) {
    world.get_mut::<Sections>(row).unwrap().observed = data.clone();
    let mut descendants = vec![row];
    let mut properties = Vec::new();
    while let Some(entity) = descendants.pop() {
        if let Some(property) = world.get::<rows::PropertyContainer>(entity) {
            properties.push((entity, property.0.clone()));
        } else if let Some(children) = world.get::<Children>(entity) {
            descendants.extend(children.iter());
        }
    }
    let mut filled = false;
    let mut unfilled = false;
    for (entity, property) in properties {
        let parent = sections.parent(&property, data);
        if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(parent) {
            world.entity_mut(entity).insert(ChildOf(parent));
        }
        if matches!(property.as_str(), "start_date" | "due_date") {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            node.width = px(0);
            node.flex_grow = 1.0;
            node.min_width = px(0);
        }
        if property == "threads" {
            unfilled |= empty("threads", data);
        } else if !matches!(property.as_str(), "head" | "body") {
            if sections.fiote || empty(&property, data) {
                unfilled = true;
            } else {
                filled = true;
            }
        }
    }
    let parent = if empty("threads", data) {
        sections.empty
    } else {
        row
    };
    if world.get::<ChildOf>(sections.threads).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.threads).insert(ChildOf(parent));
    }
    let parent = if sections.fiote || empty("start_date", data) {
        sections.empty
    } else {
        sections.filled
    };
    if world.get::<ChildOf>(sections.dates).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.dates).insert(ChildOf(parent));
    }
    world.get_mut::<Node>(sections.filled).unwrap().display =
        if filled { Display::Flex } else { Display::None };
    world.get_mut::<Node>(sections.toggle).unwrap().display = if unfilled {
        Display::Flex
    } else {
        Display::None
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fiote_castle_contains_management_without_conversation() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let mut config = Config::records();
        config.fiote = true;
        let uid = nucleus::new_uid("r");
        app.world_mut()
            .get_mut::<crate::area::InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config.clone());
        app.world_mut().resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(config),
                data: vec![json!({"uid":uid,"head":"Fiote","body":"Prompt","threads":[]})],
                ready: true,
                dirty: true,
                ..default()
            },
        );
        rows::reconcile(app.world_mut(), owner);
        assert_eq!(
            app.world_mut()
                .query::<&crate::thread_castle::ThreadCastle>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query::<&Sections>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Manage Fiote")
        );
    }

    #[test]
    fn empty_fields_collapse_and_dates_share_a_row_without_replacing_editors() {
        let (mut app, _, owner) = super::super::tests::fixture();
        app.add_plugins(crate::record_binding::RecordBindingPlugin);
        app.world_mut().resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(Config::records()),
                data: vec![json!({"uid":"record", "head":"Test", "body":"A **description**", "quantity_exact":"0"})],
                ready: true,
                dirty: true,
                ..default()
            },
        );
        rows::reconcile(app.world_mut(), owner);
        update(app.world_mut());
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
        let sections = app.world().get::<Sections>(row).unwrap().clone();
        assert!(
            !app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| matches!(
                    text.0.as_str(),
                    "Title" | "Description" | "Saved" | "Opening Record…" | "> Empty properties"
                ))
        );
        assert_eq!(
            app.world_mut()
                .query::<&crate::description::Description>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(
            app.world_mut()
                .query::<&bevy::text::EditableText>()
                .iter(app.world())
                .any(|text| text.value().to_string() == "A **description**")
        );
        assert_eq!(
            app.world().get::<Node>(sections.empty).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(sections.filled).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world()
                .get::<Node>(sections.dates)
                .unwrap()
                .flex_direction,
            FlexDirection::Row
        );
        let fields: HashMap<_, _> = app
            .world_mut()
            .query::<(Entity, &rows::PropertyContainer)>()
            .iter(app.world())
            .map(|(entity, property)| (property.0.clone(), entity))
            .collect();
        for (property, entity) in &fields {
            assert_eq!(
                app.world().get::<ChildOf>(*entity).unwrap().parent(),
                match property.as_str() {
                    "head" => sections.title,
                    "body" => sections.body,
                    "threads" => sections.threads,
                    "start_date" | "due_date" => sections.dates,
                    _ => sections.empty,
                }
            );
        }
        let children: Vec<_> = app.world().get::<Children>(row).unwrap().iter().collect();
        assert_eq!(
            children,
            [
                sections.title,
                sections.filled,
                sections.toggle,
                sections.empty,
                sections.body,
            ]
        );
        assert_eq!(
            app.world()
                .get::<ChildOf>(sections.threads)
                .unwrap()
                .parent(),
            sections.empty
        );
        Toggle.apply(app.world_mut(), row);
        assert_eq!(
            app.world().get::<Node>(sections.empty).unwrap().display,
            Display::Flex
        );
        app.world_mut()
            .resource_mut::<Runtime>()
            .areas
            .get_mut(&owner)
            .unwrap()
            .data[0]["start_date"] = json!("2026-09-19");
        update(app.world_mut());
        assert_eq!(
            app.world().get::<ChildOf>(sections.dates).unwrap().parent(),
            sections.filled
        );
        for property in ["start_date", "due_date"] {
            let entity = fields[property];
            assert_eq!(
                app.world().get::<ChildOf>(entity).unwrap().parent(),
                sections.dates
            );
            assert_eq!(app.world().get::<Node>(entity).unwrap().flex_grow, 1.0);
        }
        let threads = fields["threads"];
        let controls: Vec<_> = app
            .world()
            .get::<Children>(threads)
            .unwrap()
            .iter()
            .collect();
        for (data, parent) in [
            (json!([{"uid":"thread", "head":"Thread 1"}]), row),
            (json!([]), sections.empty),
        ] {
            app.world_mut()
                .resource_mut::<Runtime>()
                .areas
                .get_mut(&owner)
                .unwrap()
                .data[0]["threads"] = data;
            update(app.world_mut());
            assert_eq!(
                app.world()
                    .get::<ChildOf>(sections.threads)
                    .unwrap()
                    .parent(),
                parent
            );
            assert_eq!(
                app.world()
                    .get::<Children>(threads)
                    .unwrap()
                    .iter()
                    .collect::<Vec<_>>(),
                controls
            );
            if parent == row {
                assert_eq!(
                    app.world().get::<Children>(row).unwrap().last(),
                    Some(&sections.threads)
                );
            }
        }
    }
}
