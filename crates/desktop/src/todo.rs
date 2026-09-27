use crate::{actions::Action, sand_panel as panel};
use bevy::{prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const PAGE: usize = 40;

#[cfg(test)]
mod tests;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct SavedTodo {
    protein: String,
    show_ids: bool,
    draft: String,
}

impl SavedTodo {
    pub(crate) fn valid(&self) -> bool {
        self.protein.len() <= 1024 && self.draft.chars().count() <= 4096
    }
}

#[derive(Component)]
pub struct TodoSand {
    protein: String,
    query: Entity,
    input: Entity,
    list: Entity,
    status: Entity,
    ids_button: Entity,
    show_ids: bool,
    rows: Vec<Value>,
    active: usize,
    subscription: String,
    requested: bool,
    ready: bool,
    dirty: bool,
    pending: Option<(String, Mutation)>,
    undo: Vec<(String, String)>,
}

enum Mutation {
    Add(String),
    Complete(String, String),
    Undo,
}

#[derive(Resource, Default)]
struct Subscriptions(HashSet<String>);

pub struct TodoPlugin;
impl Plugin for TodoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Subscriptions>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Subscriptions>();
    let body = panel::frame(world, sand, "Todo focus");
    let query = panel::field(
        world,
        body,
        "Saved Protein name, slug or uid (blank uses the focus queue)",
        "",
    );
    let controls = panel::row(world, body);
    panel::button(world, controls, sand, "Use Protein", Command::Query);
    panel::button(world, controls, sand, "Refresh", Command::Refresh);
    let ids_button = panel::button(world, controls, sand, "Show ids: off", Command::Ids);
    panel::button(world, controls, sand, "Undo completion", Command::Undo);
    let status = crate::edit_mode::label(world, body, "Connecting to the focus queue…", 12.0);
    let list = panel::column(world, body);
    world
        .entity_mut(list)
        .insert((Queue(sand), bevy::input_focus::tab_navigation::TabIndex(0)));
    {
        let mut node = world.get_mut::<Node>(list).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.overflow = Overflow::scroll_y();
    }
    crate::scroll_sand::attach(world, list);
    world.entity_mut(list).observe(focus).observe(keyboard);
    let navigation = panel::row(world, body);
    panel::button(
        world,
        navigation,
        sand,
        "Previous",
        Command::Move(-(PAGE as isize)),
    );
    panel::button(
        world,
        navigation,
        sand,
        "Next",
        Command::Move(PAGE as isize),
    );
    let input = panel::field(world, body, "New task", "");
    world
        .entity_mut(input)
        .insert(NewTask(sand))
        .observe(add_key);
    panel::button(world, body, sand, "Add task", Command::Add);
    world.entity_mut(sand).insert(TodoSand {
        protein: String::new(),
        query,
        input,
        list,
        status,
        ids_button,
        show_ids: false,
        rows: Vec::new(),
        active: 0,
        subscription: nucleus::new_uid("todo"),
        requested: false,
        ready: false,
        dirty: true,
        pending: None,
        undo: Vec::new(),
    });
    sand
}

pub(crate) fn snapshot(world: &World, owner: Entity) -> Option<SavedTodo> {
    let view = world.get::<TodoSand>(owner)?;
    Some(SavedTodo {
        protein: view.protein.clone(),
        show_ids: view.show_ids,
        draft: world
            .get::<EditableText>(view.input)
            .map(|input| input.value().to_string())
            .unwrap_or_default(),
    })
}

pub(crate) fn restore(world: &mut World, owner: Entity, saved: SavedTodo) {
    let Some(mut view) = world.get_mut::<TodoSand>(owner) else {
        return;
    };
    view.protein = saved.protein.clone();
    view.show_ids = saved.show_ids;
    let (query, input) = (view.query, view.input);
    world
        .get_mut::<EditableText>(query)
        .unwrap()
        .editor
        .set_text(&saved.protein);
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&saved.draft);
    ids_label(world, owner);
}

#[derive(Component)]
struct Queue(Entity);
#[derive(Component)]
struct NewTask(Entity);

fn focus(
    mut event: On<Pointer<Press>>,
    lists: Query<&Queue>,
    mut focus: ResMut<bevy::input_focus::InputFocus>,
) {
    if lists.contains(event.entity) && event.button == PointerButton::Primary {
        focus.set(event.entity, bevy::input_focus::FocusCause::Pressed);
        event.propagate(false);
    }
}

fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    lists: Query<&Queue>,
    mut commands: Commands,
) {
    let Ok(queue) = lists.get(event.focused_entity) else {
        return;
    };
    if !event.input.state.is_pressed() {
        return;
    }
    let action = match event.input.key_code {
        KeyCode::KeyJ | KeyCode::ArrowDown => Command::Move(1),
        KeyCode::KeyK | KeyCode::ArrowUp => Command::Move(-1),
        KeyCode::Space | KeyCode::Enter if !event.input.repeat => Command::CompleteActive,
        _ => return,
    };
    event.propagate(false);
    let owner = queue.0;
    commands.queue(move |world: &mut World| action.apply(world, owner));
}

fn add_key(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    inputs: Query<(&NewTask, &EditableText)>,
    mut commands: Commands,
) {
    let Ok((input, text)) = inputs.get(event.focused_entity) else {
        return;
    };
    if text.is_composing() {
        return;
    }
    if event.input.key_code == KeyCode::Enter
        && event.input.state.is_pressed()
        && !event.input.repeat
    {
        event.propagate(false);
        let owner = input.0;
        commands.queue(move |world: &mut World| Command::Add.apply(world, owner));
    }
}

#[derive(Clone)]
enum Command {
    Query,
    Refresh,
    Ids,
    Add,
    Complete(String),
    CompleteActive,
    Undo,
    Move(isize),
    Select(String),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<TodoSand>(owner) else {
            return;
        };
        let status = view.status;
        if view.pending.is_some() && !matches!(self, Self::Ids | Self::Move(_) | Self::Select(_)) {
            panel::status(world, status, "Wait for the current task change to finish");
            return;
        }
        let result = match self {
            Self::Ids => {
                world.get_mut::<TodoSand>(owner).unwrap().show_ids ^= true;
                world.get_mut::<TodoSand>(owner).unwrap().dirty = true;
                ids_label(world, owner);
                Ok(())
            }
            Self::Move(delta) => {
                let mut view = world.get_mut::<TodoSand>(owner).unwrap();
                view.active = view
                    .active
                    .saturating_add_signed(*delta)
                    .min(view.rows.len().saturating_sub(1));
                view.dirty = true;
                Ok(())
            }
            Self::Select(uid) => {
                let index = view
                    .rows
                    .iter()
                    .position(|row| row["uid"].as_str() == Some(uid));
                if let Some(index) = index {
                    let mut view = world.get_mut::<TodoSand>(owner).unwrap();
                    view.active = index;
                    view.dirty = true;
                }
                Ok(())
            }
            Self::Query | Self::Refresh => {
                let name = if matches!(self, Self::Query) {
                    panel::value(world, view.query)
                } else {
                    Ok(view.protein.clone())
                };
                name.and_then(|name| {
                    if name.len() > 1024 {
                        return Err("Protein reference is too long".into());
                    }
                    let mut view = world.get_mut::<TodoSand>(owner).unwrap();
                    view.protein = name.trim().into();
                    view.subscription = nucleus::new_uid("todo");
                    view.requested = false;
                    view.ready = false;
                    view.rows.clear();
                    view.active = 0;
                    view.dirty = true;
                    panel::status(world, status, "Loading tasks…");
                    Ok(())
                })
            }
            Self::Add => panel::value(world, view.input).and_then(|draft| {
                let head = draft.trim().to_owned();
                if head.is_empty() || head.chars().count() > 4096 {
                    return Err("Enter a task of 1–4096 characters".into());
                }
                submit(
                    world,
                    owner,
                    engine::actions::Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head,
                        body: String::new(),
                        quantity: -1.0,
                    },
                    Mutation::Add(draft),
                )
            }),
            Self::Complete(uid) => complete(world, owner, uid),
            Self::CompleteActive => {
                let uid = view
                    .rows
                    .get(view.active)
                    .and_then(|row| row["uid"].as_str())
                    .map(str::to_string);
                uid.ok_or("Select a task first".into())
                    .and_then(|uid| complete(world, owner, &uid))
            }
            Self::Undo => {
                let undo = view.undo.last().cloned();
                undo.ok_or("There is no completion to undo".into())
                    .and_then(|(target, amount)| {
                        submit(
                            world,
                            owner,
                            engine::actions::Action::SetQuantityExact { target, amount },
                            Mutation::Undo,
                        )
                    })
            }
        };
        if let Err(error) = result {
            panel::status(world, status, error);
        }
    }
}

fn quantity(row: &Value) -> Result<String, String> {
    let value = match &row["quantity"] {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => return Err("The selected Protein must include quantity to complete tasks".into()),
    };
    nucleus::DecimalValue::parse_inferred(&value).map_err(|_| "Task quantity is invalid")?;
    Ok(value)
}

fn complete(world: &mut World, owner: Entity, uid: &str) -> Result<(), String> {
    let view = world.get::<TodoSand>(owner).ok_or("Todo is closed")?;
    if !view.ready {
        return Err("Wait for the task queue to load".into());
    }
    let row = view
        .rows
        .iter()
        .find(|row| row["uid"].as_str() == Some(uid))
        .ok_or("The task is no longer in this queue")?;
    let amount = quantity(row)?;
    if nucleus::DecimalValue::parse_inferred(&amount).is_ok_and(|value| value.to_string() == "0") {
        return Err("This task is already complete".into());
    }
    submit(
        world,
        owner,
        engine::actions::Action::SetQuantityExact {
            target: uid.into(),
            amount: "0".into(),
        },
        Mutation::Complete(uid.into(), amount),
    )
}

fn submit(
    world: &mut World,
    owner: Entity,
    action: engine::actions::Action,
    mutation: Mutation,
) -> Result<(), String> {
    let id = nucleus::new_uid("todo-action");
    panel::send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    let mut view = world.get_mut::<TodoSand>(owner).unwrap();
    view.pending = Some((id, mutation));
    let status = view.status;
    panel::status(world, status, "Saving task…");
    Ok(())
}

fn ids_label(world: &mut World, owner: Entity) {
    let view = world.get::<TodoSand>(owner).unwrap();
    let text = if view.show_ids {
        "Show ids: on"
    } else {
        "Show ids: off"
    };
    if let Some(label) = world
        .get::<Children>(view.ids_button)
        .and_then(|children| children.first())
        .copied()
    {
        panel::status(world, label, text);
    }
}

fn render(world: &mut World, owner: Entity) {
    let view = world.get::<TodoSand>(owner).unwrap();
    let (list, show_ids, active, ready) = (view.list, view.show_ids, view.active, view.ready);
    let start = active / PAGE * PAGE;
    let rows: Vec<_> = view.rows.iter().skip(start).take(PAGE).cloned().collect();
    let total = view.rows.len();
    panel::clear(world, list);
    if rows.is_empty() {
        crate::edit_mode::label(
            world,
            list,
            if ready {
                "Nothing needs doing."
            } else {
                "Waiting for tasks…"
            },
            16.0,
        );
    }
    for (index, row) in rows.into_iter().enumerate() {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        let entry = panel::column(world, list);
        let node = world.get_mut::<Node>(entry).unwrap().into_inner();
        node.padding = UiRect::all(px(6));
        if start + index == active {
            world
                .entity_mut(entry)
                .insert(crate::token_style::background(
                    crate::tokens::Token::Surface,
                ));
        }
        let top = panel::row(world, entry);
        panel::button(world, top, owner, "Done", Command::Complete(uid.into()));
        let title = row["head"]
            .as_str()
            .filter(|text| !text.is_empty())
            .or_else(|| row["slug"].as_str())
            .unwrap_or(uid);
        panel::button(world, top, owner, title, Command::Select(uid.into()));
        if let Some(body) = row["body"].as_str().filter(|body| !body.is_empty()) {
            crate::edit_mode::label(
                world,
                entry,
                &body.chars().take(1000).collect::<String>(),
                13.0,
            );
        }
        let meta = if show_ids {
            uid.into()
        } else {
            format!(
                "quantity {}",
                quantity(&row).unwrap_or_else(|_| "unavailable".into())
            )
        };
        crate::edit_mode::label(world, entry, &meta, 12.0);
    }
    if total > PAGE {
        crate::edit_mode::label(
            world,
            list,
            &format!("{}–{} of {total}", start + 1, (start + PAGE).min(total)),
            12.0,
        );
    }
    world.get_mut::<TodoSand>(owner).unwrap().dirty = false;
}

fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
    let view = world.get::<TodoSand>(owner).unwrap();
    let status = view.status;
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if id == &view.subscription =>
        {
            let selected = view
                .rows
                .get(view.active)
                .and_then(|row| row["uid"].as_str())
                .map(str::to_owned);
            let rows: Vec<_> = rows
                .iter()
                .filter(|row| {
                    row["uid"]
                        .as_str()
                        .is_some_and(|uid| nucleus::valid_uid(uid, "r"))
                })
                .cloned()
                .collect();
            let invalid = rows.is_empty()
                && !matches!(message, ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } if rows.is_empty());
            let mut view = world.get_mut::<TodoSand>(owner).unwrap();
            view.active = selected
                .and_then(|selected| {
                    rows.iter()
                        .position(|row| row["uid"].as_str() == Some(&selected))
                })
                .unwrap_or(view.active)
                .min(rows.len().saturating_sub(1));
            view.rows = rows;
            view.ready = true;
            view.dirty = true;
            panel::status(
                world,
                status,
                if invalid {
                    "Choose a Protein that returns Records with uid, head, body and quantity"
                } else {
                    "Live · j/k or arrows select; Enter or Space completes"
                },
            );
        }
        ServerMessage::ActionOk { id, warnings, .. }
            if view
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id) =>
        {
            let mut view = world.get_mut::<TodoSand>(owner).unwrap();
            let (_, mutation) = view.pending.take().unwrap();
            match mutation {
                Mutation::Add(draft) => {
                    let input = view.input;
                    if let Some(mut text) = world.get_mut::<EditableText>(input)
                        && text.value().to_string() == draft
                    {
                        text.editor.set_text("");
                    }
                }
                Mutation::Complete(uid, amount) => {
                    view.undo.push((uid, amount));
                    if view.undo.len() > 100 {
                        view.undo.remove(0);
                    }
                }
                Mutation::Undo => {
                    view.undo.pop();
                }
            }
            panel::status(
                world,
                status,
                if warnings.is_empty() {
                    "Saved".into()
                } else {
                    format!("Saved · {}", warnings.join(" · "))
                },
            );
        }
        ServerMessage::Error { id, message, .. }
            if id == &view.subscription
                || id == crate::cell_bridge::CONNECTION
                || view
                    .pending
                    .as_ref()
                    .is_some_and(|(pending, _)| pending == id) =>
        {
            let mut view = world.get_mut::<TodoSand>(owner).unwrap();
            if id == &view.subscription || id == crate::cell_bridge::CONNECTION {
                view.ready = false;
            }
            view.pending = None;
            panel::status(world, status, message);
        }
        _ => {}
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<TodoSand>>()
        .iter(world)
        .collect();
    let active: HashSet<_> = owners
        .iter()
        .map(|owner| world.get::<TodoSand>(*owner).unwrap().subscription.clone())
        .collect();
    let stale: Vec<_> = world
        .resource::<Subscriptions>()
        .0
        .difference(&active)
        .cloned()
        .collect();
    for id in stale {
        if panel::send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Subscriptions>().0.remove(&id);
        }
    }
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for owner in owners {
        for message in &messages {
            receive(world, owner, message);
        }
        let view = world.get::<TodoSand>(owner).unwrap();
        if !view.requested && !crate::laboratory::active(world) {
            let id = view.subscription.clone();
            let message = if view.protein.is_empty() {
                ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: protein::focus_queue("before"),
                }
            } else {
                ClientMessage::SubscribeSaved {
                    id: id.clone(),
                    name: view.protein.clone(),
                }
            };
            if panel::send(world, message).is_ok() {
                world.resource_mut::<Subscriptions>().0.insert(id);
                world.get_mut::<TodoSand>(owner).unwrap().requested = true;
            }
        }
        if world.get::<TodoSand>(owner).unwrap().dirty {
            render(world, owner);
        }
    }
}
