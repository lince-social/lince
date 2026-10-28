use super::*;
use crate::native::actions::Action;

const ACTIONS: [(DesktopAction, &str); 4] = [
    (DesktopAction::MarkScratch, "Mark as scratch"),
    (DesktopAction::ShowScratch, "Show this item here"),
    (DesktopAction::HideScratch, "Hide this scratch item"),
    (DesktopAction::UnmarkScratch, "Unmark and restore home"),
];

#[derive(Component)]
struct Panel {
    target: Target,
    destination: Destination,
    status: Entity,
    summary: Entity,
    buttons: Vec<(DesktopAction, Entity)>,
    message: String,
}

#[derive(Clone, Copy)]
struct Change(DesktopAction);

fn panel(world: &mut World, parent: Entity, target: Target) {
    let Ok(destination) = destination(world, target.root) else { return; };
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() },
        ChildOf(parent), Name::new("item.scratch-properties"))).id();
    let summary = crate::native::edit_mode::label(world, page, "Scratch item", 16.0);
    world.get_mut::<Node>(summary).unwrap().width = percent(100);
    world.entity_mut(summary).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    let buttons = ACTIONS.into_iter().map(|(action, caption)| (action, input_settings::button(world, page, page, caption, Change(action)).0)).collect();
    let status = input_settings::feedback(world, page);
    world.entity_mut(page).insert(Panel { target, destination, status, summary, buttons, message: String::new() });
}

pub(crate) fn item_panel(world: &mut World, root: Entity, parent: Entity, entity: Entity) {
    if !candidate(world, entity) || world.get::<TransientParent>(entity).is_some() { return; }
    if world.get::<LayoutBox>(entity).is_none() && let Some(item) = world.get::<CanvasItem>(entity) {
        let layout = LayoutBox::new(item.size); world.entity_mut(entity).insert(layout);
    }
    if let Ok(target) = capture(world, root, entity) { panel(world, parent, target); }
}

pub(crate) fn window_panel(world: &mut World, parent: Entity, binding: &RecordBinding) {
    let Some(metadata) = world.get::<Metadata>(binding.area).filter(|metadata| metadata.uid == binding.uid).cloned() else { return; };
    let Some(root) = root(world, binding.area) else { return; };
    let target = world.get::<Children>(root).and_then(|children| children.iter().find(|entity|
        world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == metadata.placement) && world.get::<ApplicationWindow>(*entity).is_some()));
    if let Some(mut target) = target.and_then(|entity| capture(world, root, entity).ok()) {
        target.inspector = Some((binding.area, binding.uid.clone())); panel(world, parent, target);
    } else { crate::native::edit_mode::label(world, parent, "Scratch controls require the original live application window.", 14.0); }
}

fn unavailable(world: &World, state: &Panel, action: DesktopAction) -> Option<String> {
    if let Err(reason) = guard(world, None).and_then(|_| current_destination(world, &state.destination)) { return Some(reason); }
    let model = match model(world, &state.target) { Ok(model) => model, Err(reason) => return Some(reason) };
    if action == DesktopAction::MarkScratch {
        if model.compact_group(state.target.placement).is_some() { return Some("Remove this item from its compact group before marking it as scratch.".into()); }
        if model.hidden(state.target.placement) || state.target.workspace != state.destination.workspace { return Some("Mark requires the original visible item in the current workspace.".into()); }
    } else if model.scratch(state.target.placement).is_none() { return Some("The original item is not marked as scratch.".into()); }
    None
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut state) = world.get_entity_mut(owner).ok().and_then(|mut entity| entity.take::<Panel>()) else { return; };
        if !crate::native::shortcuts::input_eligible(world, state.target.root, &state.target.canvas, owner) {
            if let Ok(mut entity) = world.get_entity_mut(owner) { entity.insert(state); } return;
        }
        let result = if let Some(reason) = unavailable(world, &state, self.0) { Err(reason) }
            else { apply_target(world, &state.target, self.0, &state.destination, None) };
        match result {
            Ok(()) => {
                if let Ok(mut target) = capture(world, state.target.root, state.target.entity) {
                    target.inspector = state.target.inspector.clone(); state.target = target;
                }
                state.message = if state.target.surface.is_some() { "Applied for this live window." } else { "Applied to this item." }.into();
            }
            Err(reason) => state.message = reason,
        }
        if let Ok(mut entity) = world.get_entity_mut(owner) { entity.insert(state); }
    }
}

pub(super) fn refresh(world: &mut World) {
    let pages = world.query_filtered::<Entity, With<Panel>>().iter(world).collect::<Vec<_>>();
    for page in pages {
        let state = world.get::<Panel>(page).unwrap();
        let source = model(world, &state.target);
        let scratch = source.as_ref().ok().and_then(|model| model.scratch(state.target.placement));
        let summary = if source.is_err() { "Original item unavailable · scratch" } else if scratch.is_some_and(|scratch| scratch.shown()) { "Scratch item · shown" }
            else if scratch.is_some() { "Scratch item · hidden" } else { "Ordinary item · scratch is optional" };
        let message = source.as_ref().err().cloned().unwrap_or_else(|| state.message.clone());
        let fields = (state.summary, state.status);
        let buttons = state.buttons.iter().map(|(action, button)| {
            let show = if *action == DesktopAction::MarkScratch { scratch.is_none() } else { scratch.is_some() };
            (*button, show, unavailable(world, state, *action))
        }).collect::<Vec<_>>();
        input_settings::text(world, fields.0, summary);
        input_settings::text(world, fields.1, message);
        for (button, show, reason) in buttons {
            let display = if show { Display::Flex } else { Display::None };
            if let Some(mut node) = world.get_mut::<Node>(button) && node.display != display { node.display = display; }
            input_settings::disable(world, button, reason.is_some());
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) && node.description() != reason.as_deref() {
                if let Some(reason) = reason { node.set_description(reason); } else { node.clear_description(); }
            }
        }
    }
}
