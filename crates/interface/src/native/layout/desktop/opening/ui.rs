use super::*;
use crate::native::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::label,
    layout::desktop::input_settings::{button, disable, feedback, field, text},
    shortcuts::{self, Settings, configuration},
};
use bevy::{input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup}, text::EditableText};

mod editor;
mod fields;

#[derive(Component)]
struct Page {
    root: Entity,
    body: Entity,
    toggle: (Entity, Entity),
    expanded: bool,
    list: Entity,
    add: Entity,
    status: Entity,
    save: Entity,
    order: Vec<String>,
    origin: Vec<String>,
    observed: Vec<(String, String, bool, String)>,
    message: String,
}

#[derive(Component, Clone)]
struct RuleControl {
    uid: String,
    action: usize,
}

#[derive(Clone)]
enum Change {
    Toggle,
    New,
    Add,
    Edit(String),
    Open(String),
    Enable(String),
    Move(String, bool),
    Remove(String),
    Save,
    Cancel,
    Refresh,
    Reload,
    Rebase,
}

#[derive(Clone, Copy)]
enum EditChange {
    Save,
    Cancel,
    Reload,
    Open,
    Enabled,
    MatchAll,
    AddCondition,
    RemoveCondition(Entity),
    Property(Entity),
    Workspace(Option<u64>),
    Flag(usize),
}

#[derive(Clone)]
struct OutputChoice(Option<String>);

struct Condition {
    row: Entity,
    property: nucleus::property_rule::Property,
    choice: (Entity, Entity),
    value: Entity,
}

enum Kind {
    Rule { enabled: bool, all: bool, conditions: Vec<Condition>, list: Entity, enabled_control: (Entity, Entity), all_control: (Entity, Entity) },
    Window,
}

#[derive(Component)]
struct Editor {
    root: Entity,
    canvas: String,
    workspace: u64,
    output: Option<crate::host::OutputId>,
    content: Entity,
    previous: Option<Entity>,
    uid: Option<String>,
    title: String,
    name: Option<Entity>,
    kind: Kind,
    form: fields::Form,
    origin: Option<Payload>,
    raw: Option<Value>,
    revision: u64,
    creating: bool,
    needs_link: bool,
    loaded: bool,
    reloading: bool,
    pending: bool,
    reload: bool,
    message: String,
    status: Entity,
    save: Entity,
    secondary: [Entity; 2],
}

#[derive(Component)]
struct WindowControls {
    root: Entity,
    area: Entity,
    uid: String,
    placement: [u8; 16],
    info: Entity,
    reapply: Entity,
    last: String,
    matched: Option<Match>,
}

#[derive(Clone, Copy)]
enum WindowAction { Overrides, NewRule, Reapply, OpenRule }

fn caption(world: &mut World, control: (Entity, Entity), value: &str) {
    text(world, control.1, value);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(control.0) { node.set_label(value); }
}

fn wrapped(world: &mut World, parent: Entity, value: &str, size: f32) -> Entity {
    let entity = label(world, parent, value, size);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    entity
}

fn list(world: &mut World, parent: Entity, height: f32) -> Entity {
    world.spawn((Node { width: percent(100), min_width: px(0), max_height: px(height), flex_direction: FlexDirection::Column, row_gap: px(6), overflow: Overflow::scroll_y(), ..Default::default() }, ScrollPosition::default(), crate::native::edit_mode::FocusScroll, ChildOf(parent))).id()
}

fn active_order(world: &World) -> Vec<String> {
    world.get_resource::<LayoutSettings>().map(|settings| settings.opening_rules.clone()).unwrap_or_default()
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, (refresh, editor::refresh).after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..Default::default() }, Name::new("park.opening-rules"), ChildOf(parent))).id();
    wrapped(world, page, "Application opening rules", 20.0);
    let toggle = button(world, page, page, "Edit opening rules", Change::Toggle);
    let body = world.spawn((Node { display: Display::None, width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..Default::default() }, ChildOf(page))).id();
    wrapped(world, body, "The first enabled matching rule supplies initial preferences. Existing windows keep their placement until you explicitly Reapply. Window overrides take precedence; unspecified fields inherit.", 14.0);
    button(world, body, page, "New opening rule", Change::New);
    let add = field(world, body, "Existing opening-rule Record UID", 128);
    button(world, body, page, "Add existing rule", Change::Add);
    let list = list(world, body, 360.0);
    let status = feedback(world, body);
    let save = button(world, body, page, "Save rule order", Change::Save).0;
    button(world, body, page, "Cancel order changes", Change::Cancel);
    button(world, body, page, "Refresh rule Records", Change::Refresh);
    button(world, body, page, "Reload active rule order", Change::Reload);
    button(world, body, page, "Review rule order against file", Change::Rebase);
    let order = active_order(world);
    world.entity_mut(page).insert(Page { root, body, toggle, expanded: false, list, add, status, save, origin: order.clone(), order, observed: Vec::new(), message: String::new() });
    refresh(world);
}

pub(super) fn references(world: &mut World) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut rules = BTreeSet::new();
    let mut windows = BTreeSet::new();
    for page in world.query::<&Page>().iter(world) { rules.extend(page.order.iter().cloned()); }
    for editor in world.query::<&Editor>().iter(world) {
        if let Some(uid) = &editor.uid {
            match &editor.kind { Kind::Rule { .. } => { rules.insert(uid.clone()); }, Kind::Window => { windows.insert(uid.clone()); } }
        }
    }
    for window in world.query::<&WindowControls>().iter(world) { windows.insert(window.uid.clone()); }
    (rules, windows)
}

pub(super) fn completed(world: &mut World, owner: Entity, completion: catalog::Completion) {
    if world.get::<Editor>(owner).is_some() { editor::completed(world, owner, completion); }
    else if let Some(mut page) = world.get_mut::<Page>(owner) {
        page.message = completion.result.map(|_| "Rule Record saved. New windows use the accepted preferences.".into()).unwrap_or_else(|error| error);
    }
}

pub(super) fn context_changed(world: &mut World) { editor::context_changed(world); }

fn open_record(world: &mut World, root: Entity, uid: &str) {
    if let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) {
        let position = world.get::<CanvasView>(root).map_or(bevy::math::DVec2::ZERO, |view| view.center);
        crate::native::full_record::open_in(world, root, workspace, position, uid, Source::Local);
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        if shortcuts::keyboard_owned(world) || world.get::<Page>(page).is_none() { return; }
        let root = world.get::<Page>(page).unwrap().root;
        match self {
            Self::Toggle => { let mut page = world.get_mut::<Page>(page).unwrap(); page.expanded = !page.expanded; }
            Self::New => editor::open(world, root, None, None, None),
            Self::Edit(uid) => editor::open(world, root, Some(uid.clone()), None, None),
            Self::Open(uid) => open_record(world, root, uid),
            Self::Refresh => catalog::refresh(world),
            Self::Add => {
                let input = world.get::<Page>(page).unwrap().add;
                let value = world.get::<EditableText>(input).map(|input| input.value().to_string().trim().to_owned()).unwrap_or_default();
                let mut state = world.get_mut::<Page>(page).unwrap();
                if !nucleus::valid_uid(&value, "r") { state.message = "Choose a valid existing Record UID.".into(); }
                else if state.order.contains(&value) { state.message = "This rule is already in the list.".into(); }
                else if state.order.len() >= nucleus::window_opening::MAX_RULES { state.message = "The ordered list already contains 256 rules.".into(); }
                else { state.order.push(value); state.message = "Rule added to this draft. Save rule order to activate the list.".into(); }
            }
            Self::Move(uid, later) => {
                let mut state = world.get_mut::<Page>(page).unwrap();
                if let Some(index) = state.order.iter().position(|current| current == uid) {
                    let target = if *later { index.checked_add(1) } else { index.checked_sub(1) };
                    if let Some(target) = target.filter(|target| *target < state.order.len()) { state.order.swap(index, target); }
                }
            }
            Self::Remove(uid) => { world.get_mut::<Page>(page).unwrap().order.retain(|current| current != uid); }
            Self::Enable(uid) => {
                let entry = world.get_resource::<Catalog>().and_then(|catalog| catalog.rules.entries.get(uid)).cloned();
                let result = if let Some(Entry { title, payload: Ok(Payload::Rule(mut rule)), .. }) = entry {
                    rule.enabled = !rule.enabled;
                    if let Some(revision) = rule.revision.checked_add(1) { rule.revision = revision; catalog::save(world, page, Some(uid.clone()), title, Payload::Rule(rule), false).map(|_| ()) }
                    else { Err("The rule revision is exhausted.".into()) }
                } else { Err("Load a valid opening rule before changing its enabled state.".into()) };
                world.get_mut::<Page>(page).unwrap().message = result.map(|_| "Saving rule…".into()).unwrap_or_else(|error| error);
            }
            Self::Cancel | Self::Reload => {
                let active = active_order(world);
                let mut page = world.get_mut::<Page>(page).unwrap();
                page.order = active.clone(); page.origin = active; page.message.clear();
            }
            Self::Rebase => {
                if let Err(error) = configuration::review::open(world, page, "opening rule order", review_snapshot, review_apply) { world.get_mut::<Page>(page).unwrap().message = error; }
            }
            Self::Save => {
                let state = world.get::<Page>(page).unwrap();
                let result = if state.origin != active_order(world) { Err("Saved rule order changed. Your draft is kept; Reload active rule order or Review it against the file.".into()) }
                    else if let Some(reason) = order_error(world, &state.order) { Err(reason) }
                    else {
                        let mut layout = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
                        layout.opening_rules = state.order.clone();
                        let settings = world.get_resource::<Settings>().cloned().unwrap_or_default();
                        configuration::commit(world, settings, layout)
                    };
                if result.is_ok() {
                    let order = active_order(world);
                    world.get_mut::<Page>(page).unwrap().origin = order;
                }
                world.get_mut::<Page>(page).unwrap().message = result.unwrap_or_else(|error| error);
            }
        }
        refresh(world);
    }
}

fn review_snapshot(world: &World, page: Entity) -> Result<configuration::review::Snapshot, String> {
    let state = world.get::<Page>(page).ok_or("The opening-rule list is unavailable.")?;
    configuration::review::Snapshot::new(&state.origin, &state.order, &active_order(world))
}

fn review_apply(world: &mut World, page: Entity, active: Value, draft: Value) -> Result<(), String> {
    let origin: Vec<String> = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let order: Vec<String> = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    let mut layout = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
    layout.opening_rules = order.clone();
    layout.validate()?;
    let mut state = world.get_mut::<Page>(page).ok_or("The opening-rule list is unavailable.")?;
    state.origin = origin;
    state.order = order;
    state.message = "Review applied to the draft. Save rule order to activate it.".into();
    Ok(())
}

fn order_error(world: &World, order: &[String]) -> Option<String> {
    for uid in order {
        match world.get_resource::<Catalog>().and_then(|catalog| catalog.rules.entries.get(uid)).map(|entry| &entry.payload) {
            Some(Ok(Payload::Rule(_))) => {}
            Some(Err(error)) => return Some(format!("Rule {uid}: {error}")),
            _ => return Some("Wait for all rule Records to load before saving their order.".into()),
        }
    }
    None
}

fn captured_window(world: &World, controls: &WindowControls) -> Option<crate::host::WindowInspection> {
    let metadata = world.get::<crate::native::full_record::window::Metadata>(controls.area)?;
    if !metadata.live || metadata.uid != controls.uid || metadata.placement != controls.placement { return None; }
    let entity = world.get::<Children>(controls.root)?.iter().find(|entity| world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == controls.placement))?;
    let window = world.get::<ApplicationWindow>(entity)?;
    let workspace = world.get::<WorkspaceMember>(entity)?.0;
    Some(crate::host::WindowInspection { owner: controls.root.to_bits(), workspace, position: world.get::<CanvasView>(controls.root).map_or([0.0; 2], |view| view.center.to_array()), surface: window.surface, placement: controls.placement })
}

pub(crate) fn window_panel(world: &mut World, row: Entity, binding: &RecordBinding) {
    let Some(metadata) = world.get::<crate::native::full_record::window::Metadata>(binding.area).filter(|metadata| metadata.uid == binding.uid && binding.source == Source::Local).cloned() else { return; };
    let Some(root) = super::super::root(world, binding.area) else { return; };
    let panel = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..Default::default() }, Name::new("window.properties.opening"), ChildOf(row))).id();
    wrapped(world, panel, "Opening preferences", 16.0);
    let info = wrapped(world, panel, "", 14.0);
    button(world, panel, panel, "Edit this window's opening overrides", WindowAction::Overrides);
    button(world, panel, panel, "Create a rule from this application", WindowAction::NewRule);
    button(world, panel, panel, "Open the matching rule Record", WindowAction::OpenRule);
    let reapply = button(world, panel, panel, "Reapply opening preferences to this window", WindowAction::Reapply).0;
    world.entity_mut(panel).insert(WindowControls { root, area: binding.area, uid: binding.uid.clone(), placement: metadata.placement, info, reapply, last: String::new(), matched: None });
}

impl Action for WindowAction {
    fn apply(&self, world: &mut World, panel: Entity) {
        if shortcuts::keyboard_owned(world) { return; }
        let Some(controls) = world.get::<WindowControls>(panel) else { return; };
        let (root, uid, placement, area) = (controls.root, controls.uid.clone(), controls.placement, controls.area);
        let target = captured_window(world, controls);
        match self {
            Self::Overrides => editor::open(world, root, Some(uid), Some(true), None),
            Self::NewRule => {
                let app_id = world.get::<crate::native::full_record::window::Metadata>(area)
                    .filter(|metadata| metadata.uid == uid && metadata.placement == placement)
                    .map(|metadata| metadata.app_id.clone()).unwrap_or_default();
                editor::open(world, root, None, None, Some(app_id));
            }
            Self::Reapply => {
                if let Some(target) = target {
                    match super::request(world, root, target) {
                        Ok(request) => { host_request(world, crate::host::HostRequest::ReapplyOpening(request)); }
                        Err(error) => crate::native::notifications::report(world, "Opening preferences", &error),
                    }
                }
                else { crate::native::notifications::report(world, "Opening preferences", "The original application is unavailable; no replacement window was selected."); }
            }
            Self::OpenRule => {
                let matched = target.and_then(|_| world.get::<Children>(root).and_then(|children| children.iter().find(|entity| world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == placement)))
                    .and_then(|entity| world.get::<Decision>(entity)).and_then(|decision| decision.matched.clone()))
                    .or_else(|| world.get::<WindowControls>(panel).and_then(|controls| controls.matched.clone()));
                if let Some(matched) = matched { open_record(world, root, &matched.uid); }
                else { crate::native::notifications::report(world, "Opening preferences", "No opening rule was applied to the captured window."); }
            }
        }
    }
}

fn refresh(world: &mut World) {
    let active = active_order(world);
    let pages: Vec<_> = world.query_filtered::<Entity, With<Page>>().iter(world).collect();
    for page in pages {
        let state = world.get::<Page>(page).unwrap();
        if state.order == state.origin && state.origin != active {
            let mut state = world.get_mut::<Page>(page).unwrap(); state.order = active.clone(); state.origin = active.clone();
        }
        let state = world.get::<Page>(page).unwrap();
        let (body, toggle, list, status, save, expanded, origin, order, message) = (state.body, state.toggle, state.list, state.status, state.save, state.expanded, state.origin.clone(), state.order.clone(), state.message.clone());
        world.get_mut::<Node>(body).unwrap().display = if expanded { Display::Flex } else { Display::None };
        caption(world, toggle, if expanded { "Hide opening rules" } else { "Edit opening rules" });
        let observed: Vec<_> = order.iter().map(|uid| {
            let entry = world.get_resource::<Catalog>().and_then(|catalog| catalog.rules.entries.get(uid));
            let title = entry.map(|entry| entry.title.clone()).unwrap_or_else(|| uid.clone());
            let (enabled, status) = match entry.map(|entry| &entry.payload) {
                Some(Ok(Payload::Rule(rule))) => (rule.enabled, format!("revision {}", rule.revision)),
                Some(Err(error)) => (false, error.clone()),
                _ => (false, "Loading…".into()),
            };
            (uid.clone(), title, enabled, status)
        }).collect();
        if world.get::<Page>(page).unwrap().observed != observed {
            let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get).and_then(|entity| world.get::<RuleControl>(entity)).cloned();
            let previous_index = previous.as_ref().and_then(|control| world.get::<Page>(page)?.observed.iter().position(|row| row.0 == control.uid));
            let children: Vec<_> = world.get::<Children>(list).map(|children| children.iter().collect()).unwrap_or_default();
            for child in children { world.despawn(child); }
            let mut restored = None;
            let mut fallback = None;
            for (index, (uid, title, enabled, status)) in observed.iter().enumerate() {
                let row = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), flex_shrink: 0.0, ..Default::default() }, ChildOf(list))).id();
                wrapped(world, row, &format!("{}. {title} · {} · {status}", index + 1, if *enabled { "enabled" } else { "disabled" }), 14.0);
                let actions = crate::native::sand_panel::row(world, row);
                world.get_mut::<Node>(actions).unwrap().flex_wrap = FlexWrap::Wrap;
                for (kind, (caption, action)) in [("Edit", Change::Edit(uid.clone())), ("Open Record", Change::Open(uid.clone())), (if *enabled { "Disable" } else { "Enable" }, Change::Enable(uid.clone())), ("Earlier", Change::Move(uid.clone(), false)), ("Later", Change::Move(uid.clone(), true)), ("Remove from list", Change::Remove(uid.clone()))].into_iter().enumerate() {
                    let button = crate::native::sand_panel::button(world, actions, page, caption, action.clone());
                    world.entity_mut(button).insert((RuleControl { uid: uid.clone(), action: kind }, Name::new(format!("opening.rule.{uid}.control.{kind}"))));
                    let blocked = matches!(&action, Change::Move(_, false) if index == 0) || matches!(&action, Change::Move(_, true) if index + 1 == observed.len())
                        || matches!(&action, Change::Enable(_)) && !world.get_resource::<Catalog>().and_then(|catalog| catalog.rules.entries.get(uid)).is_some_and(|entry| matches!(&entry.payload, Ok(Payload::Rule(_))));
                    disable(world, button, blocked);
                    if !blocked && previous.as_ref().is_some_and(|control| control.uid == *uid && control.action == kind) { restored = Some(button); }
                    if kind == 0 && previous_index.is_some_and(|old| index == old.min(observed.len().saturating_sub(1))) { fallback = Some(button); }
                }
            }
            if previous.is_some() && let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(restored.or(fallback).unwrap_or(toggle.0), FocusCause::Navigated); }
            world.get_mut::<Page>(page).unwrap().observed = observed;
        }
        let error = (origin != active).then(|| "Saved rule order changed. Your draft is kept; Reload active rule order or Review it against the file.".to_owned()).or_else(|| order_error(world, &order));
        let message = error.as_ref().map(|error| if message.is_empty() { error.clone() } else { format!("{error}\n{message}") }).unwrap_or(message);
        text(world, status, message);
        disable(world, save, order == active || error.is_some());
    }
    let controls: Vec<_> = world.query_filtered::<Entity, With<WindowControls>>().iter(world).collect();
    for panel in controls {
        let controls = world.get::<WindowControls>(panel).unwrap();
        let target = captured_window(world, controls);
        let decision = target.and_then(|_| world.get::<Children>(controls.root).and_then(|children| children.iter().find(|entity| world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == controls.placement))).and_then(|entity| world.get::<Decision>(entity)).cloned());
        let (info, reapply, last) = (controls.info, controls.reapply, controls.last.clone());
        if let Some(decision) = &decision { world.get_mut::<WindowControls>(panel).unwrap().matched = decision.matched.clone(); }
        let description = decision.map(|decision| {
            let matched = decision.matched.map(|matched| format!("Rule: {} · {} · revision {}", matched.title, matched.uid, matched.revision)).unwrap_or_else(|| "No matching application rule; normal or retained preferences were used.".into());
            format!("{matched}\nRequested: {}{}", fields::summary(&decision.properties), decision.warning.map(|warning| format!("\n{warning}")).unwrap_or_default())
        }).unwrap_or_else(|| if last.is_empty() { "The live opening decision is unavailable.".into() } else { format!("{}\nApplication closed or unavailable.", last.lines().filter(|line| *line != "Application closed or unavailable.").collect::<Vec<_>>().join("\n")) });
        text(world, info, &description);
        world.get_mut::<WindowControls>(panel).unwrap().last = description;
        let unavailable = target.is_none() || !world.contains_resource::<HostRequests>();
        disable(world, reapply, unavailable);
    }
}

#[cfg(test)]
mod tests;
