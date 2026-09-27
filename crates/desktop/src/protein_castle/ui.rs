use super::*;
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    icons::{Icon, IconButton, Tooltip},
    tokens::Token,
};
use bevy::text::EditableText;
use serde_json::json;

#[derive(Clone)]
pub(super) enum Command {
    Create,
    Delete,
    Run,
    Stop,
    Save,
    Library,
    Load(usize),
    Page(bool),
    Set(String, Value),
    Append(String, Value),
    Remove(String, usize),
    Move(String, usize, bool),
    Negate(String),
    Group(String, bool),
    Source(String),
    Include(String, Value),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        match self {
            Self::Create => {
                let Some(spaces) = world.get::<crate::workspace::Workspaces>(owner) else {
                    return;
                };
                let workspace = spaces.active;
                let position = world
                    .get::<crate::canvas::CanvasView>(owner)
                    .map_or(DVec2::ZERO, |view| view.center);
                let size = world
                    .get::<ComputedUiRenderTargetInfo>(owner)
                    .map(|target| target.logical_size());
                let zoom = world
                    .get::<crate::canvas::CanvasView>(owner)
                    .map_or(1.0, |view| view.zoom) as f32;
                let castle = spawn(world, owner, workspace, position, ProteinDraft::default());
                if let Some(size) = size.filter(|size| size.min_element() > 64.0) {
                    let mut item = world.get_mut::<crate::canvas::CanvasItem>(castle).unwrap();
                    item.size = item.size.min((size - Vec2::splat(48.0)) / zoom);
                }
                return;
            }
            Self::Delete => {
                if let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) {
                    crate::deletion::request(world, root, vec![owner]);
                }
                return;
            }
            Self::Run => {
                run(world, owner);
                return;
            }
            Self::Stop => {
                crate::protein_area::stop_editor(world, owner);
                cancel(world, owner, Some(RequestKind::Results));
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.subscription = None;
                }
                if let Some(mut result) = world.get_mut::<ProteinResults>(owner) {
                    result.current = false;
                }
                status(world, owner, "Stopped · last results");
                return;
            }
            Self::Save => {
                save(world, owner);
                return;
            }
            Self::Library => {
                library(world, owner);
                return;
            }
            Self::Page(forward) => {
                let count = world
                    .get::<ProteinResults>(owner)
                    .map_or(0, |result| result.rows.len());
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.page = if *forward {
                        (view.page + 1).min(count.saturating_sub(1) / 20)
                    } else {
                        view.page.saturating_sub(1)
                    };
                    view.output_dirty = true;
                }
                return;
            }
            _ => {}
        }
        let Some(castle) = world.get::<ProteinCastle>(owner) else {
            return;
        };
        let mut draft = castle.draft.clone();
        match self {
            Self::Load(index) => {
                let Some(row) = world
                    .get::<View>(owner)
                    .and_then(|view| view.library.get(*index))
                else {
                    return;
                };
                let ast = row["extension"].clone();
                let query: protein::Protein = match serde_json::from_value(ast) {
                    Ok(query) => query,
                    Err(error) => {
                        status(world, owner, format!("Cannot load Protein: {error}"));
                        return;
                    }
                };
                if crate::protein_area::filter::editor(world, owner)
                    && query.source != protein::Source::Record
                {
                    status(world, owner, "Area filters need a Record query");
                    return;
                }
                draft = ProteinDraft::from_protein(
                    row["head"].as_str().unwrap_or("Protein").into(),
                    row["slug"].as_str().unwrap_or_default().into(),
                    query,
                );
                world.get_mut::<View>(owner).unwrap().library_open = false;
                cancel(world, owner, Some(RequestKind::Library));
            }
            Self::Source(source) => {
                draft.query = ProteinDraft::default().query;
                draft.query["source"] = json!(source);
                if !model::has_limit(source) {
                    draft.query["limit"] = Value::Null;
                }
            }
            Self::Include(key, template) => {
                let value = &draft.query["include"][key];
                let enabled = !value.is_null() && value != &json!(false);
                draft.query["include"][key] = if enabled {
                    if template.is_boolean() {
                        json!(false)
                    } else {
                        Value::Null
                    }
                } else {
                    template.clone()
                };
            }
            Self::Set(path, value) => {
                if let Some(slot) = draft.query.pointer_mut(path) {
                    *slot = value.clone();
                }
            }
            Self::Append(path, value) => {
                if let Some(values) = draft.query.pointer_mut(path).and_then(Value::as_array_mut) {
                    if values.len() >= 128 {
                        status(world, owner, "Use at most 128 items in a group");
                        return;
                    }
                    values.push(value.clone());
                }
            }
            Self::Remove(path, index) => {
                if let Some(values) = draft.query.pointer_mut(path).and_then(Value::as_array_mut) {
                    if *index < values.len() {
                        values.remove(*index);
                    }
                }
            }
            Self::Move(path, index, forward) => {
                if let Some(values) = draft.query.pointer_mut(path).and_then(Value::as_array_mut) {
                    let next = if *forward {
                        index + 1
                    } else {
                        index.saturating_sub(1)
                    };
                    if next < values.len() && *index < values.len() {
                        values.swap(*index, next);
                    }
                }
            }
            Self::Negate(path) => {
                if let Some(slot) = draft.query.pointer_mut(path) {
                    *slot = if let Some(child) = slot.get("not") {
                        child.clone()
                    } else {
                        json!({"not":slot})
                    };
                }
            }
            Self::Group(path, any) => {
                if let Some(slot) = draft.query.pointer_mut(path) {
                    let children = slot
                        .get("all")
                        .or_else(|| slot.get("any"))
                        .cloned()
                        .unwrap_or(json!([]));
                    *slot = if *any {
                        json!({"any":children})
                    } else {
                        json!({"all":children})
                    };
                }
            }
            _ => {}
        }
        if !draft.valid_storage() {
            status(world, owner, "Protein is too large");
            return;
        }
        world.get_mut::<ProteinCastle>(owner).unwrap().draft = draft;
        changed(world, owner);
        editor(world, owner);
    }
}

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(4),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn scroll(world: &mut World, parent: Entity, width: f32) -> Entity {
    world
        .spawn((
            Node {
                width: percent(width),
                min_width: px(0),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                overflow: Overflow::scroll(),
                ..default()
            },
            ScrollPosition::default(),
            ChildOf(parent),
        ))
        .observe(
            |mut event: On<Pointer<Scroll>>, mut scrolls: Query<&mut ScrollPosition>| {
                if let Ok(mut position) = scrolls.get_mut(event.entity) {
                    let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                    position.0 -= Vec2::new(event.x, event.y) * multiplier;
                    event.propagate(false);
                }
            },
        )
        .id()
}

pub(super) fn icon(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    icon: Icon,
    tooltip: &str,
) -> Entity {
    world
        .spawn((
            IconButton::new(icon, tooltip),
            ActionButton::new(owner, crate::actions![command]),
            ChildOf(parent),
        ))
        .id()
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    command: Command,
    tooltip: &str,
) {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            Tooltip(tooltip.into()),
            ActionButton::new(owner, crate::actions![command]),
            crate::token_style::background(Token::Surface),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    label(world, entity, title, 14.0);
}

fn select(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    selected: &str,
    options: Vec<(String, Command)>,
) {
    let toggle = crate::dropdown::spawn(
        world,
        parent,
        owner,
        title,
        selected,
        options
            .into_iter()
            .map(|(title, command)| (title, crate::actions![command]))
            .collect(),
    );
    world.entity_mut(toggle).insert(Tooltip(title.into()));
}

fn choices(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    path: &str,
    selected: &str,
    values: &[&str],
) {
    select(
        world,
        parent,
        owner,
        &model::title(path.rsplit('/').next().unwrap_or_default()),
        &model::title(selected),
        values
            .iter()
            .map(|value| (model::title(value), Command::Set(path.into(), json!(value))))
            .collect(),
    );
}

#[derive(Component)]
struct Input {
    owner: Entity,
    path: String,
    list: bool,
    previous: String,
}

fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    path: &str,
    value: &str,
    list: bool,
    help: &str,
) {
    let font = world.resource::<crate::theme::Typography>().text(14.0);
    let mut text = crate::sand::editable(value);
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(if path.starts_with('$') { 256 } else { 4096 });
    world.spawn((
        text,
        font,
        crate::token_style::text(Token::Ink),
        crate::token_style::CursorToken(Token::Accent),
        Tooltip(help.into()),
        bevy::input_focus::tab_navigation::TabIndex(0),
        Input {
            owner,
            path: path.into(),
            list,
            previous: value.into(),
        },
        crate::tutorial::TutorialField::Query(owner, path.into()),
        Node {
            width: percent(100),
            min_width: px(80),
            min_height: px(28),
            flex_shrink: 0.0,
            padding: UiRect::all(px(4)),
            ..default()
        },
        ChildOf(parent),
    ));
}

pub(super) fn inputs(world: &mut World) {
    let changes: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter(|(_, input, text)| {
            !text.is_composing()
                && !crate::record_view::pending_text(text)
                && input.previous != text.value().to_string()
        })
        .map(|(entity, input, text)| {
            (
                entity,
                input.owner,
                input.path.clone(),
                input.list,
                text.value().to_string(),
            )
        })
        .collect();
    for (entity, owner, path, list, value) in changes {
        if crate::laboratory::suspended(world, owner) {
            continue;
        }
        let Some(castle) = world.get::<ProteinCastle>(owner) else {
            continue;
        };
        let mut draft = castle.draft.clone();
        match path.as_str() {
            "$name" => draft.name = value.clone(),
            "$slug" => draft.slug = value.clone(),
            _ => {
                if let Some(slot) = draft.query.pointer_mut(&path) {
                    *slot = if list {
                        json!(
                            value
                                .split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                        )
                    } else {
                        json!(value)
                    };
                }
            }
        }
        if !draft.valid_storage() {
            let previous = world.get::<Input>(entity).unwrap().previous.clone();
            world
                .get_mut::<EditableText>(entity)
                .unwrap()
                .editor
                .set_text(&previous);
            status(
                world,
                owner,
                "Protein is too large; the last edit was not applied",
            );
            continue;
        }
        world.get_mut::<ProteinCastle>(owner).unwrap().draft = draft;
        world.get_mut::<Input>(entity).unwrap().previous = value;
        changed(world, owner);
    }
}

fn changed(world: &mut World, owner: Entity) {
    crate::protein_area::editor_changed(world, owner);
    if let Some(mut result) = world.get_mut::<ProteinResults>(owner) {
        result.current = false;
    }
    let error = world
        .get::<ProteinCastle>(owner)
        .unwrap()
        .draft
        .compile()
        .err();
    status(
        world,
        owner,
        error.unwrap_or_else(|| "Draft · run to update results".into()),
    );
}

fn clear(world: &mut World, parent: Entity) {
    let children = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

pub(super) fn editor(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let parent = view.editor;
    let library = view.library_open.then(|| view.library.clone());
    clear(world, parent);
    if let Some(library) = library {
        label(world, parent, "Saved Proteins", 18.0);
        for (index, item) in library.iter().enumerate() {
            button(
                world,
                parent,
                owner,
                item["head"].as_str().unwrap_or("Protein"),
                Command::Load(index),
                item["slug"].as_str().unwrap_or_default(),
            );
        }
        if library.is_empty() {
            label(world, parent, "No saved Proteins", 14.0);
        }
        return;
    }
    let draft = world.get::<ProteinCastle>(owner).unwrap().draft.clone();
    for (title, path, value) in [
        ("Name", "$name", &draft.name),
        ("Slug", "$slug", &draft.slug),
    ] {
        label(world, parent, title, 14.0);
        input(world, parent, owner, path, value, false, title);
    }
    label(world, parent, "Source", 18.0);
    let source = draft.query["source"].as_str().unwrap_or("record");
    let area_filter = crate::protein_area::filter::editor(world, owner);
    if area_filter {
        label(world, parent, "Record", 14.0);
    } else {
        select(
            world,
            parent,
            owner,
            "Changing source resets query options",
            &model::title(source),
            model::SOURCES
                .iter()
                .map(|source| (model::title(source), Command::Source((*source).into())))
                .collect(),
        );
    }
    label(world, parent, "Filters", 18.0);
    for (index, predicate) in draft.query["where"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        filter(
            world,
            parent,
            owner,
            &format!("/where/{index}"),
            predicate,
            source,
            0,
        );
    }
    if !model::sorts(source).is_empty() {
        label(world, parent, "Sorting", 18.0);
        let sort_head = row(world, parent);
        icon(
            world,
            sort_head,
            owner,
            Command::Append("/order".into(), json!({"asc":model::sorts(source)[0]})),
            Icon::Plus,
            "Add a property sort. Earlier rules have higher priority.",
        );
        if source == "record" {
            icon(
                world,
                sort_head,
                owner,
                Command::Append(
                    "/order".into(),
                    json!({"link":{"kind":"", "higher":"from"}}),
                ),
                Icon::Group,
                "Add a relation sort",
            );
        }
        for (index, order) in draft.query["order"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let group = column(world, parent);
            let controls = row(world, group);
            label(world, controls, &(index + 1).to_string(), 14.0);
            ordering(world, controls, owner, "/order", index);
            let Some((key, value)) = order.as_object().and_then(|o| o.iter().next()) else {
                continue;
            };
            if key == "link" {
                properties(world, group, owner, &format!("/order/{index}/link"), value);
            } else {
                select(
                    world,
                    group,
                    owner,
                    "Sort direction",
                    if key == "asc" {
                        "Ascending"
                    } else {
                        "Descending"
                    },
                    ["asc", "desc"]
                        .into_iter()
                        .map(|key| {
                            (
                                if key == "asc" {
                                    "Ascending".into()
                                } else {
                                    "Descending".into()
                                },
                                Command::Set(format!("/order/{index}"), json!({key:value})),
                            )
                        })
                        .collect(),
                );
                let path = format!("/order/{index}/{key}");
                {
                    choices(
                        world,
                        group,
                        owner,
                        &path,
                        value.as_str().unwrap_or_default(),
                        model::sorts(source),
                    );
                }
            }
        }
    }
    if area_filter {
        return;
    }
    if !model::groups(source).is_empty() {
        label(world, parent, "Aggregate", 18.0);
        let aggregate = &draft.query["aggregate"];
        select(
            world,
            parent,
            owner,
            "Aggregation",
            aggregate["op"].as_str().unwrap_or("None"),
            vec![
                (
                    "None".into(),
                    Command::Set("/aggregate".into(), Value::Null),
                ),
                (
                    "Count".into(),
                    Command::Set(
                        "/aggregate".into(),
                        json!({"op":"count", "by":model::groups(source)[0]}),
                    ),
                ),
                (
                    "Sum".into(),
                    Command::Set(
                        "/aggregate".into(),
                        json!({"op":"sum", "by":model::groups(source)[0]}),
                    ),
                ),
            ]
            .into_iter()
            .filter(|(name, _)| source != "timeline" || name != "Count")
            .collect(),
        );
        if !aggregate.is_null() {
            label(world, parent, "Group by", 14.0);
            choices(
                world,
                parent,
                owner,
                "/aggregate/by",
                aggregate["by"].as_str().unwrap_or("total"),
                model::groups(source),
            );
        }
    }
    if source == "record" {
        label(world, parent, "Fields", 18.0);
        fields(world, parent, owner, &draft.query["fields"]);
        label(world, parent, "Include", 18.0);
        let include = draft.query["include"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (key, template) in [
            ("facts", json!({"limit":10})),
            ("promises", json!({"state":[]})),
            ("links", json!({"kinds":["*"],"direction":"both","depth":1})),
            ("threads", json!({"messages_limit":50})),
            ("availability", json!(true)),
            ("extension", json!({"namespace":"work"})),
            ("projection", json!({"at":""})),
            ("contact", json!(true)),
            ("conversations", json!(true)),
            ("reference_reads", json!(true)),
        ] {
            let value = include.get(key).cloned().unwrap_or(Value::Null);
            let line = row(world, parent);
            label(world, line, &model::title(key), 14.0);
            let enabled = !value.is_null() && value != json!(false);
            icon(
                world,
                line,
                owner,
                Command::Include(key.into(), template),
                if enabled { Icon::Check } else { Icon::Plus },
                if enabled {
                    "Exclude this data"
                } else {
                    "Include this data"
                },
            );
            if enabled && !value.is_boolean() {
                properties(world, parent, owner, &format!("/include/{key}"), &value);
            }
        }
    }
    if model::has_limit(source) {
        label(world, parent, "Limit", 18.0);
        input(
            world,
            parent,
            owner,
            "/limit",
            &scalar(&draft.query["limit"]),
            false,
            "Maximum rows; blank means unlimited. Aggregate behavior is defined by the source.",
        );
    }
}

fn column(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::left(px(8)),
                border: UiRect::left(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::border(Token::Accent),
            ChildOf(parent),
        ))
        .id()
}

fn ordering(world: &mut World, parent: Entity, owner: Entity, path: &str, index: usize) {
    icon(
        world,
        parent,
        owner,
        Command::Move(path.into(), index, false),
        Icon::Forward,
        "Move earlier",
    );
    icon(
        world,
        parent,
        owner,
        Command::Move(path.into(), index, true),
        Icon::Backward,
        "Move later",
    );
    icon(
        world,
        parent,
        owner,
        Command::Remove(path.into(), index),
        Icon::Close,
        "Remove",
    );
}

fn filter(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    path: &str,
    predicate: &Value,
    source: &str,
    depth: usize,
) {
    if depth > 10 {
        label(world, parent, "Maximum group depth", 14.0);
        return;
    }
    let parent = column(world, parent);
    let line = row(world, parent);
    let Some((key, value)) = predicate.as_object().and_then(|o| o.iter().next()) else {
        return;
    };
    if matches!(key.as_str(), "all" | "any") {
        if model::nested(source) {
            select(
                world,
                line,
                owner,
                "Match conditions",
                &model::title(key),
                vec![
                    ("All".into(), Command::Group(path.into(), false)),
                    ("Any".into(), Command::Group(path.into(), true)),
                ],
            );
        } else {
            label(world, line, "All", 14.0);
        }
        let children_path = format!("{path}/{key}");
        let available = model::filters(source);
        if let Some(first) = available.first() {
            icon(
                world,
                line,
                owner,
                Command::Append(children_path.clone(), model::condition(first)),
                Icon::Plus,
                "Add a condition",
            );
        }
        if depth < 9 && model::nested(source) {
            icon(
                world,
                line,
                owner,
                Command::Append(children_path.clone(), json!({"all":[]})),
                Icon::Group,
                "Add a nested group",
            );
        }
        for (index, child) in value.as_array().into_iter().flatten().enumerate() {
            let child_parent = column(world, parent);
            let controls = row(world, child_parent);
            ordering(world, controls, owner, &children_path, index);
            filter(
                world,
                child_parent,
                owner,
                &format!("{children_path}/{index}"),
                child,
                source,
                depth + 1,
            );
        }
    } else {
        let negative = key == "not";
        if model::nested(source) || negative {
            button(
                world,
                line,
                owner,
                if negative { "Not" } else { "Is" },
                Command::Negate(path.into()),
                "Negate this condition",
            );
        }
        let leaf_path = if negative {
            format!("{path}/not")
        } else {
            path.into()
        };
        let leaf = if negative { value } else { predicate };
        let Some((key, value)) = leaf.as_object().and_then(|o| o.iter().next()) else {
            return;
        };
        select(
            world,
            parent,
            owner,
            "Condition",
            &model::title(key),
            model::filters(source)
                .iter()
                .map(|key| {
                    (
                        model::title(key),
                        Command::Set(leaf_path.clone(), model::condition(key)),
                    )
                })
                .collect(),
        );
        properties(world, parent, owner, &format!("{leaf_path}/{key}"), value);
    }
}

fn fields(world: &mut World, parent: Entity, owner: Entity, value: &Value) {
    let line = row(world, parent);
    button(
        world,
        line,
        owner,
        if value.is_null() {
            "All properties"
        } else {
            "Selected properties"
        },
        Command::Set(
            "/fields".into(),
            if value.is_null() {
                json!(["head", "quantity"])
            } else {
                Value::Null
            },
        ),
        "Choose returned properties. Identity and Kind are always included.",
    );
    if let Some(values) = value.as_array() {
        for (index, value) in values.iter().enumerate() {
            let line = row(world, parent);
            label(
                world,
                line,
                &model::title(value.as_str().unwrap_or_default()),
                14.0,
            );
            icon(
                world,
                line,
                owner,
                Command::Remove("/fields".into(), index),
                Icon::Close,
                "Remove this property from the results",
            );
        }
        let available = model::FIELDS
            .iter()
            .filter(|field| !values.iter().any(|value| value.as_str() == Some(**field)))
            .map(|field| {
                (
                    model::title(field),
                    Command::Append("/fields".into(), json!(field)),
                )
            })
            .collect();
        select(
            world,
            parent,
            owner,
            "Add an output property",
            "+",
            available,
        );
    }
}

fn properties(world: &mut World, parent: Entity, owner: Entity, path: &str, value: &Value) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            label(world, parent, &model::title(key), 14.0);
            properties(world, parent, owner, &format!("{path}/{key}"), value);
        }
        return;
    }
    let key = path.rsplit('/').next().unwrap_or_default();
    let enums: &[&str] = match key {
        "direction" => &["out", "in", "both"],
        "higher" => &["from", "to"],
        "field" if path.contains("/work_date/") => &["start", "due"],
        "op" if path.contains("/work_date/") => &["eq", "lt", "lte", "gt", "gte", "exists"],
        _ => &[],
    };
    if !enums.is_empty() {
        choices(
            world,
            parent,
            owner,
            path,
            value.as_str().unwrap_or_default(),
            enums,
        );
        return;
    }
    if key == "other" {
        button(
            world,
            parent,
            owner,
            if value.is_null() {
                "Any target"
            } else {
                "Specific target"
            },
            Command::Set(
                path.into(),
                if value.is_null() {
                    json!("")
                } else {
                    Value::Null
                },
            ),
            "Choose any target or one specific Record",
        );
        if !value.is_null() {
            input(
                world,
                parent,
                owner,
                path,
                &scalar(value),
                false,
                "Target Record slug or UID",
            );
        }
        return;
    }
    if value.is_boolean() {
        button(
            world,
            parent,
            owner,
            if value == &json!(true) { "On" } else { "Off" },
            Command::Set(path.into(), json!(value != &json!(true))),
            &model::title(key),
        );
    } else if let Some(values) = value.as_array() {
        input(
            world,
            parent,
            owner,
            path,
            &values.iter().map(scalar).collect::<Vec<_>>().join(", "),
            true,
            "Values separated by commas",
        );
    } else {
        input(
            world,
            parent,
            owner,
            path,
            &scalar(value),
            false,
            match key {
                "concept_in" => {
                    "Concept name or UID; descendants and counts-as membership are included"
                }
                "value" if path.contains("/work_date/") => "Date: YYYY-MM-DD; ignored for Exists",
                "kind" if path.contains("/relation/") => "Relation name, such as assigned-to",
                _ => "Property value",
            },
        );
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

pub(super) fn output(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let parent = view.output;
    let status = view.status.clone();
    let results = world.get::<ProteinResults>(owner).unwrap();
    let count = results.rows.len();
    let page = view.page.min(count.saturating_sub(1) / 20);
    let rows = results
        .rows
        .iter()
        .skip(page * 20)
        .take(20)
        .cloned()
        .collect::<Vec<_>>();
    let columns = results.columns.clone();
    clear(world, parent);
    let status_entity = label(world, parent, &status, 14.0);
    world.entity_mut(status_entity).insert(Tooltip(status));
    let line = row(world, parent);
    label(world, line, &format!("{count} rows"), 18.0);
    icon(
        world,
        line,
        owner,
        Command::Page(false),
        Icon::Previous,
        "Previous 20 results",
    );
    icon(
        world,
        line,
        owner,
        Command::Page(true),
        Icon::Next,
        "Next 20 results",
    );
    label(
        world,
        parent,
        &format!("Page {} / {}", page + 1, count.div_ceil(20).max(1)),
        14.0,
    );
    for (index, value) in rows.iter().enumerate() {
        let group = column(world, parent);
        label(world, group, &format!("{}", page * 20 + index + 1), 16.0);
        for key in columns.iter().take(64) {
            if let Some(value) = value.get(key) {
                let text = scalar(value);
                label(world, group, &model::title(key), 14.0);
                let abbreviated: String = text.chars().take(512).collect();
                let entity = label(world, group, &abbreviated, 14.0);
                world.entity_mut(entity).insert(Tooltip(text));
            }
        }
    }
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.page = page;
    view.output_dirty = false;
}
