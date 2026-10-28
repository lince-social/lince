use super::HostRequests;
use crate::{
    host::{
        HostRequest,
        input::{InputOutcome, KeyboardRequest, KeyboardSettings, KeyboardSnapshot},
    },
    native::{
        actions::{Action, ActionButton},
        edit_mode::label,
    },
};
use bevy::{prelude::*, text::EditableText, ui::InteractionDisabled};

#[derive(Resource, Default)]
struct Requests(u64);

#[derive(Component)]
struct Page {
    body: Entity,
    toggle: Entity,
    toggle_text: Entity,
    expanded: bool,
    fields: [Entity; 5],
    summary: Entity,
    status: Entity,
    save: Entity,
    cancel: Entity,
    origin: Option<(u64, KeyboardSettings)>,
    pending: Option<(u64, KeyboardSettings)>,
    refresh: bool,
}

#[derive(Clone, Copy)]
enum Change {
    Toggle,
    Save,
    Cancel,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Requests>().add_systems(
        PostUpdate,
        refresh
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub(crate) fn button(
    world: &mut World,
    parent: Entity,
    page: Entity,
    caption: &str,
    action: impl Action,
) -> (Entity, Entity) {
    let button = world
        .spawn((
            crate::native::sand::button(0),
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(10), px(6)),
                flex_shrink: 0.0,
                ..default()
            },
            ActionButton::new(page, crate::native::actions![action]),
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(button)
        .unwrap()
        .set_label(caption);
    let text = label(world, button, caption, 14.0);
    let mut node = world.get_mut::<Node>(text).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    world.entity_mut(text).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    (button, text)
}

pub(super) fn field(world: &mut World, parent: Entity, caption: &str, maximum: usize) -> Entity {
    let label = label(world, parent, caption, 14.0);
    let mut node = world.get_mut::<Node>(label).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    world.entity_mut(label).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    let input = crate::native::sand::single_line_editor(
        "",
        world.resource::<crate::native::theme::Typography>(),
        0,
        maximum,
    );
    let entity = world
        .spawn((input, ChildOf(parent), Name::new(caption.to_owned())))
        .id();
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    let mut accessible = accesskit::Node::new(accesskit::Role::TextInput);
    accessible.set_label(caption);
    world
        .entity_mut(entity)
        .insert(bevy::a11y::AccessibilityNode::from(accessible));
    entity
}

pub(crate) fn feedback(world: &mut World, parent: Entity) -> Entity {
    let container = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                height: px(72),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::native::scroll_sand::attach(world, container);
    let status = label(world, container, "", 14.0);
    world.get_mut::<Node>(status).unwrap().min_width = px(0);
    world.entity_mut(status).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    status
}

pub(super) fn next_request(world: &mut World, last_request: u64) -> Result<u64, String> {
    world.init_resource::<Requests>();
    let id = world
        .resource::<Requests>()
        .0
        .max(last_request)
        .checked_add(1)
        .ok_or("Input request identifiers are exhausted; restart Park.")?;
    world.resource_mut::<Requests>().0 = id;
    Ok(id)
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
            Name::new("park.keyboard"),
            ChildOf(parent),
        ))
        .id();
    label(world, page, "Keyboard", 20.0);
    let summary = label(world, page, "", 14.0);
    let (toggle, toggle_text) = button(world, page, page, "Edit keyboard settings", Change::Toggle);
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
    let fields = [
        ("XKB layout (empty uses the system default)", 128),
        ("XKB variants, one per layout", 128),
        ("XKB options (empty means no options)", 512),
        ("Repeat rate in Hz (0 disables repeat)", 3),
        ("Repeat delay in milliseconds", 4),
    ]
    .map(|(caption, maximum)| field(world, body, caption, maximum));
    label(
        world,
        body,
        "Use comma-separated XKB names, such as us,br or caps:escape. Bindings keep their physical keys. Saving waits for held keys to be released.",
        14.0,
    );
    let status = feedback(world, body);
    let save = button(world, body, page, "Save keyboard settings", Change::Save).0;
    let cancel = button(world, body, page, "Cancel keyboard changes", Change::Cancel).0;
    for entity in [summary, status] {
        world.get_mut::<Node>(entity).unwrap().min_width = px(0);
        world.entity_mut(entity).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
    }
    world.entity_mut(page).insert(Page {
        body,
        toggle,
        toggle_text,
        expanded: false,
        fields,
        summary,
        status,
        save,
        cancel,
        origin: None,
        pending: None,
        refresh: true,
    });
}

fn values(settings: &KeyboardSettings) -> [String; 5] {
    [
        settings.layout.clone(),
        settings.variant.clone(),
        settings.options.clone(),
        settings.repeat_rate.to_string(),
        settings.repeat_delay_ms.to_string(),
    ]
}

fn write_fields(world: &mut World, fields: [Entity; 5], settings: &KeyboardSettings) {
    for (entity, value) in fields.into_iter().zip(values(settings)) {
        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        if input.value() != value.as_str() {
            input.editor.set_text(&value);
            input.queue_edit(bevy::text::TextEdit::TextEnd(false));
        }
    }
}

fn read_fields(world: &World, fields: [Entity; 5]) -> Result<KeyboardSettings, String> {
    if fields.iter().any(|entity| {
        world.get::<EditableText>(*entity).is_some_and(|text| {
            text.is_composing() || !text.pending_edits.is_empty() || text.pending_paste.is_some()
        })
    }) {
        return Err("Finish the current text edit before saving keyboard settings.".into());
    }
    let values = fields.map(|entity| {
        world
            .get::<EditableText>(entity)
            .unwrap()
            .value()
            .to_string()
            .trim()
            .to_owned()
    });
    let settings = KeyboardSettings {
        layout: values[0].clone(),
        variant: values[1].clone(),
        options: values[2].clone(),
        repeat_rate: values[3]
            .parse()
            .map_err(|_| "Use a whole repeat rate between 0 and 100.")?,
        repeat_delay_ms: values[4]
            .parse()
            .map_err(|_| "Use a whole repeat delay between 0 and 5000 ms.")?,
    };
    settings.validate()?;
    Ok(settings)
}

pub(crate) fn text(world: &mut World, entity: Entity, message: impl Into<String>) {
    let message = message.into();
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != message
    {
        text.0 = message;
    }
}

pub(crate) fn disable(world: &mut World, entity: Entity, disabled: bool) {
    if world.get::<InteractionDisabled>(entity).is_some() != disabled {
        if disabled {
            world.entity_mut(entity).insert(InteractionDisabled);
        } else {
            world.entity_mut(entity).remove::<InteractionDisabled>();
        }
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(state) = world.get::<Page>(page) else {
            return;
        };
        if matches!(self, Self::Toggle) {
            let (body, toggle, caption, expanded, field) = (
                state.body,
                state.toggle,
                state.toggle_text,
                !state.expanded,
                state.fields[0],
            );
            world.get_mut::<Node>(body).unwrap().display = if expanded {
                Display::Flex
            } else {
                Display::None
            };
            world.get_mut::<Page>(page).unwrap().expanded = expanded;
            let caption_value = if expanded {
                "Hide keyboard settings"
            } else {
                "Edit keyboard settings"
            };
            text(world, caption, caption_value);
            world
                .get_mut::<bevy::a11y::AccessibilityNode>(toggle)
                .unwrap()
                .set_label(caption_value);
            let available = world.contains_resource::<KeyboardSnapshot>()
                && world.contains_resource::<HostRequests>();
            if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
                focus.set(
                    if expanded && available { field } else { toggle },
                    bevy::input_focus::FocusCause::Navigated,
                );
            }
            return;
        }
        if state.pending.is_some() {
            return;
        }
        let (fields, status, origin) = (state.fields, state.status, state.origin.clone());
        let Some(snapshot) = world.get_resource::<KeyboardSnapshot>().cloned() else {
            text(
                world,
                status,
                "Keyboard settings require an available Park seat.",
            );
            return;
        };
        if matches!(self, Self::Cancel) {
            write_fields(world, fields, &snapshot.settings);
            let mut state = world.get_mut::<Page>(page).unwrap();
            state.origin = Some((snapshot.revision, snapshot.settings));
            state.refresh = true;
            text(
                world,
                status,
                "Draft discarded; the active keyboard is unchanged.",
            );
            return;
        }
        let result = (|| {
            let settings = read_fields(world, fields)?;
            let (revision, _) = origin.ok_or("The keyboard settings are still loading.")?;
            let id = next_request(world, snapshot.last_request)?;
            world
                .get_resource::<HostRequests>()
                .ok_or("The Park host is unavailable.")?
                .0
                .try_send(HostRequest::Keyboard(KeyboardRequest {
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
        text(
            world,
            status,
            match result {
                Ok(()) => "Release held keys to apply and save the keyboard settings.".into(),
                Err(error) => error,
            },
        );
        world.get_mut::<Page>(page).unwrap().refresh = true;
    }
}

fn refresh(world: &mut World) {
    let changed = world.is_resource_changed::<KeyboardSnapshot>();
    let pages: Vec<_> = world
        .query::<(Entity, &Page)>()
        .iter(world)
        .filter_map(|(entity, state)| {
            (state.refresh
                || changed
                || state.fields.iter().any(|field| {
                    world
                        .get_entity(*field)
                        .ok()
                        .and_then(|entity| {
                            entity
                                .get_ref::<EditableText>()
                                .map(|text| text.is_changed())
                        })
                        .unwrap_or(false)
                }))
            .then_some(entity)
        })
        .collect();
    if pages.is_empty() {
        return;
    }
    let snapshot = world.get_resource::<KeyboardSnapshot>().cloned();
    for page in pages {
        let state = world.get::<Page>(page).unwrap();
        let (fields, summary, status, save, cancel, origin, pending) = (
            state.fields,
            state.summary,
            state.status,
            state.save,
            state.cancel,
            state.origin.clone(),
            state.pending.clone(),
        );
        let draft = read_fields(world, fields).ok();
        if let Some(snapshot) = &snapshot {
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
                            write_fields(world, fields, &snapshot.settings);
                        }
                        let mut state = world.get_mut::<Page>(page).unwrap();
                        state.pending = None;
                        state.origin = Some((snapshot.revision, snapshot.settings.clone()));
                        let mut message = warning
                            .clone()
                            .unwrap_or_else(|| "Keyboard settings saved.".into());
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
                if clean
                    || origin
                        .as_ref()
                        .is_some_and(|(_, settings)| settings == &snapshot.settings)
                {
                    if clean {
                        write_fields(world, fields, &snapshot.settings);
                    }
                    world.get_mut::<Page>(page).unwrap().origin =
                        Some((snapshot.revision, snapshot.settings.clone()));
                } else if origin
                    .as_ref()
                    .is_some_and(|(revision, _)| *revision != snapshot.revision)
                {
                    text(
                        world,
                        status,
                        "The keyboard changed while editing. Your draft is preserved; Cancel loads the active settings.",
                    );
                }
            }
            let active = snapshot
                .layouts
                .get(snapshot.active_layout as usize)
                .map_or("system default", String::as_str);
            text(
                world,
                summary,
                format!(
                    "Active layout: {active}. Repeat: {} Hz after {} ms.",
                    snapshot.settings.repeat_rate, snapshot.settings.repeat_delay_ms
                ),
            );
        } else {
            text(
                world,
                summary,
                "Keyboard settings are available in a Park session. Sandbox uses its host compositor's keyboard.",
            );
        }
        let busy = world.get::<Page>(page).unwrap().pending.is_some();
        let available = snapshot.is_some() && world.contains_resource::<HostRequests>();
        disable(world, save, !available || busy);
        disable(world, cancel, !available || busy);
        for field in fields {
            disable(world, field, !available);
        }
        world.get_mut::<Page>(page).unwrap().refresh = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input_focus::{FocusCause, InputFocus};

    fn fixture(park: bool) -> (App, Entity, std::sync::mpsc::Receiver<HostRequest>) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<InputFocus>();
        install(&mut app);
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        if park {
            app.insert_resource(HostRequests(sender));
            app.insert_resource(KeyboardSnapshot {
                revision: 1,
                settings: KeyboardSettings {
                    layout: "us".into(),
                    ..default()
                },
                ..default()
            });
        }
        let parent = app.world_mut().spawn(Node::default()).id();
        panel(app.world_mut(), parent);
        app.update();
        let page = app
            .world_mut()
            .query_filtered::<Entity, With<Page>>()
            .single(app.world())
            .unwrap();
        for field in app.world().get::<Page>(page).unwrap().fields {
            app.world_mut()
                .get_mut::<EditableText>(field)
                .unwrap()
                .pending_edits
                .clear();
        }
        (app, page, receiver)
    }

    fn set(world: &mut World, field: Entity, value: &str) {
        let mut input = world.get_mut::<EditableText>(field).unwrap();
        input.pending_edits.clear();
        input.editor.set_text(value);
    }

    #[test]
    fn keyboard_editor_submits_typed_settings_and_preserves_fields_focus_and_failed_drafts() {
        let (mut app, page, receiver) = fixture(true);
        let fields = app.world().get::<Page>(page).unwrap().fields;
        set(app.world_mut(), fields[0], "br");
        set(app.world_mut(), fields[3], "35");
        set(app.world_mut(), fields[4], "300");
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(fields[0], FocusCause::Navigated);
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Keyboard(request) = receiver.try_recv().unwrap() else {
            panic!("Expected keyboard settings")
        };
        assert_eq!(request.revision, 1);
        assert_eq!(request.settings.layout, "br");
        assert_eq!(request.settings.repeat_rate, 35);
        let mut snapshot = app.world().resource::<KeyboardSnapshot>().clone();
        snapshot.response = Some(crate::host::input::InputResponse {
            request: request.id,
            outcome: InputOutcome::Failed("fixture save failed".into()),
        });
        app.insert_resource(snapshot);
        app.update();
        assert_eq!(app.world().get::<Page>(page).unwrap().fields, fields);
        assert_eq!(
            app.world().get::<EditableText>(fields[0]).unwrap().value(),
            "br"
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(fields[0]));
        let status = app.world().get::<Page>(page).unwrap().status;
        assert!(
            app.world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("fixture save failed")
        );
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Keyboard(retry) = receiver.try_recv().unwrap() else {
            unreachable!()
        };
        assert!(retry.id > request.id);
        let mut snapshot = app.world().resource::<KeyboardSnapshot>().clone();
        snapshot.revision = 2;
        snapshot.settings = retry.settings;
        snapshot.response = Some(crate::host::input::InputResponse {
            request: retry.id,
            outcome: InputOutcome::Saved { warning: None },
        });
        app.insert_resource(snapshot);
        app.update();
        assert!(app.world().get::<Page>(page).unwrap().pending.is_none());
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(fields[0]));
        assert_eq!(
            app.world().get::<Text>(status).unwrap().0,
            "Keyboard settings saved."
        );
    }

    #[test]
    fn a_late_save_reply_preserves_newer_edits_and_identifies_the_unsaved_draft() {
        let (mut app, page, receiver) = fixture(true);
        let fields = app.world().get::<Page>(page).unwrap().fields;
        set(app.world_mut(), fields[3], "35");
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Keyboard(request) = receiver.try_recv().unwrap() else {
            unreachable!()
        };
        set(app.world_mut(), fields[3], "36");
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(fields[3], FocusCause::Navigated);
        let mut snapshot = app.world().resource::<KeyboardSnapshot>().clone();
        snapshot.revision = 2;
        snapshot.settings = request.settings;
        snapshot.response = Some(crate::host::input::InputResponse {
            request: request.id,
            outcome: InputOutcome::Saved { warning: None },
        });
        app.insert_resource(snapshot);
        app.update();
        assert_eq!(
            app.world().get::<EditableText>(fields[3]).unwrap().value(),
            "36"
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(fields[3]));
        let state = app.world().get::<Page>(page).unwrap();
        assert_eq!(state.origin.as_ref().unwrap().1.repeat_rate, 35);
        assert!(
            app.world()
                .get::<Text>(state.status)
                .unwrap()
                .0
                .contains("Newer edits remain unsaved")
        );
        Change::Save.apply(app.world_mut(), page);
        let HostRequest::Keyboard(next) = receiver.try_recv().unwrap() else {
            unreachable!()
        };
        assert_eq!(next.revision, 2);
        assert_eq!(next.settings.repeat_rate, 36);
    }

    #[test]
    fn external_changes_and_invalid_fields_never_replace_a_draft_and_sandbox_cannot_submit() {
        let (mut app, page, receiver) = fixture(true);
        let fields = app.world().get::<Page>(page).unwrap().fields;
        set(app.world_mut(), fields[0], "br");
        set(app.world_mut(), fields[3], "999");
        Change::Save.apply(app.world_mut(), page);
        assert!(receiver.try_recv().is_err());
        let mut snapshot = app.world().resource::<KeyboardSnapshot>().clone();
        snapshot.revision = 2;
        snapshot.settings.layout = "de".into();
        app.insert_resource(snapshot);
        app.update();
        assert_eq!(
            app.world().get::<EditableText>(fields[0]).unwrap().value(),
            "br"
        );
        Change::Cancel.apply(app.world_mut(), page);
        assert_eq!(
            app.world().get::<EditableText>(fields[0]).unwrap().value(),
            "de"
        );
        assert_eq!(
            app.world()
                .get::<Page>(page)
                .unwrap()
                .origin
                .as_ref()
                .unwrap()
                .0,
            2
        );
        let (mut sandbox, page, receiver) = fixture(false);
        Change::Save.apply(sandbox.world_mut(), page);
        assert!(receiver.try_recv().is_err());
        let save = sandbox.world().get::<Page>(page).unwrap().save;
        assert!(sandbox.world().get::<InteractionDisabled>(save).is_some());
    }
}
