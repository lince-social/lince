mod persistence;
#[cfg(test)]
mod tests;

use crate::{
    actions::{Action, ActionButton},
    area::{AreaShape, InfluenceArea, RecordProperties},
    area_mutation::{HeldPoint as HeldMembership, Preparing},
    canvas::CanvasItem,
    cell_bridge::{CellBridge, CellMessage},
    icons::{Icon, IconButton},
    layout::{Arrangement, LayoutBox, LayoutRuntime, Rules, Sizing},
    protein_area::{Config, RecordBinding},
    workspace::WorkspaceMember,
};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub area: String,
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
        std::iter::once(&self.source).chain(self.columns.iter().map(|c| &c.area))
    }

    pub(crate) fn remap(&mut self, ids: &HashMap<String, String>) {
        self.source = ids[&self.source].clone();
        for column in &mut self.columns {
            column.area = ids[&column.area].clone();
        }
    }
}

#[derive(Component, Clone, Copy)]
struct Part {
    owner: Entity,
    column: usize,
}

pub(crate) fn part_owner(world: &World, entity: Entity) -> Option<Entity> {
    world.get::<Part>(entity).map(|part| part.owner)
}

#[derive(Component, Default)]
struct View {
    status: String,
    setup: bool,
    initialized: bool,
}

#[derive(Component)]
struct Status;

#[derive(Component)]
struct Card {
    column: Option<Entity>,
    quantity: String,
}

use crate::full_record::RecordCard;

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
    Create(usize),
}

pub struct KanbanPlugin;
impl Plugin for KanbanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_systems(
                Update,
                update
                    .after(crate::protein_area::UpdateProteinAreas)
                    .before(crate::physics::SimulateWorkspaces),
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
    let mut config = Config::records();
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
    if count + 8 > crate::area::MAX_AREAS {
        return None;
    }
    let mut source = rectangle(position + DVec2::new(0.0, 400.0), DVec2::new(320.0, 80.0));
    source.name = "Kanban · Task spawning".into();
    let mut spawning = config();
    spawning.placement = crate::protein_area::SpawnPlacement::MatchingAreas;
    source.protein = Some(spawning);
    let mut board = Kanban {
        source: source.id.clone(),
        columns: Vec::new(),
    };
    crate::area::spawn_area(world, root, workspace, source)?;
    for (index, (title, slug, quantity)) in COLUMNS.iter().enumerate() {
        let center = position + DVec2::new((index as f64 - 3.0) * 340.0, 0.0);
        let mut area = rectangle(center, DVec2::new(340.0, 640.0));
        area.name = (*title).into();
        area.include_right_edge = index == COLUMNS.len() - 1;
        area.change_filter = Some(config());
        area.filter = Some(config());
        area.changes.enter.quantity = Some(quantity.to_string());
        area.changes.enter.assert = vec![(*slug).into()];
        area.changes.leave.quantity = Some("0".into());
        area.changes.leave.retract = vec![(*slug).into()];
        area.filter.as_mut().unwrap().draft.query["where"][0]["all"]
            .as_array_mut()
            .unwrap()
            .push(json!({"quantity_eq":quantity.to_string()}));
        area.strength = 100.0;
        area.reach.mode = crate::area::ReachMode::Unlimited;
        area.sorting = Some(Default::default());
        board.columns.push(Column {
            area: area.id.clone(),
        });
        let area = crate::area::spawn_area(world, root, workspace, area)?;
        let mut rules = Rules::fixed(Vec2::new(340.0, 640.0));
        rules.arrangement = Arrangement::Column;
        rules.padding = 12.0;
        rules.gap = 12.0;
        rules.axes[1].overflow = crate::layout::Overflow::Scroll;
        let _ = crate::layout::configure(world, area, rules);
    }
    let targets: Vec<_> = board.columns.iter().map(|c| c.area.clone()).collect();
    if let Some(source) = world
        .query::<&mut InfluenceArea>()
        .iter_mut(world)
        .find(|a| a.id == board.source)
    {
        source.into_inner().protein.as_mut().unwrap().spawn_targets = targets;
    }
    let owner = restore(
        world,
        root,
        workspace,
        position - DVec2::new(0.0, 510.0),
        board,
    );
    let mut id = [0; 16];
    getrandom::fill(&mut id).expect("Kanban group identity");
    let board = world.get::<Kanban>(owner).unwrap().clone();
    let members: Vec<_> = std::iter::once(owner)
        .chain(board.ids().filter_map(|id| area(world, owner, id)))
        .collect();
    for member in &members {
        world
            .entity_mut(*member)
            .insert(crate::canvas_selection::SandGroup(id));
    }
    crate::topology::groups::attach(world, &members);
    Some(owner)
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
    let owner = sand(world, root, workspace, position, Vec2::new(340.0, 80.0));
    world
        .entity_mut(owner)
        .insert((board.clone(), View::default()));
    for (index, column) in board.columns.iter().enumerate() {
        if let Some(column) = area(world, owner, &column.area) {
            if let Some((_, slug, quantity)) = COLUMNS.get(index) {
                let mut area = world.get_mut::<InfluenceArea>(column).unwrap();
                if let Some(filter) = area.filter.as_mut() {
                    let conditions = &mut filter.draft.query["where"];
                    if *conditions
                        == json!([{"all":[
                            {"kind_eq":"plain"}, {"concept_in":"task"},
                            {"quantity_eq":quantity.to_string()}, {"concept_in":slug}
                        ]}])
                    {
                        conditions[0]["all"].as_array_mut().unwrap().pop();
                    }
                }
            }
            world.entity_mut(column).insert(Preparing);
        }
    }
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
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Kanban Castle",
        "Move Tasks through columns of work.",
        Command::Create,
        |world, root| {
            spawn(world, root, 1, DVec2::ZERO).unwrap();
            crate::sand_store::preview::compose(world, root)
        },
    );
}

fn header(world: &mut World, owner: Entity, column: usize, position: DVec2) -> Entity {
    let root = world.get::<ChildOf>(owner).unwrap().parent();
    let workspace = world.get::<WorkspaceMember>(owner).unwrap().0;
    let entity = if column == 0 {
        owner
    } else {
        sand(world, root, workspace, position, Vec2::new(340.0, 80.0))
    };
    world.entity_mut(entity).insert(Part { owner, column });
    let row = ui_row(world, entity);
    world.get_mut::<Node>(row).unwrap().min_height = px(40);
    let title = crate::edit_mode::label(world, row, "", 18.0);
    world.entity_mut(title).insert((
        ColumnTitle,
        TextLayout::no_wrap(),
        bevy::text::LineHeight::RelativeToFont(1.4),
    ));
    world.get_mut::<Node>(title).unwrap().flex_grow = 1.0;
    button(
        world,
        row,
        owner,
        Icon::Plus,
        "Create a blank Record in this column",
        Command::Add(column),
    );
    button(
        world,
        row,
        owner,
        Icon::Engine,
        "Configure this column and all its behaviors",
        Command::Column(column),
    );
    let count = crate::edit_mode::label(world, entity, "… records", 13.0);
    world.entity_mut(count).insert(Status);
    entity
}

#[derive(Component)]
struct ColumnTitle;

#[derive(Clone)]
enum Command {
    Create,
    Initialize,
    Column(usize),
    Add(usize),
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
        let Some(board) = world.get::<Kanban>(owner).cloned() else {
            return;
        };
        let root = world.get::<ChildOf>(owner).unwrap().parent();
        match self {
            Self::Initialize => {
                if world
                    .resource::<Requests>()
                    .pending
                    .values()
                    .any(|(_, request)| matches!(request, Request::Concepts | Request::Concept))
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
                world.get_mut::<View>(owner).unwrap().initialized = true;
                world.get_mut::<View>(owner).unwrap().status = "Setting up statuses…".into();
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
            Self::Add(index) => {
                if board
                    .columns
                    .iter()
                    .filter_map(|c| area(world, owner, &c.area))
                    .any(|e| world.get::<Preparing>(e).is_some())
                {
                    world.get_mut::<View>(owner).unwrap().status =
                        "Preparing task statuses…".into();
                    return;
                }
                if world
                    .resource::<Requests>()
                    .pending
                    .values()
                    .any(|(target, request)| {
                        *target == owner
                            && matches!(request, Request::Create(column) if column == index)
                    })
                {
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
                        "This quantity cannot be used when creating a Record".into();
                    return;
                };
                let mut tags = changes.assert;
                tags.push("task".into());
                tags.retain(|tag| !changes.retract.contains(tag));
                tags.sort();
                tags.dedup();
                request(world, owner, Request::Create(*index), |id| {
                    ClientMessage::Act {
                        id,
                        action: engine::actions::Action::CreateRecordWithTags {
                            head: String::new(),
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
                if matches!(kind, Request::Create(_)) {
                    world.get_mut::<View>(owner).unwrap().status = "Record created".into();
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
        if !world.get::<View>(owner).unwrap().initialized
            && world.get_non_send::<CellBridge>().is_some()
        {
            Command::Initialize.apply(world, owner);
        }
        maintain(world, owner, &board);
    }
    let released: Vec<_> = world
        .query_filtered::<Entity, (With<RecordCard>, With<HeldMembership>, Without<Card>)>()
        .iter(world)
        .filter(|entity| {
            world
                .get_resource::<crate::topology::input::PointerState>()
                .is_none_or(|state| state.drag.is_none_or(|(held, _)| held != *entity))
        })
        .collect();
    for entity in released {
        world.entity_mut(entity).remove::<HeldMembership>();
    }
}

fn maintain(world: &mut World, owner: Entity, board: &Kanban) {
    let Some(source) = area(world, owner, &board.source) else {
        world.get_mut::<View>(owner).unwrap().status = "Task source Area removed".into();
        return;
    };
    arrange(world, owner, board, source);
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
            area(world, owner, &column.area)
                .and_then(|area| crate::protein_area::ordered_records(world, area))
                .map(|(_, ids)| ids)
        })
        .collect();
    for (index, column) in columns.iter().enumerate() {
        let Some(column) = *column else {
            continue;
        };
        let item = *world.get::<CanvasItem>(column).unwrap();
        let part = world
            .query::<(Entity, &Part)>()
            .iter(world)
            .find(|(_, p)| p.owner == owner && p.column == index)
            .map(|(e, _)| e);
        let position = item.position + DVec2::new(0.0, -f64::from(item.size.y) * 0.5 - 40.0);
        let part = part.unwrap_or_else(|| header(world, owner, index, position));
        if world.get::<CanvasItem>(part).unwrap().size.x != item.size.x {
            world.get_mut::<CanvasItem>(part).unwrap().size.x = item.size.x;
        }
        let placement = crate::topology::spatial(world, column);
        let offset = position - item.position;
        place(
            world,
            part,
            placement.position(item.position)
                + placement.rotation() * DVec3::new(offset.x, 0.0, offset.y),
        );
        rotate(world, part, placement.rotation);
        if let Some(group) = world
            .get::<crate::canvas_selection::SandGroup>(column)
            .copied()
        {
            world.entity_mut(part).insert(group);
        } else {
            world
                .entity_mut(part)
                .remove::<crate::canvas_selection::SandGroup>();
        }
        refresh_attachment(world, part, column);
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
        let area = world.get::<InfluenceArea>(column).unwrap();
        let title = format!(
            "{}: {}",
            area.name,
            area.changes.enter.quantity.as_deref().unwrap_or("—")
        );
        let labels: Vec<_> = world
            .query::<(Entity, &ChildOf, Has<Status>, Has<ColumnTitle>)>()
            .iter(world)
            .filter(|(_, parent, status, title)| {
                (*status || *title)
                    && (parent.parent() == part
                        || world
                            .get::<ChildOf>(parent.parent())
                            .is_some_and(|p| p.parent() == part))
            })
            .map(|(e, _, status, _)| (e, status))
            .collect();
        for (label, status) in labels {
            set_label(
                world,
                label,
                if status {
                    format!("{count} records")
                } else {
                    title.clone()
                },
            );
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
                world.entity_mut(*column).remove::<Preparing>();
                crate::area_mutation::preview(world, root, *column);
                crate::area_mutation::arm(world, root, *column);
            }
            world.get_mut::<View>(owner).unwrap().setup = false;
            world.get_mut::<View>(owner).unwrap().status = "Ready".into();
        }
    }
    let cards: Vec<_> = world
        .query::<(Entity, &RecordBinding, &RecordProperties)>()
        .iter(world)
        .filter(|(e, b, _)| {
            b.area == source
                && world
                    .get::<crate::protein_area::placement::Pending>(*e)
                    .is_none()
        })
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
        let matching: Vec<_> = columns
            .iter()
            .enumerate()
            .filter(|(_, column)| {
                column.is_some_and(|column| {
                    let area = world.get::<InfluenceArea>(column).unwrap();
                    let record = world.get::<RecordProperties>(entity).unwrap();
                    area.enabled
                        && area.attraction_enabled
                        && if area.filter.is_some() {
                            world
                                .get::<crate::protein_area::filter::Matches>(column)
                                .is_some_and(|matches| {
                                    matches.allows(record, world.get::<RecordBinding>(entity))
                                })
                        } else {
                            area.matches(record)
                        }
                })
            })
            .map(|(index, _)| index)
            .collect();
        let index = (matching.len() == 1).then(|| matching[0]);
        let column = index.and_then(|i| columns[i]);
        let quantity = properties["quantity"].to_string();
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
        if changed {
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
}

pub fn status(world: &World, owner: Entity) -> &str {
    world
        .get::<Kanban>(owner)
        .and_then(|board| area(world, owner, &board.source))
        .map_or("Task source Area removed", |source| {
            crate::protein_area::calendar_status(world, source)
        })
}

#[cfg(test)]
fn column_index(data: &serde_json::Value) -> Option<usize> {
    let quantity = data["quantity"]
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

pub(crate) fn begin_drag(world: &mut World, card: Entity, point: DVec3) {
    let Some(position) = crate::topology::position(world, card) else {
        return;
    };
    let membership = crate::layout::membership(world, card).unwrap_or(position);
    let offset = world
        .get::<LayoutRuntime>(card)
        .map_or(Vec2::ZERO, |r| r.visual_offset);
    crate::layout::detach(world, card);
    let rotation = crate::topology::spatial(world, card).rotation();
    crate::topology::set_position(
        world,
        card,
        position - rotation * DVec3::new(f64::from(offset.x), 0.0, f64::from(offset.y)),
    );
    world.entity_mut(card).insert(HeldMembership(membership));
    world
        .resource_mut::<crate::topology::input::PointerState>()
        .drag = Some((card, point));
}

fn arrange(world: &mut World, owner: Entity, board: &Kanban, source: Entity) {
    let columns: Vec<_> = board
        .columns
        .iter()
        .filter_map(|c| area(world, owner, &c.area))
        .collect();
    let Some(first) = columns.first().copied() else {
        return;
    };
    for column in &columns {
        if let Some(layout) = world.get::<LayoutBox>(*column) {
            let mut size = world.get::<CanvasItem>(*column).unwrap().size;
            for axis in 0..2 {
                let rule = layout.rules.axes[axis];
                if rule.sizing == Sizing::Fixed {
                    size[axis] = rule.size.clamp(rule.min, rule.max);
                }
            }
            if world.get::<CanvasItem>(*column).unwrap().size != size {
                world.get_mut::<CanvasItem>(*column).unwrap().size = size;
            }
            if world.get::<InfluenceArea>(*column).unwrap().size != size.as_dvec2().to_array() {
                world.get_mut::<InfluenceArea>(*column).unwrap().size = size.as_dvec2().to_array();
            }
        }
    }
    let placement = crate::topology::spatial(world, first);
    let first_position = crate::topology::position(world, first).unwrap_or_default();
    let first_size = world.get::<CanvasItem>(first).unwrap().size;
    let mut left = -f64::from(first_size.x) * 0.5;
    let mut bottom = 0.0_f64;
    for column in columns {
        let size = world.get::<CanvasItem>(column).unwrap().size;
        let offset = DVec3::new(left + f64::from(size.x) * 0.5, 0.0, 0.0);
        place(
            world,
            column,
            first_position + placement.rotation() * offset,
        );
        rotate(world, column, placement.rotation);
        refresh_attachment(world, column, first);
        left += f64::from(size.x);
        bottom = bottom.max(f64::from(size.y) * 0.5);
    }
    if world
        .get::<crate::canvas_selection::SandGroup>(first)
        .is_some()
        && world.get::<crate::canvas_selection::SandGroup>(source)
            == world.get::<crate::canvas_selection::SandGroup>(first)
    {
        let width = left + f64::from(first_size.x) * 0.5;
        let center = (width - f64::from(first_size.x)) * 0.5;
        let size = world.get::<CanvasItem>(source).unwrap().size;
        place(
            world,
            source,
            first_position
                + placement.rotation()
                    * DVec3::new(center, 0.0, bottom + 40.0 + f64::from(size.y) * 0.5),
        );
        rotate(world, source, placement.rotation);
        refresh_attachment(world, source, first);
    }
}

fn place(world: &mut World, entity: Entity, point: DVec3) {
    if crate::topology::position(world, entity) != Some(point) {
        crate::topology::set_position(world, entity, point);
    }
}

fn rotate(world: &mut World, entity: Entity, rotation: [f64; 4]) {
    if crate::topology::spatial(world, entity).rotation != rotation {
        world
            .get_mut::<crate::topology::Spatial>(entity)
            .unwrap()
            .rotation = rotation;
    }
}

fn refresh_attachment(world: &mut World, entity: Entity, anchor: Entity) {
    let Some(pose) = world
        .get::<crate::topology::groups::GroupPose>(anchor)
        .copied()
    else {
        return;
    };
    if world
        .get::<crate::canvas_selection::SandGroup>(entity)
        .is_none()
    {
        return;
    }
    let inverse = bevy::math::DQuat::from_array(pose.rotation).inverse();
    let position = crate::topology::position(world, entity).unwrap_or_default();
    let rotation = crate::topology::spatial(world, entity).rotation();
    world.entity_mut(entity).insert((
        pose,
        crate::topology::Attachment {
            position: (inverse * (position - DVec3::from_array(pose.position))).to_array(),
            rotation: (inverse * rotation).to_array(),
        },
    ));
}
