use super::*;
use crate::native::{
    actions::{Action, KeyBinding, KeyBindings, Modifiers},
    layout::desktop::input_settings::{button, disable, feedback, text},
    shortcuts,
};
use bevy::input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup};
use serde_json::Value;

mod merge;
use merge::{Choice, Proposal};

#[derive(Clone, PartialEq)]
pub(crate) struct Snapshot {
    base: Value,
    local: Value,
    active: Value,
}

impl Snapshot {
    pub(crate) fn new<T: Serialize>(base: &T, local: &T, active: &T) -> Result<Self, String> {
        Ok(Self { base: serde_json::to_value(base).map_err(|error| error.to_string())?, local: serde_json::to_value(local).map_err(|error| error.to_string())?, active: serde_json::to_value(active).map_err(|error| error.to_string())? })
    }
}

type Read = fn(&World, Entity) -> Result<Snapshot, String>;
type Apply = fn(&mut World, Entity, Value, Value) -> Result<(), String>;

#[derive(Clone, PartialEq)]
struct Version {
    path: Option<PathBuf>,
    revision: u64,
    error: Option<String>,
}

fn version(world: &World) -> Option<Version> {
    world.get_resource::<Storage>().map(|storage| Version { path: storage.path.clone(), revision: storage.revision, error: storage.error.clone() })
}

#[derive(Component)]
struct Session {
    owner: Entity,
    root: Entity,
    canvas: String,
    workspace: u64,
    output: Option<crate::host::OutputId>,
    activity: Option<u64>,
    previous_owner: Option<Entity>,
    previous_focus: Option<Entity>,
    snapshot: Snapshot,
    version: Option<Version>,
    proposal: Proposal,
    sequence: u64,
    read: Read,
    apply: Apply,
    list: Entity,
    status: Entity,
    confirm: Entity,
    message: String,
}

#[derive(Component)]
struct FieldStatus { overlay: Entity, sequence: u64, index: usize }

pub(crate) fn settled(world: &World, fields: impl IntoIterator<Item = Entity>) -> Result<(), String> {
    for field in fields {
        let text = world.get::<bevy::text::EditableText>(field).ok_or("A settings field is unavailable.")?;
        if text.is_composing() || !text.pending_edits.is_empty() || text.pending_paste.is_some() { return Err("Finish text composition or the pending paste before reloading or reviewing this draft.".into()); }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Change { Cancel, Refresh, Confirm, Choose(u64, usize, Choice) }

fn within(world: &World, mut entity: Entity, owner: Entity) -> bool {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if entity == owner { return true; }
        let Some(parent) = world.get::<ChildOf>(entity) else { return false; };
        entity = parent.parent();
    }
    false
}

fn label(world: &mut World, parent: Entity, value: &str, size: f32) -> Entity {
    let entity = crate::native::edit_mode::label(world, parent, value, size);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    entity
}

fn column(world: &mut World, parent: Entity) -> Entity {
    world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0, ..Default::default() }, ChildOf(parent))).id()
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, refresh.after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

pub(crate) fn open(world: &mut World, owner: Entity, name: &str, read: Read, apply: Apply) -> Result<(), String> {
    if world.get_entity(owner).is_err() { return Err("The settings editor is unavailable.".into()); }
    let previous_owner = world.get_resource::<shortcuts::KeyboardModalOwner>().and_then(|owner| owner.0).filter(|entity| world.get_entity(*entity).is_ok());
    if previous_owner.is_some_and(|previous| !within(world, owner, previous)) { return Err("Finish the current dialog before reviewing another draft.".into()); }
    let snapshot = read(world, owner)?;
    let version = version(world);
    if version.as_ref().is_some_and(|version| version.error.is_some()) { return Err("The external configuration is unavailable or invalid. Restore a valid file before rebasing; your draft is kept.".into()); }
    let proposal = Proposal::new(&snapshot.base, &snapshot.local, &snapshot.active)?;
    let previous_focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let mut root = owner;
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if world.get::<crate::native::workspace::Workspaces>(root).is_some() { break; }
        let Some(parent) = world.get::<ChildOf>(root) else { break; };
        root = parent.parent();
    }
    let spaces = world.get::<crate::native::workspace::Workspaces>(root).ok_or("The original settings canvas is unavailable.")?;
    let (canvas, workspace) = (spaces.canvas_id.clone(), spaces.active);
    if !shortcuts::input_eligible(world, root, &canvas, owner) { return Err("The original settings editor is hidden or unavailable. Your draft is kept.".into()); }
    let output = world.get_resource::<crate::native::output_views::OutputViews>().filter(|views| views.applies_to(root)).map(|views| views.focused);
    let activity = world.get_resource::<crate::host::input::InputActivity>().map(|activity| activity.frame_revision());
    let overlay = world.spawn((
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..Default::default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)), GlobalZIndex(110), TabGroup::modal(),
        crate::native::inspection::InspectionExcluded, Name::new("configuration.review"), ChildOf(root),
        KeyBindings(vec![KeyBinding::new(KeyCode::Escape, Modifiers::NONE, crate::native::actions![Change::Cancel])]),
    )).id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Dialog);
    accessible.set_label(format!("Review {name}"));
    world.entity_mut(overlay).insert(bevy::a11y::AccessibilityNode::from(accessible));
    let panel = column(world, overlay);
    world.entity_mut(panel).insert((
        Node { width: px(680), max_width: percent(96), height: percent(90), min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::all(px(12)), ..Default::default() },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    label(world, panel, &format!("Review {name}"), 22.0);
    label(world, panel, "Independent changes are merged. Choose a value for each conflict. Continue updates your draft; normal Save applies it. Cancel keeps your draft.", 14.0);
    let list = column(world, panel);
    world.entity_mut(list).insert((
        Node { width: percent(100), min_width: px(0), min_height: px(0), flex_grow: 1.0, flex_shrink: 1.0, flex_direction: FlexDirection::Column, row_gap: px(10), overflow: Overflow::scroll_y(), ..Default::default() },
        ScrollPosition::default(), crate::native::edit_mode::FocusScroll,
    )).observe(|mut event: On<Pointer<Scroll>>| event.propagate(false));
    let status = feedback(world, panel);
    let confirm = button(world, panel, overlay, "Continue with reviewed draft", Change::Confirm).0;
    world.entity_mut(confirm).insert(Name::new("configuration.review.confirm"));
    let update = button(world, panel, overlay, "Update comparison", Change::Refresh).0;
    world.entity_mut(update).insert(Name::new("configuration.review.update"));
    let cancel = button(world, panel, overlay, "Cancel review and keep draft", Change::Cancel).0;
    world.entity_mut(cancel).insert(Name::new("configuration.review.cancel"));
    let session = Session { owner, root, canvas, workspace, output, activity, previous_owner, previous_focus, snapshot, version, proposal, sequence: 1, read, apply, list, status, confirm, message: String::new() };
    rows(world, overlay, &session);
    world.entity_mut(overlay).insert(session);
    shortcuts::own_keyboard(world, overlay);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(cancel, FocusCause::Navigated); }
    refresh(world);
    Ok(())
}

fn value(world: &mut World, parent: Entity, name: &str, value: Option<&Value>) {
    label(world, parent, name, 14.0);
    let view = column(world, parent);
    world.entity_mut(view).insert((ScrollPosition::default(), crate::native::edit_mode::FocusScroll));
    let mut node = world.get_mut::<Node>(view).unwrap();
    node.max_height = px(112);
    node.overflow = Overflow::scroll_y();
    let value = value.map(|value| serde_json::to_string_pretty(value).unwrap_or_default()).unwrap_or_else(|| "Inherit the registered default (no override)".into());
    label(world, view, &value, 14.0);
}

fn rows(world: &mut World, overlay: Entity, session: &Session) {
    let children: Vec<_> = world.get::<Children>(session.list).map(|children| children.iter().collect()).unwrap_or_default();
    for child in children { world.despawn(child); }
    if session.proposal.conflicts.is_empty() { label(world, session.list, "No conflicting fields. Continue keeps your independent edits and the accepted external changes.", 16.0); }
    for (index, conflict) in session.proposal.conflicts.iter().enumerate() {
        let row = column(world, session.list);
        let title = conflict.path.last().and_then(|id| Shortcut::from_id(id)).map(|command| command.label().to_owned())
            .unwrap_or_else(|| if conflict.path.is_empty() { "Ordered list".into() } else { conflict.path.join(" › ").replace('_', " ") });
        label(world, row, &title, 18.0);
        value(world, row, "My draft", conflict.local.as_ref());
        let local = button(world, row, overlay, "Use my draft value", Change::Choose(session.sequence, index, Choice::Local)).0;
        world.entity_mut(local).insert(Name::new(format!("configuration.review.choice.{index}.local")));
        value(world, row, "Accepted file value", conflict.file.as_ref());
        let file = button(world, row, overlay, "Use file value", Change::Choose(session.sequence, index, Choice::File)).0;
        world.entity_mut(file).insert(Name::new(format!("configuration.review.choice.{index}.file")));
        let status = label(world, row, "Choose a value", 14.0);
        world.entity_mut(status).insert(FieldStatus { overlay, sequence: session.sequence, index });
    }
}

fn current(world: &World, session: &Session) -> Result<(), String> {
    if !owner_eligible(world, session) { return Err("The original settings view is no longer active. Your draft is kept.".into()); }
    if version(world) != session.version { return Err("The configuration file changed again. Your draft is kept; Update comparison before continuing.".into()); }
    if (session.read)(world, session.owner)? != session.snapshot { return Err("The editor or active values changed. Your draft is kept; Update comparison before continuing.".into()); }
    Ok(())
}

fn owner_eligible(world: &World, session: &Session) -> bool {
    shortcuts::input_eligible(world, session.root, &session.canvas, session.owner)
        && world.get_resource::<crate::host::input::InputActivity>().is_none_or(|activity| activity.active() && session.activity == Some(activity.revision()))
        && world.get::<crate::native::workspace::Workspaces>(session.root).is_some_and(|spaces| spaces.active == session.workspace)
        && session.output.is_none_or(|output| world.get_resource::<crate::native::output_views::OutputViews>().is_some_and(|views| {
            views.applies_to(session.root) && views.focused == output && views.outputs.iter().any(|view| view.id == output && view.workspace == session.workspace)
        }))
}

fn close(world: &mut World, overlay: Entity, session: Session) {
    let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|owner| owner.0 == Some(overlay));
    let eligible = owner_eligible(world, &session);
    let previous_owner = session.previous_owner.filter(|entity| eligible && shortcuts::input_eligible(world, session.root, &session.canvas, *entity));
    let previous_focus = session.previous_focus.filter(|entity| eligible && shortcuts::input_eligible(world, session.root, &session.canvas, *entity)
        && previous_owner.is_none_or(|owner| within(world, *entity, owner)));
    world.despawn(overlay);
    if owns {
        shortcuts::release_keyboard(world, overlay);
        if let Some(owner) = previous_owner { shortcuts::own_keyboard(world, owner); }
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            if let Some(previous) = previous_focus { focus.set(previous, FocusCause::Navigated); } else { focus.clear(); }
        }
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, overlay: Entity) {
        if !world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|owner| owner.0 == Some(overlay)) || world.get::<Session>(overlay).is_none() { return; }
        let mut session = world.entity_mut(overlay).take::<Session>().unwrap();
        let result = match *self {
            Self::Cancel => { close(world, overlay, session); return; }
            Self::Choose(sequence, index, choice) => {
                if sequence != session.sequence { Err("The reviewed field belongs to an older comparison.".into()) }
                else { session.proposal.choose(index, choice).map(|_| { session.message = format!("Field {} uses {}.", index + 1, if choice == Choice::Local { "your draft" } else { "the accepted file value" }); }) }
            }
            Self::Refresh => (|| {
                let snapshot = (session.read)(world, session.owner)?;
                let version = version(world);
                if version.as_ref().is_some_and(|version| version.error.is_some()) { return Err("Restore a valid configuration file before updating the comparison.".into()); }
                let proposal = Proposal::new(&snapshot.base, &snapshot.local, &snapshot.active)?;
                session.sequence = session.sequence.checked_add(1).ok_or("Review identifiers are exhausted; close and reopen this review.")?;
                session.snapshot = snapshot; session.version = version; session.proposal = proposal;
                session.message = "Comparison updated. Review the new conflicts before continuing.".into();
                rows(world, overlay, &session);
                if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(session.confirm, FocusCause::Navigated); }
                Ok(())
            })(),
            Self::Confirm => (|| {
                current(world, &session)?;
                if !session.proposal.resolved() { return Err("Choose my draft or file value for every conflicting field.".into()); }
                (session.apply)(world, session.owner, session.snapshot.active.clone(), session.proposal.value.clone().ok_or("The reviewed draft is unavailable.")?)
            })(),
        };
        if result.is_ok() && matches!(self, Self::Confirm) { close(world, overlay, session); return; }
        if let Err(error) = result { session.message = error; }
        world.entity_mut(overlay).insert(session);
        refresh(world);
    }
}

fn refresh(world: &mut World) {
    let overlays: Vec<_> = world.query_filtered::<Entity, With<Session>>().iter(world).collect();
    for overlay in overlays {
        let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|owner| owner.0 == Some(overlay));
        if !owns || !owner_eligible(world, world.get::<Session>(overlay).unwrap()) {
            let session = world.entity_mut(overlay).take::<Session>().unwrap();
            close(world, overlay, session);
            continue;
        }
        let session = world.get::<Session>(overlay).unwrap();
        let error = current(world, session).err();
        let unresolved = session.proposal.conflicts.iter().filter(|conflict| conflict.choice.is_none()).count();
        let message = error.clone().unwrap_or_else(|| format!("{}\n{unresolved} unresolved fields. Active settings are unchanged by this review.", session.message));
        let (status, confirm, available) = (session.status, session.confirm, error.is_none() && session.proposal.resolved());
        let sequence = session.sequence;
        let choices: Vec<_> = session.proposal.conflicts.iter().map(|conflict| conflict.choice).collect();
        text(world, status, message);
        disable(world, confirm, !available);
        let labels: Vec<_> = world.query::<(Entity, &FieldStatus)>().iter(world).filter(|(_, field)| field.overlay == overlay && field.sequence == sequence).map(|(entity, field)| (entity, field.index)).collect();
        for (entity, index) in labels {
            text(world, entity, match choices.get(index).copied().flatten() { None => "Choose a value", Some(Choice::Local) => "Selected: my draft value", Some(Choice::File) => "Selected: accepted file value" });
        }
    }
}

#[cfg(test)]
mod tests;
