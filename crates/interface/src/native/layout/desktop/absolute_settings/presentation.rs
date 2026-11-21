// non-lean-yet
// Scope: struct Presentation, struct View, struct Details, struct RowLabel.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 210, 227, 240,
//   309, 314.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (102, 123, 197, 213, 232, 236, 237, 243, 260, 267, 280, 289, 306, 331, 344, 382,
//   396).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 161.

use super::*;
use crate::native::layout::desktop::input_settings::{caption, prepared::Prepared};
use bevy::ecs::query::QueryState;
use std::collections::HashMap;

#[derive(Resource)]
struct Presentation {
    pages: QueryState<Entity, With<Page>>,
    pending: Vec<Entity>,
    snapshot: AbsoluteSnapshot,
    views: HashMap<Entity, View>,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query_filtered(),
            pending: Vec::new(),
            snapshot: AbsoluteSnapshot::default(),
            views: HashMap::new(),
        }
    }
}

#[derive(Default)]
struct View {
    codes: Option<Vec<u32>>,
    rows: Vec<(u32, (Entity, Entity))>,
    mappings: Prepared<(), BTreeMap<u32, u32>>,
    preference: Prepared<Option<String>, AbsolutePreference>,
    labels: BTreeMap<u32, RowLabel>,
    output: Option<(Option<String>, bool, String)>,
    details: Option<Details>,
}

struct Details {
    available: bool,
    device: Option<AbsoluteDevice>,
    text: String,
}

struct RowLabel {
    index: usize,
    target: Option<u32>,
    text: String,
}

impl View {
    fn matches(
        &mut self,
        world: &World,
        state: &Page,
        snapshot: &AbsoluteSnapshot,
        expected: &AbsolutePreferences,
    ) -> bool {
        let Some(key) = device(state, snapshot).and_then(|device| device.key.as_ref()) else {
            return false;
        };
        let Some((_, origin)) = &state.origin else {
            return false;
        };
        let preference = self.preference.prepare(
            world,
            [state.calibration, state.buttons],
            &state.output,
            || preference(world, state.calibration, state.buttons, &state.output),
        );
        let Ok(preference) = preference else {
            return false;
        };
        let selected_matches = if *preference == AbsolutePreference::default() {
            !expected.contains_key(key)
        } else {
            expected.get(key) == Some(preference)
        };
        selected_matches
            && expected
                .iter()
                .filter(|(candidate, _)| *candidate != key)
                .eq(origin.iter().filter(|(candidate, _)| *candidate != key))
    }

    fn output(&mut self, output: Option<&str>, disconnected: bool) -> &str {
        if self
            .output
            .as_ref()
            .is_none_or(|(previous, was_disconnected, _)| {
                previous.as_deref() != output || *was_disconnected != disconnected
            })
        {
            self.output = Some((
                output.map(str::to_owned),
                disconnected,
                format!(
                    "Output: {}{}",
                    output.unwrap_or("Automatic"),
                    if disconnected { " (disconnected)" } else { "" }
                ),
            ));
            engine::performance::add("absolute-settings-labels-prepared", 1);
        }
        &self.output.as_ref().unwrap().2
    }

    fn details(&mut self, available: bool, selected: Option<&AbsoluteDevice>) -> &str {
        if self.details.as_ref().is_none_or(|previous| {
            previous.available != available || previous.device.as_ref() != selected
        }) {
            let text = if !available {
                "Physical touch/tablet configuration belongs to the active host compositor in Sandbox and nested Park.".into()
            } else if let Some(device) = selected {
                format!("{} · {}{}\n{}\nCalibration: {}\nEffective: {}\nTool buttons: {}", device.name, if device.touch { "Touch " } else { "" }, if device.tablet { "Tablet" } else { "" }, device.error.as_deref().unwrap_or("Saved output preference applies to new sequences; held input retains its owner."), if device.calibration { "supported" } else { "unavailable on this device" }, device.effective_calibration.map(|matrix| matrix.into_iter().map(|value| value.to_string()).collect::<Vec<_>>().join(" ")).unwrap_or_else(|| "device default / unavailable".into()), if device.buttons.is_empty() { "no supported tool buttons reported".into() } else { device.buttons.iter().map(|button| button.to_string()).collect::<Vec<_>>().join(", ") })
            } else {
                "No selected touch/tablet device is connected. Saved preferences are retained; Cancel reloads connected devices.".into()
            };
            self.details = Some(Details {
                available,
                device: selected.cloned(),
                text,
            });
            engine::performance::add("absolute-settings-details-prepared", 1);
        }
        &self.details.as_ref().unwrap().text
    }

    fn buttons(&mut self, world: &mut World, editor: Entity, available: bool) {
        if self.rows.is_empty() {
            return;
        }
        let values = self
            .mappings
            .prepare(world, [editor], &(), || mappings(world, editor));
        for (index, &(code, row)) in self.rows.iter().enumerate() {
            let target = values
                .as_ref()
                .ok()
                .map(|values| values.get(&code).copied().unwrap_or(code));
            let label = self.labels.entry(code).or_insert_with(|| RowLabel {
                index: usize::MAX,
                target: None,
                text: String::new(),
            });
            if label.index != index || label.target != target {
                label.index = index;
                label.target = target;
                let value = target
                    .map(|target| mapping_name(code, target))
                    .unwrap_or_else(|| "Invalid advanced draft".into());
                label.text = format!("Tool button {}: {} · click to cycle", index + 1, value);
                engine::performance::add("absolute-settings-labels-prepared", 1);
            }
            caption(world, row, &label.text);
            disable(world, row.0, !available);
        }
    }
}

pub(super) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("absolute-settings-presentation");
    world.init_resource::<Presentation>();
    world.resource_scope(|world, mut presentation: Mut<Presentation>| {
        let Presentation {
            pages,
            pending,
            views,
            ..
        } = &mut *presentation;
        pending.clear();
        pending.extend(pages.iter(world));
        views.retain(|entity, _| world.get::<Page>(*entity).is_some());
        if pending.is_empty() {
            presentation.snapshot = AbsoluteSnapshot::default();
            return;
        }
        if let Some(snapshot) = world.get_resource::<AbsoluteSnapshot>() {
            if presentation.snapshot != *snapshot {
                presentation.snapshot.clone_from(snapshot);
                engine::performance::add("absolute-settings-snapshots-prepared", 1);
            }
        } else if presentation.snapshot != AbsoluteSnapshot::default() {
            presentation.snapshot = AbsoluteSnapshot::default();
            engine::performance::add("absolute-settings-snapshots-prepared", 1);
        }
        let Presentation {
            pending,
            snapshot,
            views,
            ..
        } = &mut *presentation;
        for &page in pending.iter() {
            render(world, page, snapshot, views.entry(page).or_default());
        }
    });
}

fn render(world: &mut World, page: Entity, snapshot: &AbsoluteSnapshot, view: &mut View) {
    let state = world.get::<Page>(page).unwrap();
    if state.listed.len() != snapshot.devices.len()
        || !state
            .listed
            .iter()
            .zip(&snapshot.devices)
            .all(|((id, generation, name), device)| {
                *id == device.id && *generation == device.generation && *name == device.name
            })
    {
        let listed = snapshot
            .devices
            .iter()
            .map(|device| (device.id.clone(), device.generation, device.name.clone()))
            .collect::<Vec<_>>();
        let list = state.list;
        let rows = std::mem::take(&mut world.get_mut::<Page>(page).unwrap().rows);
        for row in rows {
            if let Ok(entity) = world.get_entity_mut(row) {
                entity.despawn();
            }
        }
        let rows = listed
            .iter()
            .map(|(id, generation, name)| {
                button(
                    world,
                    list,
                    page,
                    name,
                    Change::Select(id.clone(), *generation),
                )
                .0
            })
            .collect();
        let mut state = world.get_mut::<Page>(page).unwrap();
        state.rows = rows;
        state.listed = listed;
    }
    if world.get::<Page>(page).unwrap().origin.is_none() {
        world.get_mut::<Page>(page).unwrap().selected = snapshot
            .devices
            .first()
            .map(|device| (device.id.clone(), device.generation));
        load(world, page, &snapshot);
    }
    let state = world.get::<Page>(page).unwrap();
    let codes = device(state, snapshot)
        .map(|device| device.buttons.as_slice())
        .unwrap_or_default();
    let container = state.button_list;
    if view.codes.as_deref() != Some(codes) {
        let obsolete = state
            .button_rows
            .iter()
            .filter_map(|(code, row)| (!codes.contains(code)).then_some((*code, row.0)))
            .collect::<Vec<_>>();
        for (code, entity) in obsolete {
            if let Ok(entity) = world.get_entity_mut(entity) {
                entity.despawn();
            }
            world
                .get_mut::<Page>(page)
                .unwrap()
                .button_rows
                .remove(&code);
        }
        for &code in codes {
            if !world
                .get::<Page>(page)
                .unwrap()
                .button_rows
                .contains_key(&code)
            {
                let row = button(
                    world,
                    container,
                    page,
                    "Tool button",
                    Change::MapButton(code),
                );
                world
                    .get_mut::<Page>(page)
                    .unwrap()
                    .button_rows
                    .insert(code, row);
            }
        }
        let previous = view.codes.get_or_insert_with(Vec::new);
        previous.clear();
        previous.extend_from_slice(codes);
    }
    let state = world.get::<Page>(page).unwrap();
    if let Some((id, settings)) = &state.pending
        && let Some(response) = snapshot
            .response
            .as_ref()
            .filter(|response| response.request == *id)
    {
        let status = state.status;
        match &response.outcome {
            InputOutcome::Pending => {}
            InputOutcome::Saved { warning } => {
                let reloaded = state.pending_reload;
                let unchanged = view.matches(world, state, snapshot, settings)
                    || reloaded && device(state, snapshot).is_none();
                if unchanged {
                    load(world, page, &snapshot);
                }
                let mut state = world.get_mut::<Page>(page).unwrap();
                state.pending = None;
                state.pending_reload = false;
                state.origin = Some((snapshot.revision, snapshot.settings.clone()));
                text(
                    world,
                    status,
                    warning
                        .clone()
                        .unwrap_or_else(|| match (reloaded, unchanged) {
                            (true, true) => "Device settings reloaded from the saved file.".into(),
                            (true, false) => {
                                "Device settings reloaded; newer edits remain unsaved.".into()
                            }
                            (false, true) => "Device settings saved.".into(),
                            (false, false) => {
                                "Device settings saved; newer edits remain unsaved.".into()
                            }
                        }),
                );
                if !unchanged {
                    invalidate_actions(world, page);
                }
            }
            InputOutcome::Failed(reason) => {
                let mut state = world.get_mut::<Page>(page).unwrap();
                state.pending = None;
                state.pending_reload = false;
                text(world, status, reason);
            }
        }
    } else if state.pending.is_none()
        && state.origin.as_ref().is_some_and(|(revision, settings)| {
            *revision != snapshot.revision && view.matches(world, state, snapshot, settings)
        })
    {
        load(world, page, &snapshot);
    }
    let state = world.get::<Page>(page).unwrap();
    let selected = device(state, &snapshot);
    let available = snapshot.available
        && world.contains_resource::<HostRequests>()
        && selected.is_some_and(|device| device.key.is_some());
    let (summary, output, calibration, buttons, reset, save, cancel, busy) = (
        state.summary,
        state.output_button,
        state.calibration,
        state.buttons,
        state.reset,
        state.save,
        state.cancel,
        state.pending.is_some(),
    );
    let disconnected = state.output.as_ref().is_some_and(|name| {
        world
            .get_resource::<DisplaySnapshot>()
            .is_none_or(|display| {
                !display
                    .heads
                    .iter()
                    .any(|head| &head.name == name && head.current.is_some())
            })
    });
    let calibration_available = selected.is_some_and(|device| device.calibration);
    let buttons_available = selected.is_some_and(|device| !device.buttons.is_empty());
    view.rows.clear();
    view.rows
        .extend(state.button_rows.iter().map(|(code, row)| (*code, *row)));
    view.labels
        .retain(|code, _| state.button_rows.contains_key(code));
    let output_text = view.output(state.output.as_deref(), disconnected);
    caption(world, output, output_text);
    let details = view.details(snapshot.available, selected);
    text(world, summary, details);
    view.buttons(world, buttons, available && !busy);
    disable(world, output.0, !available || busy);
    let reloading = world.get::<Page>(page).unwrap().pending_reload && busy;
    disable(
        world,
        calibration,
        !available || !calibration_available || reloading,
    );
    disable(world, reset, !available || !calibration_available || busy);
    disable(
        world,
        buttons,
        !available || !buttons_available || reloading,
    );
    disable(world, save, !available || busy);
    disable(world, cancel, busy);
    let reload = world.get::<Page>(page).unwrap().reload;
    let can_reload = snapshot.available && world.contains_resource::<HostRequests>() && !busy;
    disable(world, reload, !can_reload);
}
