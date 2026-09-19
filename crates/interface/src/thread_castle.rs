pub(crate) mod tests;

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
}

#[derive(Component)]
struct Page {
    castle: Entity,
    uid: String,
    tab: Entity,
    tab_label: Entity,
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
        app.add_observer(scroll)
            .add_systems(Update, crate::full_record::receive.after(crate::cell_bridge::ReceiveCell))
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
    crate::fiote::session::populate(world, parent, binding.clone());
    crate::edit_mode::label(world, parent, "Threads", 20.0);
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
    form(world, parent, binding.clone(), None);
    world.entity_mut(parent).insert(ThreadCastle {
        binding,
        tabs,
        active: None,
        pages: HashMap::new(),
    });
    refresh(world, parent, data);
}

pub fn open(world: &mut World, root: Entity, reference: &str, source: Source) -> Option<Entity> {
    let area = crate::full_record::open(world, root, reference, source)?;
    let entity = area;
    let mut area = world.get_mut::<crate::area::InfluenceArea>(entity)?;
    area.name = "Thread Castle".into();
    let config = area.protein.as_mut()?;
    config.draft.name = "Threads".into();
    config
        .bindings
        .retain(|binding| binding.property == "threads");
    Some(entity)
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
    let input = world
        .spawn((
            crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(container),
        ))
        .insert(Node {
            width: percent(100),
            min_height: px(48),
            max_height: px(160),
            ..default()
        })
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(65_536);
    if thread.is_none() {
        let mut editor = world.get_mut::<EditableText>(input).unwrap();
        editor.allow_newlines = false;
        editor.visible_lines = Some(1.0);
        editor.max_characters = Some(256);
    }
    let status = crate::edit_mode::label(world, container, "", 12.0);
    control(
        world,
        container,
        container,
        if thread.is_some() {
            "Send message"
        } else {
            "Add thread"
        },
        Send,
    );
    if let Some(thread) = &thread {
        crate::fiote::session::thread_controls(world, container, thread, &binding);
    }
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
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(castle),
        ))
        .id();
    let tab = control(world, tabs, castle, title, Switch(uid.into()));
    let tab_label = world.get::<Children>(tab).unwrap()[0];
    let editor = world
        .spawn((
            crate::sand::text_editor(title, world.resource::<crate::theme::Typography>(), 0),
            ChildOf(entity),
        ))
        .id();
    crate::record_binding::attach(
        world,
        editor,
        RecordBinding {
            uid: uid.into(),
            ..binding.clone()
        },
        "head",
        None,
    );
    {
        let mut title = world.get_mut::<EditableText>(editor).unwrap();
        title.allow_newlines = false;
        title.visible_lines = Some(1.0);
    }
    let status = crate::edit_mode::label(world, entity, "", 12.0);
    let older = control(world, entity, entity, "Older messages", LoadOlder);
    let viewport = world
        .spawn((
            Node {
                width: percent(100),
                height: px(360),
                min_height: px(120),
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
                row_gap: px(12),
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
        tab_label,
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

fn message(world: &mut World, parent: Entity, binding: &RecordBinding, data: &Value) -> Entity {
    let uid = data["uid"].as_str().unwrap();
    let entity = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let author = data["author"].as_str().unwrap_or("Unknown author");
    let author = if data["organ_uid"].as_str() == Some(author) {
        data["organ_name"].as_str().unwrap_or(author)
    } else {
        author
    };
    crate::edit_mode::label(world, entity, &format!("Written by {author}"), 13.0);
    if let Some(at) = data["created_at"].as_str() {
        crate::edit_mode::label(world, entity, at, 12.0);
    }
    let status = crate::edit_mode::label(world, entity, "", 12.0);
    let input = world
        .spawn((
            crate::sand::text_editor(
                data["body"].as_str().unwrap_or_default(),
                world.resource::<crate::theme::Typography>(),
                0,
            ),
            ChildOf(entity),
        ))
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(65_536);
    crate::record_binding::attach(
        world,
        input,
        RecordBinding {
            uid: uid.into(),
            ..binding.clone()
        },
        "body",
        Some(status),
    );
    crate::description::attach_editor(
        world,
        entity,
        input,
        crate::description::Context {
            owner: entity,
            source: binding.source.clone(),
        },
    );
    control(world, entity, entity, "Delete message", AskDelete(true));
    let confirmation = world
        .spawn((
            Node {
                display: Display::None,
                width: percent(100),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                ..default()
            },
            ChildOf(entity),
        ))
        .id();
    crate::edit_mode::label(world, confirmation, "Delete this message Record?", 14.0);
    control(world, confirmation, entity, "Delete Record", Delete);
    control(world, confirmation, entity, "Cancel", AskDelete(false));
    world.entity_mut(entity).insert(Message {
        binding: binding.clone(),
        uid: uid.into(),
        status,
        confirmation,
        pending: false,
    });
    entity
}

pub fn refresh(world: &mut World, parent: Entity, data: &Value) -> bool {
    let Some(mut castle) = world.entity_mut(parent).take::<ThreadCastle>() else {
        return false;
    };
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
        if castle.active.is_none() {
            castle.active = Some(uid.into());
        }
        let mut page = world.entity_mut(entity).take::<Page>().unwrap();
        world.get_mut::<Text>(page.tab_label).unwrap().0 = title.into();
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
                .or_insert_with(|| message(world, page.list, &castle.binding, data));
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
        if value.trim().is_empty() {
            return;
        }
        let action = if let Some(thread) = &form.thread {
            engine::actions::Action::CreateMessage {
                thread: thread.clone(),
                body: value.clone(),
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

pub(crate) fn finished(world: &mut World, entity: Entity, error: Option<String>) -> bool {
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
        .unwrap_or_else(|| "Sent".into());
    true
}
