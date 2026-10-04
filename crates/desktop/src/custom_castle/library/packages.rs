use super::*;
use nucleus::sand_package::{self as model, Entry, Identity, Package, Response};

#[derive(Component, Default)]
struct State {
    organ: Option<String>,
    offset: u32,
    next: Option<u32>,
    entries: Vec<Entry>,
    package: Option<Package>,
    public: bool,
    verified: bool,
    pending: Option<(String, model::Command)>,
    loaded: bool,
    dirty: bool,
    status: String,
}

#[derive(Component)]
struct View {
    root: Entity,
    contacts: Entity,
    entries: Entity,
    detail: Entity,
    status: Entity,
}

#[derive(Clone)]
enum Command {
    Source(Option<String>),
    Page(u32),
    Inspect(Identity),
    Save {
        name: Entity,
        text: Entity,
        credits: Entity,
        kind: model::Kind,
    },
    Public(bool),
    Receive,
    Add,
}

pub(super) fn show(world: &mut World, root: Entity, parent: Entity) {
    if world.get::<State>(root).is_none() {
        world.entity_mut(root).insert(State::default());
    }
    let body = panel::column(world, parent);
    crate::edit_mode::label(world, body, "Public Sands and Castles", 18.0);
    crate::edit_mode::label(
        world,
        body,
        "Choose an Organ contact to browse its published packages. Saved and received packages start private. Receiving preserves a package in your library; adding it to the canvas is a separate action.",
        14.0,
    );
    let contacts = panel::row(world, body);
    let status = crate::edit_mode::label(world, body, "", 14.0);
    let entries = panel::column(world, body);
    let detail = panel::column(world, body);
    crate::edit_mode::label(
        world,
        body,
        "Create a package from a component selected in the local Organ library below. Include its license text and any dependency licenses and credits. This saves an immutable private snapshot; publication is a separate choice.",
        14.0,
    );
    let name = panel::field(world, body, "License name", "");
    let text = panel::field(
        world,
        body,
        "License text (include dependency licenses)",
        "",
    );
    let credits = panel::field(world, body, "Credits (one per line)", "");
    for field in [text, credits] {
        if let Some(mut input) = world.get_mut::<bevy::text::EditableText>(field) {
            input.allow_newlines = true;
            input.visible_lines = Some(3.0);
            input.max_characters = Some(65_536);
        }
    }
    let controls = panel::row(world, body);
    for (caption, kind) in [
        ("Save private Sand package", model::Kind::Sand),
        ("Save private Castle package", model::Kind::Castle),
    ] {
        panel::button(
            world,
            controls,
            root,
            caption,
            Command::Save {
                name,
                text,
                credits,
                kind,
            },
        );
    }
    world.entity_mut(body).insert(View {
        root,
        contacts,
        entries,
        detail,
        status,
    });
    world.get_mut::<State>(root).unwrap().dirty = true;
    render(world, root);
}

fn request(world: &mut World, root: Entity, command: model::Command) -> Result<(), String> {
    let id = nucleus::new_uid("package-action");
    panel::send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action: Backend::SandPackage {
                request: command.clone(),
            },
        },
    )?;
    let mut state = world.get_mut::<State>(root).unwrap();
    state.pending = Some((id, command));
    state.status = "Loading package library…".into();
    state.dirty = true;
    Ok(())
}

impl Action for Command {
    fn apply(&self, world: &mut World, root: Entity) {
        if !world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled)
            || crate::laboratory::active(world)
        {
            return;
        }
        let Some(state) = world.get::<State>(root) else {
            return;
        };
        if state.pending.is_some() && !matches!(self, Self::Source(_)) {
            return;
        }
        let result = (|| {
            let state = world.get::<State>(root).unwrap();
            let organ = state.organ.clone();
            let identity = || {
                state
                    .package
                    .as_ref()
                    .map(|package| package.manifest.identity.clone())
                    .ok_or("Select a package first".to_string())
            };
            let command = match self {
                Self::Source(organ) => {
                    let mut state = world.get_mut::<State>(root).unwrap();
                    state.organ = organ.clone();
                    state.pending = None;
                    state.offset = 0;
                    state.entries.clear();
                    state.package = None;
                    state.next = None;
                    state.loaded = false;
                    model::Command::List {
                        organ: organ.clone(),
                        offset: 0,
                    }
                }
                Self::Page(offset) => {
                    world.get_mut::<State>(root).unwrap().offset = *offset;
                    model::Command::List {
                        organ,
                        offset: *offset,
                    }
                }
                Self::Inspect(identity) => {
                    world.get_mut::<State>(root).unwrap().package = None;
                    model::Command::Inspect {
                        organ,
                        identity: identity.clone(),
                    }
                }
                Self::Save {
                    name,
                    text,
                    credits,
                    kind,
                } => {
                    let library = world.get::<Library>(root).unwrap();
                    if library.organ.is_some() {
                        return Err("Select a component in the local Organ library first".into());
                    }
                    let record = library
                        .selected
                        .clone()
                        .ok_or("Select a saved local component first")?;
                    model::Command::Save {
                        record,
                        kind: kind.clone(),
                        licenses: vec![model::License {
                            name: panel::value(world, *name)?.trim().into(),
                            text: panel::value(world, *text)?,
                        }],
                        credits: panel::value(world, *credits)?
                            .lines()
                            .map(str::trim)
                            .filter(|line| !line.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    }
                }
                Self::Public(public) => {
                    if organ.is_some() {
                        return Err("Receive the package into your local library first".into());
                    }
                    model::Command::SetPublic {
                        identity: identity()?,
                        public: *public,
                    }
                }
                Self::Receive => model::Command::Receive {
                    organ: organ.ok_or("Choose an Organ contact first")?,
                    identity: identity()?,
                },
                Self::Add => {
                    if organ.is_some() {
                        return Err("Receive the package into your local library first".into());
                    }
                    if !state.verified {
                        return Err(
                            "The original Organ signing key is not verified on this device".into(),
                        );
                    }
                    model::Command::Enable {
                        identity: identity()?,
                    }
                }
            };
            request(world, root, command)
        })();
        let mut state = world.get_mut::<State>(root).unwrap();
        if let Err(error) = result {
            state.status = error;
        }
        state.dirty = true;
        render(world, root);
    }
}

fn runnable(package: &Package) -> Result<CustomCastle, String> {
    package.validate_execution()?;
    decode(&json!({"kind":"sand","head":package.manifest.name,"body":package.payload}))
}

pub(super) fn update(world: &mut World, root: Entity, messages: &[ServerMessage]) {
    if world.get::<State>(root).is_none() {
        return;
    }
    let contacts_id = world.get::<Library>(root).unwrap().contacts_id.clone();
    if messages.iter().any(|message| matches!(message, ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } if id == &contacts_id)) { world.get_mut::<State>(root).unwrap().dirty = true; }
    for message in messages {
        let pending = world.get::<State>(root).unwrap().pending.clone();
        let Some((pending_id, command)) = pending else {
            continue;
        };
        match message {
            ServerMessage::ActionOk {
                id,
                data: Some(data),
                ..
            } if id == &pending_id => {
                let response = serde_json::from_value::<Response>(data.clone());
                let mut state = world.get_mut::<State>(root).unwrap();
                state.pending = None;
                state.dirty = true;
                match response {
                    Ok(Response::Catalogue { entries, next }) => {
                        state.entries = entries;
                        state.next = next;
                        state.loaded = true;
                        state.status = if state.organ.is_some() {
                            "Public packages from this Organ"
                        } else {
                            "Local package library"
                        }
                        .into();
                    }
                    Ok(Response::Package {
                        package,
                        public,
                        origin_verified,
                    }) => {
                        state.public = public;
                        state.verified = origin_verified;
                        state.package = Some(package);
                        state.status = "Package metadata loaded".into();
                        if matches!(command, model::Command::Enable { .. }) {
                            let result = runnable(state.package.as_ref().unwrap());
                            drop(state);
                            let result = result.and_then(|castle| castle.spawn(world, root));
                            let mut state = world.get_mut::<State>(root).unwrap();
                            state.status = result
                                .map(|_| "Package added to your canvas".into())
                                .unwrap_or_else(|error| error);
                        }
                    }
                    Ok(Response::Saved { identity }) => {
                        state.organ = None;
                        state.offset = 0;
                        state.loaded = false;
                        state.package = None;
                        state.entries.clear();
                        state.status = if matches!(command, model::Command::Receive { .. }) { "Received privately. Select it in your local library to review and add it to the canvas." } else { "Package saved. Select it in your local library to review its publication and execution choices." }.into();
                        drop(state);
                        let _ = request(
                            world,
                            root,
                            model::Command::Inspect {
                                organ: None,
                                identity,
                            },
                        );
                    }
                    Err(error) => {
                        state.status = format!("Invalid package response: {error}");
                        state.loaded = true;
                    }
                }
            }
            ServerMessage::Error { id, message, .. } if id == &pending_id => {
                let mut state = world.get_mut::<State>(root).unwrap();
                state.pending = None;
                state.loaded = true;
                state.status = message.clone();
                state.dirty = true;
            }
            _ => {}
        }
    }
    let state = world.get::<State>(root).unwrap();
    if !state.loaded && state.pending.is_none() && !crate::laboratory::active(world) {
        let command = model::Command::List {
            organ: state.organ.clone(),
            offset: state.offset,
        };
        if let Err(error) = request(world, root, command) {
            let mut state = world.get_mut::<State>(root).unwrap();
            state.status = error;
            state.loaded = true;
            state.dirty = true;
        }
    }
    render(world, root);
}

fn render(world: &mut World, root: Entity) {
    if !world.get::<State>(root).is_some_and(|state| state.dirty) {
        return;
    }
    let contacts = world.get::<Library>(root).unwrap().contacts.clone();
    let views: Vec<_> = world
        .query::<&View>()
        .iter(world)
        .filter(|view| view.root == root)
        .map(|view| (view.contacts, view.entries, view.detail, view.status))
        .collect();
    for (source, list, detail, status) in views {
        let state = world.get::<State>(root).unwrap();
        let (organ, entries, package, public, verified, next, offset, message) = (
            state.organ.clone(),
            state.entries.clone(),
            state.package.clone(),
            state.public,
            state.verified,
            state.next,
            state.offset,
            state.status.clone(),
        );
        let source_name = organ
            .as_ref()
            .map(|uid| {
                contacts
                    .iter()
                    .find(|contact| contact["uid"] == *uid)
                    .and_then(|contact| contact["head"].as_str())
                    .unwrap_or(uid)
            })
            .unwrap_or("Local package library");
        panel::status(world, status, format!("{source_name}: {message}"));
        panel::clear(world, source);
        panel::button(
            world,
            source,
            root,
            "Local package library",
            Command::Source(None),
        );
        for contact in &contacts {
            if contact["slug"] == "local-organ" {
                continue;
            }
            if let Some(uid) = contact["uid"].as_str() {
                let name = contact["head"].as_str().unwrap_or(uid);
                panel::button(
                    world,
                    source,
                    root,
                    &format!("Public packages: {name}"),
                    Command::Source(Some(uid.into())),
                );
            }
        }
        panel::clear(world, list);
        if entries.is_empty() {
            crate::edit_mode::label(world, list, "No packages in this catalogue.", 14.0);
        }
        for entry in entries {
            let caption = format!(
                "{} · {:?} · v{} · {}",
                entry.name,
                entry.kind,
                entry.identity.version,
                if entry.public { "public" } else { "private" }
            );
            panel::button(
                world,
                list,
                root,
                &caption,
                Command::Inspect(entry.identity),
            );
        }
        let navigation = panel::row(world, list);
        panel::button(
            world,
            navigation,
            root,
            "Refresh packages",
            Command::Page(offset),
        );
        if offset > 0 {
            panel::button(
                world,
                navigation,
                root,
                "Previous packages",
                Command::Page(offset.saturating_sub(model::PAGE_SIZE)),
            );
        }
        if let Some(next) = next {
            panel::button(
                world,
                navigation,
                root,
                "Next packages",
                Command::Page(next),
            );
        }
        panel::clear(world, detail);
        if let Some(package) = package {
            let manifest = &package.manifest;
            crate::edit_mode::label(
                world,
                detail,
                if public {
                    "Visibility: public to Organ contacts"
                } else {
                    "Visibility: private"
                },
                14.0,
            );
            crate::edit_mode::label(
                world,
                detail,
                "Adding uses this device’s existing access. Linked Records are not included.",
                14.0,
            );
            for line in [
                format!(
                    "{} · {:?} · version {}",
                    manifest.name, manifest.kind, manifest.identity.version
                ),
                format!("Identity: {}", manifest.identity.id),
                format!("Author: {}", manifest.author),
                format!("Origin Organ: {}", manifest.identity.origin),
                format!(
                    "Signing key: {} · {}",
                    manifest.key_id,
                    if verified {
                        "verified origin"
                    } else {
                        "original Organ key not verified on this device"
                    }
                ),
                format!("Execution model: {}", manifest.execution),
                format!("Requested permissions: {}", manifest.permissions.join(", ")),
                format!("Digest: {}", package.digest),
            ] {
                crate::edit_mode::label(world, detail, &line, 14.0);
            }
            for license in &manifest.licenses {
                crate::edit_mode::label(
                    world,
                    detail,
                    &format!("License: {}\n{}", license.name, license.text),
                    14.0,
                );
            }
            for credit in &manifest.credits {
                crate::edit_mode::label(world, detail, &format!("Credit: {credit}"), 14.0);
            }
            if let Err(reason) = runnable(&package) {
                crate::edit_mode::label(
                    world,
                    detail,
                    &format!("Unavailable on this device: {reason}"),
                    14.0,
                );
            }
            let controls = panel::row(world, detail);
            if organ.is_some() {
                panel::button(
                    world,
                    controls,
                    root,
                    "Receive into local library (private)",
                    Command::Receive,
                );
            } else {
                if verified && runnable(&package).is_ok() {
                    panel::button(world, controls, root, "Add to canvas", Command::Add);
                }
                panel::button(
                    world,
                    controls,
                    root,
                    if public {
                        "Make package private"
                    } else {
                        "Publish package to Organ contacts"
                    },
                    Command::Public(!public),
                );
            }
        }
    }
    world.get_mut::<State>(root).unwrap().dirty = false;
}

#[cfg(test)]
mod tests;
