// non-lean-yet
// Scope: mod editor, mod fields, mod presentation, mod references.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 176, 203, 237,
//   245, 256, 280, 375, 382, 387, 394, 408, 409.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (134, 199, 201, 207, 212, 226, 232, 240, 244, 248, 251, 262, 264).

use super::*;
use crate::native::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::label,
    layout::desktop::input_settings::{button, caption, disable, display, feedback, field, text},
    shortcuts::{self, Settings, configuration},
};
use bevy::{input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup}, text::EditableText};

mod editor;
mod fields;
mod presentation;
use presentation::refresh;
mod references;
pub(super) use references::References;

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
struct Editor(Option<EditorState>);

struct EditorState {
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
    preparation: editor::draft::Draft,
    presentation: editor::presentation::Presentation,
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

fn active_rules(world: &World) -> &[String] {
    world
        .get_resource::<LayoutSettings>()
        .map_or(&[], |settings| settings.opening_rules.as_slice())
}

fn active_order(world: &World) -> Vec<String> {
    active_rules(world).to_vec()
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
    world.entity_mut(page).insert(Page { root, body, toggle, expanded: false, list, add, status, save, origin: order.clone(), order, message: String::new() });
    refresh(world);
}

pub(super) fn completed(world: &mut World, owner: Entity, completion: catalog::Completion) {
    if editor::state(world, owner).is_some() { editor::completed(world, owner, completion); }
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
                let value = world
                    .get::<EditableText>(input)
                    .map(|input| input.value().to_cow().trim().to_owned())
                    .unwrap_or_default();
                let mut state = world.get_mut::<Page>(page).unwrap();
                if !nucleus::valid_uid(&value, "r") {
                    state.message = "Choose a valid existing Record UID.".into();
                } else if state.order.contains(&value) {
                    state.message = "This rule is already in the list.".into();
                } else if state.order.len() >= nucleus::window_opening::MAX_RULES {
                    state.message = "The ordered list already contains 256 rules.".into();
                } else {
                    state.order.push(value);
                    state.message =
                        "Rule added to this draft. Save rule order to activate the list.".into();
                }
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

fn review_snapshot(world: &World, page: Entity, previous: Option<&configuration::review::Snapshot>) -> Result<configuration::review::Snapshot, String> {
    let state = world.get::<Page>(page).ok_or("The opening-rule list is unavailable.")?;
    configuration::review::Snapshot::new(&state.origin, &state.order, &active_order(world), previous)
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

enum OrderProblem<'a> {
    Invalid { uid: &'a str, error: &'a str },
    Loading,
}

impl std::fmt::Display for OrderProblem<'_> {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { uid, error } => write!(output, "Rule {uid}: {error}"),
            Self::Loading => {
                output.write_str("Wait for all rule Records to load before saving their order.")
            }
        }
    }
}

fn order_problem<'a>(world: &'a World, order: &'a [String]) -> Option<OrderProblem<'a>> {
    let entries = world
        .get_resource::<Catalog>()
        .map(|catalog| &catalog.rules.entries);
    for uid in order {
        match entries
            .and_then(|entries| entries.get(uid))
            .map(|entry| &entry.payload)
        {
            Some(Ok(Payload::Rule(_))) => {}
            Some(Err(error)) => return Some(OrderProblem::Invalid { uid, error }),
            _ => return Some(OrderProblem::Loading),
        }
    }
    None
}

fn order_error(world: &World, order: &[String]) -> Option<String> {
    order_problem(world, order).map(|problem| problem.to_string())
}

fn captured_target(
    world: &World,
    controls: &WindowControls,
) -> Option<(Entity, crate::host::WindowInspection)> {
    let metadata = world.get::<crate::native::full_record::window::Metadata>(controls.area)?;
    if !metadata.live || metadata.uid != controls.uid || metadata.placement != controls.placement {
        return None;
    }
    let entity = world
        .get::<Children>(controls.root)?
        .iter()
        .find(|entity| {
            world
                .get::<LayoutBox>(*entity)
                .is_some_and(|layout| layout.id == controls.placement)
        })?;
    let window = world.get::<ApplicationWindow>(entity)?;
    let workspace = world.get::<WorkspaceMember>(entity)?.0;
    Some((
        entity,
        crate::host::WindowInspection {
            owner: controls.root.to_bits(),
            workspace,
            position: world
                .get::<CanvasView>(controls.root)
                .map_or([0.0; 2], |view| view.center.to_array()),
            surface: window.surface,
            placement: controls.placement,
        },
    ))
}

fn captured_window(
    world: &World,
    controls: &WindowControls,
) -> Option<crate::host::WindowInspection> {
    captured_target(world, controls).map(|(_, target)| target)
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
            Self::Overrides => {
                let record = target.and_then(|target| world.get::<Children>(root).and_then(|children| children.iter().find(|entity| world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == target.placement))))
                    .and_then(|entity| world.get::<RecordBinding>(entity)).map(|binding| binding.uid.clone());
                if let Some(record) = record { editor::open(world, root, Some(record), Some(true), None); }
                else { crate::native::notifications::report(world, "Opening preferences", "Attach a Record to store opening overrides."); }
            }
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

#[cfg(test)]
mod tests;
