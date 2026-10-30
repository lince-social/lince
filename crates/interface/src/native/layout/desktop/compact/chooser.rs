use super::*;
use crate::native::{actions::{KeyBinding, KeyBindings, Modifiers}, edit_mode::label, shortcuts};
use bevy::input_focus::{InputFocus, FocusCause, tab_navigation::TabGroup};
use bevy::picking::events::Scroll;

#[derive(Component)]
struct Chooser {
    anchor: Target,
    origin: Option<CompactGroup>,
    choices: Vec<Choice>,
    previous: Option<Entity>,
    confirm: Entity,
    status: Entity,
    message: String,
}

struct Choice {
    target: Target,
    selected: bool,
    button: (Entity, Entity),
    ordinal: usize,
}

#[derive(Clone, Copy)]
enum Edit { Choice(PlacementId), Confirm, Cancel }

pub(super) fn open(world: &mut World, anchor: Target, add: bool) -> Result<(), &'static str> {
    if shortcuts::keyboard_owned(world) { return Err("Finish the current dialog or key capture first."); }
    let origin = model(world, &anchor, false)?.compact_group(anchor.placement).cloned();
    if origin.is_some() != add { return Err("The original compact membership changed."); }
    let choices = candidates(world, &anchor);
    if choices.is_empty() { return Err("There are no other eligible items in this workspace."); }
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let overlay = world.spawn((
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), GlobalZIndex(100), TabGroup::modal(),
        crate::native::inspection::InspectionExcluded, Name::new("park.compact-chooser"), ChildOf(anchor.root),
        KeyBindings(vec![KeyBinding::new(KeyCode::Escape, Modifiers::NONE, crate::native::actions![Edit::Cancel])]),
    )).id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Dialog);
    accessible.set_label(if add { "Add compact members" } else { "Create compact group" });
    world.entity_mut(overlay).insert(bevy::a11y::AccessibilityNode::from(accessible));
    let panel = world.spawn((Node {
        width: px(560), max_width: percent(96), height: percent(90), min_height: px(0),
        flex_direction: FlexDirection::Column, padding: UiRect::all(px(12)), row_gap: px(8), ..default()
    }, crate::native::token_style::background(crate::native::tokens::Token::Surface), ChildOf(overlay))).id();
    wrapped(world, panel, if add { "Add compact members" } else { "Create compact group" }, 22.0);
    wrapped(world, panel, "Select existing items to share one tiled slot. Nothing changes until you confirm. Cancel keeps the current arrangement.", 14.0);
    let list = world.spawn((Node {
        width: percent(100), min_width: px(0), min_height: px(0), flex_direction: FlexDirection::Column,
        flex_grow: 1.0, flex_shrink: 1.0, row_gap: px(6), overflow: Overflow::scroll_y(), ..default()
    }, ScrollPosition::default(), crate::native::edit_mode::FocusScroll, ChildOf(panel))).id();
    world.entity_mut(list).observe(|mut event: On<Pointer<Scroll>>| event.propagate(false));
    let choices = choices.into_iter().enumerate().map(|(ordinal, target)| {
        let caption = caption(world, &target, ordinal, false);
        let button = input_settings::button(world, list, overlay, &caption, Edit::Choice(target.placement));
        world.entity_mut(button.0).insert((Name::new(format!("compact.choice.{}", target.entity.to_bits())), crate::native::capture::render::TargetSource(target.entity)));
        let name = name(world, &target, ordinal);
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button.0) { node.set_role(accesskit::Role::CheckBox); node.set_label(name); node.set_toggled(accesskit::Toggled::False); }
        Choice { target, selected: false, button, ordinal }
    }).collect();
    let status = input_settings::feedback(world, panel);
    let confirm = input_settings::button(world, panel, overlay, if add { "Add selected items" } else { "Create selected group" }, Edit::Confirm).0;
    let cancel = input_settings::button(world, panel, overlay, "Cancel compact changes", Edit::Cancel).0;
    world.entity_mut(overlay).insert(Chooser { anchor, origin, choices, previous, confirm, status, message: String::new() });
    shortcuts::own_keyboard(world, overlay);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(cancel, FocusCause::Navigated); }
    input_settings::disable(world, confirm, true);
    Ok(())
}

fn wrapped(world: &mut World, parent: Entity, text: &str, size: f32) {
    let entity = label(world, parent, text, size);
    world.get_mut::<Node>(entity).unwrap().width = percent(100);
    world.get_mut::<Node>(entity).unwrap().min_width = px(0);
    world.entity_mut(entity).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
}

fn caption(world: &World, target: &Target, ordinal: usize, selected: bool) -> String {
    format!("{} {}", if selected { "✓" } else { "□" }, name(world, target, ordinal))
}

fn name(world: &World, target: &Target, ordinal: usize) -> String {
    format!("{} · {}", ordinal + 1, overview::item_name(world, target.entity))
}

fn scope(world: &World, state: &Chooser, owner: Entity) -> bool {
    world.get::<Workspaces>(state.anchor.root).is_some_and(|spaces| spaces.canvas_id == state.anchor.canvas && spaces.active == state.anchor.workspace)
        && world.get::<HostViewport>(state.anchor.root).map(|viewport| viewport.0.id) == state.anchor.output
        && [state.anchor.root, owner].into_iter().all(|entity| world.get_entity(entity).is_ok()
            && world.get::<Node>(entity).is_none_or(|node| node.display != Display::None)
            && world.get::<Visibility>(entity).is_none_or(|visibility| *visibility != Visibility::Hidden)
            && world.get::<bevy::ui::InteractionDisabled>(entity).is_none())
}

fn selected(world: &World, state: &Chooser) -> Result<Vec<PlacementId>, &'static str> {
    if busy(world) { return Err("Finish the current drag before confirming compact changes."); }
    let current = model(world, &state.anchor, false)?;
    if current.compact_group(state.anchor.placement) != state.origin.as_ref() { return Err("The compact membership changed. Cancel and reopen to review it."); }
    if current.mode() != WorkspaceLayout::Tiled || current.fullscreen().is_some() { return Err("Return to tiled presentation without fullscreen before confirming."); }
    let selected = state.choices.iter().filter(|choice| choice.selected).map(|choice| {
        model(world, &choice.target, false)?;
        if !groupable(world, choice.target.root, choice.target.entity) { return Err("A selected item changed. Review or cancel this choice."); }
        Ok(choice.target.placement)
    }).collect::<Result<Vec<_>, _>>()?;
    if selected.is_empty() { return Err("Select at least one other item."); }
    Ok(selected)
}

fn proposed(world: &World, state: &Chooser) -> Result<Model, &'static str> {
    let selected = selected(world, state)?;
    let current = model(world, &state.anchor, false)?;
    let mut candidate = current.clone();
    if state.origin.is_some() { candidate.add_compact_members(state.anchor.placement, &selected) }
    else { candidate.create_compact_group(state.anchor.placement, &selected) }
        .map_err(|_| "The selected members cannot form this compact group.")?;
    Ok(candidate)
}

fn close(world: &mut World, owner: Entity, state: Chooser) {
    let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
    if owns { shortcuts::release_keyboard(world, owner); }
    if let Ok(entity) = world.get_entity_mut(owner) { entity.despawn(); }
    if owns {
        let previous = state.previous.filter(|entity| shortcuts::input_eligible(world, state.anchor.root, &state.anchor.canvas, *entity));
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            if let Some(previous) = previous { focus.set(previous, FocusCause::Navigated); } else { focus.clear(); }
        }
    }
}

impl Action for Edit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut state) = world.get_entity_mut(owner).ok().and_then(|mut entity| entity.take::<Chooser>()) else { return; };
        let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
        if !owns || !scope(world, &state, owner) || !shortcuts::input_eligible(world, state.anchor.root, &state.anchor.canvas, owner) {
            close(world, owner, state);
            return;
        }
        match *self {
            Self::Cancel => { close(world, owner, state); return; }
            Self::Choice(id) => {
                if let Some(choice) = state.choices.iter_mut().find(|choice| choice.target.placement == id)
                    && model(world, &choice.target, false).is_ok() && groupable(world, choice.target.root, choice.target.entity)
                { choice.selected = !choice.selected; state.message.clear(); }
            }
            Self::Confirm => match proposed(world, &state) {
                Ok(candidate) => {
                    let anchor = state.anchor.clone();
                    close(world, owner, state);
                    if let Err(reason) = commit(world, &anchor, candidate, Some(anchor.placement)) { crate::native::notifications::report(world, "Compact group", reason); }
                    return;
                }
                Err(reason) => state.message = reason.into(),
            },
        }
        world.entity_mut(owner).insert(state);
    }
}

pub(super) fn refresh(world: &mut World) {
    let owners: Vec<_> = world.query_filtered::<Entity, With<Chooser>>().iter(world).collect();
    for owner in owners {
        let Some(state) = world.get::<Chooser>(owner) else { continue; };
        let owns = world.get_resource::<shortcuts::KeyboardModalOwner>().is_some_and(|modal| modal.0 == Some(owner));
        if !owns || !scope(world, state, owner) {
            let state = world.entity_mut(owner).take::<Chooser>().unwrap(); close(world, owner, state); continue;
        }
        let result = selected(world, state);
        let confirm = state.confirm;
        let status = state.status;
        let message = if state.message.is_empty() { result.as_ref().err().copied().unwrap_or_default().to_owned() } else { state.message.clone() };
        let rows = state.choices.iter().map(|choice| (choice.button, caption(world, &choice.target, choice.ordinal, choice.selected), name(world, &choice.target, choice.ordinal), choice.selected,
            model(world, &choice.target, false).is_err() || !groupable(world, choice.target.root, choice.target.entity))).collect::<Vec<_>>();
        input_settings::disable(world, confirm, result.is_err());
        input_settings::text(world, status, message);
        for ((button, label), caption, name, selected, disabled) in rows {
            input_settings::text(world, label, &caption);
            input_settings::disable(world, button, disabled);
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
                let toggled = if selected { accesskit::Toggled::True } else { accesskit::Toggled::False };
                if node.label() != Some(name.as_str()) { node.set_label(name); }
                if node.toggled() != Some(toggled) { node.set_toggled(toggled); }
            }
        }
    }
}

#[cfg(test)]
mod tests;
