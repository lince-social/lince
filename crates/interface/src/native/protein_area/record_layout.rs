// non-lean-yet
// Scope: struct RecordActions, struct Sections, struct Contents, fn section.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 249, 341, 358,
//   444, 446, 588, 623, 634, 641, 903, 973, 981, 1096, 1104, 1109, 1164, 1172, 1237, 1302, 1353,
//   1395.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (366).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   273, 980, 1103, 1171, 1284, 1352, 1489.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (110, 118, 125, 163, 172, 178, 184, 185, 186, 190, 192, 453, 560, 573, 581, 598,
//   607, 629, 712, 847, 981, 987, 991, 995, 1003, 1011, 1021, 1026, 1031, 1034, 1036, 1046, 1058,
//   1065, 1076, 1078, 1081, 1095, 1104, 1112, 1117, 1135, 1137, 1143, 1151, 1163, 1172, 1174, 1188,
//   1194, 1197, 1205, 1218, 1226, 1230, 1247, 1254, 1258, 1264, 1270, 1271, 1281, 1285, 1288, 1301,
//   1353, 1377, 1381, 1387, 1405, 1416, 1431, 1450, 1460, 1466, 1470, 1480, 1485, 1491, 1494, 1500,
//   1513, 1520, 1527, 1534).
// - [Clippy probe: clippy::allow_attributes] Use a reasoned expect when the diagnostic reliably
//   fires. Sites 907.

use super::*;
use crate::host::performance::{self, Count, Stage};
use crate::native::actions::{Action, ActionButton};
use bevy::ecs::change_detection::Tick;
use std::sync::Arc;

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
    presentation: crate::native::record_presentation::RecordPresentation,
    observed: Arc<Contents>,
    source: Option<(Tick, Option<Tick>)>,
    snapshot: Option<std::sync::Weak<Vec<Value>>>,
}

#[derive(Default, PartialEq, Eq)]
struct Contents {
    filled: std::collections::HashSet<String>,
}

impl Contents {
    fn properties(data: &Value) -> impl Iterator<Item = &str> {
        data.as_object()
            .into_iter()
            .flat_map(|data| data.keys())
            .map(String::as_str)
            .filter(|property| !matches!(*property, "start_date" | "due_date" | "work_timer"))
            .chain(["start_date", "due_date", "work_timer"])
            .filter(move |property| !empty(property, data))
    }

    fn new(data: &Value) -> Self {
        let _contents = performance::Span::new(Stage::RecordSectionContents);
        let filled: std::collections::HashSet<_> =
            Self::properties(data).map(str::to_owned).collect();
        performance::add(Count::RecordSectionPropertyNamesCopied, filled.len() as u64);
        Self { filled }
    }

    fn matches(&self, data: &Value) -> bool {
        let _contents = performance::Span::new(Stage::RecordSectionContents);
        performance::count(Count::RecordSectionContentChecks);
        let mut count = 0;
        for property in Self::properties(data) {
            if !self.filled.contains(property) {
                return false;
            }
            count += 1;
        }
        count == self.filled.len()
    }

    fn empty(&self, property: &str) -> bool {
        !self.filled.contains(property)
    }
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
        .get::<crate::native::record_presentation::RecordPresentation>(row)
        .copied()
        .unwrap_or_default();
    let fiote = world
        .get::<RecordBinding>(row)
        .and_then(|binding| world.get::<crate::native::area::InfluenceArea>(binding.area))
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.fiote);
    let title = section(world, row);
    world
        .entity_mut(title)
        .insert(crate::native::token_style::border(
            crate::native::tokens::Token::Accent,
        ));
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
            crate::native::sand::button(0),
            crate::native::sand::Borderless,
            crate::native::icons::Tooltip("Show or hide properties".into()),
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
    let arrow = crate::native::edit_mode::label(world, toggle, "⌄", 18.0);
    let compact = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::sand::Borderless,
            crate::native::icons::Tooltip(
                "Move filled properties into or out of the accordion".into(),
            ),
            ActionButton::new(row, crate::native::actions![Compact]),
            ChildOf(toolbar),
        ))
        .id();
    let compact_label =
        crate::native::edit_mode::label(world, compact, "Show filled properties", 14.0);
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
        let label = crate::native::edit_mode::label(world, group, title, 16.0);
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
    let label = crate::native::edit_mode::label(world, actions, "Record actions", 16.0);
    world.get_mut::<Node>(label).unwrap().width = percent(100);
    {
        let mut node = world.get_mut::<Node>(actions).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    world
        .entity_mut(toggle)
        .insert(ActionButton::new(row, crate::native::actions![Toggle]));
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
        observed: Arc::new(Contents::default()),
        source: None,
        snapshot: None,
    };
    world
        .entity_mut(row)
        .insert(crate::native::canvas_host::authoring::PartName(
            "record".into(),
        ));
    for (name, entity) in [
        ("title", title),
        ("identity", identity),
        ("assertions", assertions),
        ("filled", filled),
        ("toolbar", toolbar),
        ("disclosure", toggle),
        ("compact", compact),
        ("empty", empty),
        ("description", body),
        ("dates", dates),
        ("threads", threads),
        ("actions", actions),
        ("identity-section", groups[0]),
        ("planning-section", groups[1]),
        ("work-section", groups[2]),
        ("discussion-section", groups[3]),
    ] {
        world
            .entity_mut(entity)
            .insert(crate::native::canvas_host::authoring::PartName(name.into()));
    }
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
        crate::native::token_style::background(crate::native::tokens::Token::Accent),
        ChildOf(parent),
    ));
}

fn empty(property: &str, data: &Value) -> bool {
    let value = &data[property];
    match property {
        "start_date" | "due_date" => {
            empty_value(&data["start_date"]) && empty_value(&data["due_date"])
        }
        "work_timer" => data["work_logs"].as_array().is_none_or(Vec::is_empty),
        "quantity" | "spent_seconds" => {
            value.is_null() || value.as_str() == Some("0") || value.as_f64() == Some(0.0)
        }
        _ => empty_value(value),
    }
}

fn empty_value(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|value| value.trim().is_empty())
        || value.as_array().is_some_and(Vec::is_empty)
}

impl Sections {
    pub(super) fn parent(&self, property: &str, data: &Value) -> Entity {
        self.property_parent(property, empty(property, data))
    }

    fn property_parent(&self, property: &str, empty: bool) -> Entity {
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
            _ if empty => self.empty,
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
            .get::<crate::native::record_presentation::RecordPresentation>(row)
            .copied()
            .unwrap_or(sections.presentation);
        presentation.expanded = !presentation.expanded;
        world.entity_mut(row).insert(presentation);
        crate::native::record_presentation::save(world, row);
        arrange_contents(world, row, &sections, sections.observed.clone());
    }
}

#[derive(Clone)]
struct Compact;
impl Action for Compact {
    fn apply(&self, world: &mut World, row: Entity) {
        let Some(mut presentation) =
            world.get_mut::<crate::native::record_presentation::RecordPresentation>(row)
        else {
            return;
        };
        presentation.hide_filled = !presentation.hide_filled;
        presentation.expanded = false;
        crate::native::record_presentation::save(world, row);
        if let Some(sections) = world.get::<Sections>(row).cloned() {
            arrange_contents(world, row, &sections, sections.observed.clone());
        }
    }
}

pub(super) fn update(world: &mut World) {
    world
        .run_system_cached(update_sections)
        .expect("update Record sections");
}

fn update_sections(
    world: &mut World,
    sections: &mut QueryState<(
        Entity,
        &'static Sections,
        Ref<'static, RecordBinding>,
        Option<Ref<'static, rows::Row>>,
    )>,
    toolbars: &mut QueryState<(Entity, &'static Sections)>,
    mut activity: Local<Activity>,
    mut toolbar_cards: Local<
        Vec<(
            Entity,
            Entity,
            crate::native::record_presentation::RecordPresentation,
        )>,
    >,
) {
    let _sections = performance::Span::new(Stage::RecordSectionsUpdate);
    let runtime = world.get_resource::<Runtime>();
    let mut records: HashMap<Entity, HashMap<&str, &Value>> = HashMap::new();
    let cards: Vec<_> = sections
        .iter(world)
        .filter_map(|(row, sections, binding, record)| {
            if super::availability::hidden(world, row)
                || super::presentation::raw(world, row).is_some()
            {
                return None;
            }
            let source = Some((
                binding.last_changed(),
                record.as_ref().map(|record| record.last_changed()),
            ));
            let snapshot = runtime
                .and_then(|runtime| runtime.areas.get(&binding.area))
                .map(|state| &state.data);
            let snapshot_changed = sections.snapshot.as_ref().map(std::sync::Weak::as_ptr)
                != snapshot.map(std::sync::Arc::as_ptr);
            let presentation = world
                .get::<crate::native::record_presentation::RecordPresentation>(row)
                .copied()
                .unwrap_or_default();
            if source == sections.source
                && !snapshot_changed
                && presentation == sections.presentation
            {
                return None;
            }
            let data = if let Some(snapshot) = snapshot {
                records
                    .entry(binding.area)
                    .or_insert_with(|| {
                        let mut records = HashMap::with_capacity(snapshot.len());
                        for data in snapshot.iter() {
                            if let Some(uid) = data["uid"].as_str() {
                                records.entry(uid).or_insert(data);
                            }
                        }
                        records
                    })
                    .get(binding.uid.as_str())
                    .copied()
            } else {
                record.as_ref().map(|row| row.data.as_ref())
            }?;
            let contents_changed = !sections.observed.matches(data);
            let changed = contents_changed || presentation != sections.presentation;
            Some((
                row,
                source,
                snapshot.map(std::sync::Arc::downgrade),
                changed.then(|| {
                    let contents = if contents_changed {
                        Arc::new(Contents::new(data))
                    } else {
                        sections.observed.clone()
                    };
                    (sections.clone(), contents)
                }),
            ))
        })
        .collect();
    drop(records);
    for (row, source, snapshot, changed) in cards {
        let mut sections = world.get_mut::<Sections>(row).unwrap();
        sections.source = source;
        sections.snapshot = snapshot;
        if let Some((sections, contents)) = changed {
            arrange_contents(world, row, &sections, contents);
        }
    }
    toolbar_cards.extend(
        toolbars
            .iter(world)
            .map(|(row, sections)| (row, sections.toolbar, sections.presentation)),
    );
    performance::add(Count::RecordToolbarsPrepared, toolbar_cards.len() as u64);
    if toolbar_cards.is_empty() {
        toolbar_cards.shrink_to_fit();
    }
    for (row, toolbar, presentation) in toolbar_cards.drain(..) {
        if super::presentation::raw(world, row).is_none() {
            toolbar_visibility(world, row, toolbar, presentation, &mut activity);
        }
    }
    activity.finish();
}

#[derive(Default)]
struct Activity {
    hovered: std::collections::HashSet<Entity>,
    focused: std::collections::HashSet<Entity>,
    captured: bool,
}

impl Activity {
    fn capture(&mut self, world: &World) {
        if self.captured {
            return;
        }
        let _capture = performance::Span::new(Stage::RecordToolbarActivity);
        crate::native::pointer_hit::extend_ancestors(
            world,
            &mut self.hovered,
            world
                .get_resource::<bevy::picking::hover::HoverMap>()
                .into_iter()
                .flat_map(|hover| hover.values())
                .flat_map(|hits| hits.keys().copied()),
        );
        crate::native::pointer_hit::extend_ancestors(
            world,
            &mut self.focused,
            world
                .get_resource::<bevy::input_focus::InputFocus>()
                .and_then(|focus| focus.get()),
        );
        performance::count(Count::RecordToolbarActivityPreparations);
        performance::add(
            Count::RecordToolbarAncestorsPrepared,
            (self.hovered.len() + self.focused.len()) as u64,
        );
        self.captured = true;
    }

    fn finish(&mut self) {
        self.hovered.clear();
        self.focused.clear();
        if !self.captured {
            self.hovered.shrink_to_fit();
            self.focused.shrink_to_fit();
        }
        self.captured = false;
    }
}

fn toolbar_visibility(
    world: &mut World,
    row: Entity,
    toolbar: Entity,
    presentation: crate::native::record_presentation::RecordPresentation,
    activity: &mut Activity,
) {
    if super::availability::hidden(world, row) {
        return;
    }
    let minimal = presentation.hide_filled && !presentation.expanded;
    let active = if minimal {
        activity.capture(world);
        let relations = world
            .get::<RecordBinding>(row)
            .and_then(|binding| world.get::<InfluenceArea>(binding.area))
            .and_then(|area| area.protein.as_ref())
            .is_some_and(|config| config.relations);
        !relations && activity.hovered.contains(&row) || activity.focused.contains(&row)
    } else {
        true
    };
    let display = if minimal && !active {
        Display::None
    } else {
        Display::Flex
    };
    let position_type = if minimal {
        PositionType::Absolute
    } else {
        PositionType::Relative
    };
    let width = if minimal { Val::Auto } else { percent(100) };
    let right = if minimal { px(8) } else { Val::Auto };
    let top = if minimal { px(4) } else { Val::Auto };
    let mut node = world.get_mut::<Node>(toolbar).unwrap();
    if node.display != display
        || node.position_type != position_type
        || node.width != width
        || node.right != right
        || node.top != top
    {
        node.display = display;
        node.position_type = position_type;
        node.width = width;
        node.right = right;
        node.top = top;
    }
    let compact = world.get::<Sections>(row).unwrap().compact;
    let display = if minimal {
        Display::None
    } else {
        Display::Flex
    };
    world
        .get_mut::<Node>(compact)
        .unwrap()
        .map_unchanged(|node| &mut node.display)
        .set_if_neq(display);
}

pub(super) fn arrange(world: &mut World, row: Entity, sections: &Sections, data: &Value) {
    let contents = if sections.observed.matches(data) {
        sections.observed.clone()
    } else {
        Arc::new(Contents::new(data))
    };
    arrange_contents(world, row, sections, contents);
}

fn set_display(world: &mut World, entity: Entity, display: Display) {
    if world
        .get_mut::<Node>(entity)
        .unwrap()
        .map_unchanged(|node| &mut node.display)
        .set_if_neq(display)
    {
        performance::add(Count::RecordSectionNodeWrites, 1);
    }
}

fn set_label(world: &mut World, entity: Entity, label: &str) {
    let mut text = world.get_mut::<Text>(entity).unwrap();
    if text.0 != label {
        text.0.clear();
        text.0.push_str(label);
        performance::add(Count::RecordSectionLabelWrites, 1);
    }
}

fn arrange_contents(world: &mut World, row: Entity, sections: &Sections, contents: Arc<Contents>) {
    let _arrange = performance::Span::new(Stage::RecordSectionArrange);
    if super::availability::hidden(world, row) {
        return;
    }
    if super::presentation::raw(world, row).is_some() {
        return;
    }
    let mut sections = sections.clone();
    sections.presentation = world
        .get::<crate::native::record_presentation::RecordPresentation>(row)
        .copied()
        .unwrap_or_default();
    {
        let mut current = world.get_mut::<Sections>(row).unwrap();
        if current.presentation != sections.presentation {
            current.presentation = sections.presentation;
        }
        if !Arc::ptr_eq(&current.observed, &contents) {
            current.observed = contents.clone();
        }
    }
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
    let ordering = sections.presentation.expanded.then(|| {
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
        ordering
    });
    let mut filled = false;
    let mut unfilled = false;
    for (entity, property) in properties {
        set_display(
            world,
            entity,
            if property != "head"
                && sections.presentation.hide_filled
                && !sections.presentation.expanded
            {
                Display::None
            } else {
                Display::Flex
            },
        );
        let parent = sections.property_parent(&property, contents.empty(&property));
        if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(parent) {
            world.entity_mut(entity).insert(ChildOf(parent));
        }
        if !matches!(
            property.as_str(),
            "head" | "body" | "assertions" | "threads"
        ) {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            let basis = px(match property.as_str() {
                "start_date" | "due_date" => 240.0,
                "work_logs" | "work_timer" => 280.0,
                _ => 140.0,
            });
            if node.width != Val::Auto
                || node.flex_basis != basis
                || node.flex_grow != 1.0
                || node.flex_shrink != 1.0
                || node.min_width != px(0)
                || node.max_width != percent(100)
            {
                node.width = Val::Auto;
                node.flex_basis = basis;
                node.flex_grow = 1.0;
                node.flex_shrink = 1.0;
                node.min_width = px(0);
                node.max_width = percent(100);
                performance::add(Count::RecordSectionNodeWrites, 1);
            }
        }
        if property == "threads" {
            unfilled |= contents.empty("threads");
        } else if !matches!(
            property.as_str(),
            "head" | "body" | "quantity" | "slug" | "assertions"
        ) {
            if sections.fiote || contents.empty(&property) {
                unfilled = true;
            } else {
                filled = true;
            }
        }
    }
    let parent = if sections.presentation.expanded {
        sections.groups[3]
    } else if sections.presentation.hide_filled || contents.empty("threads") {
        sections.empty
    } else {
        row
    };
    if world.get::<ChildOf>(sections.threads).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.threads).insert(ChildOf(parent));
    }
    let parent = if sections.presentation.expanded {
        sections.groups[1]
    } else if sections.presentation.hide_filled || sections.fiote || contents.empty("start_date") {
        sections.empty
    } else {
        sections.filled
    };
    if world.get::<ChildOf>(sections.dates).map(ChildOf::parent) != Some(parent) {
        world.entity_mut(sections.dates).insert(ChildOf(parent));
    }
    if let Some(ordering) = ordering {
        for parent in sections.groups.into_iter().chain([sections.dates]) {
            let Some(children) = world.get::<Children>(parent) else {
                continue;
            };
            if children
                .iter()
                .map(|entity| ordering.get(&entity).copied().unwrap_or(0))
                .is_sorted()
            {
                continue;
            }
            let mut ordered: Vec<_> = children.iter().collect();
            ordered.sort_by_key(|entity| ordering.get(entity).copied().unwrap_or(0));
            world.entity_mut(parent).replace_children(&ordered);
            performance::add(Count::RecordSectionChildOrdersChanged, 1);
        }
    }
    for entity in [sections.identity, sections.assertions, sections.body] {
        set_display(
            world,
            entity,
            if sections.presentation.hide_filled {
                Display::None
            } else {
                Display::Flex
            },
        );
    }
    set_display(
        world,
        sections.empty,
        if sections.presentation.expanded {
            Display::Flex
        } else {
            Display::None
        },
    );
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
                && !super::availability::chrome(world, *entity)
                && world
                    .get::<crate::native::shader_effects::EffectSlot>(*entity)
                    .is_none()
                && world
                    .get::<Node>(*entity)
                    .is_some_and(|node| node.position_type != PositionType::Absolute)
        })
        .collect();
    for entity in controls {
        world.entity_mut(entity).insert(ChildOf(sections.actions));
    }
    set_display(
        world,
        sections.actions,
        if minimal {
            Display::None
        } else {
            Display::Flex
        },
    );
    if world
        .get_mut::<Node>(sections.title)
        .unwrap()
        .map_unchanged(|node| &mut node.border)
        .set_if_neq(UiRect::bottom(px(if minimal { 0 } else { 1 })))
    {
        performance::add(Count::RecordSectionNodeWrites, 1);
    }
    set_label(
        world,
        sections.compact_label,
        if sections.presentation.hide_filled {
            "Show filled properties"
        } else {
            "Title only"
        },
    );
    set_label(
        world,
        sections.arrow,
        if sections.presentation.expanded {
            "⌃"
        } else {
            "⌄"
        },
    );
    set_display(
        world,
        sections.filled,
        if filled && !sections.presentation.hide_filled {
            Display::Flex
        } else {
            Display::None
        },
    );
    set_display(
        world,
        sections.toggle,
        if unfilled || sections.presentation.hide_filled {
            Display::Flex
        } else {
            Display::None
        },
    );
    let mut activity = Activity::default();
    toolbar_visibility(
        world,
        row,
        sections.toolbar,
        sections.presentation,
        &mut activity,
    );
    rows::materialize_visible(world, row);
    super::presentation::refresh_fields(world, row);
}

pub(super) fn restore(world: &mut World, row: Entity) {
    if let Some(sections) = world.get::<Sections>(row).cloned() {
        arrange_contents(world, row, &sections, sections.observed.clone());
    }
}

#[allow(
    clippy::unwrap_used,
    reason = "Record layout Laboratory fixtures treat missing test records and rendered fields as failed cases"
)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shader_space_stays_before_the_title_when_controls_are_arranged() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let data = json!({"uid":"record", "head":"Task", "quantity":"-1", "shader_effect":nucleus::shader_effect::Extension::single(1, nucleus::shader_effect::Effect::default())});
        app.world_mut().resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(
                    Config {
                        shader_effects: true,
                        ..Config::records()
                    }
                    .into(),
                ),
                data: vec![data].into(),
                ready: true,
                dirty: true,
                ..default()
            },
        );
        rows::reconcile(app.world_mut(), owner);
        let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
        let sections = app.world().get::<Sections>(row).unwrap();
        let children = app.world().get::<Children>(row).unwrap();
        assert!(
            app.world()
                .get::<crate::native::shader_effects::EffectSlot>(children[0])
                .is_some()
        );
        assert_eq!(children[1], sections.title);
        assert_eq!(
            app.world().get::<ChildOf>(children[0]).unwrap().parent(),
            row
        );
    }

    crate::native::laboratory_cases! {
        relation_records_keep_every_non_title_property_in_a_saved_accordion,
        title_only_records_hide_properties_and_controls_until_opened,
        fiote_castle_starts_in_conversations,
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
                applied: Some(
                    Config {
                        hide_filled: true,
                        ..Config::records()
                    }
                    .into(),
                ),
                data: vec![data.clone()].into(),
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
            crate::native::sand_panel::button(app.world_mut(), added, row, "Later feature", Toggle);
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
        assert!(
            app.world()
                .get::<crate::native::sand::Borderless>(title)
                .is_some()
        );
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
            applied: Some(config.into()),
            data: vec![json!({"uid":"record", "head":"Title", "body":"Body", "quantity":"7", "slug":"slug", "start_date":"2026-09-22", "due_date":"2026-09-23", "threads":[{"uid":"thread", "head":"Thread"}]})].into(),
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
        let config = crate::native::relation_castle::config();
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config.clone());
        app.world_mut().resource_mut::<Runtime>().areas.insert(owner, State {
            applied: Some(config.into()),
            data: vec![json!({"uid":"record", "head":"Visible title", "slug":"filled", "body":"Filled body", "quantity":"7", "start_date":"2026-09-22", "due_date":"2026-09-23", "threads":[]})].into(),
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
                .get::<crate::native::record_presentation::RecordPresentation>(row)
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
                .get::<crate::native::record_presentation::RecordPresentation>(row)
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
    fn fiote_castle_starts_in_conversations() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let mut config = Config::records();
        config.fiote = true;
        let uid = nucleus::new_uid("r");
        app.world_mut()
            .get_mut::<crate::native::area::InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config.clone());
        app.world_mut().resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(config.into()),
                data: vec![json!({"uid":uid,"head":"Fiote","body":"Prompt","threads":[]})].into(),
                ready: true,
                dirty: true,
                ..default()
            },
        );
        rows::reconcile(app.world_mut(), owner);
        assert_eq!(
            app.world_mut()
                .query::<&crate::native::thread_castle::ThreadCastle>()
                .iter(app.world())
                .count(),
            1
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
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "New thread")
        );
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .all(|text| text.0 != "Working directory override · blank inherits")
        );
    }

    #[cfg_attr(test, test)]
    fn empty_fields_collapse_and_dates_share_a_row_without_replacing_editors() {
        let (mut app, _, owner) = super::super::tests::fixture();
        app.add_plugins(crate::native::record_binding::RecordBindingPlugin);
        app.world_mut().resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(Config::records().into()),
                data: vec![json!({"uid":"record", "head":"Test", "body":"A **description**", "quantity":"0"})].into(),
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
                .query::<&crate::native::description::Description>()
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
                .get::<crate::native::sand::Square>(fields["head"])
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
                .get::<crate::native::assertion_editor::Field>(fields["assertions"])
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
        std::sync::Arc::make_mut(
            &mut app
                .world_mut()
                .resource_mut::<Runtime>()
                .areas
                .get_mut(&owner)
                .unwrap()
                .data,
        )[0]["start_date"] = json!("2026-09-19");
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
            std::sync::Arc::make_mut(
                &mut app
                    .world_mut()
                    .resource_mut::<Runtime>()
                    .areas
                    .get_mut(&owner)
                    .unwrap()
                    .data,
            )[0]["threads"] = data;
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
