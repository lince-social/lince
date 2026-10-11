use super::*;
use crate::actions::{Action, ActionButton};

#[derive(Component)]
pub(super) struct RecordActions;

#[derive(Component, Clone)]
pub(super) struct Sections {
    title: Entity,
    identity: Entity,
    assertions: Entity,
    filled: Entity,
    toolbar: Entity,
    toggle: Entity,
    arrow: Entity,
    compact: Entity,
    compact_label: Entity,
    empty: Entity,
    body: Entity,
    dates: Entity,
    threads: Entity,
    actions: Entity,
    groups: [Entity; 4],
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
    world
        .entity_mut(title)
        .insert(crate::token_style::border(crate::tokens::Token::Accent));
    let identity = section(world, row);
    {
        let mut node = world.get_mut::<Node>(identity).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    let assertions = section(world, row);
    let filled = section(world, row);
    {
        let mut node = world.get_mut::<Node>(filled).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    let toolbar = section(world, row);
    {
        let mut node = world.get_mut::<Node>(toolbar).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.align_items = AlignItems::Center;
        node.column_gap = px(8);
    }
    divider(world, toolbar);
    let toggle = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Borderless,
            crate::icons::Tooltip("Show or hide properties".into()),
            Node {
                min_height: px(28),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            },
            ChildOf(toolbar),
        ))
        .id();
    let arrow = crate::edit_mode::label(world, toggle, "⌄", 18.0);
    let compact = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Borderless,
            crate::icons::Tooltip("Move filled properties into or out of the accordion".into()),
            ActionButton::new(row, crate::actions![Compact]),
            ChildOf(toolbar),
        ))
        .id();
    let compact_label = crate::edit_mode::label(world, compact, "Show filled properties", 14.0);
    divider(world, toolbar);
    let empty = section(world, row);
    {
        let mut node = world.get_mut::<Node>(empty).unwrap();
        node.display = Display::None;
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    let groups = ["Identity and description", "Planning", "Work", "Discussion"].map(|title| {
        let group = section(world, empty);
        {
            let mut node = world.get_mut::<Node>(group).unwrap();
            node.flex_direction = FlexDirection::Row;
            node.flex_wrap = FlexWrap::Wrap;
            node.column_gap = px(8);
        }
        let label = crate::edit_mode::label(world, group, title, 16.0);
        world.get_mut::<Node>(label).unwrap().width = percent(100);
        group
    });
    let body = section(world, row);
    let threads = section(world, row);
    let dates = section(world, empty);
    world.get_mut::<Node>(dates).unwrap().flex_direction = FlexDirection::Row;
    world.get_mut::<Node>(dates).unwrap().column_gap = px(8);
    world.get_mut::<Node>(dates).unwrap().flex_wrap = FlexWrap::Wrap;
    let actions = section(world, row);
    world.entity_mut(actions).insert(RecordActions);
    let label = crate::edit_mode::label(world, actions, "Record actions", 16.0);
    world.get_mut::<Node>(label).unwrap().width = percent(100);
    {
        let mut node = world.get_mut::<Node>(actions).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    world
        .entity_mut(toggle)
        .insert(ActionButton::new(row, crate::actions![Toggle]));
    let sections = Sections {
        title,
        identity,
        assertions,
        filled,
        toolbar,
        toggle,
        arrow,
        compact,
        compact_label,
        empty,
        body,
        dates,
        threads,
        actions,
        groups,
        fiote,
        presentation,
        observed: Value::Null,
    };
    world.entity_mut(row).insert(sections.clone());
    sections
}

pub(super) fn action_parent(world: &World, row: Entity) -> Entity {
    world
        .get::<Sections>(row)
        .map_or(row, |sections| sections.actions)
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
        if self.presentation.expanded {
            return match property {
                "head" => self.title,
                "start_date" | "due_date" => self.dates,
                "threads" => self.threads,
                "assignees" | "estimate_min" => self.groups[1],
                "work_logs" | "work_timer" | "spent_seconds" => self.groups[2],
                _ => self.groups[0],
            };
        }
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
    let cards: Vec<_> = world
        .query::<(Entity, &Sections)>()
        .iter(world)
        .map(|(row, sections)| (row, sections.toolbar, sections.presentation))
        .collect();
    for (row, toolbar, presentation) in cards {
        if super::presentation::current(world, row).is_none() {
            toolbar_visibility(world, row, toolbar, presentation);
        }
    }
}

fn toolbar_visibility(
    world: &mut World,
    row: Entity,
    toolbar: Entity,
    presentation: crate::record_presentation::RecordPresentation,
) {
    let contains = |mut entity| loop {
        if entity == row {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    };
    let relations = world
        .get::<RecordBinding>(row)
        .and_then(|binding| world.get::<InfluenceArea>(binding.area))
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.relations);
    let active = !relations
        && world
            .get_resource::<bevy::picking::hover::HoverMap>()
            .is_some_and(|hover| {
                hover
                    .values()
                    .any(|hits| hits.keys().copied().any(contains))
            })
        || world
            .get_resource::<bevy::input_focus::InputFocus>()
            .and_then(|focus| focus.get())
            .is_some_and(contains);
    let display = if presentation.hide_filled && !presentation.expanded && !active {
        Display::None
    } else {
        Display::Flex
    };
    let mut node = world.get_mut::<Node>(toolbar).unwrap();
    let mut next = node.clone();
    next.display = display;
    let minimal = presentation.hide_filled && !presentation.expanded;
    next.position_type = if minimal {
        PositionType::Absolute
    } else {
        PositionType::Relative
    };
    next.width = if minimal { Val::Auto } else { percent(100) };
    next.right = if minimal { px(8) } else { Val::Auto };
    next.top = if minimal { px(4) } else { Val::Auto };
    node.set_if_neq(next);
    let compact = world.get::<Sections>(row).unwrap().compact;
    world.get_mut::<Node>(compact).unwrap().display = if minimal {
        Display::None
    } else {
        Display::Flex
    };
}

pub(super) fn arrange(world: &mut World, row: Entity, sections: &Sections, data: &Value) {
    if super::presentation::current(world, row).is_some() {
        return;
    }
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
    if sections.presentation.expanded {
        let order = [
            "head",
            "slug",
            "quantity",
            "assertions",
            "body",
            "assignees",
            "start_date",
            "due_date",
            "estimate_min",
            "work_logs",
            "work_timer",
            "spent_seconds",
            "threads",
        ];
        properties.sort_by_key(|(_, property)| {
            order
                .iter()
                .position(|key| *key == property)
                .unwrap_or(order.len())
        });
    }
    let mut ordering: HashMap<_, _> = properties
        .iter()
        .enumerate()
        .map(|(index, (entity, _))| (*entity, index + 1))
        .collect();
    for (entity, key) in [
        (sections.dates, "start_date"),
        (sections.threads, "threads"),
    ] {
        ordering.insert(
            entity,
            properties
                .iter()
                .position(|(_, property)| property == key)
                .map_or(usize::MAX, |index| index + 1),
        );
    }
    let mut filled = false;
    let mut unfilled = false;
    for (entity, property) in properties {
        world.get_mut::<Node>(entity).unwrap().display = if property != "head"
            && sections.presentation.hide_filled
            && !sections.presentation.expanded
        {
            Display::None
        } else {
            Display::Flex
        };
        let parent = sections.parent(&property, data);
        if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(parent) {
            world.entity_mut(entity).insert(ChildOf(parent));
        }
        if !matches!(
            property.as_str(),
            "head" | "body" | "assertions" | "threads"
        ) {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            node.width = Val::Auto;
            node.flex_basis = px(match property.as_str() {
                "start_date" | "due_date" => 240.0,
                "work_logs" | "work_timer" => 280.0,
                _ => 140.0,
            });
            node.flex_grow = 1.0;
            node.flex_shrink = 1.0;
            node.min_width = px(0);
            node.max_width = percent(100);
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
    let parent = if sections.presentation.expanded {
        sections.groups[3]
    } else if sections.presentation.hide_filled || empty("threads", data) {
        sections.empty
    } else {
        row
    };
    if world.get::<ChildOf>(sections.threads).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.threads).insert(ChildOf(parent));
    }
    let parent = if sections.presentation.expanded {
        sections.groups[1]
    } else if sections.presentation.hide_filled || sections.fiote || empty("start_date", data) {
        sections.empty
    } else {
        sections.filled
    };
    if world.get::<ChildOf>(sections.dates).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.dates).insert(ChildOf(parent));
    }
    if sections.presentation.expanded {
        for parent in sections.groups.into_iter().chain([sections.dates]) {
            let original: Vec<_> = world
                .get::<Children>(parent)
                .map(|children| children.iter().collect())
                .unwrap_or_default();
            let mut ordered = original.clone();
            ordered.sort_by_key(|entity| ordering.get(entity).copied().unwrap_or(0));
            if ordered != original {
                world.entity_mut(parent).replace_children(&ordered);
            }
        }
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
    let minimal = sections.presentation.hide_filled && !sections.presentation.expanded;
    let managed = [
        sections.title,
        sections.identity,
        sections.assertions,
        sections.filled,
        sections.toolbar,
        sections.empty,
        sections.body,
        sections.threads,
        sections.actions,
    ];
    let controls: Vec<_> = world
        .get::<Children>(row)
        .into_iter()
        .flatten()
        .copied()
        .filter(|entity| {
            !managed.contains(entity)
                && world
                    .get::<Node>(*entity)
                    .is_some_and(|node| node.position_type != PositionType::Absolute)
        })
        .collect();
    for entity in controls {
        world.entity_mut(entity).insert(ChildOf(sections.actions));
    }
    world.get_mut::<Node>(sections.actions).unwrap().display = if minimal {
        Display::None
    } else {
        Display::Flex
    };
    world.get_mut::<Node>(sections.title).unwrap().border =
        UiRect::bottom(px(if minimal { 0 } else { 1 }));
    world.get_mut::<Text>(sections.compact_label).unwrap().0 = if sections.presentation.hide_filled
    {
        "Show filled properties"
    } else {
        "Title only"
    }
    .into();
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
    toolbar_visibility(world, row, sections.toolbar, sections.presentation);
    super::presentation::refresh_fields(world, row);
}

pub(super) fn restore(world: &mut World, row: Entity) {
    if let Some(sections) = world.get::<Sections>(row).cloned() {
        arrange(world, row, &sections, &sections.observed);
    }
}

pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    crate::laboratory_cases! {
        relation_records_keep_every_non_title_property_in_a_saved_accordion,
        title_only_records_hide_properties_and_controls_until_opened,
        fiote_castle_contains_management_without_conversation,
        empty_fields_collapse_and_dates_share_a_row_without_replacing_editors,
        new_controls_follow_disclosure_and_title_never_has_an_inner_box,
    }

    #[cfg_attr(test, test)]
    fn new_controls_follow_disclosure_and_title_never_has_an_inner_box() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let data = json!({"uid":"record", "head":"Varrer Casa", "slug":"varrer-casa", "start_date":"2026-10-08", "due_date":"2026-10-23"});
        app.world_mut().resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(Config {
                    hide_filled: true,
                    ..Config::records()
                }),
                data: vec![data.clone()],
                ready: true,
                dirty: true,
                ..default()
            },
        );
        rows::reconcile(app.world_mut(), owner);
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
        let sections = app.world().get::<Sections>(row).unwrap().clone();
        let added = app.world_mut().spawn((Node::default(), ChildOf(row))).id();
        let button =
            crate::sand_panel::button(app.world_mut(), added, row, "Later feature", Toggle);
        arrange(app.world_mut(), row, &sections, &data);
        assert_eq!(
            app.world().get::<ChildOf>(added).unwrap().parent(),
            sections.actions
        );
        assert_eq!(
            app.world().get::<Node>(sections.actions).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(sections.title).unwrap().border,
            UiRect::all(px(0))
        );
        let title = app
            .world_mut()
            .query::<(Entity, &bevy::text::EditableText)>()
            .iter(app.world())
            .find(|(_, text)| text.value() == "Varrer Casa")
            .unwrap()
            .0;
        assert!(app.world().get::<crate::sand::Borderless>(title).is_some());
        assert_eq!(
            app.world().get::<Node>(title).unwrap().border,
            UiRect::all(px(0))
        );
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(title, bevy::input_focus::FocusCause::Pressed);
        update(app.world_mut());
        assert_eq!(
            app.world()
                .get::<Node>(sections.toolbar)
                .unwrap()
                .position_type,
            PositionType::Absolute
        );
        assert_eq!(
            app.world().get::<Node>(sections.actions).unwrap().display,
            Display::None
        );
        Toggle.apply(app.world_mut(), row);
        assert_eq!(
            app.world().get::<Node>(sections.actions).unwrap().display,
            Display::Flex
        );
        assert_eq!(app.world().get::<ChildOf>(button).unwrap().parent(), added);
        assert_eq!(
            app.world().get::<Node>(sections.title).unwrap().border,
            UiRect::bottom(px(1))
        );
        for section in
            sections
                .groups
                .into_iter()
                .chain([sections.empty, sections.filled, sections.dates])
        {
            assert_eq!(
                app.world().get::<Node>(section).unwrap().flex_wrap,
                FlexWrap::Wrap
            );
        }
        let dates: Vec<_> = app
            .world_mut()
            .query::<(Entity, &rows::PropertyContainer)>()
            .iter(app.world())
            .filter(|(_, property)| matches!(property.0.as_str(), "start_date" | "due_date"))
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(
            app.world().get::<ChildOf>(sections.dates).unwrap().parent(),
            sections.groups[1]
        );
        for date in dates {
            let controls = app
                .world()
                .get::<Children>(date)
                .unwrap()
                .iter()
                .find(|entity| {
                    app.world()
                        .get::<Children>(*entity)
                        .is_some_and(|children| {
                            children.iter().any(|child| {
                                app.world().get::<bevy::text::EditableText>(child).is_some()
                            })
                        })
                })
                .unwrap();
            assert_eq!(
                app.world().get::<Node>(controls).unwrap().flex_wrap,
                FlexWrap::NoWrap
            );
            assert_eq!(app.world().get::<Children>(controls).unwrap().len(), 3);
        }
    }

    #[cfg_attr(test, test)]
    fn title_only_records_hide_properties_and_controls_until_opened() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let config = Config {
            hide_filled: true,
            delete_button: true,
            ..Config::records()
        };
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config.clone());
        app.world_mut().resource_mut::<Runtime>().areas.insert(owner, State {
            applied: Some(config),
            data: vec![json!({"uid":"record", "head":"Title", "body":"Body", "quantity":"7", "slug":"slug", "start_date":"2026-09-22", "due_date":"2026-09-23", "threads":[{"uid":"thread", "head":"Thread"}]})],
            ready: true, dirty: true, ..default()
        });
        rows::reconcile(app.world_mut(), owner);
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
        let sections = app.world().get::<Sections>(row).unwrap().clone();
        let fields: Vec<_> = app
            .world_mut()
            .query::<(Entity, &rows::PropertyContainer)>()
            .iter(app.world())
            .map(|(entity, field)| (entity, field.0.clone()))
            .collect();
        assert_eq!(
            app.world().get::<Node>(sections.toolbar).unwrap().display,
            Display::None
        );
        for (entity, property) in &fields {
            assert_eq!(
                app.world().get::<Node>(*entity).unwrap().display,
                if property == "head" {
                    Display::Flex
                } else {
                    Display::None
                }
            );
        }
        let buttons: Vec<_> = app
            .world_mut()
            .query::<(Entity, &ActionButton)>()
            .iter(app.world())
            .filter(|(_, button)| button.target == row)
            .map(|(entity, _)| entity)
            .collect();
        assert!(!buttons.is_empty());
        for entity in buttons {
            let mut ancestor = entity;
            while app.world().get::<Node>(ancestor).unwrap().display != Display::None {
                assert_ne!(ancestor, row, "Visible Record control in title-only mode");
                ancestor = app.world().get::<ChildOf>(ancestor).unwrap().parent();
            }
        }
        Toggle.apply(app.world_mut(), row);
        for (entity, _) in &fields {
            assert_eq!(
                app.world().get::<Node>(*entity).unwrap().display,
                Display::Flex
            );
        }
        Toggle.apply(app.world_mut(), row);
        Compact.apply(app.world_mut(), row);
        for (entity, _) in fields {
            assert_eq!(
                app.world().get::<Node>(entity).unwrap().display,
                Display::Flex
            );
        }
    }

    #[cfg_attr(test, test)]
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
            app.world().get::<Node>(sections.toolbar).unwrap().display,
            Display::None
        );
        app.init_resource::<bevy::picking::hover::HoverMap>();
        app.world_mut()
            .resource_mut::<bevy::picking::hover::HoverMap>()
            .entry(bevy::picking::pointer::PointerId::Mouse)
            .or_default()
            .insert(
                sections.title,
                bevy::picking::backend::HitData::new(owner, 0.0, None, None),
            );
        update(app.world_mut());
        assert_eq!(
            app.world().get::<Node>(sections.toolbar).unwrap().display,
            Display::None
        );
        assert!(
            !app.world()
                .get::<crate::record_presentation::RecordPresentation>(row)
                .unwrap()
                .expanded
        );
        for entity in app.world().get::<Children>(row).unwrap().iter() {
            if app.world().get::<ActionButton>(entity).is_some()
                || app
                    .world()
                    .get::<super::super::presentation::Controls>(entity)
                    .is_some()
            {
                assert_eq!(
                    app.world().get::<Node>(entity).unwrap().display,
                    Display::None
                );
            }
        }
        app.world_mut()
            .resource_mut::<bevy::picking::hover::HoverMap>()
            .clear();
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(sections.title, bevy::input_focus::FocusCause::Pressed);
        update(app.world_mut());
        assert_eq!(
            app.world().get::<Node>(sections.toolbar).unwrap().display,
            Display::Flex
        );
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(owner, bevy::input_focus::FocusCause::Pressed);
        update(app.world_mut());
        assert_eq!(
            app.world().get::<Node>(sections.toolbar).unwrap().display,
            Display::None
        );
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

    #[cfg_attr(test, test)]
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

    #[cfg_attr(test, test)]
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
        let children: Vec<_> = app
            .world()
            .get::<Children>(row)
            .unwrap()
            .iter()
            .filter(|entity| {
                app.world().get::<ActionButton>(*entity).is_none()
                    && app
                        .world()
                        .get::<super::super::presentation::Controls>(*entity)
                        .is_none()
            })
            .collect();
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
                sections.actions,
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
        assert_eq!(
            app.world().get::<ChildOf>(sections.dates).unwrap().parent(),
            sections.groups[1]
        );
        Toggle.apply(app.world_mut(), row);
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
