use super::*;
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    icons::{Icon, IconButton, Tooltip},
    sand::Square,
};
use bevy::text::EditableText;

#[derive(Clone)]
enum Command {
    HideFilled,
    Relations,
    Motion,
    Placement(SpawnPlacement),
    SettlingTicks(u16),
    SpawnTarget(String),
    Records,
    ClosestDate,
    Enable,
    Remove,
    Query,
    Source(bool),
    Organ(String),
    Add(String),
    RemoveField(usize),
    Move(usize, bool),
    Square(usize),
    Editable(usize),
    Overflow(usize),
    DeleteButton,
    Page(bool),
    Login,
    GroupProperty(bool, Option<String>),
    GroupOrder(bool),
    GroupDirection(bool),
}

#[derive(Clone, Copy)]
enum Field {
    CenterForce,
    Repulsion,
    Cooling,
    Organ,
    Width,
    Gap,
    Columns,
    FieldWidth(usize),
    Height(usize),
    Username,
    Password,
    GroupStrength,
}
#[derive(Component)]
struct Input {
    area: Entity,
    field: Field,
    observed: String,
}
#[derive(Component)]
struct Status(Entity);
#[derive(Component)]
struct PasswordMask(Entity);

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    icon: Icon,
    title: &str,
) -> Entity {
    world
        .spawn((
            Square,
            IconButton::new(icon, title),
            ActionButton::new(owner, crate::actions![command]),
            ChildOf(parent),
        ))
        .id()
}
fn text_button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    title: &str,
    tooltip: &str,
) {
    let entity = world
        .spawn((
            Square,
            bevy::ui_widgets::Button,
            ActionButton::new(owner, crate::actions![command]),
            Node {
                padding: UiRect::all(px(6)),
                ..default()
            },
            Tooltip(tooltip.into()),
            crate::token_style::background(crate::tokens::Token::Surface),
            ChildOf(parent),
        ))
        .id();
    label(world, entity, title, 14.0);
}
fn input(
    world: &mut World,
    parent: Entity,
    area: Entity,
    field: Field,
    title: &str,
    value: String,
    tooltip: &str,
) {
    label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor(&value, world.resource::<crate::theme::Typography>(), 0);
    let entity = world.spawn(bundle).id();
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .max_characters = Some(256);
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            min_height: px(30),
            flex_shrink: 0.0,
            ..default()
        },
        Input {
            area,
            field,
            observed: value,
        },
        Tooltip(tooltip.into()),
        ChildOf(parent),
    ));
    if matches!(field, Field::Password) {
        world
            .entity_mut(entity)
            .remove::<crate::token_style::TextToken>()
            .insert(TextColor(Color::NONE));
        let font = world.resource::<crate::theme::Typography>().text(22.0);
        world.spawn((
            Text::new(""),
            font,
            crate::token_style::text(crate::tokens::Token::Ink),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                ..default()
            },
            Pickable::IGNORE,
            PasswordMask(entity),
            ChildOf(entity),
        ));
    }
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            return;
        };
        let target = world
            .get::<filter::Subscription>(owner)
            .map_or(owner, |filter| filter.0);
        if !crate::area_panel::owns(world, root, target)
            || (!matches!(self, Self::Page(_))
                && !world
                    .get::<crate::edit_mode::EditMode>(root)
                    .is_some_and(|mode| mode.enabled && mode.areas))
        {
            return;
        }
        if matches!(self, Self::Query) {
            let existing = world
                .query::<(Entity, &QueryEditor)>()
                .iter(world)
                .find(|(_, link)| link.0 == owner)
                .map(|(entity, _)| entity);
            if let Some(entity) = existing {
                world.despawn(entity);
            }
            let Some(config) = configuration(world, owner) else {
                return;
            };
            let workspace = world
                .get::<crate::workspace::WorkspaceMember>(owner)
                .unwrap()
                .0;
            let position = world
                .get::<crate::canvas::CanvasView>(root)
                .map_or(bevy::math::DVec2::ZERO, |view| view.center);
            let entity =
                crate::protein_castle::spawn(world, root, workspace, position, config.draft);
            world.entity_mut(entity).insert(QueryEditor(owner));
            crate::protein_castle::refresh_editor(world, entity);
            mirror_editor(world, owner);
            return;
        }
        if let Self::Page(next) = self {
            if let Some(state) = world.resource_mut::<Runtime>().areas.get_mut(&owner) {
                state.page = if *next {
                    (state.page + 1).min(state.data.len().saturating_sub(1) / 200)
                } else {
                    state.page.saturating_sub(1)
                };
                state.dirty = true;
            }
            return;
        }
        if matches!(self, Self::Login) {
            let mut username = String::new();
            let mut password = String::new();
            let mut erase = Vec::new();
            for (entity, field, text) in
                world.query::<(Entity, &Input, &EditableText)>().iter(world)
            {
                if field.area == owner {
                    match field.field {
                        Field::Username => username = text.value().to_string(),
                        Field::Password => {
                            password = text.value().to_string();
                            erase.push(entity);
                        }
                        _ => {}
                    }
                }
            }
            for entity in erase {
                world
                    .get_mut::<EditableText>(entity)
                    .unwrap()
                    .editor
                    .set_text("");
            }
            let result = world
                .resource::<Runtime>()
                .areas
                .get(&owner)
                .and_then(|state| state.remote.as_ref())
                .map(|remote| {
                    remote
                        .outgoing
                        .try_send(ClientMessage::LiveLogin { username, password })
                });
            if !matches!(result, Some(Ok(()))) {
                status(world, owner, "Start the live connection before logging in");
            }
            return;
        }
        let mut configuration = configuration(world, owner);
        if matches!(self, Self::Enable) {
            configuration = Some(Config::default());
        } else if matches!(self, Self::Remove) {
            configuration = None;
        } else if let Some(config) = configuration.as_mut() {
            match self {
                Self::Placement(value) => config.placement = *value,
                Self::SettlingTicks(value) => config.settling_ticks = *value,
                Self::SpawnTarget(id) => {
                    if config.spawn_targets.contains(id) {
                        config.spawn_targets.retain(|target| target != id);
                    } else {
                        config.spawn_targets.push(id.clone());
                    }
                }
                _ => {}
            }
            match self {
                Self::HideFilled => config.hide_filled = !config.hide_filled,
                Self::Relations => {
                    config.relations = !config.relations;
                    if config.relations {
                        config.record_cards = true;
                        config.bindings = Config::records().bindings;
                    }
                }
                Self::Motion => {
                    config.motion = if config.motion.is_some() {
                        None
                    } else {
                        config.grouping = Default::default();
                        Some(Default::default())
                    };
                }
                Self::ClosestDate => config.closest_end_date = !config.closest_end_date,
                Self::Records => {
                    config.record_cards = true;
                    config.show_labels = true;
                    config.bindings = Config::records().bindings;
                }
                Self::Source(remote) => {
                    config.source = if *remote {
                        Source::Organ(String::new())
                    } else {
                        Source::Local
                    };
                    config.enabled = true;
                }
                Self::Organ(uid) => {
                    config.source = Source::Organ(uid.clone());
                    config.enabled = true;
                }
                Self::Add(property) if config.bindings.len() < 32 => {
                    config.bindings.push(Binding::new(property))
                }
                Self::RemoveField(index) if *index < config.bindings.len() => {
                    config.bindings.remove(*index);
                }
                Self::Move(index, next) => {
                    let other = if *next {
                        index + 1
                    } else {
                        index.saturating_sub(1)
                    };
                    if *index < config.bindings.len() && other < config.bindings.len() {
                        config.bindings.swap(*index, other);
                    }
                }
                Self::Square(index) => {
                    if let Some(binding) = config.bindings.get_mut(*index) {
                        binding.square = !binding.square;
                    }
                }
                Self::Editable(index) => {
                    if let Some(binding) = config.bindings.get_mut(*index) {
                        if protein::record_schema::fields()
                            .iter()
                            .any(|field| field.key == binding.property && field.editable)
                        {
                            binding.editable = !binding.editable;
                            if binding.editable
                                && matches!(
                                    binding.property.as_str(),
                                    "assignees" | "assertions" | "work_logs"
                                )
                            {
                                binding.height = binding.height.max(240.0);
                            }
                        }
                    }
                }
                Self::Overflow(index) => {
                    if let Some(binding) = config.bindings.get_mut(*index) {
                        let index = OverflowMode::ALL
                            .iter()
                            .position(|mode| *mode == binding.overflow)
                            .unwrap();
                        binding.overflow = OverflowMode::ALL[(index + 1) % OverflowMode::ALL.len()];
                    }
                }
                Self::DeleteButton => config.delete_button = !config.delete_button,
                Self::GroupProperty(horizontal, property) => {
                    let axis = if *horizontal {
                        &mut config.grouping.horizontal
                    } else {
                        &mut config.grouping.vertical
                    };
                    *axis = property.as_deref().map(GroupAxis::new);
                }
                Self::GroupOrder(horizontal) | Self::GroupDirection(horizontal) => {
                    let axis = if *horizontal {
                        &mut config.grouping.horizontal
                    } else {
                        &mut config.grouping.vertical
                    };
                    if let Some(axis) = axis {
                        if matches!(self, Self::GroupOrder(_)) {
                            axis.descending = !axis.descending;
                        } else {
                            axis.reverse = !axis.reverse;
                        }
                    }
                }
                _ => {}
            }
        }
        if matches!(self, Self::Motion)
            && configuration
                .as_ref()
                .is_some_and(|config| config.motion.is_some())
            && let Some(workspace) = world
                .get::<crate::workspace::WorkspaceMember>(owner)
                .copied()
        {
            crate::workspace_config::set_physics(world, root, workspace.0, true);
        }
        set_configuration(world, owner, configuration);
        crate::edit_mode::render_panel(world, root);
    }
}

pub(super) fn inputs(world: &mut World) {
    let updates: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter(|(_, input, text)| {
            !matches!(input.field, Field::Username | Field::Password)
                && !text.is_composing()
                && text.pending_paste.is_none()
                && text.value().to_string() != input.observed
        })
        .map(|(entity, input, text)| (entity, input.area, input.field, text.value().to_string()))
        .collect();
    for (entity, owner, field, value) in updates {
        let Some(mut config) = configuration(world, owner) else {
            continue;
        };
        let number = value
            .parse::<f32>()
            .ok()
            .filter(|number| number.is_finite());
        if (!matches!(field, Field::Organ) && number.is_none())
            || (matches!(field, Field::Columns) && value.parse::<usize>().is_err())
        {
            world.get_mut::<Input>(entity).unwrap().observed = value;
            status(world, owner, "Enter a valid number");
            continue;
        }
        match field {
            Field::CenterForce | Field::Repulsion | Field::Cooling => {
                if let (Some(number), Some(motion)) = (number, config.motion.as_mut()) {
                    match field {
                        Field::CenterForce => motion.center = f64::from(number),
                        Field::Repulsion => motion.repulsion = f64::from(number),
                        Field::Cooling => motion.cooling = f64::from(number),
                        _ => {}
                    }
                }
            }
            Field::Organ => {
                config.source = Source::Organ(value.clone());
                config.enabled = true;
            }
            Field::Width => {
                if let Some(number) = number {
                    config.width = number;
                }
            }
            Field::Gap => {
                if let Some(number) = number {
                    config.gap = number;
                }
            }
            Field::GroupStrength => {
                if let Some(number) = number {
                    config.grouping.strength = f64::from(number);
                }
            }
            Field::Columns => {
                if let Ok(number) = value.parse() {
                    config.columns = number;
                }
            }
            Field::FieldWidth(index) => {
                if let (Some(number), Some(binding)) = (number, config.bindings.get_mut(index)) {
                    binding.width = number;
                }
            }
            Field::Height(index) => {
                if let (Some(number), Some(binding)) = (number, config.bindings.get_mut(index)) {
                    binding.height = number;
                }
            }
            _ => {}
        }
        if config.valid() {
            set_configuration(world, owner, Some(config));
            world.get_mut::<Input>(entity).unwrap().observed = value;
        } else {
            status(world, owner, "Size or count is outside the supported range");
        }
    }
}

pub(super) fn statuses(world: &mut World) {
    let masks: Vec<_> = world
        .query::<(Entity, &PasswordMask)>()
        .iter(world)
        .map(|(entity, mask)| {
            (
                entity,
                world
                    .get::<EditableText>(mask.0)
                    .map_or(0, |text| text.value().chars().count()),
            )
        })
        .collect();
    for (entity, count) in masks {
        if let Some(mut text) = world.get_mut::<Text>(entity) {
            let value = "•".repeat(count.min(32));
            if text.0 != value {
                text.0 = value;
            }
        }
    }
    let updates: Vec<_> = world
        .query::<(Entity, &Status)>()
        .iter(world)
        .map(|(entity, area)| {
            (
                entity,
                world
                    .resource::<Runtime>()
                    .areas
                    .get(&area.0)
                    .map(|state| {
                        if world.get::<filter::Subscription>(area.0).is_some() {
                            return format!("{} · {} matches", state.status, state.data.len());
                        }
                        format!(
                            "{} · {} records · {} / {}",
                            state.status,
                            state.data.len(),
                            state.page + 1,
                            state.data.len().div_ceil(200).max(1)
                        )
                    })
                    .unwrap_or_else(|| "Stopped".into()),
            )
        })
        .collect();
    for (entity, value) in updates {
        if let Some(mut text) = world.get_mut::<Text>(entity) {
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}

pub(crate) fn controls(world: &mut World, _: Entity, panel: Entity, owner: Entity) {
    let filtering = world.get::<filter::Subscription>(owner).is_some();
    let title = label(
        world,
        panel,
        if filtering {
            if world
                .get::<filter::Subscription>(owner)
                .is_some_and(|s| s.1)
            {
                "Match for property changes"
            } else {
                "Match for attraction and sorting"
            }
        } else {
            "Protein"
        },
        18.0,
    );
    if filtering {
        world.entity_mut(title).insert(Tooltip("Matches existing Record Sands from the selected source for force, sorting, immunity, size and entry/exit changes. Sorting follows this query's order. Fetches identities only, with no row limit, and never spawns Sands. Up to 100,000 matches; larger responses stop the filter.".into()));
    }
    let Some(config) = configuration(world, owner) else {
        button(
            world,
            panel,
            owner,
            Command::Enable,
            Icon::Plus,
            "Make this a Protein Area",
        );
        return;
    };
    if !filtering {
        if config.record_cards {
            text_button(
                world,
                panel,
                owner,
                Command::HideFilled,
                if config.hide_filled {
                    "New Records: title only"
                } else {
                    "New Records: show filled properties"
                },
                "Set the starting accordion layout. Each Record also has its own saved choice.",
            );
        }
        if !config.group_with_source {
            text_button(
                world,
                panel,
                owner,
                Command::Relations,
                if config.relations {
                    "Assertion arrows: on"
                } else {
                    "Assertion arrows: off"
                },
                "Connect spawned Records using the query's included links",
            );
            text_button(
                world,
                panel,
                owner,
                Command::Motion,
                if config.motion.is_some() {
                    "Center and repulsion: on"
                } else {
                    "Center and repulsion: off"
                },
                "Pull only this Protein's spawned Sands toward its center and keep them apart. Motion settles until something changes.",
            );
            if let Some(motion) = &config.motion {
                input(
                    world,
                    panel,
                    owner,
                    Field::CenterForce,
                    "Center force",
                    motion.center.to_string(),
                    "0 to 100",
                );
                input(
                    world,
                    panel,
                    owner,
                    Field::Repulsion,
                    "Repulsion",
                    motion.repulsion.to_string(),
                    "0 to 100000",
                );
                input(
                    world,
                    panel,
                    owner,
                    Field::Cooling,
                    "Settling speed",
                    motion.cooling.to_string(),
                    "0.1 to 10; higher values settle sooner",
                );
            }
        }
        crate::dropdown::spawn(
            world,
            panel,
            owner,
            "Spawn placement",
            match config.placement {
                SpawnPlacement::Source => "At the source",
                SpawnPlacement::MatchingAreas => "In matching areas",
                SpawnPlacement::Physics => "After physics steps",
            },
            vec![
                (
                    "At the source".into(),
                    crate::actions![Command::Placement(SpawnPlacement::Source)],
                ),
                (
                    "In matching areas".into(),
                    crate::actions![Command::Placement(SpawnPlacement::MatchingAreas)],
                ),
                (
                    "After physics steps".into(),
                    crate::actions![Command::Placement(SpawnPlacement::Physics)],
                ),
            ],
        );
        if config.placement == SpawnPlacement::Physics {
            crate::dropdown::spawn(
                world,
                panel,
                owner,
                "Initial physics steps",
                &config.settling_ticks.to_string(),
                [30, 120, 300, 600]
                    .into_iter()
                    .map(|ticks| {
                        (
                            ticks.to_string(),
                            crate::actions![Command::SettlingTicks(ticks)],
                        )
                    })
                    .collect(),
            );
        }
        if config.placement != SpawnPlacement::Source {
            label(world, panel, "Destination areas · empty selects all", 14.0);
            let root = world.get::<ChildOf>(owner).map(ChildOf::parent);
            let member = world
                .get::<crate::workspace::WorkspaceMember>(owner)
                .copied();
            let targets: Vec<_> = world
                .query::<(
                    Entity,
                    &InfluenceArea,
                    &ChildOf,
                    &crate::workspace::WorkspaceMember,
                )>()
                .iter(world)
                .filter(|(e, _, parent, workspace)| {
                    *e != owner && Some(parent.parent()) == root && Some(**workspace) == member
                })
                .map(|(_, a, _, _)| (a.id.clone(), a.name.clone()))
                .collect();
            for (id, title) in targets {
                let selected = config.spawn_targets.contains(&id);
                text_button(
                    world,
                    panel,
                    owner,
                    Command::SpawnTarget(id),
                    &format!("{} {title}", if selected { "✓" } else { "○" }),
                    "Choose this destination area",
                );
            }
        }
    }
    let row = crate::area_panel::row(world, panel);
    button(
        world,
        row,
        owner,
        Command::Query,
        Icon::Pencil,
        "Edit the query in a Protein Castle; changes return to this Area",
    );
    button(
        world,
        row,
        owner,
        Command::Remove,
        Icon::Close,
        if filtering {
            "Remove the Protein filter and restore simple matching rules"
        } else {
            "Remove Protein spawning from this Area"
        },
    );
    let status = label(world, panel, "Stopped", 14.0);
    world.entity_mut(status).insert(Status(owner));
    label(world, panel, "Data", 14.0);
    let toggle = crate::dropdown::spawn(
        world,
        panel,
        owner,
        "Data source",
        if config.source == Source::Local {
            "Local"
        } else {
            "Organ"
        },
        vec![
            ("Local".into(), crate::actions![Command::Source(false)]),
            ("Organ".into(), crate::actions![Command::Source(true)]),
        ],
    );
    world.entity_mut(toggle).insert(Tooltip(
        "Read local data or live data from another Organ using its login and permissions".into(),
    ));
    if let Source::Organ(organ) = &config.source {
        input(
            world,
            panel,
            owner,
            Field::Organ,
            "Organ",
            organ.clone(),
            "Known Organ UID",
        );
        let contacts: Vec<_> = world
            .query::<&crate::area::RecordProperties>()
            .iter(world)
            .filter(|properties| properties.0["kind"] == "organ")
            .filter_map(|properties| {
                Some((
                    properties.0["uid"].as_str()?.to_string(),
                    properties.0["head"].as_str().unwrap_or("Organ").to_string(),
                ))
            })
            .collect();
        for (uid, title) in contacts
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_iter()
            .take(32)
        {
            text_button(
                world,
                panel,
                owner,
                Command::Organ(uid.clone()),
                &title,
                &uid,
            );
        }
        input(
            world,
            panel,
            owner,
            Field::Username,
            "Username",
            String::new(),
            "Only needed when the remote Organ asks for a password login",
        );
        input(
            world,
            panel,
            owner,
            Field::Password,
            "Password",
            String::new(),
            "Used once for this connection and never saved in the workspace",
        );
        button(
            world,
            panel,
            owner,
            Command::Login,
            Icon::Check,
            "Log in to the selected Organ",
        );
    }
    label(world, panel, "End date order", 14.0);
    button(
        world,
        panel,
        owner,
        Command::ClosestDate,
        if config.closest_end_date {
            Icon::Recenter
        } else {
            Icon::Forward
        },
        if config.closest_end_date {
            "Nearest to today first; click to use query order"
        } else {
            "Query order; click to put dates nearest to today first"
        },
    );
    if filtering {
        return;
    }
    if config.motion.is_none() {
        grouping_controls(world, panel, owner, &config);
    }
    label(world, panel, "Row template", 18.0);
    text_button(
        world,
        panel,
        owner,
        Command::Records,
        "Record Castle",
        "Use the Record Castle with editable properties",
    );
    input(
        world,
        panel,
        owner,
        Field::Width,
        "Width",
        config.width.to_string(),
        "Outer Castle width; growing fields stop at this width",
    );
    input(
        world,
        panel,
        owner,
        Field::Columns,
        "Columns",
        config.columns.to_string(),
        "Columns inside each group intersection, or across the Area when grouping is off",
    );
    input(
        world,
        panel,
        owner,
        Field::Gap,
        "Gap",
        config.gap.to_string(),
        "Space between row Castles",
    );
    for (index, binding) in config.bindings.iter().enumerate() {
        let field = protein::record_schema::fields()
            .into_iter()
            .find(|field| field.key == binding.property)
            .unwrap();
        label(world, panel, field.title, 16.0);
        let row = crate::area_panel::row(world, panel);
        button(
            world,
            row,
            owner,
            Command::Square(index),
            if binding.square {
                Icon::Square
            } else {
                Icon::Text
            },
            "Switch between Square and text",
        );
        if field.editable {
            button(
                world,
                row,
                owner,
                Command::Editable(index),
                if binding.editable {
                    Icon::EditableText
                } else {
                    Icon::Text
                },
                "Switch between display and editing the Record property",
            );
        }
        button(
            world,
            row,
            owner,
            Command::Move(index, false),
            Icon::Forward,
            "Move property up",
        );
        button(
            world,
            row,
            owner,
            Command::Move(index, true),
            Icon::Backward,
            "Move property down",
        );
        button(
            world,
            row,
            owner,
            Command::RemoveField(index),
            Icon::Close,
            "Remove this property from the template and query",
        );
        input(
            world,
            panel,
            owner,
            Field::FieldWidth(index),
            "Width",
            binding.width.to_string(),
            "Property width",
        );
        input(
            world,
            panel,
            owner,
            Field::Height(index),
            "Height",
            binding.height.to_string(),
            "Property height",
        );
        text_button(
            world,
            panel,
            owner,
            Command::Overflow(index),
            binding.overflow.title(),
            "Cycle clipping, scrolling and growing",
        );
    }
    label(world, panel, "Add property", 14.0);
    let toggle = crate::dropdown::spawn(
        world,
        panel,
        owner,
        "Add property",
        "+",
        protein::record_schema::fields()
            .into_iter()
            .map(|field| {
                (
                    field.title.into(),
                    crate::actions![Command::Add(field.key.into())],
                )
            })
            .collect(),
    );
    world
        .entity_mut(toggle)
        .insert(Tooltip("Add a property returned by the Protein".into()));
    let row = crate::area_panel::row(world, panel);
    button(
        world,
        row,
        owner,
        Command::DeleteButton,
        if config.delete_button {
            Icon::Check
        } else {
            Icon::Delete
        },
        "Include a delete Record button in every row Castle",
    );
    button(
        world,
        row,
        owner,
        Command::Page(false),
        Icon::Previous,
        "Previous 200 Records",
    );
    button(
        world,
        row,
        owner,
        Command::Page(true),
        Icon::Next,
        "Next 200 Records",
    );
    label(world, panel, "Preview", 14.0);
    let preview = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                overflow: Overflow::clip(),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(panel),
        ))
        .id();
    let data = protein::record_schema::fields()
        .into_iter()
        .map(|field| (field.key.to_string(), Value::String(field.title.into())))
        .collect::<serde_json::Map<_, _>>();
    rows::content(world, preview, &config, &Value::Object(data), None);
}

pub(super) fn navigation(world: &mut World, owner: Entity, page: usize, count: usize) -> Entity {
    let parent = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(-44),
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    button(
        world,
        parent,
        owner,
        Command::Page(false),
        Icon::Previous,
        "Previous Records",
    );
    label(
        world,
        parent,
        &format!("{} / {} · {count}", page + 1, count.div_ceil(200)),
        14.0,
    );
    button(
        world,
        parent,
        owner,
        Command::Page(true),
        Icon::Next,
        "Next Records",
    );
    parent
}

fn grouping_controls(world: &mut World, panel: Entity, owner: Entity, config: &Config) {
    let title = label(world, panel, "Grouping", 18.0);
    world.entity_mut(title).insert(Tooltip("Group the visible page by one property per axis. Groups are ordered across all results before paging. Assignee and Assertion sets keep each Record in one group. Unset values come last. Grouping properties may also appear on the cards.".into()));
    for (horizontal, axis, title) in [
        (true, &config.grouping.horizontal, "Horizontal"),
        (false, &config.grouping.vertical, "Vertical"),
    ] {
        label(world, panel, title, 14.0);
        let fields = protein::record_schema::fields();
        let selected = axis
            .as_ref()
            .and_then(|axis| fields.iter().find(|field| field.key == axis.property))
            .map_or("None", |field| field.title);
        let mut choices = vec![(
            "None".into(),
            crate::actions![Command::GroupProperty(horizontal, None)],
        )];
        choices.extend(
            fields
                .iter()
                .filter(|field| grouping::PROPERTIES.contains(&field.key))
                .map(|field| {
                    (
                        field.title.into(),
                        crate::actions![Command::GroupProperty(horizontal, Some(field.key.into()))],
                    )
                }),
        );
        let toggle = crate::dropdown::spawn(world, panel, owner, title, selected, choices);
        world.entity_mut(toggle).insert(Tooltip("Fetch this property for grouping, independently of the row template. Dates sort by date, quantities by number, and text alphabetically.".into()));
        if let Some(axis) = axis {
            let row = crate::area_panel::row(world, panel);
            label(world, row, "Order", 14.0);
            button(
                world,
                row,
                owner,
                Command::GroupOrder(horizontal),
                if axis.descending {
                    Icon::Backward
                } else {
                    Icon::Forward
                },
                if axis.descending {
                    "Descending order; switch to ascending"
                } else {
                    "Ascending order; switch to descending"
                },
            );
            label(world, row, "Direction", 14.0);
            let direction = button(
                world,
                row,
                owner,
                Command::GroupDirection(horizontal),
                if axis.reverse {
                    Icon::Previous
                } else {
                    Icon::Next
                },
                match (horizontal, axis.reverse) {
                    (true, false) => "Groups extend right; switch to left",
                    (true, true) => "Groups extend left; switch to right",
                    (false, false) => "Groups extend down; switch to up",
                    (false, true) => "Groups extend up; switch to down",
                },
            );
            if !horizontal {
                world
                    .entity_mut(direction)
                    .insert(UiTransform::from_rotation(Rot2::radians(
                        std::f32::consts::FRAC_PI_2,
                    )));
            }
        }
    }
    if config.grouping.active() {
        input(
            world,
            panel,
            owner,
            Field::GroupStrength,
            "Group attraction",
            config.grouping.strength.to_string(),
            "Pull each card toward its sorted place along the configured axes when physics is enabled. Zero keeps grouping without attraction. Pinned cards are not pulled. Reach follows this Area's reach setting. Group Areas are managed here and regenerated from the query.",
        );
    }
}
