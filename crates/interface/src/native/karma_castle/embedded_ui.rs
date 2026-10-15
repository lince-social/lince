use bevy::{prelude::*, text::EditableText};
use engine::signal_endpoints::Request;
use store::signal_endpoints::Endpoint;

use super::*;

#[derive(Component)]
struct Devices {
    panel: Entity,
    rows: Vec<Endpoint>,
    audience: String,
    pending: Option<String>,
    pending_save: bool,
    editing: bool,
    target: Option<String>,
    revision: Option<i64>,
    enabled: bool,
    fields: [String; 7],
    issued_token: Option<(String, String)>,
}

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
}

#[derive(Clone)]
enum Control {
    Refresh,
    New,
    Edit(String),
    SetEnabled(String, i64, bool),
    Save,
    Close,
    CopySetup(String),
    IssueToken(String, i64),
    CopyToken(String),
}

fn defaults() -> [String; 7] {
    [
        String::new(),
        String::new(),
        String::new(),
        "60".into(),
        "0".into(),
        "100".into(),
        (chrono::Utc::now() + chrono::Duration::days(90)).to_rfc3339(),
    ]
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let panel = ui::stack(world, parent);
    world.entity_mut(panel).insert(Name::new("Karma embedded"));
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world.entity_mut(owner).insert(Devices {
        panel,
        rows: Vec::new(),
        audience: String::new(),
        pending: None,
        editing: false,
        target: None,
        revision: None,
        enabled: false,
        fields: defaults(),
        pending_save: false,
        issued_token: None,
    });
    render(world, owner);
}

pub(super) fn toggle(world: &mut World, owner: Entity) {
    let panel = world.get::<Devices>(owner).unwrap().panel;
    let opening = world.get::<Node>(panel).unwrap().display == Display::None;
    world.get_mut::<Node>(panel).unwrap().display = if opening {
        Display::Flex
    } else {
        Display::None
    };
    if opening {
        Control::Refresh.apply(world, owner);
    }
}

fn capture(world: &mut World, owner: Entity) {
    let fields: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    if let Some(mut state) = world.get_mut::<Devices>(owner) {
        for (index, value) in fields {
            state.fields[index] = value;
        }
    }
}

fn submit(world: &mut World, owner: Entity, request: Request) {
    let saving = matches!(request, Request::Save { .. });
    let id = nucleus::new_uid("embedded-ui");
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::SignalEndpoints { request },
        },
    ) {
        Ok(()) => {
            world.get_mut::<Devices>(owner).unwrap().pending = Some(id);
            world.get_mut::<Devices>(owner).unwrap().pending_save = saving;
            status(world, owner, "Waiting for Lince to confirm device access…");
        }
        Err(error) => status(world, owner, error),
    }
}

impl crate::native::actions::Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner)
            || world
                .get::<Devices>(owner)
                .is_none_or(|state| state.pending.is_some())
        {
            return;
        }
        capture(world, owner);
        match self {
            Self::IssueToken(endpoint, revision) => {
                submit(
                    world,
                    owner,
                    Request::IssueToken {
                        endpoint: endpoint.clone(),
                        expected_revision: *revision,
                    },
                );
                return;
            }
            Self::CopyToken(endpoint) => {
                let Some((_, token)) = world
                    .get::<Devices>(owner)
                    .unwrap()
                    .issued_token
                    .as_ref()
                    .filter(|(uid, _)| uid == endpoint)
                else {
                    return;
                };
                let token = token.clone();
                let result = world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .ok_or_else(|| "Clipboard is unavailable".to_string())
                    .and_then(|mut clipboard| {
                        clipboard.set_text(token).map_err(|error| error.to_string())
                    });
                status(
                    world,
                    owner,
                    result.map_or_else(|error| error, |()| "Access token copied".into()),
                );
                return;
            }
            Self::CopySetup(uid) => {
                let state = world.get::<Devices>(owner).unwrap();
                let Some(row) = state.rows.iter().find(|row| &row.uid == uid) else {
                    return;
                };
                let config = serde_json::json!({"audience": state.audience, "endpoint": row.uid, "signal": row.signal_uid,
                    "credential_epoch": row.credential_epoch, "last_sequence": row.last_sequence, "cooldown_seconds": row.cooldown_seconds,
                    "path": format!("/signals/{}/readings", row.signal_uid)}).to_string();
                let result = world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .ok_or_else(|| "Clipboard is unavailable".to_string())
                    .and_then(|mut clipboard| {
                        clipboard
                            .set_text(config)
                            .map_err(|error| error.to_string())
                    });
                status(
                    world,
                    owner,
                    result.map_or_else(
                        |error| error,
                        |()| {
                            "Endpoint setup copied; add your public HTTPS server address in firmware"
                                .into()
                        },
                    ),
                );
                return;
            }
            Self::Refresh => {
                submit(world, owner, Request::Inspect);
                return;
            }
            Self::SetEnabled(device, revision, enabled) => {
                submit(
                    world,
                    owner,
                    Request::SetEnabled {
                        endpoint: device.clone(),
                        expected_revision: *revision,
                        enabled: *enabled,
                    },
                );
                return;
            }
            Self::New => {
                let mut state = world.get_mut::<Devices>(owner).unwrap();
                state.target = None;
                state.revision = None;
                state.enabled = false;
                state.fields = defaults();
                state.editing = true;
            }
            Self::Edit(uid) => {
                let row = world
                    .get::<Devices>(owner)
                    .unwrap()
                    .rows
                    .iter()
                    .find(|row| row.uid == *uid)
                    .cloned();
                let Some(row) = row else { return };
                let mut state = world.get_mut::<Devices>(owner).unwrap();
                state.target = Some(row.uid);
                state.revision = Some(row.revision);
                state.enabled = row.enabled;
                state.fields = [
                    row.name,
                    row.public_key,
                    row.signal_uid,
                    row.cooldown_seconds.to_string(),
                    row.minimum,
                    row.maximum,
                    chrono::DateTime::from_timestamp(row.expires_at, 0)
                        .map(|date| date.to_rfc3339())
                        .unwrap_or_default(),
                ];
                state.editing = true;
            }
            Self::Close => world.get_mut::<Devices>(owner).unwrap().editing = false,
            Self::Save => {
                let state = world.get::<Devices>(owner).unwrap();
                let Ok(cooldown_seconds) = state.fields[3].parse() else {
                    status(
                        world,
                        owner,
                        "Cooldown must be an integer number of seconds",
                    );
                    return;
                };
                let request = Request::Save {
                    endpoint: state.target.clone(),
                    expected_revision: state.revision,
                    name: state.fields[0].clone(),
                    public_key: state.fields[1].clone(),
                    signal_slug: state.fields[2].clone(),
                    enabled: state.enabled,
                    cooldown_seconds,
                    minimum: state.fields[4].clone(),
                    maximum: state.fields[5].clone(),
                    expires_at: state.fields[6].clone(),
                };
                submit(world, owner, request);
                return;
            }
        }
        render(world, owner);
    }
}

fn render(world: &mut World, owner: Entity) {
    let state = world.get::<Devices>(owner).unwrap();
    let panel = state.panel;
    let rows = state.rows.clone();
    let fields = state.fields.clone();
    let editing = state.editing;
    let new = state.target.is_none();
    let audience = state.audience.clone();
    let token_endpoint = state
        .issued_token
        .as_ref()
        .map(|(endpoint, _)| endpoint.clone());
    if let Some(children) = world.get::<Children>(panel) {
        for child in children.iter().collect::<Vec<_>>() {
            world.entity_mut(child).despawn();
        }
    }
    crate::native::edit_mode::label(world, panel, "Embedded Signals", 16.0);
    let line = ui::row(world, panel);
    crate::native::castle_feed::button(world, line, owner, "Enroll device", Control::New);
    crate::native::castle_feed::button(world, line, owner, "Refresh access", Control::Refresh);
    crate::native::edit_mode::label(
        world,
        panel,
        "Each device can update only its granted Signal. Access starts disabled. Use an access token or signed requests; private keys stay on the device.",
        13.0,
    );
    if editing {
        for (index, label) in [
            (0, "Device name"),
            (
                1,
                "Optional device public key · P-256 compressed SEC1, lowercase hex",
            ),
            (
                2,
                if new {
                    "Signal slug · existing or new"
                } else {
                    "Granted Signal UID"
                },
            ),
            (3, "Minimum seconds between readings (1–86400)"),
            (4, "Minimum accepted value"),
            (5, "Maximum accepted value"),
            (6, "Access expiry · RFC3339 date and time"),
        ] {
            crate::native::edit_mode::label(world, panel, label, 13.0);
            if index == 2 && !new {
                crate::native::edit_mode::label(world, panel, &fields[index], 13.0);
                continue;
            }
            let typography = world.resource::<crate::native::theme::Typography>();
            let bundle = crate::native::sand::text_editor(&fields[index], typography, 0);
            let entity = world
                .spawn(bundle)
                .insert((
                    ChildOf(panel),
                    Input { owner, index },
                    Node {
                        width: percent(100),
                        min_height: px(30),
                        ..default()
                    },
                ))
                .id();
            let mut text = world.get_mut::<EditableText>(entity).unwrap();
            text.allow_newlines = false;
            text.visible_lines = Some(1.0);
            text.max_characters = Some(128);
            let mut accessible = accesskit::Node::new(accesskit::Role::TextInput);
            accessible.set_label(label);
            world
                .entity_mut(entity)
                .insert(bevy::a11y::AccessibilityNode(accessible));
        }
        let line = ui::row(world, panel);
        crate::native::castle_feed::button(world, line, owner, "Save access", Control::Save);
        crate::native::castle_feed::button(world, line, owner, "Close editor", Control::Close);
    }
    if !audience.is_empty() {
        crate::native::edit_mode::label(
            world,
            panel,
            &format!("Server identity: {audience}"),
            12.0,
        );
    }
    for row in rows {
        let expired = row.expires_at <= chrono::Utc::now().timestamp();
        let line = ui::row(world, panel);
        crate::native::edit_mode::label(
            world,
            line,
            &format!(
                "{} · {} · {}s cooldown · {} to {}",
                row.name,
                if !row.enabled {
                    "disabled"
                } else if expired {
                    "expired"
                } else {
                    "enabled"
                },
                row.cooldown_seconds,
                row.minimum,
                row.maximum
            ),
            14.0,
        );
        let line = ui::row(world, panel);
        world.get_mut::<Node>(line).unwrap().flex_wrap = FlexWrap::Wrap;
        crate::native::castle_feed::button(
            world,
            line,
            owner,
            "Edit access",
            Control::Edit(row.uid.clone()),
        );
        crate::native::castle_feed::button(
            world,
            line,
            owner,
            "Copy setup",
            Control::CopySetup(row.uid.clone()),
        );
        crate::native::castle_feed::button(
            world,
            line,
            owner,
            "Issue token",
            Control::IssueToken(row.uid.clone(), row.revision),
        );
        if token_endpoint.as_deref() == Some(&row.uid) {
            crate::native::castle_feed::button(
                world,
                line,
                owner,
                "Copy issued token",
                Control::CopyToken(row.uid.clone()),
            );
        }
        crate::native::castle_feed::button(
            world,
            line,
            owner,
            if row.enabled { "Disable" } else { "Enable" },
            Control::SetEnabled(row.uid.clone(), row.revision, !row.enabled),
        );
        crate::native::edit_mode::label(
            world,
            panel,
            &format!(
                "Device {} · Signal {} · access generation {} · sequence {}",
                row.uid, row.signal_uid, row.credential_epoch, row.last_sequence
            ),
            12.0,
        );
        crate::native::edit_mode::label(
            world,
            panel,
            &format!(
                "Last accepted reading: {} · expires: {}",
                row.last_received_at
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|at| at.to_rfc3339())
                    .unwrap_or_else(|| "none".into()),
                chrono::DateTime::from_timestamp(row.expires_at, 0)
                    .map(|at| at.to_rfc3339())
                    .unwrap_or_default()
            ),
            12.0,
        );
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &Devices)>()
        .iter(world)
        .find(|(_, state)| state.pending.as_ref() == Some(id))
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    capture(world, owner);
    world.get_mut::<Devices>(owner).unwrap().pending = None;
    match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => {
            let rows = serde_json::from_value(data["endpoints"].clone());
            let Ok(rows) = rows else {
                status(
                    world,
                    owner,
                    "Could not read device access; refresh to retry",
                );
                return true;
            };
            let mut state = world.get_mut::<Devices>(owner).unwrap();
            state.rows = rows;
            state.audience = data["audience"].as_str().unwrap_or_default().into();
            if let (Some(endpoint), Some(token)) = (
                data["issued_token"]["endpoint"].as_str(),
                data["issued_token"]["token"].as_str(),
            ) {
                state.issued_token = Some((endpoint.into(), token.into()));
            }
            if state.pending_save && data["saved"].is_string() {
                state.editing = false;
            }
            render(world, owner);
            status(
                world,
                owner,
                if data["issued_token"].is_object() {
                    "Access token issued; copy it before closing this castle"
                } else {
                    "Device access confirmed by Lince"
                },
            );
        }
        ServerMessage::Error { message, .. } => status(world, owner, message),
        _ => status(
            world,
            owner,
            "Device response unavailable; refresh to retry",
        ),
    }
    true
}

#[cfg(test)]
mod tests;
