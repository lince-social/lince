mod controls;
#[cfg(test)]
mod integration;
pub(crate) mod mentions;
pub(crate) mod message_view;
pub(crate) mod tests;
pub(crate) mod transcript;

use crate::{
    actions::Action,
    protein_area::{RecordBinding, Source},
};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const PAGE_SIZE: usize = 50;

#[derive(Component)]
pub struct ThreadCastle {
    binding: RecordBinding,
    tabs: Entity,
    active: Option<String>,
    pages: HashMap<String, Entity>,
    prefer_first: bool,
    status: Entity,
    creating: bool,
    select_new: Option<String>,
}

#[derive(Component)]
struct Page {
    castle: Entity,
    uid: String,
    tab: Entity,
    viewport: Entity,
    list: Entity,
    older: Entity,
    status: Entity,
    messages: HashMap<String, Entity>,
    order: Vec<String>,
    limit: usize,
    requested: Option<usize>,
    has_more: bool,
}

#[derive(Component)]
struct Viewport {
    page: Entity,
    height: f32,
    initialized: bool,
    prepend: bool,
}

#[derive(Component)]
struct Message {
    binding: RecordBinding,
    uid: String,
    status: Entity,
    confirmation: Entity,
    pending: bool,
    identity: Entity,
    author_name: Entity,
    input: Entity,
    preview: Entity,
}

#[derive(Component)]
struct ThreadForm {
    binding: RecordBinding,
    thread: Option<String>,
    input: Entity,
    status: Entity,
    pending: Option<String>,
}

pub struct ThreadCastlePlugin;

impl Plugin for ThreadCastlePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "native-media")]
        app.add_plugins(crate::communication::calls::CallsPlugin);
        app.add_plugins(crate::record_creation::RecordCreationPlugin);
        app.add_systems(
            Update,
            transcript::receive.after(crate::cell_bridge::ReceiveCell),
        );
        app.add_observer(scroll)
            .add_systems(
                PostUpdate,
                message_view::sync_edits
                    .after(crate::record_binding::SyncBindings)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                composer_keys.before(bevy::text::EditableTextSystems),
            )
            .add_systems(
                PostUpdate,
                mentions::sync
                    .after(bevy::text::EditableTextSystems)
                    .before(submit_keys)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                mentions::paint.after(bevy::ui::UiSystems::PostLayout),
            )
            .add_systems(
                PostUpdate,
                (
                    submit_keys,
                    select_focused_tab,
                    composer_size,
                    message_view::statuses,
                )
                    .after(bevy::text::EditableTextSystems)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                Update,
                crate::full_record::receive.after(crate::cell_bridge::ReceiveCell),
            )
            .add_systems(
                PostUpdate,
                anchor_scroll.after(bevy::ui::UiSystems::PostLayout),
            )
            .add_systems(
                Update,
                load_errors.after(crate::protein_area::UpdateProteinAreas),
            );
    }
}

pub fn populate(world: &mut World, parent: Entity, binding: RecordBinding, data: &Value) {
    if let Some(mut node) = world.get_mut::<Node>(parent) {
        node.border = UiRect::all(px(1));
        node.padding = UiRect::all(px(8));
    }
    world
        .entity_mut(parent)
        .insert(crate::token_style::border(crate::tokens::Token::Accent));
    crate::fiote::session::populate(world, parent, binding.clone());
    let header = world
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::edit_mode::label(world, header, "Threads", 20.0);
    let add = control(world, header, parent, "+", controls::Add);
    world.entity_mut(add).insert(Node {
        width: px(28),
        height: px(28),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    });
    let status = message_view::status(world, parent);
    let tabs = world
        .spawn((
            Node {
                width: percent(100),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let prefer_first = world
        .get::<crate::area::InfluenceArea>(binding.area)
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.fiote);
    world.entity_mut(parent).insert(ThreadCastle {
        binding,
        tabs,
        active: None,
        pages: HashMap::new(),
        prefer_first,
        status,
        creating: false,
        select_new: None,
    });
    refresh(world, parent, data);
}

pub fn open(world: &mut World, root: Entity, reference: &str, source: Source) -> Option<Entity> {
    crate::full_record::open(world, root, reference, source)
}

fn control(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    action: impl Action,
) -> Entity {
    crate::description::button(world, parent, owner, title, action)
}

fn form(world: &mut World, parent: Entity, binding: RecordBinding, thread: Option<String>) {
    let container = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: percent(100),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let composer = world
        .spawn((
            Node {
                width: percent(100),
                align_items: AlignItems::End,
                column_gap: px(4),
                border: UiRect::top(px(1)),
                ..default()
            },
            crate::token_style::border(crate::tokens::Token::Accent),
            ChildOf(container),
        ))
        .id();
    let input = world
        .spawn((
            crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(composer),
            crate::sand::Borderless,
            crate::icons::Tooltip("Message · Enter to send · Shift+Enter for a new line · Type @ to link a Record, person or agent".into()),
        ))
        .insert(Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            min_height: px(32),
            padding: UiRect::all(px(6)),
            max_height: px(160),
            ..default()
        })
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(65_536);
    world.get_mut::<EditableText>(input).unwrap().visible_lines = Some(1.0);
    world.get_mut::<TextFont>(input).unwrap().font_size = 16.0.into();
    mentions::attach(world, input);
    let send = control(world, composer, container, "↑", Send);
    world.entity_mut(send).insert((
        crate::sand::Borderless,
        crate::icons::Tooltip("Send message".into()),
    ));
    if let Some(mut accessibility) = world.get_mut::<AccessibilityNode>(send) {
        accessibility.set_label("Send message");
    }
    let status = message_view::status(world, container);

    if let Some(thread) = &thread {
        crate::fiote::session::thread_controls(world, container, thread, &binding);
    }
    let mut children: Vec<_> = world
        .get::<Children>(container)
        .unwrap()
        .iter()
        .filter(|child| *child != composer)
        .collect();
    children.push(composer);
    world.entity_mut(container).replace_children(&children);
    world.entity_mut(container).insert(ThreadForm {
        binding,
        thread,
        input,
        status,
        pending: None,
    });
}

fn page(
    world: &mut World,
    castle: Entity,
    tabs: Entity,
    binding: &RecordBinding,
    uid: &str,
    title: &str,
) -> Entity {
    let entity = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::all(px(0)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(castle),
        ))
        .id();
    let tab = world
        .spawn((
            AccessibilityNode::from(accesskit::Node::new(accesskit::Role::Tab)),
            Node {
                align_items: AlignItems::Center,
                height: px(32),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(tabs),
        ))
        .id();
    let editor = world
        .spawn((
            crate::sand::text_editor(title, world.resource::<crate::theme::Typography>(), 0),
            ChildOf(tab),
            TabName {
                castle,
                thread: uid.into(),
            },
        ))
        .insert(Node {
            min_width: px(80),
            max_width: px(200),
            height: px(32),
            padding: UiRect::all(px(4)),
            ..default()
        })
        .id();
    world.get_mut::<TextFont>(editor).unwrap().font_size = 14.0.into();
    let delete = control(world, tab, entity, "×", controls::AskDelete);
    world.entity_mut(delete).insert(Node {
        width: px(28),
        height: px(32),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    });
    let status = message_view::status(world, entity);
    crate::record_binding::attach(
        world,
        editor,
        RecordBinding {
            uid: uid.into(),
            ..binding.clone()
        },
        "head",
        Some(status),
    );
    {
        let mut title = world.get_mut::<EditableText>(editor).unwrap();
        title.allow_newlines = false;
        title.visible_lines = Some(1.0);
        title.max_characters = Some(256);
    }
    let status = message_view::status(world, entity);
    #[cfg(feature = "native-media")]
    crate::communication::calls::populate(world, entity, binding, uid);
    let older = control(world, entity, entity, "Older messages", LoadOlder);
    let viewport = world
        .spawn((
            Node {
                width: percent(100),
                max_height: px(360),
                min_height: px(80),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            ScrollPosition::default(),
            Viewport {
                page: entity,
                height: 0.0,
                initialized: false,
                prepend: false,
            },
            ChildOf(entity),
        ))
        .id();
    let list = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(viewport),
        ))
        .id();
    form(world, entity, binding.clone(), Some(uid.into()));
    world.entity_mut(entity).insert(Page {
        castle,
        uid: uid.into(),
        tab,
        viewport,
        list,
        older,
        status,
        messages: HashMap::new(),
        order: Vec::new(),
        limit: PAGE_SIZE,
        requested: None,
        has_more: false,
    });
    entity
}

pub fn refresh(world: &mut World, parent: Entity, data: &Value) -> bool {
    let Some(mut castle) = world.entity_mut(parent).take::<ThreadCastle>() else {
        return false;
    };
    let mut selected_new = false;
    let mut retained = HashSet::new();
    for thread in data["threads"].as_array().into_iter().flatten() {
        let Some(uid) = thread["uid"].as_str() else {
            continue;
        };
        retained.insert(uid.to_owned());
        let title = thread["head"].as_str().unwrap_or("Thread");
        let entity = *castle
            .pages
            .entry(uid.into())
            .or_insert_with(|| page(world, parent, castle.tabs, &castle.binding, uid, title));
        if castle.select_new.as_deref() == Some(uid) {
            selected_new = true;
            castle.active = Some(uid.into());
            castle.select_new = None;
        }
        if castle.active.is_none() {
            castle.active = Some(uid.into());
        }
        if castle.prefer_first && title == "Thread 1" {
            castle.active = Some(uid.into());
            castle.prefer_first = false;
        }
        let mut page = world.entity_mut(entity).take::<Page>().unwrap();
        #[cfg(feature = "native-media")]
        crate::communication::calls::summaries(world, entity, thread);
        let limit = thread["messages_limit"]
            .as_u64()
            .unwrap_or(PAGE_SIZE as u64) as usize;
        page.limit = limit;
        if page.requested.is_some_and(|requested| limit >= requested) {
            page.requested = None;
            world.get_mut::<Text>(page.status).unwrap().0.clear();
        }
        page.has_more = thread["messages_has_more"].as_bool().unwrap_or(false);
        world.get_mut::<Node>(page.older).unwrap().display = if page.has_more {
            Display::Flex
        } else {
            Display::None
        };
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        let mut entities = Vec::new();
        let mut previous_author = None;
        for data in thread["messages"].as_array().into_iter().flatten() {
            let Some(uid) = data["uid"].as_str() else {
                continue;
            };
            if !seen.insert(uid.to_owned()) {
                continue;
            }
            order.push(uid.to_owned());
            let entity = *page
                .messages
                .entry(uid.into())
                .or_insert_with(|| message_view::spawn(world, page.list, &castle.binding, data));
            if world.get::<transcript::Transcript>(entity).is_none() {
                transcript::populate(world, entity, &castle.binding, data);
            }
            message_view::refresh(world, entity, data, previous_author);
            previous_author = data["author"].as_str();
            transcript::refresh(world, entity, data);
            entities.push(entity);
        }
        if !page.order.is_empty()
            && order.first() != page.order.first()
            && order.iter().any(|uid| page.order.first() == Some(uid))
        {
            world.get_mut::<Viewport>(page.viewport).unwrap().prepend = true;
        }
        let kept: HashSet<_> = order.iter().collect();
        page.messages.retain(|uid, entity| {
            if kept.contains(uid) {
                true
            } else {
                let _ = world.despawn(*entity);
                false
            }
        });
        if order != page.order {
            world.entity_mut(page.list).replace_children(&entities);
        }
        page.order = order;
        world.entity_mut(entity).insert(page);
    }
    castle.pages.retain(|uid, entity| {
        if retained.contains(uid) {
            true
        } else {
            if let Some(page) = world.get::<Page>(*entity) {
                let tab = page.tab;
                world.despawn(tab);
            }
            world.despawn(*entity);
            false
        }
    });
    if castle
        .active
        .as_ref()
        .is_none_or(|uid| !retained.contains(uid))
    {
        castle.active = data["threads"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|thread| thread["uid"].as_str())
            .find(|uid| retained.contains(*uid))
            .map(str::to_owned);
    }
    world.entity_mut(parent).insert(castle);
    show_active(world, parent);
    if selected_new {
        focus_composer(world, parent);
    }
    true
}

fn show_active(world: &mut World, owner: Entity) {
    let Some(castle) = world.get::<ThreadCastle>(owner) else {
        return;
    };
    let pages: Vec<_> = castle
        .pages
        .iter()
        .map(|(uid, entity)| (*entity, castle.active.as_ref() == Some(uid)))
        .collect();
    for (entity, active) in pages {
        world.get_mut::<Node>(entity).unwrap().display =
            if active { Display::Flex } else { Display::None };
        let tab = world.get::<Page>(entity).unwrap().tab;
        if let Some(mut node) = world.get_mut::<AccessibilityNode>(tab) {
            node.set_selected(active);
        }
        world
            .entity_mut(tab)
            .insert(crate::token_style::background(if active {
                crate::tokens::Token::Accent
            } else {
                crate::tokens::Token::Surface
            }));
    }
}

#[derive(Clone)]
struct Switch(String);
impl Action for Switch {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut castle) = world.get_mut::<ThreadCastle>(owner) else {
            return;
        };
        if !castle.pages.contains_key(&self.0) {
            return;
        }
        castle.active = Some(self.0.clone());
        if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
            focus.clear();
        }
        show_active(world, owner);
    }
}

#[derive(Clone)]
struct LoadOlder;
impl Action for LoadOlder {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(page) = world.get::<Page>(entity) else {
            return;
        };
        if page.requested.is_some() || !page.has_more {
            return;
        }
        let Some(castle) = world.get::<ThreadCastle>(page.castle) else {
            return;
        };
        let (binding, uid, limit, status) = (
            castle.binding.clone(),
            page.uid.clone(),
            page.limit.saturating_add(PAGE_SIZE),
            page.status,
        );
        match crate::protein_area::load_thread_messages(world, &binding, &uid, limit) {
            Ok(()) => {
                world.get_mut::<Page>(entity).unwrap().requested = Some(limit);
                world.get_mut::<Text>(status).unwrap().0 = "Loading older messages…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

fn scroll(
    event: On<Pointer<Scroll>>,
    views: Query<(&Viewport, &ScrollPosition)>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    if event.entity != event.original_event_target() || event.y <= 0.0 {
        return;
    }
    let mut cursor = Some(event.entity);
    while let Some(entity) = cursor {
        if let Ok((view, position)) = views.get(entity) {
            if position.0.y <= 32.0 {
                let page = view.page;
                commands.queue(move |world: &mut World| LoadOlder.apply(world, page));
            }
            return;
        }
        cursor = parents.get(entity).ok().map(ChildOf::parent);
    }
}

fn anchor_scroll(mut views: Query<(&mut Viewport, &mut ScrollPosition, &ComputedNode)>) {
    for (mut view, mut position, node) in &mut views {
        if node.size().y <= 0.0 {
            continue;
        }
        let height = node.content_size.y * node.inverse_scale_factor();
        let viewport = node.size().y * node.inverse_scale_factor();
        let maximum = (height - viewport).max(0.0);
        let bottom = position.0.y >= (view.height - viewport).max(0.0) - 2.0;
        if !view.initialized || (bottom && !view.prepend) {
            position.0.y = maximum;
        } else if view.prepend {
            position.0.y = (position.0.y + height - view.height).clamp(0.0, maximum);
        }
        view.height = height;
        view.initialized = true;
        view.prepend = false;
    }
}

fn load_errors(world: &mut World) {
    let errors: Vec<_> = world
        .query::<(Entity, &Page)>()
        .iter(world)
        .filter(|(_, page)| page.requested.is_some())
        .filter_map(|(entity, page)| {
            let castle = world.get::<ThreadCastle>(page.castle)?;
            let status = crate::protein_area::thread_load_error(world, castle.binding.area)?;
            Some((entity, page.status, status.to_owned()))
        })
        .collect();
    for (entity, label, error) in errors {
        world.get_mut::<Page>(entity).unwrap().requested = None;
        world.get_mut::<Text>(label).unwrap().0 = error;
    }
}

#[derive(Clone)]
struct AskDelete(bool);
impl Action for AskDelete {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(message) = world.get::<Message>(entity) else {
            return;
        };
        if message.pending {
            return;
        }
        let confirmation = message.confirmation;
        world.get_mut::<Node>(confirmation).unwrap().display =
            if self.0 { Display::Flex } else { Display::None };
    }
}

#[derive(Clone)]
struct Delete;
impl Action for Delete {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(message) = world.get::<Message>(entity) else {
            return;
        };
        if message.pending {
            return;
        }
        let (binding, uid, status) = (message.binding.clone(), message.uid.clone(), message.status);
        match crate::protein_area::execute(
            world,
            &binding,
            entity,
            engine::actions::Action::DeleteRecord { target: uid },
        ) {
            Ok(()) => {
                world.get_mut::<Message>(entity).unwrap().pending = true;
                world.get_mut::<Text>(status).unwrap().0 = "Deleting…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

#[derive(Clone)]
struct Send;

#[derive(Component)]
struct TabName {
    castle: Entity,
    thread: String,
}

#[derive(Component)]
struct SubmitRequested;

fn select_focused_tab(world: &mut World) {
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let Some((castle, thread)) = focused
        .and_then(|entity| world.get::<TabName>(entity))
        .map(|tab| (tab.castle, tab.thread.clone()))
    else {
        return;
    };
    if let Some(mut state) = world.get_mut::<ThreadCastle>(castle)
        && state.active.as_ref() != Some(&thread)
    {
        state.active = Some(thread);
        show_active(world, castle);
    }
}

fn composer_size(
    forms: Query<&ThreadForm>,
    mut inputs: Query<&mut EditableText, Changed<EditableText>>,
) {
    for form in &forms {
        if let Ok(mut input) = inputs.get_mut(form.input) {
            let lines = 1 + input
                .value()
                .chars()
                .filter(|ch| *ch == '\n')
                .take(4)
                .count();
            if input.visible_lines != Some(lines as f32) {
                input.visible_lines = Some(lines as f32);
            }
        }
    }
}

fn composer_keys(world: &mut World) {
    if mentions::keys(world) {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if !keys.just_pressed(KeyCode::Enter) && !keys.just_pressed(KeyCode::NumpadEnter) {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]
    .iter()
    .any(|key| keys.pressed(*key))
    {
        return;
    }
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let form = world
        .query::<(Entity, &ThreadForm)>()
        .iter(world)
        .find(|(_, form)| Some(form.input) == focused)
        .map(|(entity, form)| (entity, form.input));
    let Some((form, input)) = form else { return };
    let mut text = world.get_mut::<EditableText>(input).unwrap();
    if text.is_composing()
        || text.pending_edits.iter().any(|edit| {
            matches!(
                edit,
                bevy::text::TextEdit::ImeCommit { .. } | bevy::text::TextEdit::ImeSetCompose { .. }
            )
        })
    {
        return;
    }
    text.pending_edits.retain(|edit| !matches!(edit, bevy::text::TextEdit::Insert(value) if value.as_str() == "\n" || value.as_str() == "\r"));
    if shift {
        text.queue_edit(bevy::text::TextEdit::Insert("\n".into()));
    } else {
        world.entity_mut(form).insert(SubmitRequested);
    }
}

fn submit_keys(world: &mut World) {
    let forms: Vec<_> = world
        .query_filtered::<Entity, With<SubmitRequested>>()
        .iter(world)
        .collect();
    for form in forms {
        world.entity_mut(form).remove::<SubmitRequested>();
        Send.apply(world, form);
    }
}

impl Action for Send {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<ThreadForm>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        let Some(text) = world.get::<EditableText>(form.input) else {
            return;
        };
        if text.is_composing() || crate::record_view::pending_text(text) {
            return;
        }
        let value = text.value().to_string();
        if value.trim().is_empty() && form.thread.is_some() {
            return;
        }
        let binding = form.binding.clone();
        let input = form.input;
        let thread = form.thread.clone();
        let in_thread = thread.is_some();
        if thread.as_deref().is_some_and(|thread| {
            crate::fiote::session::thread_command(world, &binding, thread, &value)
        }) {
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text("");
            return;
        }
        if in_thread && !crate::fiote::session::ready(world, &binding) {
            return;
        }
        let body = mentions::body(world, input, &value);
        let form = world.get::<ThreadForm>(entity).unwrap();
        let action = if let Some(thread) = &form.thread {
            engine::actions::Action::CreateMessage {
                thread: thread.clone(),
                body,
                author: None,
                state: nucleus::MessageState::Finished,
                parent: None,
                references: Vec::new(),
            }
        } else {
            engine::actions::Action::CreateThread {
                target: form.binding.uid.clone(),
                head: value.clone(),
            }
        };
        let binding = form.binding.clone();
        let status = form.status;
        match crate::protein_area::execute(world, &binding, entity, action) {
            Ok(()) => {
                world.get_mut::<ThreadForm>(entity).unwrap().pending = Some(value);
                world.get_mut::<Text>(status).unwrap().0 = "Sending…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

fn focus_composer(world: &mut World, owner: Entity) {
    let Some(castle) = world.get::<ThreadCastle>(owner) else {
        return;
    };
    let (active, binding) = (castle.active.clone(), castle.binding.clone());
    let input = world
        .query::<&ThreadForm>()
        .iter(world)
        .find(|form| {
            form.thread == active
                && form.binding.uid == binding.uid
                && form.binding.area == binding.area
        })
        .map(|form| form.input);
    if let Some(input) = input
        && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
    {
        focus.set(input, bevy::input_focus::FocusCause::Navigated);
    }
}

pub(crate) fn created(world: &mut World, entity: Entity, created: Option<&str>) {
    let Some(thread) = created else { return };
    let Some(mut castle) = world.get_mut::<ThreadCastle>(entity) else {
        return;
    };
    if !castle.creating {
        return;
    }
    if castle.pages.contains_key(thread) {
        castle.active = Some(thread.into());
        show_active(world, entity);
        focus_composer(world, entity);
    } else {
        castle.select_new = Some(thread.into());
    }
}

pub(crate) fn finished(world: &mut World, entity: Entity, error: Option<String>) -> bool {
    if controls::finished(world, entity, error.clone()) {
        return true;
    }
    if let Some(mut message) = world.get_mut::<Message>(entity) {
        message.pending = false;
        let (status, confirmation) = (message.status, message.confirmation);
        world.get_mut::<Node>(confirmation).unwrap().display = Display::None;
        world.get_mut::<Text>(status).unwrap().0 = error
            .map(|error| format!("Not deleted: {error}"))
            .unwrap_or_default();
        return true;
    }
    let Some(mut form) = world.get_mut::<ThreadForm>(entity) else {
        return false;
    };
    let sent = form.pending.take();
    let input = form.input;
    let status = form.status;
    if error.is_none()
        && sent.as_ref().is_some_and(|sent| {
            world
                .get::<EditableText>(input)
                .is_some_and(|text| &text.value().to_string() == sent)
        })
    {
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("");
    }
    world.get_mut::<Text>(status).unwrap().0 = error
        .map(|error| format!("Not sent: {error}"))
        .unwrap_or_default();
    true
}

pub(crate) fn select_thread(world: &mut World, binding: &RecordBinding, thread: &str) {
    let owners: Vec<_> = world
        .query::<(Entity, &ThreadCastle)>()
        .iter(world)
        .filter(|(_, castle)| {
            castle.binding.uid == binding.uid && castle.binding.area == binding.area
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        Switch(thread.to_string()).apply(world, owner);
    }
}
