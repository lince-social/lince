use super::*;
use crate::native::{actions::{Action, KeyBinding, KeyBindings, Modifiers}, edit_mode::label, shortcuts};
use bevy::{input_focus::tab_navigation::TabGroup, picking::events::Scroll};

#[derive(Component)]
struct Chooser {
    destination: Destination,
    choices: Vec<(Target, (Entity, Entity))>,
    chosen: Option<usize>,
    previous: Option<Entity>,
    confirm: Entity,
    status: Entity,
    message: String,
}

#[derive(Clone, Copy)]
enum Edit { Choose(usize), Confirm, Cancel }

pub(super) fn open(world: &mut World, destination: Destination, choices: Vec<Target>) -> Result<(), String> {
    if choices.is_empty() { return Err("No existing items have been marked as scratch. Mark an item first; this command never launches a program.".into()); }
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let overlay = world.spawn((Node {
        position_type: PositionType::Absolute, width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default()
    }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), GlobalZIndex(100), TabGroup::modal(),
        crate::native::inspection::InspectionExcluded, Name::new("park.scratch-chooser"), ChildOf(destination.root),
        KeyBindings(vec![KeyBinding::new(KeyCode::Escape, Modifiers::NONE, crate::native::actions![Edit::Cancel])]))).id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Dialog);
    accessible.set_label("Show an existing scratch item");
    world.entity_mut(overlay).insert(bevy::a11y::AccessibilityNode::from(accessible));
    let panel = world.spawn((Node { width: px(560), max_width: percent(96), height: percent(90), min_height: px(0),
        flex_direction: FlexDirection::Column, padding: UiRect::all(px(12)), row_gap: px(8), ..default()
    }, crate::native::token_style::background(crate::native::tokens::Token::Surface), ChildOf(overlay))).id();
    wrapped(world, panel, "Show a scratch item here", 22.0);
    wrapped(world, panel, "Choose an existing item. Show moves that same item to this workspace as a floating item. Cancel keeps every item where it is.", 14.0);
    let list = world.spawn((Node { width: percent(100), min_width: px(0), min_height: px(0), flex_direction: FlexDirection::Column,
        flex_grow: 1.0, flex_shrink: 1.0, row_gap: px(6), overflow: Overflow::scroll_y(), ..default()
    }, ScrollPosition::default(), crate::native::edit_mode::FocusScroll, ChildOf(panel))).id();
    world.entity_mut(list).observe(|mut event: On<Pointer<Scroll>>| event.propagate(false));
    let choices = choices.into_iter().enumerate().map(|(index, target)| {
        let name = caption(world, &target, index);
        let button = input_settings::button(world, list, overlay, &name, Edit::Choose(index));
        world.entity_mut(button.0).insert((Name::new(format!("scratch.choice.{index}")), crate::native::topology::presentation::VisualOwner(target.entity)));
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button.0) { node.set_role(accesskit::Role::RadioButton); node.set_toggled(accesskit::Toggled::False); }
        (target, button)
    }).collect();
    let status = input_settings::feedback(world, panel);
    let confirm = input_settings::button(world, panel, overlay, "Show selected item here", Edit::Confirm).0;
    let cancel = input_settings::button(world, panel, overlay, "Cancel scratch selection", Edit::Cancel).0;
    world.entity_mut(overlay).insert(Chooser { destination, choices, chosen: None, previous, confirm, status, message: String::new() });
    input_settings::disable(world, confirm, true);
    shortcuts::own_keyboard(world, overlay);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(cancel, FocusCause::Navigated); }
    Ok(())
}

fn wrapped(world: &mut World, parent: Entity, text: &str, size: f32) {
    let entity = label(world, parent, text, size);
    world.get_mut::<Node>(entity).unwrap().width = percent(100);
    world.get_mut::<Node>(entity).unwrap().min_width = px(0);
    world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
}

fn caption(world: &World, target: &Target, index: usize) -> String {
    let workspace = world.get::<Workspaces>(target.root).and_then(|spaces| spaces.entries.iter().find(|space| space.id == target.workspace))
        .map(|space| space.name.as_str()).unwrap_or("Unavailable workspace");
    let state = model(world, target).ok().and_then(|model| model.scratch(target.placement)).map(|scratch| if scratch.shown() { "shown" } else { "hidden" }).unwrap_or("unavailable");
    format!("{} · {} · {workspace} · {state}", index + 1, overview::item_name(world, target.entity))
}

fn scope(world: &World, owner: Entity, state: &Chooser) -> bool {
    current_destination(world, &state.destination).is_ok()
        && !world.get_resource::<crate::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active))
        && [owner, state.destination.root].into_iter().all(|entity| world.get_entity(entity).is_ok()
            && world.get::<Node>(entity).is_none_or(|node| node.display != Display::None)
            && world.get::<Visibility>(entity).is_none_or(|visibility| *visibility != Visibility::Hidden)
            && world.get::<bevy::ui::InteractionDisabled>(entity).is_none())
}

fn chosen<'a>(world: &World, state: &'a Chooser, owner: Entity) -> Result<&'a Target, String> {
    guard(world, Some(owner))?;
    current_destination(world, &state.destination)?;
    let target = &state.choices.get(state.chosen.ok_or("Choose an existing scratch item first.")?).ok_or("The selected item is unavailable.")?.0;
    if model(world, target)?.scratch(target.placement).is_none() { return Err("The selected item is no longer marked. Review or cancel this selection.".into()); }
    Ok(target)
}

fn close(world: &mut World, owner: Entity, state: Chooser, restore: bool) {
    let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
    if owns { shortcuts::release_keyboard(world, owner); }
    if let Ok(entity) = world.get_entity_mut(owner) { entity.despawn(); }
    if owns && restore {
        let previous = state.previous.filter(|entity| shortcuts::input_eligible(world, state.destination.root, &state.destination.canvas, *entity));
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            if let Some(previous) = previous { focus.set(previous, FocusCause::Navigated); } else { focus.clear(); }
        }
    }
}

impl Action for Edit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut state) = world.get_entity_mut(owner).ok().and_then(|mut entity| entity.take::<Chooser>()) else { return; };
        let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
        if !owns || !scope(world, owner, &state) || !shortcuts::input_eligible(world, state.destination.root, &state.destination.canvas, owner) {
            close(world, owner, state, true); return;
        }
        match *self {
            Self::Cancel => { close(world, owner, state, true); return; }
            Self::Choose(index) => {
                if state.choices.get(index).is_some_and(|(target, _)| model(world, target).is_ok_and(|model| model.scratch(target.placement).is_some())) {
                    state.chosen = Some(index); state.message.clear();
                }
            }
            Self::Confirm => {
                let result = chosen(world, &state, owner).cloned().and_then(|target| {
                    apply_target(world, &target, DesktopAction::ShowScratch, &state.destination, Some(owner))?;
                    Ok(target.entity)
                });
                match result {
                    Ok(target) => { close(world, owner, state, false); focus::keyboard_focus(world, target); return; }
                    Err(reason) => state.message = reason,
                }
            }
        }
        if let Ok(mut entity) = world.get_entity_mut(owner) { entity.insert(state); }
    }
}

pub(super) fn refresh(world: &mut World) {
    let owners = world.query_filtered::<Entity, With<Chooser>>().iter(world).collect::<Vec<_>>();
    for owner in owners {
        let Some(state) = world.get::<Chooser>(owner) else { continue; };
        let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
        if !owns || !scope(world, owner, state) {
            let state = world.entity_mut(owner).take::<Chooser>().unwrap(); close(world, owner, state, true); continue;
        }
        let result = chosen(world, state, owner).map(|_| ());
        let message = if state.message.is_empty() { result.as_ref().err().map(String::as_str).unwrap_or("").to_owned() } else { state.message.clone() };
        let fields = (state.confirm, state.status);
        let rows = state.choices.iter().enumerate().map(|(index, (target, button))| {
            (button.0, button.1, caption(world, target, index), model(world, target).is_ok_and(|model| model.scratch(target.placement).is_some()), state.chosen == Some(index))
        }).collect::<Vec<_>>();
        input_settings::text(world, fields.1, message);
        input_settings::disable(world, fields.0, result.is_err());
        for (button, label, text, valid, selected) in rows {
            input_settings::text(world, label, &text);
            input_settings::disable(world, button, !valid);
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
                if node.label() != Some(text.as_str()) { node.set_label(text); }
                let toggled = if selected { accesskit::Toggled::True } else { accesskit::Toggled::False };
                if node.toggled() != Some(toggled) { node.set_toggled(toggled); }
            }
        }
    }
}

pub(super) fn cancel(world: &mut World) {
    let owners = world.query_filtered::<Entity, With<Chooser>>().iter(world).collect::<Vec<_>>();
    for owner in owners {
        if let Some(state) = world.entity_mut(owner).take::<Chooser>() { close(world, owner, state, false); }
    }
}

#[cfg(test)]
mod tests;
