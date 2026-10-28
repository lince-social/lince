use super::{HostRequests, input_settings::{button, disable, feedback, field, next_request, text}};
use crate::{host::{HostRequest, display::DisplaySnapshot, input::{AbsoluteDevice, AbsolutePreference, AbsolutePreferences, AbsoluteRequest, AbsoluteReload, AbsoluteSnapshot, InputOutcome}}, native::{actions::Action, edit_mode::label}};
use bevy::{input_focus::{FocusCause, InputFocus}, prelude::*, text::{EditableText, TextEdit}};
use std::collections::BTreeMap;

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
    let value = world.get::<EditableText>(editor).ok_or("Button editor is unavailable.")?.value().to_string();
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

fn current(world: &World, page: Entity, snapshot: &AbsoluteSnapshot) -> Result<AbsolutePreferences, String> {
    let state = world.get::<Page>(page).ok_or("The device editor closed.")?;
    let device = device(state, snapshot).ok_or("The selected device was removed or replaced; Cancel reloads the devices.")?;
    let key = device.key.as_ref().ok_or("This device has no unambiguous persistent identity.")?;
    let calibration = world.get::<EditableText>(state.calibration).ok_or("Calibration editor is unavailable.")?.value().to_string();
    let calibration = if calibration.trim().is_empty() { None } else {
        let values = calibration.split_whitespace().map(|value| value.parse::<f32>().map_err(|_| "Calibration needs six numbers.")).collect::<Result<Vec<_>, _>>()?;
        Some(values.try_into().map_err(|_| "Calibration needs exactly six numbers.")?)
    };
    let preference = AbsolutePreference { output: state.output.clone(), calibration, buttons: mappings(world, state.buttons)? };
    preference.validate()?;
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

fn refresh(world: &mut World) {
    let snapshot = world.get_resource::<AbsoluteSnapshot>().cloned().unwrap_or_default();
    let pages = world.query_filtered::<Entity, With<Page>>().iter(world).collect::<Vec<_>>();
    for page in pages {
        let state = world.get::<Page>(page).unwrap();
        let listed = snapshot.devices.iter().map(|device| (device.id.clone(), device.generation, device.name.clone())).collect::<Vec<_>>();
        if state.listed != listed {
            let (list, rows) = (state.list, state.rows.clone());
            for row in rows { if let Ok(entity) = world.get_entity_mut(row) { entity.despawn(); } }
            let rows = listed.iter().map(|(id, generation, name)| button(world, list, page, name, Change::Select(id.clone(), *generation)).0).collect();
            let mut state = world.get_mut::<Page>(page).unwrap();
            state.rows = rows;
            state.listed = listed;
        }
        if world.get::<Page>(page).unwrap().origin.is_none() {
            world.get_mut::<Page>(page).unwrap().selected = snapshot.devices.first().map(|device| (device.id.clone(), device.generation));
            load(world, page, &snapshot);
        }
        let state = world.get::<Page>(page).unwrap();
        let codes = device(state, &snapshot).map(|device| device.buttons.clone()).unwrap_or_default();
        let container = state.button_list;
        let obsolete = state.button_rows.iter().filter_map(|(code, row)| (!codes.contains(code)).then_some((*code, row.0))).collect::<Vec<_>>();
        for (code, entity) in obsolete {
            if let Ok(entity) = world.get_entity_mut(entity) { entity.despawn(); }
            world.get_mut::<Page>(page).unwrap().button_rows.remove(&code);
        }
        for code in codes {
            if !world.get::<Page>(page).unwrap().button_rows.contains_key(&code) {
                let row = button(world, container, page, "Tool button", Change::MapButton(code));
                world.get_mut::<Page>(page).unwrap().button_rows.insert(code, row);
            }
        }
        let state = world.get::<Page>(page).unwrap();
        let draft = current(world, page, &snapshot).ok();
        let pending = state.pending.clone();
        if let Some((id, settings)) = pending && let Some(response) = snapshot.response.as_ref().filter(|response| response.request == id) {
            let status = state.status;
            match &response.outcome {
                InputOutcome::Pending => {}
                InputOutcome::Saved { warning } => {
                    let reloaded = state.pending_reload;
                    let unchanged = draft.as_ref() == Some(&settings) || reloaded && device(state, &snapshot).is_none();
                    if unchanged { load(world, page, &snapshot); }
                    let mut state = world.get_mut::<Page>(page).unwrap();
                    state.pending = None;
                    state.pending_reload = false;
                    state.origin = Some((snapshot.revision, snapshot.settings.clone()));
                    text(world, status, warning.clone().unwrap_or_else(|| match (reloaded, unchanged) {
                        (true, true) => "Device settings reloaded from the saved file.".into(),
                        (true, false) => "Device settings reloaded; newer edits remain unsaved.".into(),
                        (false, true) => "Device settings saved.".into(),
                        (false, false) => "Device settings saved; newer edits remain unsaved.".into(),
                    }));
                    if !unchanged { invalidate_actions(world, page); }
                }
                InputOutcome::Failed(reason) => { let mut state = world.get_mut::<Page>(page).unwrap(); state.pending = None; state.pending_reload = false; text(world, status, reason); }
            }
        } else if state.pending.is_none() && state.origin.as_ref().is_some_and(|(revision, settings)| *revision != snapshot.revision && draft.as_ref() == Some(settings)) {
            load(world, page, &snapshot);
        }
        let state = world.get::<Page>(page).unwrap();
        let selected = device(state, &snapshot);
        let available = snapshot.available && world.contains_resource::<HostRequests>() && selected.is_some_and(|device| device.key.is_some());
        let (summary, output, calibration, buttons, reset, save, cancel, busy) = (state.summary, state.output_button, state.calibration, state.buttons, state.reset, state.save, state.cancel, state.pending.is_some());
        let disconnected = state.output.as_ref().is_some_and(|name| world.get_resource::<DisplaySnapshot>().is_none_or(|display| !display.heads.iter().any(|head| &head.name == name && head.current.is_some())));
        let caption = format!("Output: {}{}", state.output.as_deref().unwrap_or("Automatic"), if disconnected { " (disconnected)" } else { "" });
        let details = if !snapshot.available { "Physical touch/tablet configuration belongs to the active host compositor in Sandbox and nested Park.".into() }
            else if let Some(device) = selected { format!("{} · {}{}\n{}\nCalibration: {}\nEffective: {}\nTool buttons: {}", device.name, if device.touch { "Touch " } else { "" }, if device.tablet { "Tablet" } else { "" }, device.error.as_deref().unwrap_or("Saved output preference applies to new sequences; held input retains its owner."), if device.calibration { "supported" } else { "unavailable on this device" }, device.effective_calibration.map(|matrix| matrix.into_iter().map(|value| value.to_string()).collect::<Vec<_>>().join(" ")).unwrap_or_else(|| "device default / unavailable".into()), if device.buttons.is_empty() { "no supported tool buttons reported".into() } else { device.buttons.iter().map(|button| button.to_string()).collect::<Vec<_>>().join(", ") }) }
            else { "No selected touch/tablet device is connected. Saved preferences are retained; Cancel reloads connected devices.".into() };
        let calibration_available = selected.is_some_and(|device| device.calibration);
        let buttons_available = selected.is_some_and(|device| !device.buttons.is_empty());
        let button_rows = state.button_rows.clone();
        let values = mappings(world, buttons);
        for (index, (code, row)) in button_rows.into_iter().enumerate() {
            let value = values.as_ref().map(|values| mapping_name(code, values.get(&code).copied().unwrap_or(code))).unwrap_or_else(|_| "Invalid advanced draft".into());
            let caption = format!("Tool button {}: {} · click to cycle", index + 1, value);
            text(world, row.1, &caption);
            world.get_mut::<bevy::a11y::AccessibilityNode>(row.0).unwrap().set_label(caption);
            disable(world, row.0, !available || busy);
        }
        text(world, summary, details);
        text(world, output.1, &caption);
        world.get_mut::<bevy::a11y::AccessibilityNode>(output.0).unwrap().set_label(caption);
        disable(world, output.0, !available || busy);
        let reloading = world.get::<Page>(page).unwrap().pending_reload && busy;
        disable(world, calibration, !available || !calibration_available || reloading);
        disable(world, reset, !available || !calibration_available || busy);
        disable(world, buttons, !available || !buttons_available || reloading);
        disable(world, save, !available || busy);
        disable(world, cancel, busy);
        let reload = world.get::<Page>(page).unwrap().reload;
        let can_reload = snapshot.available && world.contains_resource::<HostRequests>() && !busy;
        disable(world, reload, !can_reload);
    }
}

#[cfg(test)]
#[path = "absolute_settings/tests.rs"]
mod tests;
