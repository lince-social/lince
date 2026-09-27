use crate::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    cell_bridge::{CellBridge, CellMessage, RECORDS, ReceiveCell},
    edit_mode::{EditAction, label},
    theme::Typography,
};
use bevy::{
    a11y::AccessibilityNode,
    input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup},
    prelude::*,
    text::EditableText,
};
use cell::{ClientMessage, ServerMessage};
use std::collections::BTreeMap;

pub(crate) const SIZE: Vec2 = Vec2::new(520.0, 40.0);

const HELP: &str = "Operation cheat sheet\n@slug: set a Record’s quantity to zero.\n/: find commands.\nUp / Down: choose a suggestion.\nTab or click: complete the input.\nEnter: send the operation.\nEscape: leave Operation.\n/help: open the full cheat sheet.";

const COMMANDS: &[(&str, &str, EditAction)] = &[
    ("/edit", "Open edit mode", EditAction::Open),
    ("/help", "Open the cheat sheet", EditAction::Shortcuts),
    ("/store", "Open the Sand store", EditAction::Store),
    ("/workspaces", "Open workspaces", EditAction::Workspaces),
];

#[derive(Resource, Default)]
struct Catalog {
    records: BTreeMap<String, (String, String)>,
    revision: u64,
    error: Option<String>,
    ready: bool,
    next_request: u64,
}

#[derive(Component)]
pub struct OperationSand {
    root: Entity,
    input: Entity,
    feedback: Entity,
    suggestions: Entity,
    status: Entity,
    query: String,
    revision: u64,
    items: Vec<(String, String)>,
    selected: usize,
    pending: Option<(String, String)>,
}

#[derive(Component)]
struct OperationInput(Entity);

#[derive(Component)]
struct Popup {
    panel: Entity,
    previous: Option<Entity>,
}

pub struct OperationPlugin;

impl Plugin for OperationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Catalog>()
            .add_message::<CellMessage>()
            .add_systems(Update, receive.after(ReceiveCell))
            .add_systems(
                PostUpdate,
                (refresh, feedback_visibility)
                    .chain()
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

#[derive(Clone, Copy)]
pub struct OpenOperation;

impl Action for OpenOperation {
    fn apply(&self, world: &mut World, root: Entity) {
        if let Some(popup) = world.get::<Popup>(root) {
            let input = world.get::<OperationSand>(popup.panel).unwrap().input;
            world
                .resource_mut::<InputFocus>()
                .set(input, FocusCause::Navigated);
            return;
        }
        let previous = world.resource::<InputFocus>().get();
        let overlay = world
            .spawn((
                crate::inspection::InspectionExcluded,
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Start,
                    padding: UiRect::top(percent(10)),
                    ..default()
                },
                GlobalZIndex(60),
                TabGroup::modal(),
                ChildOf(root),
            ))
            .id();
        let panel = world
            .spawn((
                Node {
                    width: px(520),
                    height: px(SIZE.y),
                    max_width: percent(94),
                    ..default()
                },
                ChildOf(overlay),
            ))
            .id();
        let input = populate(world, root, panel);
        world.entity_mut(root).insert(Popup { panel, previous });
        world
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Navigated);
    }
}

#[derive(Clone, Copy)]
enum OperationAction {
    Submit,
    Complete,
    Choose(usize),
    Move(bool),
    Close,
}

impl Action for OperationAction {
    fn apply(&self, world: &mut World, target: Entity) {
        let sand = world
            .get::<OperationInput>(target)
            .map_or(target, |input| input.0);
        let Some(state) = world.get::<OperationSand>(sand) else {
            return;
        };
        let input = state.input;
        if world
            .get::<EditableText>(input)
            .is_some_and(|text| text.is_composing() || pending_text(text))
        {
            return;
        }
        match *self {
            Self::Close => close(world, sand),
            Self::Submit => submit(world, sand),
            Self::Move(next) => {
                let mut state = world.get_mut::<OperationSand>(sand).unwrap();
                if !state.items.is_empty() {
                    let count = state.items.len();
                    state.selected = (state.selected + if next { 1 } else { count - 1 }) % count;
                }
                render_suggestions(world, sand);
                world
                    .resource_mut::<InputFocus>()
                    .set(input, FocusCause::Navigated);
            }
            Self::Complete | Self::Choose(_) => {
                let index = if let Self::Choose(index) = *self {
                    index
                } else {
                    state.selected
                };
                let Some((value, _)) = state.items.get(index).cloned() else {
                    return;
                };
                let mut text = world.get_mut::<EditableText>(input).unwrap();
                text.editor.set_text(&value);
                text.queue_edit(bevy::text::TextEdit::TextEnd(false));
                world
                    .resource_mut::<InputFocus>()
                    .set(input, FocusCause::Pressed);
            }
        }
    }
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    world.entity_mut(sand).insert((
        crate::sand::Square,
        crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
        crate::token_style::background(crate::tokens::Token::Surface),
        crate::token_style::border(crate::tokens::Token::Accent),
    ));
    if let Some(mut node) = world.get_mut::<Node>(sand) {
        node.flex_direction = FlexDirection::Column;
        node.padding = UiRect::ZERO;
        node.row_gap = px(0);
        node.border = UiRect::ZERO;
        node.overflow = Overflow::visible();
    }
    let bundle = crate::sand::text_editor("", world.resource::<Typography>(), 0);
    let input = world
        .spawn((
            bundle,
            OperationInput(sand),
            AccessibilityNode::default(),
            crate::icons::Tooltip(HELP.into()),
            ChildOf(sand),
        ))
        .id();
    {
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
    }
    {
        let mut node = world.get_mut::<Node>(input).unwrap();
        node.height = percent(100);
        node.min_height = px(SIZE.y);
        node.flex_shrink = 0.0;
    }
    world
        .get_mut::<AccessibilityNode>(input)
        .unwrap()
        .set_label("Operation: enter @slug or /command");
    world.entity_mut(input).insert(KeyBindings(vec![
        KeyBinding::new(
            KeyCode::Enter,
            Modifiers::NONE,
            crate::actions![OperationAction::Submit],
        ),
        KeyBinding::new(
            KeyCode::Tab,
            Modifiers::NONE,
            crate::actions![OperationAction::Complete],
        ),
        KeyBinding::new(
            KeyCode::ArrowUp,
            Modifiers::NONE,
            crate::actions![OperationAction::Move(false)],
        ),
        KeyBinding::new(
            KeyCode::ArrowDown,
            Modifiers::NONE,
            crate::actions![OperationAction::Move(true)],
        ),
        KeyBinding::new(
            KeyCode::Escape,
            Modifiers::NONE,
            crate::actions![OperationAction::Close],
        ),
    ]));
    let feedback = world
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                top: percent(100),
                width: percent(100),
                max_height: px(280),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(4),
                border: UiRect::all(px(1)),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
            ZIndex(1),
            ChildOf(sand),
        ))
        .id();
    crate::scroll_sand::attach(world, feedback);
    let suggestions = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(feedback),
        ))
        .id();
    let status = label(world, feedback, "", 14.0);
    world.get_mut::<Node>(status).unwrap().display = Display::None;
    world.entity_mut(sand).insert(OperationSand {
        root,
        input,
        feedback,
        suggestions,
        status,
        query: String::new(),
        revision: u64::MAX,
        items: Vec::new(),
        selected: 0,
        pending: None,
    });
    world
        .entity_mut(sand)
        .insert(KeyBindings(vec![KeyBinding::new(
            KeyCode::Escape,
            Modifiers::NONE,
            crate::actions![OperationAction::Close],
        )]));
    input
}

fn action_button(
    world: &mut World,
    parent: Entity,
    sand: Entity,
    action: OperationAction,
    title: &str,
) {
    let button = world
        .spawn((
            crate::sand::button(0),
            ActionButton::new(sand, crate::actions![action]),
            Node {
                padding: UiRect::all(px(6)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<AccessibilityNode>(button)
        .unwrap()
        .set_label(title);
    label(world, button, title, 14.0);
}

fn close(world: &mut World, sand: Entity) {
    let root = world.get::<OperationSand>(sand).unwrap().root;
    if world
        .get::<Popup>(root)
        .is_some_and(|popup| popup.panel == sand)
    {
        let popup = world.entity_mut(root).take::<Popup>().unwrap();
        let overlay = world.get::<ChildOf>(sand).unwrap().parent();
        world.despawn(overlay);
        let previous = popup
            .previous
            .filter(|entity| world.get_entity(*entity).is_ok());
        let mut focus = world.resource_mut::<InputFocus>();
        if let Some(previous) = previous {
            focus.set(previous, FocusCause::Navigated);
        } else {
            focus.clear();
        }
    } else {
        world.resource_mut::<InputFocus>().clear();
    }
}

fn suggestions(catalog: &Catalog, query: &str) -> Vec<(String, String)> {
    if let Some(prefix) = query.strip_prefix('@') {
        catalog
            .records
            .range(prefix.to_owned()..)
            .take_while(|(slug, _)| slug.starts_with(prefix))
            .take(6)
            .map(|(slug, (_, head))| (format!("@{slug}"), head.clone()))
            .collect()
    } else if query.starts_with('/') {
        COMMANDS
            .iter()
            .filter(|(command, _, _)| command.starts_with(query))
            .map(|(command, title, _)| ((*command).into(), (*title).into()))
            .collect()
    } else {
        Vec::new()
    }
}

fn pending_text(text: &EditableText) -> bool {
    use bevy::text::TextEdit;
    text.pending_paste.is_some()
        || text.pending_edits.iter().any(|edit| match edit {
            TextEdit::ImeSetCompose { value, .. } => !value.is_empty(),
            TextEdit::Cut
            | TextEdit::Paste
            | TextEdit::Insert(_)
            | TextEdit::Backspace
            | TextEdit::BackspaceWord
            | TextEdit::Delete
            | TextEdit::DeleteWord
            | TextEdit::ImeCommit { .. } => true,
            _ => false,
        })
}

fn refresh(world: &mut World) {
    let edits: Vec<_> = world
        .query::<(Entity, &OperationSand)>()
        .iter(world)
        .filter_map(|(sand, state)| {
            let text = world.get::<EditableText>(state.input)?;
            if text.is_composing() || pending_text(text) {
                return None;
            }
            let query = text.value().to_string();
            let catalog = world.resource::<Catalog>();
            (query != state.query || catalog.revision != state.revision)
                .then(|| (sand, query, catalog.revision))
        })
        .collect();
    for (sand, query, revision) in edits {
        let catalog = world.resource::<Catalog>();
        let items = suggestions(catalog, query.trim());
        let message = if query.trim().starts_with('@') && !catalog.ready {
            catalog
                .error
                .as_deref()
                .unwrap_or("Loading Records…")
                .to_owned()
        } else if items.is_empty() && !query.trim().is_empty() {
            "No matches. Enter an exact @slug or /command.".into()
        } else {
            String::new()
        };
        let mut state = world.get_mut::<OperationSand>(sand).unwrap();
        let changed = state.query != query;
        state.query = query;
        state.revision = revision;
        state.items = items;
        state.selected = 0;
        let status = state.status;
        let idle = world.get::<Text>(status).is_some_and(|text| {
            text.0.is_empty() || text.0 == "Loading Records…" || text.0.starts_with("No matches.")
        });
        if world.get::<OperationSand>(sand).unwrap().pending.is_none()
            && (changed || idle && !message.is_empty())
        {
            self::status(world, sand, message);
        }
        render_suggestions(world, sand);
    }
}

fn render_suggestions(world: &mut World, sand: Entity) {
    let state = world.get::<OperationSand>(sand).unwrap();
    let parent = state.suggestions;
    let items = state.items.clone();
    let selected = state.selected;
    world.entity_mut(parent).despawn_children();
    world.get_mut::<Node>(parent).unwrap().display = if items.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    for (index, (value, title)) in items.iter().enumerate() {
        let marker = if index == selected { "› " } else { "" };
        action_button(
            world,
            parent,
            sand,
            OperationAction::Choose(index),
            &format!("{marker}{value} — {title}"),
        );
    }
}

fn status(world: &mut World, sand: Entity, message: impl Into<String>) {
    let entity = world.get::<OperationSand>(sand).unwrap().status;
    let message = message.into();
    world.get_mut::<Node>(entity).unwrap().display = if message.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    world.get_mut::<Text>(entity).unwrap().0 = message;
}

fn feedback_visibility(world: &mut World) {
    let focus = world.resource::<InputFocus>().get();
    let panels: Vec<_> = world
        .query::<(Entity, &OperationSand)>()
        .iter(world)
        .map(|(sand, state)| {
            let mut current = focus;
            let mut focused = false;
            while let Some(entity) = current {
                if entity == sand {
                    focused = true;
                    break;
                }
                current = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            let content = !state.items.is_empty()
                || world
                    .get::<Text>(state.status)
                    .is_some_and(|text| !text.0.is_empty());
            (
                state.feedback,
                if focused && content {
                    Display::Flex
                } else {
                    Display::None
                },
            )
        })
        .collect();
    for (panel, display) in panels {
        let mut node = world.get_mut::<Node>(panel).unwrap();
        if node.display != display {
            node.display = display;
        }
    }
}

fn submit(world: &mut World, sand: Entity) {
    let state = world.get::<OperationSand>(sand).unwrap();
    if state.pending.is_some() {
        return;
    }
    let query = world
        .get::<EditableText>(state.input)
        .unwrap()
        .value()
        .to_string();
    let value = query.trim();
    if let Some((_, _, action)) = COMMANDS.iter().find(|(command, _, _)| *command == value) {
        let root = state.root;
        close(world, sand);
        EditAction::Open.apply(world, root);
        action.apply(world, root);
        return;
    }
    let catalog = world.resource::<Catalog>();
    let record = value
        .strip_prefix('@')
        .and_then(|slug| catalog.records.get(slug));
    let Some((uid, _)) = record.filter(|_| catalog.ready) else {
        status(
            world,
            sand,
            "Enter an exact @slug or /command. Tab completes a suggestion.",
        );
        return;
    };
    let uid = uid.clone();
    let mut catalog = world.resource_mut::<Catalog>();
    catalog.next_request += 1;
    let id = format!("operation-{}", catalog.next_request);
    let request = ClientMessage::Act {
        id: id.clone(),
        action: engine::actions::Action::SetQuantityExact {
            target: uid,
            amount: "0".into(),
        },
    };
    let result = world
        .get_non_send::<CellBridge>()
        .ok_or("Not connected. Your input is still here.")
        .and_then(|bridge| {
            bridge
                .outgoing
                .try_send(request)
                .map_err(|error| match error {
                    tokio::sync::mpsc::error::TrySendError::Full(_) => {
                        "The connection is busy. Try again."
                    }
                    tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                        "Connection closed. Your input is still here."
                    }
                })
        });
    match result {
        Ok(()) => {
            world.get_mut::<OperationSand>(sand).unwrap().pending = Some((id, query));
            status(world, sand, "Setting quantity to zero…");
        }
        Err(error) => status(world, sand, error),
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == RECORDS =>
            {
                let mut catalog = world.resource_mut::<Catalog>();
                catalog.records = rows
                    .iter()
                    .filter_map(|row| {
                        let slug = row["slug"].as_str()?.trim_start_matches('@');
                        let uid = row["uid"].as_str()?;
                        (!slug.is_empty() && !uid.is_empty()).then(|| {
                            (
                                slug.into(),
                                (uid.into(), row["head"].as_str().unwrap_or(slug).into()),
                            )
                        })
                    })
                    .collect();
                catalog.ready = true;
                catalog.error = None;
                catalog.revision += 1;
            }
            ServerMessage::ActionOk { id, warnings, .. } => acknowledge(world, &id, Ok(warnings)),
            ServerMessage::Error { id, message, .. } => {
                if id == RECORDS || id == crate::cell_bridge::CONNECTION {
                    let mut catalog = world.resource_mut::<Catalog>();
                    catalog.ready = false;
                    catalog.records.clear();
                    catalog.error = Some(message.clone());
                    catalog.revision += 1;
                }
                acknowledge(world, &id, Err(message));
            }
            _ => {}
        }
    }
}

fn acknowledge(world: &mut World, id: &str, result: Result<Vec<String>, String>) {
    let pending: Vec<_> = world
        .query::<(Entity, &OperationSand)>()
        .iter(world)
        .filter_map(|(sand, state)| {
            let (request, query) = state.pending.as_ref()?;
            (request == id || id == crate::cell_bridge::CONNECTION).then(|| (sand, query.clone()))
        })
        .collect();
    for (sand, query) in pending {
        world.get_mut::<OperationSand>(sand).unwrap().pending = None;
        let message = match &result {
            Ok(warnings) => {
                let suffix = if warnings.is_empty() {
                    String::new()
                } else {
                    format!(" {}", warnings.join("; "))
                };
                format!("{}: quantity set to zero.{suffix}", query.trim())
            }
            Err(error) => format!("Change not confirmed: {error}"),
        };
        status(world, sand, message);
    }
}

#[cfg(test)]
mod tests;
