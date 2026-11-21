// non-lean-yet
// Scope: const MAX_ACTION_BYTES, struct Context, struct AssistedFocus, struct RevealIntent.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 272, 276, 339.
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   context:interactive (126), owner_context:interactive (130).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (475, 488).
// - [Clippy probe: clippy::allow_attributes] Use a reasoned expect when the diagnostic reliably
//   fires. Sites 510.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 486, 487.

use super::*;
use accesskit::{Action, ActionData, ActionRequest};
use bevy::{ecs::change_detection::DetectChanges, input_focus::{InputFocus, FocusCause}, text::{EditableText, FontCx, LayoutCx, TextEdit}};
use crate::native::{actions::ActionButton, layout::{LayoutBox, desktop::{self, ApplicationWindow}}, workspace::{WorkspaceMember, Workspaces}, shortcuts};
use std::hash::{Hash, Hasher};

pub const MAX_ACTION_BYTES: usize = 65_536;

#[derive(Clone, PartialEq, Eq)]
pub struct Context {
    pub root: Entity,
    canvas: String,
    workspace: u64,
    output: Option<crate::host::OutputId>,
    modal: Option<Entity>,
    owner: Option<Entity>,
    placement: Option<[u8; 16]>,
    surface: Option<crate::host::SurfaceId>,
    record: Option<(Entity, String)>,
    control: Option<(Entity, u32)>,
    text: Option<u64>,
    text_generation: Option<parley::Generation>,
    protected: bool,
    activity: Option<u64>,
    display_revision: u64,
    mapping_revision: u64,
}

impl Context {
    pub fn output(&self) -> Option<crate::host::OutputId> { self.output }

    pub(crate) fn same_input_target(&self, mut current: Self) -> bool {
        current.text_generation = self.text_generation;
        self == &current
    }

    pub fn same_owner(&self, other: &Self) -> bool {
        self.root == other.root && self.canvas == other.canvas && self.workspace == other.workspace
            && self.output == other.output && self.modal == other.modal && self.owner == other.owner
            && self.placement == other.placement && self.surface == other.surface && self.record == other.record
            && self.control.map(|(target, _)| target) == other.control.map(|(target, _)| target)
            && self.protected == other.protected
            && self.activity == other.activity
    }

    pub fn describes(&self, world: &World, entity: Entity) -> bool {
        scope(world, entity, false, false).is_some_and(|current| {
            self.same_owner(&current) && self.display_revision == current.display_revision && self.mapping_revision == current.mapping_revision
        })
    }
}

#[derive(Resource)]
struct AssistedFocus {
    root: Entity,
    canvas: String,
}

#[derive(Resource)]
struct RevealIntent {
    entity: Entity,
    context: Context,
    focused: bool,
}

pub fn reveal_target(world: &mut World) -> Option<Entity> {
    let intent = world.get_resource::<RevealIntent>()?;
    let entity = intent.entity;
    let valid = intent.context.describes(world, entity)
        && owner_context(world, entity, true).is_some()
        && (!intent.focused
            || world.get_resource::<InputFocus>().and_then(InputFocus::get) == Some(entity));
    if valid {
        Some(entity)
    } else {
        world.remove_resource::<RevealIntent>();
        None
    }
}

pub fn revealed(world: &mut World, entity: Entity) {
    if world
        .get_resource::<RevealIntent>()
        .is_some_and(|intent| intent.entity == entity)
    {
        world.remove_resource::<RevealIntent>();
    }
}

pub(super) fn navigated(world: &mut World, entity: Entity) {
    if world.get_resource::<InputFocus>().and_then(InputFocus::get) != Some(entity)
        || owner_context(world, entity, true).is_none()
    {
        return;
    }
    reveal(world, entity);
    request_reveal(world, entity, true);
}

fn request_reveal(world: &mut World, entity: Entity, focused: bool) {
    if world
        .get_resource::<crate::host::magnification::Snapshot>()
        .is_some_and(|snapshot| snapshot.available)
        && let Some(context) = owner_context(world, entity, true)
    { world.insert_resource(RevealIntent { entity, context, focused }); }
}

pub fn owns_keyboard(world: &World) -> bool {
    let Some(owner) = world.get_resource::<AssistedFocus>() else {
        return false;
    };
    let Some(entity) = world.get_resource::<InputFocus>().and_then(InputFocus::get) else {
        return false;
    };
    owner_context(world, entity, true).is_some_and(|context| {
        context.root == owner.root
            && context.canvas == owner.canvas
            && context.surface.is_none()
            && world
                .get::<Workspaces>(context.root)
                .is_some_and(|spaces| spaces.active == context.workspace)
    }) && (world.get::<ActionButton>(entity).is_some()
        || world.get::<EditableText>(entity).is_some())
}

pub fn context(world: &World, entity: Entity, interactive: bool) -> Option<Context> {
    scope(world, entity, interactive, true)
}

pub fn owner_context(world: &World, entity: Entity, interactive: bool) -> Option<Context> {
    scope(world, entity, interactive, false)
}

pub struct PointerContext {
    pub owner: Option<Context>,
    pub lineage: Vec<Entity>,
    pub actions: Vec<(Entity, Entity, u32)>,
    pub eligible: bool,
    next: Option<Entity>,
    visible: bool,
}

impl PointerContext {
    fn visit(&mut self, world: &World, entity: Entity) {
        self.eligible &= interactive_node(world, entity);
        self.lineage.push(entity);
        if let Some(action) = world
            .get_entity(entity)
            .ok()
            .and_then(|entity| entity.get_ref::<ActionButton>())
        {
            self.actions
                .push((entity, action.target, action.last_changed().get()));
        }
        self.next = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
}

pub fn pointer_context(world: &World, entity: Entity) -> Option<PointerContext> {
    let mut path = PointerContext {
        owner: None,
        lineage: Vec::new(),
        actions: Vec::new(),
        eligible: true,
        next: Some(entity),
        visible: true,
    };
    let owner = scope_with_path(world, entity, false, false, Some(&mut path));
    path.owner = owner;
    if !path.visible {
        return None;
    }
    for _ in path.lineage.len()..crate::native::layout::MAX_DEPTH {
        let Some(current) = path.next else {
            break;
        };
        if !visible_node(world, current) {
            return None;
        }
        path.visit(world, current);
    }
    path.eligible &= path.next.is_none();
    Some(path)
}

fn visible_node(world: &World, entity: Entity) -> bool {
    world
        .get::<bevy::ecs::entity_disabling::Disabled>(entity)
        .is_none()
        && world
            .get::<Node>(entity)
            .is_none_or(|node| node.display != Display::None)
        && world
            .get::<Visibility>(entity)
            .is_none_or(|visible| *visible != Visibility::Hidden)
        && world
            .get::<InheritedVisibility>(entity)
            .is_none_or(|visible| visible.get())
}

fn interactive_node(world: &World, entity: Entity) -> bool {
    world.get::<bevy::ui::InteractionDisabled>(entity).is_none()
        && world
            .get::<AccessibilityNode>(entity)
            .is_none_or(|node| !node.is_disabled())
}

fn scope(world: &World, entity: Entity, interactive: bool, capture_text: bool) -> Option<Context> {
    scope_with_path(world, entity, interactive, capture_text, None)
}

fn scope_with_path(
    world: &World,
    entity: Entity,
    interactive: bool,
    capture_text: bool,
    mut path: Option<&mut PointerContext>,
) -> Option<Context> {
    let activity = if let Some(activity) = world.get_resource::<crate::host::input::InputActivity>()
    {
        let revision = activity.revision();
        if revision & 1 == 0 || activity.frame_revision() != revision {
            return None;
        }
        Some(revision)
    } else {
        None
    };
    if world
        .get_resource::<crate::host::control::Status>()
        .is_some_and(|status| status.available && (status.locked || !status.active))
    {
        return None;
    }
    let modal = world
        .get_resource::<shortcuts::KeyboardModalOwner>()
        .and_then(|owner| owner.0);
    let mut current = Some(entity);
    let mut owner = None;
    let mut placement = None;
    let mut surface = None;
    let mut record = None;
    let mut inside_modal = modal.is_none();
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let id = current?;
        if world.get_entity(id).is_err() {
            return None;
        }
        if !visible_node(world, id) {
            if let Some(path) = path.as_deref_mut() {
                path.visible = false;
            }
            return None;
        }
        if interactive && !interactive_node(world, id) {
            return None;
        }
        if let Some(path) = path.as_deref_mut() {
            path.visit(world, id);
        }
        inside_modal |= modal == Some(id);
        if owner.is_none() && world.get::<crate::native::canvas::CanvasItem>(id).is_some() {
            owner = Some(id);
            placement = world.get::<LayoutBox>(id).map(|layout| layout.id);
            surface = world
                .get::<ApplicationWindow>(id)
                .map(|window| window.surface);
        }
        if record.is_none() {
            record = world
                .get::<crate::native::protein_area::RecordBinding>(id)
                .map(|binding| (binding.area, binding.uid.clone()))
                .or_else(|| {
                    world
                        .get::<crate::native::full_record::window::Metadata>(id)
                        .map(|metadata| (id, metadata.uid.clone()))
                });
        }
        if let Some(spaces) = world.get::<Workspaces>(id) {
            if !inside_modal || owner.is_some_and(|owner| !desktop::visible_item(world, id, owner))
            {
                return None;
            }
            let workspace = owner
                .and_then(|owner| world.get::<WorkspaceMember>(owner))
                .map_or(spaces.active, |member| member.0);
            let views = world
                .get_resource::<crate::native::output_views::OutputViews>()
                .filter(|views| views.applies_to(id));
            let output = if let Some(views) = views {
                Some(
                    views
                        .outputs
                        .iter()
                        .find(|output| output.workspace == workspace)?
                        .id,
                )
            } else {
                if workspace != spaces.active {
                    return None;
                }
                world
                    .get::<desktop::HostViewport>(id)
                    .map(|viewport| viewport.0.id)
            };
            let control = if let Some(path) = path.as_deref() {
                path.actions
                    .first()
                    .filter(|(id, _, _)| *id == entity)
                    .map(|(_, target, tick)| (*target, *tick))
            } else {
                world
                    .get_entity(entity)
                    .ok()
                    .and_then(|entity| entity.get_ref::<ActionButton>())
                    .map(|button| (button.target, button.last_changed().get()))
            };
            let text = if capture_text {
                world.get::<EditableText>(entity).map(|input| {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    input.value().to_cow().hash(&mut hash);
                    input.max_characters.hash(&mut hash);
                    input.allow_newlines.hash(&mut hash);
                    hash.finish()
                })
            } else {
                None
            };
            let mappings = world.get_resource::<crate::host::mapping::Snapshot>();
            let mapping =
                output.and_then(|output| mappings.and_then(|mappings| mappings.get(output)));
            let text_generation = capture_text
                .then(|| world.get::<EditableText>(entity))
                .flatten()
                .filter(|input| input.editor.try_layout().is_some())
                .map(|input| input.editor.generation());
            return Some(Context {
                root: id,
                canvas: spaces.canvas_id.clone(),
                workspace,
                output,
                modal,
                owner,
                placement,
                surface,
                record,
                control,
                text,
                text_generation,
                protected: protected_text_input(world, entity),
                activity,
                display_revision: if mapping.is_some() {
                    0
                } else {
                    world
                        .get_resource::<crate::host::display::DisplaySnapshot>()
                        .map_or(0, |display| display.revision)
                },
                mapping_revision: mapping
                    .map(crate::host::mapping::Mapping::revision)
                    .unwrap_or_else(|| {
                        mappings.map_or(0, crate::host::mapping::Snapshot::revision)
                    }),
            });
        }
        current = path.as_deref().map_or_else(
            || world.get::<ChildOf>(id).map(ChildOf::parent),
            |path| path.next,
        );
    }
    None
}

fn editable(world: &World, entity: Entity) -> bool {
    world.get::<EditableText>(entity).is_some()
        && world
            .get::<AccessibilityNode>(entity)
            .is_none_or(|node| !node.is_read_only())
        && (world
            .get::<crate::native::record_binding::TextBinding>(entity)
            .is_none()
            || crate::native::record_binding::can_edit(world, entity))
}

pub fn supported_actions(world: &World, entity: Entity) -> Vec<Action> {
    if owner_context(world, entity, true).is_none() {
        return Vec::new();
    }
    control_actions(world, entity)
}

fn control_actions(world: &World, entity: Entity) -> Vec<Action> {
    let mut actions = vec![Action::ScrollIntoView];
    if world
        .get::<bevy::input_focus::tab_navigation::TabIndex>(entity)
        .is_some()
        || world.get::<EditableText>(entity).is_some()
        || world.get::<ActionButton>(entity).is_some()
    {
        actions.extend([Action::Focus, Action::Blur]);
    }
    if world.get::<ActionButton>(entity).is_some()
        || world.get::<bevy::ui_widgets::Button>(entity).is_some()
    {
        actions.push(Action::Click);
    }
    if editable(world, entity) {
        actions.extend([Action::SetValue, Action::ReplaceSelectedText]);
        if !protected_text_input(world, entity) { actions.push(Action::SetTextSelection); }
    }
    if world.get::<ScrollPosition>(entity).is_some() { actions.extend([Action::ScrollUp, Action::ScrollDown, Action::ScrollLeft, Action::ScrollRight]); }
    actions
}

fn reveal(world: &mut World, entity: Entity) {
    let mut current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(parent) = current else { break; };
        current = world.get::<ChildOf>(parent).map(ChildOf::parent);
        if world.get::<ScrollPosition>(parent).is_some() { crate::native::scroll_sand::reveal(world, parent, entity, 8.0); }
    }
}

fn focus(world: &mut World, entity: Entity) {
    reveal(world, entity);
    world.init_resource::<InputFocus>();
    world
        .resource_mut::<InputFocus>()
        .set(entity, FocusCause::Navigated);
    request_reveal(world, entity, true);
    if let Some(context) = owner_context(world, entity, true) {
        world.insert_resource(AssistedFocus {
            root: context.root,
            canvas: context.canvas,
        });
    }
}

pub fn perform(
    world: &mut World,
    entity: Entity,
    captured: &Context,
    request: &ActionRequest,
) -> Result<(), &'static str> {
    if context(world, entity, true).as_ref() != Some(captured) {
        return Err("The original accessibility control is hidden, locked or changed.");
    }
    if !control_actions(world, entity).contains(&request.action) {
        return Err("This control does not support the requested accessibility action.");
    }
    match request.action {
        Action::Focus => focus(world, entity),
        Action::Blur => {
            if let Some(mut input) = world.get_resource_mut::<InputFocus>()
                && input.get() == Some(entity)
            {
                input.clear();
                world.remove_resource::<AssistedFocus>();
            }
        }
        Action::Click => {
            focus(world, entity);
            world.trigger(bevy::ui_widgets::Activate { entity });
        }
        Action::ScrollIntoView => { reveal(world, entity); request_reveal(world, entity, false); }
        Action::SetValue | Action::ReplaceSelectedText => {
            let Some(ActionData::Value(value)) = &request.data else { return Err("The accessibility text value is missing."); };
            if value.len() > MAX_ACTION_BYTES { return Err("The accessibility text value exceeds the supported action size."); }
            let input = world.get::<EditableText>(entity).ok_or("The original text field is unavailable.")?;
            if !input.pending_edits.is_empty() || input.pending_paste.is_some() || input.is_composing() { return Err("Finish the current text edit or composition before replacing this value."); }
            if value.chars().any(|character| character.is_control() && !(input.allow_newlines && matches!(character, '\n' | '\t')))
                || request.action == Action::SetValue && input.max_characters.is_some_and(|max| value.chars().count() > max)
            { return Err("The accessibility value does not fit this field."); }
            focus(world, entity);
            let mut input = world.get_mut::<EditableText>(entity).unwrap();
            if request.action == Action::SetValue { input.queue_edit(TextEdit::SelectAll); }
            crate::controls::editing::commit(&mut input, value);
        }
        Action::SetTextSelection => {
            let Some(ActionData::SetTextSelection(selection)) = &request.data else { return Err("The accessibility selection is missing."); };
            if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>() { return Err("The text layout is not ready."); }
            if world.get::<EditableText>(entity).is_none_or(|input| !input.pending_edits.is_empty() || input.pending_paste.is_some() || input.is_composing()) {
                return Err("The original text layout changed before this selection.");
            }
            focus(world, entity);
            world.resource_scope(|world, mut fonts: Mut<FontCx>| {
                world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
                    world.get_mut::<EditableText>(entity).unwrap().editor.driver(&mut fonts.context, &mut layout.0).select_from_accesskit(selection);
                });
            });
        }
        Action::ScrollUp | Action::ScrollDown | Action::ScrollLeft | Action::ScrollRight => {
            let node = world.get::<ComputedNode>(entity).ok_or("The scroll viewport is unavailable.")?;
            let scale = node.inverse_scale_factor();
            let maximum = (node.content_size - node.size() + node.scrollbar_size).max(Vec2::ZERO) * scale;
            let page = node.size() * scale * 0.9;
            if !scale.is_finite() || scale <= 0.0 || !maximum.is_finite() { return Err("The scroll viewport has invalid geometry."); }
            let delta = match request.action { Action::ScrollUp => Vec2::new(0.0, -page.y), Action::ScrollDown => Vec2::new(0.0, page.y),
                Action::ScrollLeft => Vec2::new(-page.x, 0.0), _ => Vec2::new(page.x, 0.0) };
            let mut scroll = world.get_mut::<ScrollPosition>(entity).ok_or("This control no longer scrolls.")?;
            let next = (scroll.0 + delta).clamp(Vec2::ZERO, maximum);
            if scroll.0 != next { scroll.0 = next; }
        }
        _ => return Err("This accessibility action is not supported by the native control."),
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    Ok(())
}

#[allow(
    clippy::unwrap_used,
    reason = "Laboratory accessibility fixtures require their expected captured actions and controls"
)]
pub(super) mod tests;
