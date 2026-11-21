// non-lean-yet
// Scope: mod presentation, struct Page, enum Change, fn install.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 87, 119, 129,
//   131, 159, 163, 194, 199, 206, 207, 209, 219.
// - [review: §3 Ownership] Check that these inputs can be borrowed without changing ownership
//   semantics. Sites set_value:value → &str (72).
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 102, 103,
//   116, 117, 157, 219.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   184, 209.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (80, 85, 141, 142, 145, 158, 159, 162, 171, 172, 176, 189, 194, 198, 199, 204,
//   209, 211, 216, 220, 221).

use super::{HostRequests, input_settings::{button, disable, feedback, field, next_request, text}};
use crate::{host::{HostRequest, display::DisplaySnapshot, input::{AbsoluteDevice, AbsolutePreference, AbsolutePreferences, AbsoluteRequest, AbsoluteReload, AbsoluteSnapshot, InputOutcome}}, native::{actions::Action, edit_mode::label}};
use bevy::{input_focus::{FocusCause, InputFocus}, prelude::*, text::{EditableText, TextEdit}};
use std::collections::BTreeMap;

mod presentation;
use presentation::refresh;

#[derive(Component)]
struct Page {
    body: Entity,
    toggle: (Entity, Entity),
    list: Entity,
    rows: Vec<Entity>,
    listed: Vec<(String, u64, String)>,
    selected: Option<(String, u64)>,
    output: Option<String>,
    output_button: (Entity, Entity),
    calibration: Entity,
    buttons: Entity,
    button_list: Entity,
    button_rows: BTreeMap<u32, (Entity, Entity)>,
    advanced: Entity,
    reset: Entity,
    summary: Entity,
    status: Entity,
    save: Entity,
    cancel: Entity,
    reload: Entity,
    expanded: bool,
    origin: Option<(u64, AbsolutePreferences)>,
    pending: Option<(u64, AbsolutePreferences)>,
    pending_reload: bool,
}

#[derive(Clone)]
enum Change { Toggle, Select(String, u64), Output, ResetCalibration, MapButton(u32), Advanced, Save, Cancel, Reload }

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, refresh.after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() }, ChildOf(parent))).id();
    label(world, page, "Touch and tablet", 18.0);
    let toggle = button(world, page, page, "Edit touch/tablet settings", Change::Toggle);
    let body = world.spawn((Node { width: percent(100), min_width: px(0), display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() }, ChildOf(page))).id();
    let summary = feedback(world, body);
    let list = world.spawn((Node { width: percent(100), min_width: px(0), max_height: px(180), flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }, ChildOf(body))).id();
    crate::native::scroll_sand::attach(world, list);
    let output_button = button(world, body, page, "Output: Automatic", Change::Output);
    let calibration = field(world, body, "Calibration: a b c d e f (empty uses device default)", 192);
    let reset = button(world, body, page, "Reset calibration to device default", Change::ResetCalibration).0;
    label(world, body, "Tool button mappings", 15.0);
    let button_list = world.spawn((Node { width: percent(100), min_width: px(0), max_height: px(180), flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }, ChildOf(body))).id();
    crate::native::scroll_sand::attach(world, button_list);
    button(world, body, page, "Show / hide advanced button codes", Change::Advanced);
    let advanced = world.spawn((Node { width: percent(100), min_width: px(0), display: Display::None, flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }, ChildOf(body))).id();
    let buttons = field(world, advanced, "Advanced mappings: source=target, separated by spaces", 256);
    let status = feedback(world, body);
    let save = button(world, body, page, "Save device settings", Change::Save).0;
    let cancel = button(world, body, page, "Cancel draft / restore active settings", Change::Cancel).0;
    let reload = button(world, body, page, "Reload saved file / discard draft", Change::Reload).0;
    world.entity_mut(page).insert(Page { body, toggle, list, rows: Vec::new(), listed: Vec::new(), selected: None, output: None, output_button, calibration, buttons, button_list, button_rows: BTreeMap::new(), advanced, reset, summary, status, save, cancel, reload, expanded: false, origin: None, pending: None, pending_reload: false });
}

fn device<'a>(state: &Page, snapshot: &'a AbsoluteSnapshot) -> Option<&'a AbsoluteDevice> {
    let (id, generation) = state.selected.as_ref()?;
    snapshot.devices.iter().find(|device| &device.id == id && device.generation == *generation)
}

fn set_value(world: &mut World, entity: Entity, value: String) {
    if let Some(mut editor) = world.get_mut::<EditableText>(entity) && editor.value() != value.as_str() {
        editor.editor.set_text(&value);
        editor.queue_edit(TextEdit::TextEnd(false));
    }
}

fn load(world: &mut World, page: Entity, snapshot: &AbsoluteSnapshot) {
    let state = world.get::<Page>(page).unwrap();
    let setting = device(state, snapshot).and_then(|device| device.key.as_ref()).and_then(|key| snapshot.settings.get(key)).cloned().unwrap_or_default();
    let (calibration, buttons) = (state.calibration, state.buttons);
    set_value(world, calibration, setting.calibration.map(|matrix| matrix.into_iter().map(|value| value.to_string()).collect::<Vec<_>>().join(" ")).unwrap_or_default());
    set_value(world, buttons, setting.buttons.iter().map(|(source, target)| format!("{source}={target}")).collect::<Vec<_>>().join(" "));
    let mut state = world.get_mut::<Page>(page).unwrap();
    state.output = setting.output;
    state.origin = Some((snapshot.revision, snapshot.settings.clone()));
    drop(state);
    invalidate_actions(world, page);
}

fn invalidate_actions(world: &mut World, page: Entity) {
    let entities = world.query::<(Entity, &crate::native::actions::ActionButton)>().iter(world).filter_map(|(entity, action)| (action.target == page).then_some(entity)).collect::<Vec<_>>();
    for entity in entities { if let Some(mut action) = world.get_mut::<crate::native::actions::ActionButton>(entity) { action.set_changed(); } }
}

fn mappings(world: &World, editor: Entity) -> Result<BTreeMap<u32, u32>, String> {
    let value = world.get::<EditableText>(editor).ok_or("Button editor is unavailable.")?.value().to_cow();
    let mut mappings = BTreeMap::new();
    for mapping in value.split_whitespace() {
        let (source, target) = mapping.split_once('=').ok_or("Use source=target for each button mapping.")?;
        let source = source.parse::<u32>().map_err(|_| "Button codes must be integers.")?;
        let target = target.parse::<u32>().map_err(|_| "Button codes must be integers.")?;
        if mappings.insert(source, target).is_some() { return Err("Each source button may appear only once.".into()); }
    }
    Ok(mappings)
}

fn mapping_name(source: u32, target: u32) -> String {
    if source == target { "Device default".into() } else { match target { 272 => "Left button".into(), 273 => "Right button".into(), 274 => "Middle button".into(), _ => format!("Button {target}") } }
}

fn preference(world: &World, calibration: Entity, buttons: Entity, output: &Option<String>) -> Result<AbsolutePreference, String> {
    let calibration = world.get::<EditableText>(calibration).ok_or("Calibration editor is unavailable.")?.value().to_cow();
    let calibration = if calibration.trim().is_empty() { None } else {
        let values = calibration.split_whitespace().map(|value| value.parse::<f32>().map_err(|_| "Calibration needs six numbers.")).collect::<Result<Vec<_>, _>>()?;
        Some(values.try_into().map_err(|_| "Calibration needs exactly six numbers.")?)
    };
    let preference = AbsolutePreference { output: output.clone(), calibration, buttons: mappings(world, buttons)? };
    preference.validate()?;
    Ok(preference)
}

fn current(world: &World, page: Entity, snapshot: &AbsoluteSnapshot) -> Result<AbsolutePreferences, String> {
    let state = world.get::<Page>(page).ok_or("The device editor closed.")?;
    let device = device(state, snapshot).ok_or("The selected device was removed or replaced; Cancel reloads the devices.")?;
    let key = device.key.as_ref().ok_or("This device has no unambiguous persistent identity.")?;
    let preference = preference(world, state.calibration, state.buttons, &state.output)?;
    let mut settings = state.origin.as_ref().ok_or("Device settings are still loading.")?.1.clone();
    if preference == AbsolutePreference::default() { settings.remove(key); }
    else { settings.insert(key.clone(), preference); }
    Ok(settings)
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(state) = world.get::<Page>(page) else { return; };
        let (status, pending) = (state.status, state.pending.is_some());
        if matches!(self, Self::Toggle) {
            let (body, toggle, expanded) = (state.body, state.toggle, !state.expanded);
            world.get_mut::<Page>(page).unwrap().expanded = expanded;
            world.get_mut::<Node>(body).unwrap().display = if expanded { Display::Flex } else { Display::None };
            let caption = if expanded { "Hide touch/tablet settings" } else { "Edit touch/tablet settings" };
            text(world, toggle.1, caption);
            world.get_mut::<bevy::a11y::AccessibilityNode>(toggle.0).unwrap().set_label(caption);
            if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(toggle.0, FocusCause::Navigated); }
            return;
        }
        if pending { return; }
        let snapshot = world.get_resource::<AbsoluteSnapshot>().cloned().unwrap_or_default();
        match self {
            Self::Reload => {
                let result = (|| {
                    if !snapshot.available { return Err("Saved physical device settings require a standalone Park session.".into()); }
                    let id = next_request(world, snapshot.last_request)?;
                    world.get_resource::<HostRequests>().ok_or("The Park host is unavailable.")?.0.try_send(HostRequest::AbsoluteReload(AbsoluteReload { id, revision: snapshot.revision, settings: () }))
                        .map_err(|_| "Park is busy or unavailable; your draft is preserved.")?;
                    if device(world.get::<Page>(page).unwrap(), &snapshot).is_none() {
                        world.get_mut::<Page>(page).unwrap().selected = snapshot.devices.first().map(|device| (device.id.clone(), device.generation));
                    }
                    load(world, page, &snapshot);
                    let mut state = world.get_mut::<Page>(page).unwrap();
                    state.pending = Some((id, snapshot.settings.clone()));
                    state.pending_reload = true;
                    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
                    Ok::<(), String>(())
                })();
                text(world, status, result.err().unwrap_or_else(|| "Waiting for released input to load the saved device settings.".into()));
            }
            Self::Advanced => {
                let advanced = world.get::<Page>(page).unwrap().advanced;
                let mut node = world.get_mut::<Node>(advanced).unwrap();
                node.display = if node.display == Display::None { Display::Flex } else { Display::None };
            }
            Self::MapButton(code) => {
                let state = world.get::<Page>(page).unwrap();
                if !device(state, &snapshot).is_some_and(|device| device.buttons.contains(code)) { return; }
                let editor = state.buttons;
                let result = mappings(world, editor);
                let Ok(mut values) = result else { text(world, status, "Correct the advanced button codes before changing a mapping."); return; };
                let current = values.get(code).copied().unwrap_or(*code);
                let mut choices = vec![*code];
                for value in [272, 273, 274, current] { if !choices.contains(&value) { choices.push(value); } }
                let next = choices[(choices.iter().position(|value| *value == current).unwrap_or(0) + 1) % choices.len()];
                if next == *code { values.remove(code); } else { values.insert(*code, next); }
                set_value(world, editor, values.iter().map(|(source, target)| format!("{source}={target}")).collect::<Vec<_>>().join(" "));
            }
            Self::Select(id, generation) => {
                let state = world.get::<Page>(page).unwrap();
                if state.selected.is_some() && current(world, page, &snapshot).ok().as_ref() != state.origin.as_ref().map(|(_, settings)| settings) {
                    text(world, status, "Save or Cancel this draft before choosing another device.");
                    return;
                }
                world.get_mut::<Page>(page).unwrap().selected = Some((id.clone(), *generation));
                load(world, page, &snapshot);
            }
            Self::Cancel => {
                let valid = device(world.get::<Page>(page).unwrap(), &snapshot).is_some();
                if !valid { world.get_mut::<Page>(page).unwrap().selected = snapshot.devices.first().map(|device| (device.id.clone(), device.generation)); }
                load(world, page, &snapshot);
                text(world, status, "Draft discarded; reloaded the current device settings.");
            }
            Self::Output => {
                let state = world.get::<Page>(page).unwrap();
                let mut choices = vec![None];
                if let Some(display) = world.get_resource::<DisplaySnapshot>() { choices.extend(display.heads.iter().map(|head| Some(head.name.clone()))); }
                if !choices.contains(&state.output) { choices.push(state.output.clone()); }
                let index = choices.iter().position(|choice| choice == &state.output).unwrap_or(0);
                world.get_mut::<Page>(page).unwrap().output = choices[(index + 1) % choices.len()].clone();
            }
            Self::ResetCalibration => { let editor = world.get::<Page>(page).unwrap().calibration; set_value(world, editor, String::new()); }
            Self::Save => {
                let result = (|| {
                    if !snapshot.available { return Err("Physical devices belong to the host compositor in Sandbox or nested Park.".into()); }
                    let settings = current(world, page, &snapshot)?;
                    let revision = world.get::<Page>(page).unwrap().origin.as_ref().unwrap().0;
                    if revision != snapshot.revision { return Err("Devices changed while editing; Cancel reloads them and preserves active settings.".into()); }
                    let id = next_request(world, snapshot.last_request)?;
                    world.get_resource::<HostRequests>().ok_or("The Park host is unavailable.")?.0.try_send(HostRequest::Absolute(AbsoluteRequest { id, revision, settings: settings.clone() })).map_err(|_| "Park is busy or unavailable; your draft is preserved.")?;
                    world.get_mut::<Page>(page).unwrap().pending = Some((id, settings));
                    world.get_mut::<Page>(page).unwrap().pending_reload = false;
                    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
                    Ok::<(), String>(())
                })();
                text(world, status, result.err().unwrap_or_else(|| "Lift fingers and move the tool out of proximity to apply and save these settings.".into()));
            }
            Self::Toggle => {}
        }
    }
}


#[cfg(test)]
#[path = "absolute_settings/tests.rs"]
mod tests;
