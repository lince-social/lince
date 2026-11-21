// non-lean-yet
// Scope: mod presentation, mod setup, struct Devices, enum OwnedReply.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 175, 204, 220,
//   272, 297, 301, 302, 303, 305, 306, 325, 327, 328, 329, 332, 333, 334.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (127).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   138, 139.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (92, 111, 112, 113, 147, 180, 181, 213, 235, 280, 290, 296, 313, 315, 357, 372).

use bevy::{ecs::entity_disabling::Disabled, prelude::*, text::EditableText};
use engine::signal_endpoints::Request;
use store::signal_endpoints::Endpoint;

use super::*;
mod presentation;
mod setup;

#[derive(Component)]
struct Devices {
    panel: Entity,
    rows: Arc<Vec<Arc<Endpoint>>>,
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

impl replies::Pending for Devices {
    fn pending_id(&self) -> Option<&str> {
        self.pending.as_deref()
    }
}

pub(super) enum OwnedReply {
    Setup(Entity),
    Devices(Entity),
}

pub(super) struct ReplyOwners {
    setup: replies::Owners<setup::Form>,
    devices: replies::Owners<Devices>,
}

impl FromWorld for ReplyOwners {
    fn from_world(world: &mut World) -> Self {
        Self {
            setup: replies::Owners::from_world(world),
            devices: replies::Owners::from_world(world),
        }
    }
}

impl ReplyOwners {
    pub(super) fn select(&mut self, world: &World, id: &str) -> Option<OwnedReply> {
        self.setup
            .first(world, id)
            .map(OwnedReply::Setup)
            .or_else(|| self.devices.first(world, id).map(OwnedReply::Devices))
    }
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
    world.entity_mut(panel).insert(Name::new("Karma ailuros"));
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world.entity_mut(owner).insert(Devices {
        panel,
        rows: Arc::default(),
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
    setup::spawn(world, owner, parent);
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
    setup::toggle(world, owner, opening);
}

fn capture(world: &mut World, owner: Entity) {
    world
        .run_system_cached_with(capture_fields, owner)
        .expect("capture Ailuros access fields");
}

fn capture_fields(
    In(owner): In<Entity>,
    inputs: Query<(&Input, &EditableText)>,
    mut devices: Query<&mut Devices, Allow<Disabled>>,
) {
    let _capture = nucleus::performance::Span::new("ailuros-field-capture");
    let Ok(mut state) = devices.get_mut(owner) else { return };
    for (input, text) in &inputs {
        if input.owner == owner && text.value() != state.fields[input.index].as_str() {
            crate::controls::copy_value(text, &mut state.fields[input.index]);
            nucleus::performance::add("ailuros-fields-captured", 1);
        }
    }
}

fn publish(world: &mut World, owner: Entity, rows: Vec<Endpoint>, audience: &str) {
    let _publish = nucleus::performance::Span::new("ailuros-row-publication");
    let mut state = world.get_mut::<Devices>(owner).unwrap();
    let previous = &state.rows;
    let common = previous.iter().zip(&rows)
        .take_while(|(old, new)| old.as_ref() == *new)
        .count();
    if common != previous.len() || common != rows.len() {
        let count = rows.len();
        let rows = rows.into_iter().enumerate().map(|(index, row)| {
            previous.get(index)
                .filter(|old| index < common || old.as_ref() == &row)
                .cloned()
                .unwrap_or_else(|| Arc::new(row))
        }).collect();
        state.rows = Arc::new(rows);
        nucleus::performance::add("ailuros-rows-published", count as u64);
    }
    if state.audience != audience {
        audience.clone_into(&mut state.audience);
    }
}

fn submit(world: &mut World, owner: Entity, request: Request) {
    let saving = matches!(request, Request::Save { .. });
    let id = nucleus::new_uid("ailuros-ui");
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
                let config = serde_json::json!({"audience": state.audience, "device": row.uid, "signal": row.signal_uid,
                    "key_epoch": row.credential_epoch, "last_sequence": row.last_sequence, "cooldown_seconds": row.cooldown_seconds,
                    "path": "/ailuros/readings", "stream_path": format!("/ailuros/streams/{}", row.signal_uid)}).to_string();
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
                state.target = Some(row.uid.clone());
                state.revision = Some(row.revision);
                state.enabled = row.enabled;
                state.fields = [
                    row.name.clone(),
                    row.public_key.clone(),
                    row.signal_uid.clone(),
                    row.cooldown_seconds.to_string(),
                    row.minimum.clone(),
                    row.maximum.clone(),
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
    presentation::render(world, owner);
}

pub(super) fn receive(world: &mut World, reply: OwnedReply, message: &ServerMessage) {
    let owner = match reply {
        OwnedReply::Setup(owner) => {
            setup::receive_owned(world, owner, message);
            return;
        }
        OwnedReply::Devices(owner) => owner,
    };
    capture(world, owner);
    world.get_mut::<Devices>(owner).unwrap().pending = None;
    match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => {
            let rows = Vec::<Endpoint>::deserialize(&data["endpoints"]);
            let Ok(rows) = rows else {
                status(
                    world,
                    owner,
                    "Could not read device access; refresh to retry",
                );
                return;
            };
            publish(world, owner, rows, data["audience"].as_str().unwrap_or_default());
            let mut state = world.get_mut::<Devices>(owner).unwrap();
            if let (Some(endpoint), Some(token)) = (
                data["issued_token"]["endpoint"].as_str(),
                data["issued_token"]["token"].as_str(),
            ) {
                state.issued_token = Some((endpoint.into(), token.into()));
            }
            if state.pending_save && data["saved"].is_string() {
                state.editing = false;
            }
            setup::observe(world, owner, data);
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
}

pub(super) fn maintain(world: &mut World) {
    setup::maintain(world);
}

#[cfg(test)]
mod tests;
