use super::{
    HostRequests,
    input_settings::{button, disable, feedback, field, next_request, text},
};
use crate::{
    host::{
        HostRequest,
        input::{
            InputOutcome, PointerCapabilities, PointerRequest, PointerSettings, PointerSnapshot,
        },
    },
    native::{actions::Action, edit_mode::label},
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::EditableText,
};

#[derive(Component)]
struct Page {
    body: Entity,
    toggle: (Entity, Entity),
    expanded: bool,
    choices: [(Entity, Entity); 3],
    speed: Entity,
    summary: Entity,
    devices: Entity,
    status: Entity,
    save: Entity,
    cancel: Entity,
    draft: PointerSettings,
    origin: Option<(u64, PointerSettings)>,
    pending: Option<(u64, PointerSettings)>,
    refresh: bool,
}

#[derive(Clone, Copy)]
enum Change {
    Toggle,
    Choice(usize),
    Save,
    Cancel,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        PostUpdate,
        refresh
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    let page = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            Name::new("park.pointer"),
            ChildOf(parent),
        ))
        .id();
    label(world, page, "Pointer and touchpad", 20.0);
    let summary = label(world, page, "", 14.0);
    let toggle = button(world, page, page, "Edit pointer settings", Change::Toggle);
    let body = world
        .spawn((
            Node {
                display: Display::None,
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(page),
        ))
        .id();
    label(
        world,
        body,
        "Preferences apply to every capable device. Default uses each device's libinput default; unavailable controls retain their saved preference for future devices.",
        14.0,
    );
    let choices = ["Tap to click", "Natural scrolling", "Left-handed buttons"]
        .into_iter()
        .enumerate()
        .map(|(index, caption)| {
            let control = button(world, body, page, caption, Change::Choice(index));
            world
                .entity_mut(control.0)
                .insert(Name::new(format!("pointer.preference.{index}")));
            control
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let speed = field(
        world,
        body,
        "Acceleration speed (-1 to 1; empty uses device defaults)",
        32,
    );
    label(world, body, "Connected devices · scroll for more", 14.0);
    let devices = feedback(world, body);
    world
        .entity_mut(devices)
        .insert(Name::new("pointer.devices"));
    let viewport = world.get::<ChildOf>(devices).unwrap().parent();
    world.get_mut::<Node>(viewport).unwrap().height = px(160);
    let status = feedback(world, body);
    let save = button(world, body, page, "Save pointer settings", Change::Save).0;
    let cancel = button(world, body, page, "Cancel pointer changes", Change::Cancel).0;
    world.entity_mut(page).insert(Page {
        body,
        toggle,
        expanded: false,
        choices,
        speed,
        summary,
        devices,
        status,
        save,
        cancel,
        draft: PointerSettings::default(),
        origin: None,
        pending: None,
        refresh: true,
    });
}

fn support(snapshot: &PointerSnapshot) -> PointerCapabilities {
    let mut result = PointerCapabilities::default();
    for device in &snapshot.devices {
        result.tap |= device.capabilities.tap;
        result.natural_scroll |= device.capabilities.natural_scroll;
        result.left_handed |= device.capabilities.left_handed;
        result.acceleration |= device.capabilities.acceleration;
    }
    result
}

fn current(world: &World, page: Entity) -> Result<PointerSettings, String> {
    let state = world.get::<Page>(page).unwrap();
    let input = world.get::<EditableText>(state.speed).unwrap();
    if input.is_composing() || !input.pending_edits.is_empty() || input.pending_paste.is_some() {
        return Err("Finish the current text edit before saving pointer settings.".into());
    }
    let value = input.value().to_string();
    let value = value.trim();
    let mut draft = state.draft.clone();
    draft.acceleration = if value.is_empty() {
        None
    } else {
        Some(value.parse().map_err(
            |_| "Use an acceleration speed from -1 to 1, or leave it empty for device defaults.",
        )?)
    };
    draft.validate()?;
    Ok(draft)
}

fn load(world: &mut World, page: Entity, snapshot: &PointerSnapshot) {
    let speed = world.get::<Page>(page).unwrap().speed;
    let value = snapshot
        .settings
        .acceleration
        .map(|speed| speed.to_string())
        .unwrap_or_default();
    let mut input = world.get_mut::<EditableText>(speed).unwrap();
    if input.value() != value.as_str() {
        input.editor.set_text(&value);
        input.queue_edit(bevy::text::TextEdit::TextEnd(false));
    }
    let mut state = world.get_mut::<Page>(page).unwrap();
    state.draft = snapshot.settings.clone();
    state.origin = Some((snapshot.revision, snapshot.settings.clone()));
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(state) = world.get::<Page>(page) else {
            return;
        };
        let (status, pending) = (state.status, state.pending.is_some());
        if matches!(self, Self::Toggle) {
            let (body, toggle, expanded, speed) =
                (state.body, state.toggle, !state.expanded, state.speed);
            world.get_mut::<Page>(page).unwrap().expanded = expanded;
            world.get_mut::<Node>(body).unwrap().display = if expanded {
                Display::Flex
            } else {
                Display::None
            };
            let caption = if expanded {
                "Hide pointer settings"
            } else {
                "Edit pointer settings"
            };
            text(world, toggle.1, caption);
            world
                .get_mut::<bevy::a11y::AccessibilityNode>(toggle.0)
                .unwrap()
                .set_label(caption);
            let target = if expanded
                && world
                    .get_resource::<PointerSnapshot>()
                    .is_some_and(|snapshot| snapshot.available && support(snapshot).acceleration)
            {
                speed
            } else {
                toggle.0
            };
            if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
                focus.set(target, FocusCause::Navigated);
            }
            return;
        }
        if pending && !matches!(self, Self::Choice(_)) {
            return;
        }
        let Some(snapshot) = world
            .get_resource::<PointerSnapshot>()
            .cloned()
            .filter(|snapshot| snapshot.available)
        else {
            text(
                world,
                status,
                "Physical pointer settings require a standalone Park seat.",
            );
            return;
        };
        match self {
            Self::Choice(index) => {
                let capabilities = support(&snapshot);
                if ![
                    capabilities.tap,
                    capabilities.natural_scroll,
                    capabilities.left_handed,
                ]
                .get(*index)
                .copied()
                .unwrap_or(false)
                {
                    return;
                }
                let mut state = world.get_mut::<Page>(page).unwrap();
                let value = match *index {
                    0 => &mut state.draft.tap,
                    1 => &mut state.draft.natural_scroll,
                    _ => &mut state.draft.left_handed,
                };
                *value = match value {
                    None => Some(true),
                    Some(true) => Some(false),
                    Some(false) => None,
                };
            }
            Self::Cancel => {
                load(world, page, &snapshot);
                text(
                    world,
                    status,
                    "Draft discarded; the active pointer settings are unchanged.",
                );
            }
            Self::Save => {
                let result = (|| {
                    let settings = current(world, page)?;
                    let revision = world
                        .get::<Page>(page)
                        .unwrap()
                        .origin
                        .as_ref()
                        .ok_or("Pointer settings are still loading.")?
                        .0;
                    let id = next_request(world, snapshot.last_request)?;
                    world
                        .get_resource::<HostRequests>()
                        .ok_or("The Park host is unavailable.")?
                        .0
                        .try_send(HostRequest::Pointer(PointerRequest {
                            id,
                            revision,
                            settings: settings.clone(),
                        }))
                        .map_err(|_| "Park is busy or unavailable; your draft is preserved.")?;
                    world.get_mut::<Page>(page).unwrap().pending = Some((id, settings));
                    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
                        wake.ring();
                    }
                    Ok::<(), String>(())
                })();
                text(world, status, match result { Ok(()) => "Release pointer buttons and finish scrolling or gestures to apply and save settings.".into(), Err(error) => error });
            }
            Self::Toggle => {}
        }
        world.get_mut::<Page>(page).unwrap().refresh = true;
    }
}

fn describe(value: Option<bool>) -> &'static str {
    match value {
        None => "Unavailable",
        Some(true) => "On",
        Some(false) => "Off",
    }
}

fn refresh(world: &mut World) {
    let changed = world.is_resource_changed::<PointerSnapshot>();
    let pages: Vec<_> = world
        .query::<(Entity, &Page)>()
        .iter(world)
        .filter_map(|(entity, state)| {
            (state.refresh
                || changed
                || world
                    .get_entity(state.speed)
                    .ok()
                    .and_then(|entity| {
                        entity
                            .get_ref::<EditableText>()
                            .map(|text| text.is_changed())
                    })
                    .unwrap_or(false))
            .then_some(entity)
        })
        .collect();
    if pages.is_empty() {
        return;
    }
    let snapshot = world
        .get_resource::<PointerSnapshot>()
        .cloned()
        .unwrap_or_default();
    for page in pages {
        let state = world.get::<Page>(page).unwrap();
        let (summary, devices, status, speed, choices, save, cancel, origin, pending) = (
            state.summary,
            state.devices,
            state.status,
            state.speed,
            state.choices,
            state.save,
            state.cancel,
            state.origin.clone(),
            state.pending.clone(),
        );
        let draft = current(world, page).ok();
        let response = pending.as_ref().and_then(|(request, _)| {
            snapshot
                .response
                .as_ref()
                .filter(|response| response.request == *request)
        });
        if let Some(response) = response {
            match &response.outcome {
                InputOutcome::Pending => {}
                InputOutcome::Saved { warning } => {
                    let unchanged =
                        draft.as_ref() == pending.as_ref().map(|(_, settings)| settings);
                    if unchanged {
                        load(world, page, &snapshot);
                    }
                    let mut state = world.get_mut::<Page>(page).unwrap();
                    state.pending = None;
                    state.origin = Some((snapshot.revision, snapshot.settings.clone()));
                    let mut message = warning
                        .clone()
                        .unwrap_or_else(|| "Pointer settings saved.".into());
                    if !unchanged {
                        message.push_str(" Newer edits remain unsaved in this draft.");
                    }
                    text(world, status, message);
                }
                InputOutcome::Failed(reason) => {
                    world.get_mut::<Page>(page).unwrap().pending = None;
                    text(world, status, reason.clone());
                }
            }
        } else if pending.is_none() {
            let clean = origin
                .as_ref()
                .is_none_or(|(_, settings)| draft.as_ref() == Some(settings));
            if clean {
                load(world, page, &snapshot);
            } else if origin
                .as_ref()
                .is_some_and(|(revision, _)| *revision != snapshot.revision)
            {
                text(
                    world,
                    status,
                    "The pointer devices changed while editing. Your draft is preserved; Cancel reloads the active devices and settings.",
                );
            }
        }
        let capabilities = support(&snapshot);
        let available = snapshot.available && world.contains_resource::<HostRequests>();
        let any = capabilities.tap
            || capabilities.natural_scroll
            || capabilities.left_handed
            || capabilities.acceleration;
        text(
            world,
            summary,
            if !snapshot.available {
                "Physical device settings belong to the active host compositor in Sandbox and nested Park.".into()
            } else if !any {
                "No compatible pointer devices are currently available. Saved preferences are retained for reconnect.".into()
            } else {
                format!(
                    "{} pointer devices. Preferences apply only to supported controls; effective values appear below.",
                    snapshot.devices.len()
                )
            },
        );
        let draft = world.get::<Page>(page).unwrap().draft.clone();
        for (index, ((button, caption), value)) in choices
            .into_iter()
            .zip([draft.tap, draft.natural_scroll, draft.left_handed])
            .enumerate()
        {
            let name = ["Tap to click", "Natural scrolling", "Left-handed buttons"][index];
            let value = value.map(describe_value).unwrap_or("Device default");
            let caption_value = format!("{name}: {value}");
            text(world, caption, &caption_value);
            world
                .get_mut::<bevy::a11y::AccessibilityNode>(button)
                .unwrap()
                .set_label(caption_value);
            disable(
                world,
                button,
                !available
                    || ![
                        capabilities.tap,
                        capabilities.natural_scroll,
                        capabilities.left_handed,
                    ][index],
            );
        }
        let details = snapshot
            .devices
            .iter()
            .map(|device| {
                let mut value = format!(
                    "{} [{}]\nTap: {} · Natural scroll: {} · Left-handed: {} · Acceleration: {}",
                    device.name,
                    device.id,
                    describe(device.effective.tap),
                    describe(device.effective.natural_scroll),
                    describe(device.effective.left_handed),
                    device
                        .effective
                        .acceleration
                        .map(|speed| speed.to_string())
                        .unwrap_or_else(|| "Unavailable".into())
                );
                if let Some(error) = &device.error {
                    value.push_str(&format!("\n{error}"));
                }
                value
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        text(world, devices, details);
        let busy = world.get::<Page>(page).unwrap().pending.is_some();
        disable(world, speed, !available || !capabilities.acceleration);
        disable(world, save, !available || !any || busy);
        disable(world, cancel, !available || busy);
        world.get_mut::<Page>(page).unwrap().refresh = false;
    }
}

fn describe_value(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::input::{InputResponse, PointerDevice};

    fn fixture(available: bool) -> (App, Entity, std::sync::mpsc::Receiver<HostRequest>) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<InputFocus>();
        super::super::input_settings::install(&mut app);
        install(&mut app);
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        app.insert_resource(HostRequests(sender));
        let effective = PointerSettings {
            tap: Some(false),
            natural_scroll: Some(false),
            left_handed: Some(false),
            acceleration: Some(0.0),
        };
        app.insert_resource(PointerSnapshot {
            available,
            revision: 1,
            devices: vec![PointerDevice {
                id: "test-pointer".into(),
                name: "Test pointer".into(),
                capabilities: PointerCapabilities {
                    tap: true,
                    natural_scroll: true,
                    left_handed: true,
                    acceleration: true,
                },
                defaults: effective.clone(),
                effective,
                error: None,
            }],
            ..Default::default()
        });
        let parent = app.world_mut().spawn(Node::default()).id();
        panel(app.world_mut(), parent);
        app.update();
        let page = app
            .world_mut()
            .query_filtered::<Entity, With<Page>>()
            .single(app.world())
            .unwrap();
        let speed = app.world().get::<Page>(page).unwrap().speed;
        app.world_mut()
            .get_mut::<EditableText>(speed)
            .unwrap()
            .pending_edits
            .clear();
        (app, page, receiver)
    }

    fn speed(world: &mut World, page: Entity, value: &str) -> Entity {
        let entity = world.get::<Page>(page).unwrap().speed;
        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        input.pending_edits.clear();
        input.editor.set_text(value);
        entity
    }

    #[test]
    fn pointer_editor_preserves_failed_and_newer_drafts_and_saves_typed_preferences() {
        let (mut app, page, receiver) = fixture(true);
        Change::Choice(0).apply(app.world_mut(), page);
        let field = speed(app.world_mut(), page, "0.4");
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Pointer(request) = receiver.try_recv().unwrap() else {
            unreachable!()
        };
        assert_eq!(request.settings.tap, Some(true));
        assert_eq!(request.settings.acceleration, Some(0.4));
        let mut snapshot = app.world().resource::<PointerSnapshot>().clone();
        snapshot.response = Some(InputResponse {
            request: request.id,
            outcome: InputOutcome::Failed("fixture save failure".into()),
        });
        app.insert_resource(snapshot);
        app.update();
        assert_eq!(current(app.world(), page).unwrap(), request.settings);
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Pointer(retry) = receiver.try_recv().unwrap() else {
            unreachable!()
        };
        speed(app.world_mut(), page, "0.5");
        let mut snapshot = app.world().resource::<PointerSnapshot>().clone();
        snapshot.revision = 2;
        snapshot.settings = retry.settings;
        snapshot.response = Some(InputResponse {
            request: retry.id,
            outcome: InputOutcome::Saved { warning: None },
        });
        app.insert_resource(snapshot);
        app.update();
        assert_eq!(current(app.world(), page).unwrap().acceleration, Some(0.5));
        assert_eq!(
            app.world()
                .get::<Page>(page)
                .unwrap()
                .origin
                .as_ref()
                .unwrap()
                .1
                .acceleration,
            Some(0.4)
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        let status = app.world().get::<Page>(page).unwrap().status;
        assert!(
            app.world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("Newer edits remain unsaved")
        );
    }

    #[test]
    fn hotplug_does_not_retarget_a_draft_and_unavailable_seats_cannot_submit() {
        let (mut app, page, receiver) = fixture(true);
        Change::Choice(0).apply(app.world_mut(), page);
        let mut snapshot = app.world().resource::<PointerSnapshot>().clone();
        snapshot.revision = 2;
        snapshot.devices[0].id = "replacement-pointer".into();
        app.insert_resource(snapshot);
        app.update();
        assert_eq!(
            app.world()
                .get::<Page>(page)
                .unwrap()
                .origin
                .as_ref()
                .unwrap()
                .0,
            1
        );
        assert_eq!(current(app.world(), page).unwrap().tap, Some(true));
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Pointer(request) = receiver.try_recv().unwrap() else {
            unreachable!()
        };
        assert_eq!(request.revision, 1);
        let (mut nested, page, receiver) = fixture(false);
        Change::Save.apply(nested.world_mut(), page);
        assert!(receiver.try_recv().is_err());
        let save = nested.world().get::<Page>(page).unwrap().save;
        assert!(
            nested
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
    }
}
