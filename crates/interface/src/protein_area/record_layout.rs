use super::*;
use crate::actions::{Action, ActionButton};

#[derive(Component, Clone)]
pub(super) struct Sections {
    title: Entity,
    identity: Entity,
    assertions: Entity,
    filled: Entity,
    toggle: Entity,
    arrow: Entity,
    empty: Entity,
    body: Entity,
    dates: Entity,
    threads: Entity,
    fiote: bool,
    presentation: crate::record_presentation::RecordPresentation,
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
    let presentation = world
        .get::<crate::record_presentation::RecordPresentation>(row)
        .copied()
        .unwrap_or_default();
    let fiote = world
        .get::<RecordBinding>(row)
        .and_then(|binding| world.get::<crate::area::InfluenceArea>(binding.area))
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.fiote);
    let title = section(world, row);
    let identity = section(world, row);
    {
        let mut node = world.get_mut::<Node>(identity).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    let assertions = section(world, row);
    let filled = section(world, row);
    let toolbar = section(world, row);
    world.get_mut::<Node>(toolbar).unwrap().flex_direction = FlexDirection::Row;
    let toggle = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Borderless,
            crate::icons::Tooltip("Show or hide properties".into()),
            Node {
                width: px(0),
                flex_grow: 1.0,
                min_height: px(28),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            },
            ChildOf(toolbar),
        ))
        .id();
    divider(world, toggle);
    let compact = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Borderless,
            crate::icons::Tooltip("Move filled properties into or out of the accordion".into()),
            ActionButton::new(row, crate::actions![Compact]),
            ChildOf(toolbar),
        ))
        .id();
    crate::edit_mode::label(world, compact, "⋯", 18.0);
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
        identity,
        assertions,
        filled,
        toggle,
        arrow,
        empty,
        body,
        dates,
        threads,
        fiote,
        presentation,
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
        "quantity" | "spent_seconds" => {
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
        if self.presentation.hide_filled
            && !matches!(property, "head" | "start_date" | "due_date" | "threads")
        {
            return self.empty;
        }
        match property {
            "head" => self.title,
            "quantity" | "slug" => self.identity,
            "assertions" => self.assertions,
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
        let mut presentation = world
            .get::<crate::record_presentation::RecordPresentation>(row)
            .copied()
            .unwrap_or(sections.presentation);
        presentation.expanded = !presentation.expanded;
        world.entity_mut(row).insert(presentation);
        crate::record_presentation::save(world, row);
        arrange(world, row, &sections, &sections.observed);
    }
}

#[derive(Clone)]
struct Compact;
impl Action for Compact {
    fn apply(&self, world: &mut World, row: Entity) {
        let Some(mut presentation) =
            world.get_mut::<crate::record_presentation::RecordPresentation>(row)
        else {
            return;
        };
        presentation.hide_filled = !presentation.hide_filled;
        presentation.expanded = false;
        crate::record_presentation::save(world, row);
        if let Some(sections) = world.get::<Sections>(row).cloned() {
            arrange(world, row, &sections, &sections.observed);
        }
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
            (data != &sections.observed
                || world.get::<crate::record_presentation::RecordPresentation>(row)
                    != Some(&sections.presentation))
            .then(|| (row, sections.clone(), data.clone()))
        })
        .collect();
    for (row, sections, data) in cards {
        arrange(world, row, &sections, &data);
    }
}

pub(super) fn arrange(world: &mut World, row: Entity, sections: &Sections, data: &Value) {
    let mut sections = sections.clone();
    sections.presentation = world
        .get::<crate::record_presentation::RecordPresentation>(row)
        .copied()
        .unwrap_or_default();
    world.get_mut::<Sections>(row).unwrap().presentation = sections.presentation;
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
        if matches!(
            property.as_str(),
            "start_date" | "due_date" | "quantity" | "slug"
        ) {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            let compact = sections.presentation.hide_filled
                && matches!(property.as_str(), "quantity" | "slug");
            node.width = if compact { percent(100) } else { px(0) };
            node.flex_grow = if compact { 0.0 } else { 1.0 };
            node.min_width = px(0);
        }
        if property == "threads" {
            unfilled |= empty("threads", data);
        } else if !matches!(
            property.as_str(),
            "head" | "body" | "quantity" | "slug" | "assertions"
        ) {
            if sections.fiote || empty(&property, data) {
                unfilled = true;
            } else {
                filled = true;
            }
        }
    }
    let parent = if sections.presentation.hide_filled || empty("threads", data) {
        sections.empty
    } else {
        row
    };
    if world.get::<ChildOf>(sections.threads).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.threads).insert(ChildOf(parent));
    }
    let parent = if sections.presentation.hide_filled || sections.fiote || empty("start_date", data)
    {
        sections.empty
    } else {
        sections.filled
    };
    if world.get::<ChildOf>(sections.dates).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.dates).insert(ChildOf(parent));
    }
    for entity in [sections.identity, sections.assertions, sections.body] {
        world.get_mut::<Node>(entity).unwrap().display = if sections.presentation.hide_filled {
            Display::None
        } else {
            Display::Flex
        };
    }
    world.get_mut::<Node>(sections.empty).unwrap().display = if sections.presentation.expanded {
        Display::Flex
    } else {
        Display::None
    };
    world.get_mut::<Text>(sections.arrow).unwrap().0 = if sections.presentation.expanded {
        "⌃"
    } else {
        "⌄"
    }
    .into();
    world.get_mut::<Node>(sections.filled).unwrap().display =
        if filled && !sections.presentation.hide_filled {
            Display::Flex
        } else {
            Display::None
        };
    world.get_mut::<Node>(sections.toggle).unwrap().display =
        if unfilled || sections.presentation.hide_filled {
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
    fn relation_records_keep_every_non_title_property_in_a_saved_accordion() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let config = crate::relation_castle::config();
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config.clone());
        app.world_mut().resource_mut::<Runtime>().areas.insert(owner, State {
            applied: Some(config),
            data: vec![json!({"uid":"record", "head":"Visible title", "slug":"filled", "body":"Filled body", "quantity":"7", "start_date":"2026-09-22", "due_date":"2026-09-23", "threads":[]})],
            ready: true, dirty: true, ..default()
        });
        rows::reconcile(app.world_mut(), owner);
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
        let sections = app.world().get::<Sections>(row).unwrap().clone();
        assert_eq!(
            app.world().get::<Node>(sections.empty).unwrap().display,
            Display::None
        );
        let fields: Vec<_> = app
            .world_mut()
            .query::<(Entity, &rows::PropertyContainer)>()
            .iter(app.world())
            .map(|(entity, field)| (entity, field.0.clone()))
            .collect();
        for (entity, property) in &fields {
            let expected = if property == "head" {
                sections.title
            } else {
                sections.empty
            };
            let mut parent = *entity;
            while parent != expected {
                parent = app.world().get::<ChildOf>(parent).unwrap().parent();
            }
        }
        Compact.apply(app.world_mut(), row);
        assert!(
            !app.world()
                .get::<crate::record_presentation::RecordPresentation>(row)
                .unwrap()
                .hide_filled
        );
        assert_eq!(
            app.world().get::<Node>(sections.body).unwrap().display,
            Display::Flex
        );
        Compact.apply(app.world_mut(), row);
        Toggle.apply(app.world_mut(), row);
        assert_eq!(
            app.world().get::<Node>(sections.empty).unwrap().display,
            Display::Flex
        );
        for (entity, _) in fields {
            assert!(app.world().get_entity(entity).is_ok());
        }
        let encoded = serde_json::to_vec(app.world().get::<InfluenceArea>(owner).unwrap()).unwrap();
        let restored: InfluenceArea = serde_json::from_slice(&encoded).unwrap();
        assert!(restored.validate());
        assert!(restored.records["record"].presentation.hide_filled);
        assert!(restored.records["record"].presentation.expanded);
        app.world_mut().despawn(row);
        app.world_mut().entity_mut(owner).insert(restored);
        app.world_mut()
            .resource_mut::<Runtime>()
            .areas
            .get_mut(&owner)
            .unwrap()
            .dirty = true;
        rows::reconcile(app.world_mut(), owner);
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
        let sections = app.world().get::<Sections>(row).unwrap();
        assert!(sections.presentation.hide_filled && sections.presentation.expanded);
        assert_eq!(
            app.world().get::<Node>(sections.empty).unwrap().display,
            Display::Flex
        );
    }

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
                data: vec![json!({"uid":"record", "head":"Test", "body":"A **description**", "quantity":"0"})],
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
            1
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
        assert!(
            app.world()
                .get::<crate::sand::Square>(fields["head"])
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<Node>(sections.identity)
                .unwrap()
                .flex_direction,
            FlexDirection::Row
        );
        assert!(
            app.world()
                .get::<crate::assertion_editor::Field>(fields["assertions"])
                .is_some()
        );
        for (property, entity) in &fields {
            assert_eq!(
                app.world().get::<ChildOf>(*entity).unwrap().parent(),
                match property.as_str() {
                    "head" => sections.title,
                    "quantity" | "slug" => sections.identity,
                    "assertions" => sections.assertions,
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
                sections.identity,
                sections.assertions,
                sections.filled,
                app.world()
                    .get::<ChildOf>(sections.toggle)
                    .unwrap()
                    .parent(),
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
