mod persistence;
#[cfg(test)]
mod tests;

use crate::{
    actions::{Action, ActionButton},
    area::{AreaShape, InfluenceArea, RecordProperties},
    area_mutation::HeldPoint as HeldMembership,
    canvas::CanvasItem,
    cell_bridge::{CellBridge, CellMessage},
    icons::{Icon, IconButton, Tooltip},
    layout::{Arrangement, LayoutBox, LayoutRuntime, Rules, Sizing},
    protein_area::{Config, RecordBinding},
    workspace::WorkspaceMember,
};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
    text::EditableText,
};
use cell::{ClientMessage, ServerMessage};
pub(crate) use persistence::{SavedKanban, snapshot};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};

const COLUMNS: [(&str, &str, i32); 7] = [
    ("Backlog", "backlog", 0),
    ("Todo", "todo", -1),
    ("Next", "next", -2),
    ("WIP", "wip", -3),
    ("Review", "review", -4),
    ("Done", "done", 1),
    ("Documented", "documented", 2),
];

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Kanban {
    pub source: String,
    pub columns: Vec<Column>,
    pub stationary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub area: String,
    pub attraction: String,
}

impl Kanban {
    pub(crate) fn valid(&self) -> bool {
        let mut ids = HashSet::new();
        self.columns.len() == COLUMNS.len()
            && self.ids().all(|id| {
                id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) && ids.insert(id)
            })
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.source)
            .chain(self.columns.iter().flat_map(|c| [&c.area, &c.attraction]))
    }

    pub(crate) fn remap(&mut self, ids: &HashMap<String, String>) {
        self.source = ids[&self.source].clone();
        for column in &mut self.columns {
            column.area = ids[&column.area].clone();
            column.attraction = ids[&column.attraction].clone();
        }
    }
}

#[derive(Component, Clone, Copy)]
struct Part {
    owner: Entity,
    column: usize,
    count: bool,
}

pub(crate) fn part_owner(world: &World, entity: Entity) -> Option<Entity> {
    world.get::<Part>(entity).map(|part| part.owner)
}

#[derive(Component, Default)]
struct View {
    status: String,
    setup: bool,
}

#[derive(Component)]
struct Status;

#[derive(Component)]
struct Card {
    column: Option<Entity>,
    quantity: String,
}

#[derive(Resource, Default)]
struct Requests {
    next: u64,
    pending: HashMap<String, (Entity, Request)>,
    outgoing: VecDeque<ClientMessage>,
}

#[derive(Clone)]
enum Request {
    Concepts,
    Concept,
    Create(Entity, String),
}

pub struct KanbanPlugin;
impl Plugin for KanbanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_systems(
                Update,
                update.after(crate::protein_area::UpdateProteinAreas),
            )
            .add_systems(PostUpdate, receive.after(crate::cell_bridge::ReceiveCell));
    }
}

fn rectangle(center: DVec2, size: DVec2) -> InfluenceArea {
    InfluenceArea::new(
        AreaShape::Polygon(vec![
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
            [-0.5, -0.5],
        ]),
        center,
        size,
    )
}

fn config() -> Config {
    let mut config = Config::tasks();
    config.enabled = true;
    config.closest_end_date = true;
    config.draft.query = json!({"source":"record", "where":[{"all":[{"kind_eq":"plain"},{"concept_in":"task"}]}], "order":[{"asc":"due_date"}], "limit":null});
    config
}

pub fn spawn(world: &mut World, root: Entity, workspace: u64, position: DVec2) -> Option<Entity> {
    let count = world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .filter(|e| world.get::<InfluenceArea>(**e).is_some())
        .count();
    if count + 15 > crate::area::MAX_AREAS {
        return None;
    }
    let mut source = rectangle(position - DVec2::new(0.0, 420.0), DVec2::new(320.0, 80.0));
    source.name = "Kanban · Task spawning".into();
    source.protein = Some(config());
    let mut board = Kanban {
        source: source.id.clone(),
        columns: Vec::new(),
        stationary: true,
    };
    crate::area::spawn_area(world, root, workspace, source)?;
    for (index, (title, slug, quantity)) in COLUMNS.iter().enumerate() {
        let center = position + DVec2::new((index as f64 - 3.0) * 360.0, 0.0);
        let mut area = rectangle(center, DVec2::new(340.0, 640.0));
        area.name = format!("{title} · Entry and exit");
        area.filter = Some(config());
        area.changes.enter.quantity = Some(quantity.to_string());
        area.changes.enter.assert = vec![(*slug).into()];
        area.changes.leave.quantity = Some("0".into());
        area.changes.leave.retract = vec![(*slug).into()];
        let mut pull = area.clone();
        pull.id = rectangle(center, DVec2::ONE).id;
        pull.name = format!("{title} · Attraction and sorting");
        pull.changes = Default::default();
        pull.filter.as_mut().unwrap().draft.query["where"][0]["all"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"quantity_eq":quantity.to_string()}),
                json!({"concept_in":slug}),
            ]);
        pull.strength = 100.0;
        pull.reach.mode = crate::area::ReachMode::Unlimited;
        pull.sorting = Some(Default::default());
        board.columns.push(Column {
            area: area.id.clone(),
            attraction: pull.id.clone(),
        });
        let area = crate::area::spawn_area(world, root, workspace, area)?;
        let mut rules = Rules::fixed(Vec2::new(340.0, 640.0));
        rules.arrangement = Arrangement::Column;
        rules.padding = 12.0;
        rules.gap = 12.0;
        rules.axes[1].overflow = crate::layout::Overflow::Scroll;
        let _ = crate::layout::configure(world, area, rules);
        crate::area::spawn_area(world, root, workspace, pull)?;
    }
    Some(restore(
        world,
        root,
        workspace,
        position - DVec2::new(0.0, 510.0),
        board,
    ))
}

fn sand(world: &mut World, root: Entity, workspace: u64, position: DVec2, size: Vec2) -> Entity {
    world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            CanvasItem { position, size },
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::topology::Spatial {
                world_pinned: true,
                depth: Some(8.0),
                ..default()
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(4),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id()
}

pub(crate) fn restore(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    board: Kanban,
) -> Entity {
    let owner = sand(world, root, workspace, position, Vec2::new(420.0, 100.0));
    world.entity_mut(owner).insert((board, View::default()));
    let row = ui_row(world, owner);
    crate::edit_mode::label(world, row, "Kanban", 20.0);
    button(
        world,
        row,
        owner,
        Icon::Play,
        "Set up task statuses and arm column changes",
        Command::Start,
    );
    button(
        world,
        row,
        owner,
        Icon::Stop,
        "Disarm column changes",
        Command::Stop,
    );
    button(
        world,
        row,
        owner,
        Icon::Square,
        "Toggle stationary columns and attraction",
        Command::Stationary,
    );
    button(
        world,
        row,
        owner,
        Icon::Pencil,
        "Edit the shared Task query and template",
        Command::Query,
    );
    let label = crate::edit_mode::label(world, owner, "Stopped", 13.0);
    world.entity_mut(label).insert(Status);
    owner
}

fn area(world: &World, owner: Entity, id: &str) -> Option<Entity> {
    let root = world.get::<ChildOf>(owner)?.parent();
    let workspace = world.get::<WorkspaceMember>(owner)?.0;
    world.get::<Children>(root)?.iter().find(|e| {
        world.get::<InfluenceArea>(*e).is_some_and(|a| a.id == id)
            && world
                .get::<WorkspaceMember>(*e)
                .is_some_and(|w| w.0 == workspace)
    })
}

fn ui_row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                column_gap: px(4),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn set_label(world: &mut World, entity: Entity, value: String) {
    if world
        .get::<Text>(entity)
        .is_some_and(|text| text.0 != value)
    {
        world.get_mut::<Text>(entity).unwrap().0 = value;
    }
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    icon: Icon,
    tip: &str,
    command: Command,
) -> Entity {
    world
        .spawn((
            crate::sand::Square,
            IconButton::new(icon, tip),
            ChildOf(parent),
            ActionButton::new(owner, crate::actions![command]),
        ))
        .id()
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    let row = ui_row(world, parent);
    crate::edit_mode::label(world, row, "Kanban Castle", 18.0);
    button(
        world,
        row,
        root,
        Icon::Plus,
        "Add Kanban Areas, column controls, and Task Castles",
        Command::Create,
    );
}

fn header(world: &mut World, owner: Entity, column: usize, count: bool, position: DVec2) -> Entity {
    let root = world.get::<ChildOf>(owner).unwrap().parent();
    let workspace = world.get::<WorkspaceMember>(owner).unwrap().0;
    let size = if count {
        Vec2::new(76.0, 104.0)
    } else {
        Vec2::new(256.0, 104.0)
    };
    let entity = sand(world, root, workspace, position, size);
    world.entity_mut(entity).insert(Part {
        owner,
        column,
        count,
    });
    if count {
        crate::edit_mode::label(world, entity, "Records", 13.0);
        let text = crate::edit_mode::label(world, entity, "…", 24.0);
        world.entity_mut(text).insert(Status);
    } else {
        let row = ui_row(world, entity);
        crate::edit_mode::label(world, row, COLUMNS[column].0, 18.0);
        button(
            world,
            row,
            owner,
            Icon::Info,
            "Configure this column Area",
            Command::Column(column),
        );
        button(
            world,
            row,
            owner,
            Icon::Attract,
            "Configure attraction, matching, and sorting",
            Command::Attraction(column),
        );
        let row = ui_row(world, entity);
        let editor = world
            .spawn(crate::sand::text_editor(
                "",
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .id();
        world.entity_mut(editor).insert((
            ChildOf(row),
            Tooltip("New Task title".into()),
            Node {
                width: px(0),
                flex_grow: 1.0,
                min_width: px(0),
                min_height: px(28),
                ..default()
            },
        ));
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .max_characters = Some(500);
        button(
            world,
            row,
            owner,
            Icon::Plus,
            "Create a Task in this column",
            Command::Add(column, editor),
        );
    }
    entity
}

#[derive(Clone)]
enum Command {
    Create,
    Start,
    Stop,
    Stationary,
    Query,
    Column(usize),
    Attraction(usize),
    Add(usize, Entity),
    Move(i32),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Create) {
            if let Some(workspace) = world
                .get::<crate::workspace::Workspaces>(owner)
                .map(|w| w.active)
            {
                let position = world
                    .get::<crate::canvas::CanvasView>(owner)
                    .map_or(DVec2::ZERO, |v| v.center);
                spawn(world, owner, workspace, position);
            }
            return;
        }
        if let Self::Move(delta) = self {
            move_card(world, owner, *delta);
            return;
        }
        let Some(board) = world.get::<Kanban>(owner).cloned() else {
            return;
        };
        let root = world.get::<ChildOf>(owner).unwrap().parent();
        match self {
            Self::Start => {
                if world
                    .resource::<Requests>()
                    .pending
                    .values()
                    .any(|(e, _)| *e == owner)
                {
                    return;
                }
                request(world, owner, Request::Concepts, |id| {
                    ClientMessage::Subscribe {
                        id,
                        protein: serde_json::from_value(
                            json!({"source":"concept", "fields":["uid","name"], "limit":null}),
                        )
                        .unwrap(),
                    }
                });
                world.get_mut::<View>(owner).unwrap().status = "Setting up statuses…".into();
            }
            Self::Stop => {
                world.get_mut::<View>(owner).unwrap().setup = false;
                for column in &board.columns {
                    if let Some(area) = area(world, owner, &column.area) {
                        crate::area_mutation::disarm(world, area, "Stopped from Kanban controls");
                    }
                }
            }
            Self::Stationary => {
                world.get_mut::<Kanban>(owner).unwrap().stationary = !board.stationary;
                let cards: Vec<_> = world
                    .query::<(Entity, &RecordBinding)>()
                    .iter(world)
                    .filter(|(_, binding)| Some(binding.area) == area(world, owner, &board.source))
                    .map(|(e, _)| e)
                    .collect();
                for card in cards {
                    if board.stationary {
                        let _ = crate::layout::detach(world, card);
                    }
                    world.entity_mut(card).remove::<Card>();
                }
            }
            Self::Query => {
                if let Some(source) = area(world, owner, &board.source) {
                    crate::protein_area::open_query(world, source);
                }
            }
            Self::Column(index) => {
                if let Some(area) = board
                    .columns
                    .get(*index)
                    .and_then(|c| area(world, owner, &c.area))
                {
                    crate::edit_mode::EditAction::Open.apply(world, root);
                    crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(area))
                        .apply(world, root);
                }
            }
            Self::Attraction(index) => {
                if let Some(area) = board
                    .columns
                    .get(*index)
                    .and_then(|c| area(world, owner, &c.attraction))
                {
                    crate::edit_mode::EditAction::Open.apply(world, root);
                    crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(area))
                        .apply(world, root);
                }
            }
            Self::Add(index, editor) => {
                if world
                    .resource::<Requests>()
                    .pending
                    .values()
                    .any(|(_, r)| matches!(r, Request::Create(e, _) if e == editor))
                {
                    return;
                }
                let head = world
                    .get::<EditableText>(*editor)
                    .map(|text| text.value().to_string())
                    .unwrap_or_default();
                if head.trim().is_empty() {
                    world.get_mut::<View>(owner).unwrap().status = "Enter a Task title".into();
                    return;
                }
                let Some(changes) = board
                    .columns
                    .get(*index)
                    .and_then(|column| area(world, owner, &column.area))
                    .and_then(|area| world.get::<InfluenceArea>(area))
                    .map(|area| area.changes.enter.clone())
                else {
                    return;
                };
                let zero = nucleus::DecimalValue::parse_inferred("0").unwrap();
                let quantity = if let Some(value) = changes.quantity.as_ref() {
                    engine::area_transition::QuantityOperation::parse(value)
                        .and_then(|(operation, operand)| operation.evaluate(zero, operand))
                } else {
                    Some(zero)
                };
                let Some(quantity) = quantity.filter(|value| {
                    nucleus::DecimalValue::from_f64_lossy(value.to_f64())
                        .is_ok_and(|roundtrip| roundtrip.exact_numeric_cmp(*value).is_eq())
                }) else {
                    world.get_mut::<View>(owner).unwrap().status =
                        "This quantity cannot be used when creating a Task".into();
                    return;
                };
                let mut tags = changes.assert;
                tags.push("task".into());
                tags.retain(|tag| !changes.retract.contains(tag));
                tags.sort();
                tags.dedup();
                request(world, owner, Request::Create(*editor, head.clone()), |id| {
                    ClientMessage::Act {
                        id,
                        action: engine::actions::Action::CreateRecordWithTags {
                            head,
                            body: String::new(),
                            quantity: quantity.to_f64(),
                            tags,
                        },
                    }
                });
            }
            _ => {}
        }
    }
}

fn request(
    world: &mut World,
    owner: Entity,
    kind: Request,
    message: impl FnOnce(String) -> ClientMessage,
) {
    world.init_resource::<Requests>();
    let mut state = world.resource_mut::<Requests>();
    state.next += 1;
    let id = format!("kanban-{}", state.next);
    state.pending.insert(id.clone(), (owner, kind));
    state.outgoing.push_back(message(id));
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|m| m.0.clone())
        .collect();
    for message in messages {
        if matches!(&message, ServerMessage::Error { id, .. } if id == crate::cell_bridge::CONNECTION)
        {
            world.resource_mut::<Requests>().pending.clear();
            world.resource_mut::<Requests>().outgoing.clear();
            for mut view in world.query::<&mut View>().iter_mut(world) {
                view.status = "Cell disconnected".into();
                view.setup = false;
            }
            continue;
        }
        let id = match &message {
            ServerMessage::Snapshot { id, .. }
            | ServerMessage::ActionOk { id, .. }
            | ServerMessage::Error { id, .. } => id.clone(),
            _ => continue,
        };
        let Some((owner, kind)) = world.resource_mut::<Requests>().pending.remove(&id) else {
            continue;
        };
        if matches!(kind, Request::Concepts) {
            world
                .resource_mut::<Requests>()
                .outgoing
                .push_back(ClientMessage::Unsubscribe { id });
        }
        if world.get::<Kanban>(owner).is_none() {
            continue;
        }
        match message {
            ServerMessage::Snapshot { rows, .. } if matches!(kind, Request::Concepts) => {
                let names: HashSet<_> =
                    rows.iter().filter_map(|row| row["name"].as_str()).collect();
                for name in std::iter::once("task").chain(COLUMNS.iter().map(|(_, slug, _)| *slug))
                {
                    if !names.contains(name) {
                        request(world, owner, Request::Concept, |id| ClientMessage::Act {
                            id,
                            action: engine::actions::Action::CreateConcept {
                                lingua: "g_local".into(),
                                name: name.into(),
                                parents: Vec::new(),
                            },
                        });
                    }
                }
                world.get_mut::<View>(owner).unwrap().setup = true;
            }
            ServerMessage::ActionOk { .. } => {
                if let Request::Create(editor, submitted) = kind {
                    if let Some(mut text) = world.get_mut::<EditableText>(editor) {
                        if text.value().to_string() == submitted {
                            text.editor.set_text("");
                        }
                    }
                    world.get_mut::<View>(owner).unwrap().status = "Task created".into();
                }
            }
            ServerMessage::Error { message, .. } => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.setup = false;
                view.status = message;
            }
            _ => {}
        }
    }
}

fn update(world: &mut World) {
    if crate::laboratory::active(world) {
        return;
    }
    while let Some(message) = world.resource_mut::<Requests>().outgoing.pop_front() {
        let Some(bridge) = world.get_non_send::<CellBridge>() else {
            world
                .resource_mut::<Requests>()
                .outgoing
                .push_front(message);
            break;
        };
        if let Err(error) = bridge.outgoing.try_send(message) {
            match error {
                tokio::sync::mpsc::error::TrySendError::Full(message) => {
                    world
                        .resource_mut::<Requests>()
                        .outgoing
                        .push_front(message);
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    world.resource_mut::<Requests>().pending.clear();
                    world.resource_mut::<Requests>().outgoing.clear();
                    for mut view in world.query::<&mut View>().iter_mut(world) {
                        view.status = "Cell disconnected".into();
                        view.setup = false;
                    }
                }
            }
            break;
        }
    }
    let stale: Vec<_> = world
        .query::<(Entity, &Part)>()
        .iter(world)
        .filter(|(_, part)| world.get::<Kanban>(part.owner).is_none())
        .map(|(e, _)| e)
        .collect();
    for entity in stale {
        world.despawn(entity);
    }
    let owners: Vec<_> = world
        .query::<(Entity, &Kanban)>()
        .iter(world)
        .map(|(e, k)| (e, k.clone()))
        .collect();
    for (owner, board) in owners {
        maintain(world, owner, &board);
    }
}

fn maintain(world: &mut World, owner: Entity, board: &Kanban) {
    let Some(source) = area(world, owner, &board.source) else {
        world.get_mut::<View>(owner).unwrap().status = "Task source Area removed".into();
        return;
    };
    let root = world.get::<ChildOf>(owner).unwrap().parent();
    let columns: Vec<_> = board
        .columns
        .iter()
        .map(|c| area(world, owner, &c.area))
        .collect();
    let data = crate::protein_area::calendar_feed(world, source)
        .map(|(rows, _)| rows.to_vec())
        .unwrap_or_default();
    let orders: Vec<_> = board
        .columns
        .iter()
        .map(|column| {
            area(world, owner, &column.attraction)
                .and_then(|area| crate::protein_area::ordered_records(world, area))
                .map(|(_, ids)| ids)
        })
        .collect();
    for (index, column) in columns.iter().enumerate() {
        let Some(column) = *column else {
            continue;
        };
        let item = *world.get::<CanvasItem>(column).unwrap();
        if let Some(pull) = area(world, owner, &board.columns[index].attraction) {
            let placement = crate::topology::spatial(world, column);
            world.entity_mut(pull).insert(placement);
            let mut pull_item = world.get_mut::<CanvasItem>(pull).unwrap();
            pull_item.position = item.position;
            pull_item.size = item.size;
        }
        for count in [false, true] {
            let part = world
                .query::<(Entity, &Part)>()
                .iter(world)
                .find(|(_, p)| p.owner == owner && p.column == index && p.count == count)
                .map(|(e, _)| e);
            let count_width = 76.0_f32.min(item.size.x * 0.3);
            let part_width = if count {
                count_width
            } else {
                (item.size.x - count_width - 8.0).max(1.0)
            };
            let position = item.position
                + DVec2::new(
                    if count {
                        f64::from(item.size.x - count_width) * 0.5
                    } else {
                        -f64::from(count_width + 8.0) * 0.5
                    },
                    -f64::from(item.size.y) * 0.5 - 64.0,
                );
            let part = part.unwrap_or_else(|| header(world, owner, index, count, position));
            if world.get::<CanvasItem>(part).unwrap().size.x != part_width {
                world.get_mut::<CanvasItem>(part).unwrap().size.x = part_width;
            }
            let placement = crate::topology::spatial(world, column);
            let offset = position - item.position;
            let position = placement.position(item.position)
                + placement.rotation() * DVec3::new(offset.x, 0.0, offset.y);
            if crate::topology::position(world, part) != Some(position) {
                crate::topology::set_position(world, part, position);
            }
            if crate::topology::spatial(world, part).rotation != placement.rotation {
                world
                    .get_mut::<crate::topology::Spatial>(part)
                    .unwrap()
                    .rotation = placement.rotation;
            }
            if count {
                let count = data
                    .iter()
                    .filter(|data| {
                        data["uid"].as_str().is_some_and(|uid| {
                            orders[index]
                                .as_ref()
                                .is_some_and(|ids| ids.iter().any(|id| id == uid))
                        })
                    })
                    .count();
                let labels: Vec<_> = world
                    .get::<Children>(part)
                    .into_iter()
                    .flatten()
                    .filter(|e| world.get::<Status>(**e).is_some())
                    .copied()
                    .collect();
                for label in labels {
                    set_label(world, label, count.to_string());
                }
            }
        }
    }
    let setup = world.get::<View>(owner).unwrap().setup;
    if setup
        && !world
            .resource::<Requests>()
            .pending
            .values()
            .any(|(e, _)| *e == owner)
    {
        let ready = columns.iter().all(|column| {
            column.is_some_and(|column| {
                world
                    .get::<crate::protein_area::filter::Matches>(column)
                    .is_some_and(|f| f.current)
            })
        });
        if ready {
            for column in columns.iter().flatten() {
                crate::area_mutation::preview(world, root, *column);
                crate::area_mutation::arm(world, root, *column);
            }
            world.get_mut::<View>(owner).unwrap().setup = false;
        }
    }
    let cards: Vec<_> = world
        .query::<(Entity, &RecordBinding, &RecordProperties)>()
        .iter(world)
        .filter(|(_, b, _)| b.area == source)
        .map(|(e, _, r)| (e, r.0.clone()))
        .collect();
    for (entity, properties) in cards {
        let released = world.get::<HeldMembership>(entity).is_some();
        if released {
            if world
                .get_resource::<crate::topology::input::PointerState>()
                .is_some_and(|p| p.drag.is_some_and(|(e, _)| e == entity))
            {
                continue;
            }
            world.entity_mut(entity).remove::<HeldMembership>();
        }
        let uid = properties["uid"].as_str().unwrap_or_default();
        let matching: Vec<_> = orders
            .iter()
            .enumerate()
            .filter(|(index, ids)| {
                ids.as_ref()
                    .is_some_and(|ids| ids.iter().any(|id| id == uid))
                    && columns[*index]
                        .and_then(|column| world.get::<InfluenceArea>(column))
                        .is_some_and(|area| state_matches(&properties, &area.changes.enter))
            })
            .map(|(index, _)| index)
            .collect();
        let index = (matching.len() == 1).then(|| matching[0]);
        let column = index.and_then(|i| columns[i]);
        let quantity = properties["quantity_exact"].to_string();
        let old = world.get::<Card>(entity);
        let returned = released
            && column.is_some_and(|column| {
                crate::topology::position(world, entity).is_some_and(|point| {
                    crate::topology::influence::contains(
                        world.get::<InfluenceArea>(column).unwrap(),
                        crate::topology::spatial(world, column),
                        point,
                    )
                })
            });
        let changed = returned || old.is_none_or(|c| c.quantity != quantity || c.column != column);
        if changed && board.stationary {
            if let Some(column) = column {
                let _ = crate::layout::attach(world, entity, column);
                let mut rules = Rules::fixed(world.get::<CanvasItem>(entity).unwrap().size);
                rules.axes[0].sizing = Sizing::Fill;
                rules.axes[1].sizing = Sizing::Fit;
                let _ = crate::layout::configure(world, entity, rules);
            } else {
                let _ = crate::layout::detach(world, entity);
            }
        }
        if changed {
            world.entity_mut(entity).insert(Card { column, quantity });
        }
        let order = index
            .and_then(|index| orders[index].as_ref())
            .and_then(|ids| ids.iter().position(|id| id == uid))
            .unwrap_or(0) as i32;
        if world
            .get::<LayoutBox>(entity)
            .is_some_and(|layout| layout.parent.is_some() && layout.order != order)
        {
            world.get_mut::<LayoutBox>(entity).unwrap().order = order;
        }
    }
    let armed = columns
        .iter()
        .flatten()
        .filter(|column| crate::area_mutation::armed(world, **column))
        .count();
    let status = format!(
        "{} · {} · {armed}/7 armed · {}",
        world.get::<View>(owner).unwrap().status,
        crate::protein_area::calendar_status(world, source),
        if board.stationary {
            "Stationary"
        } else {
            "Attraction"
        }
    );
    let labels: Vec<_> = world
        .get::<Children>(owner)
        .into_iter()
        .flatten()
        .filter(|e| world.get::<Status>(**e).is_some())
        .copied()
        .collect();
    for label in labels {
        set_label(world, label, status.clone());
    }
}

pub fn status(world: &World, owner: Entity) -> &str {
    world
        .get::<Kanban>(owner)
        .and_then(|board| area(world, owner, &board.source))
        .map_or("Task source Area removed", |source| {
            crate::protein_area::calendar_status(world, source)
        })
}

fn state_matches(
    data: &serde_json::Value,
    changes: &engine::area_transition::RecordChanges,
) -> bool {
    let has = |name: &String| {
        data["assertions"].as_array().is_some_and(|assertions| {
            assertions.iter().any(|assertion| {
                assertion["object"].is_null()
                    && (assertion["predicate"].as_str() == Some(name.as_str())
                        || assertion["predicate_uid"].as_str() == Some(name.as_str()))
            })
        })
    };
    let quantity = changes
        .quantity
        .as_ref()
        .and_then(|value| engine::area_transition::QuantityOperation::parse(value));
    changes.assert.iter().all(has)
        && !changes.retract.iter().any(has)
        && quantity.is_none_or(|(operation, expected)| {
            operation != engine::area_transition::QuantityOperation::Set
                || data["quantity_exact"]
                    .as_str()
                    .and_then(|value| nucleus::DecimalValue::parse_inferred(value).ok())
                    .is_some_and(|actual| actual.exact_numeric_cmp(expected).is_eq())
        })
}

#[cfg(test)]
fn column_index(data: &serde_json::Value) -> Option<usize> {
    let quantity = data["quantity_exact"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| data["quantity"].as_f64())?;
    COLUMNS.iter().position(|(_, slug, q)| {
        f64::from(*q) == quantity
            && data["assertions"].as_array().is_some_and(|assertions| {
                assertions.iter().any(|a| {
                    a["predicate"] == *slug || a["name"] == *slug || a["canonical_name"] == *slug
                })
            })
    })
}

pub(crate) fn card_controls(world: &mut World, card: Entity) {
    let row = ui_row(world, card);
    let handle = world
        .spawn((
            crate::sand::Square,
            IconButton::new(Icon::Group, "Drag Task to another column"),
            ChildOf(row),
        ))
        .observe(drag_card)
        .id();
    let _ = handle;
    button(
        world,
        row,
        card,
        Icon::Previous,
        "Move Task to the previous column",
        Command::Move(-1),
    );
    button(
        world,
        row,
        card,
        Icon::Next,
        "Move Task to the next column",
        Command::Move(1),
    );
}

fn drag_card(
    mut event: On<Pointer<Press>>,
    parents: Query<&ChildOf>,
    cards: Query<(), With<RecordBinding>>,
    mut commands: Commands,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let mut card = event.entity;
    while !cards.contains(card) {
        let Ok(parent) = parents.get(card) else {
            return;
        };
        card = parent.parent();
    }
    commands.queue(move |world: &mut World| {
        let Some(root) = world.get::<ChildOf>(card).map(ChildOf::parent) else {
            return;
        };
        let Some(position) = crate::topology::position(world, card) else {
            return;
        };
        let cursor = world
            .query::<&Window>()
            .iter(world)
            .find_map(Window::cursor_position);
        let Some(point) = cursor.and_then(|cursor| {
            crate::topology::input::plane_point(world, root, cursor, position.y)
        }) else {
            return;
        };
        let membership = crate::layout::membership(world, card).unwrap_or(position);
        let offset = world
            .get::<LayoutRuntime>(card)
            .map_or(Vec2::ZERO, |r| r.visual_offset);
        let _ = crate::layout::detach(world, card);
        let rotation = crate::topology::spatial(world, card).rotation();
        crate::topology::set_position(
            world,
            card,
            position - rotation * DVec3::new(f64::from(offset.x), 0.0, f64::from(offset.y)),
        );
        world.entity_mut(card).insert(HeldMembership(membership));
        if let Some(mut state) = world.get_resource_mut::<crate::topology::input::PointerState>() {
            state.drag = Some((card, point));
        }
    });
    event.propagate(false);
}

fn move_card(world: &mut World, card: Entity, delta: i32) {
    let Some(binding) = world.get::<RecordBinding>(card) else {
        return;
    };
    let source = binding.area;
    let board = world
        .query::<(Entity, &Kanban)>()
        .iter(world)
        .find(|(e, b)| area(world, *e, &b.source) == Some(source))
        .map(|(e, b)| (e, b.clone()));
    let Some((owner, board)) = board else {
        return;
    };
    let current = world.get::<Card>(card).and_then(|c| c.column);
    let index = board
        .columns
        .iter()
        .position(|c| area(world, owner, &c.area) == current)
        .unwrap_or(0);
    let next = (index as i32 + delta).clamp(0, 6) as usize;
    if next == index {
        return;
    }
    let Some(target) = area(world, owner, &board.columns[next].area) else {
        return;
    };
    if !crate::area_mutation::armed(world, target) {
        world.get_mut::<View>(owner).unwrap().status = "Arm columns before moving Tasks".into();
        return;
    }
    let Some(position) = crate::topology::position(world, target) else {
        return;
    };
    let _ = crate::layout::detach(world, card);
    crate::topology::set_position(world, card, position);
}
