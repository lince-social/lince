use crate::{
    actions::Action,
    protein_area::{RecordBinding, Source},
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::EditableText,
};
use cell::{
    FioteAuthKind, FioteAuthMethod, FioteProvider, FioteRequest, FioteSecret, FioteSettings,
    FioteStatus,
};

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Manage,
    Agent,
    Closed,
    Providers,
    Methods,
    Credentials,
    Unlock,
    Browser,
}

#[derive(Component)]
struct Panel {
    view_uid: String,
    pending_command: Option<String>,
    binding: RecordBinding,
    status: Entity,
    content: Entity,
    step: Step,
    provider: Option<FioteProvider>,
    method: Option<FioteAuthMethod>,
    labels: Vec<String>,
    selection: usize,
    choices: Vec<Entity>,
    fields: Vec<Entity>,
    pending: Option<String>,
    saved: Option<FioteStatus>,
    poll: std::time::Instant,
    automatic: bool,
}

#[derive(Component)]
struct SecretField;
#[derive(Component)]
struct Mask(Entity);
#[derive(Resource, Default)]
struct Active(Option<Entity>);
#[derive(Component)]
struct ThreadControl {
    area: Entity,
    view_uid: String,
    record: String,
    thread: String,
    status: Entity,
    pending: Option<String>,
    tools_request: bool,
    inspecting: bool,
    instructions: Entity,
    poll: std::time::Instant,
    toggle: Entity,
    copy: Entity,
    stop: Entity,
    connection: Option<cell::FioteToolConnection>,
}

pub struct Plugin;
impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Active>()
            .init_resource::<InputFocus>()
            .add_systems(
                Update,
                (
                    receive.after(crate::cell_bridge::ReceiveCell),
                    masks,
                    keyboard,
                    poll,
                    poll_threads,
                ),
            )
            .add_systems(
                PreUpdate,
                protect_keys.before(bevy::text::EditableTextSystems),
            );
    }
}

fn field(world: &mut World, parent: Entity, title: &str, secret: bool) -> Entity {
    crate::edit_mode::label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0);
    let entity = world.spawn((bundle, ChildOf(parent))).id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(4096);
    if secret {
        world
            .entity_mut(entity)
            .remove::<crate::token_style::TextToken>()
            .insert((
                SecretField,
                TextColor(Color::NONE),
                bevy::a11y::AccessibilityNode(accesskit::Node::new(accesskit::Role::PasswordInput)),
            ));
        let font = world.resource::<crate::theme::Typography>().text(18.0);
        world.spawn((
            Text::new(""),
            font,
            crate::token_style::text(crate::tokens::Token::Ink),
            Mask(entity),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(6),
                top: px(6),
                ..default()
            },
            ChildOf(entity),
        ));
    }
    entity
}

pub fn populate(world: &mut World, parent: Entity, binding: RecordBinding) {
    if binding.source != Source::Local || !nucleus::valid_uid(&binding.uid, "r") {
        return;
    }
    let automatic = world
        .get::<crate::area::InfluenceArea>(binding.area)
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.fiote);
    let owner = world
        .spawn((
            Node {
                display: if automatic {
                    Display::Flex
                } else {
                    Display::None
                },
                flex_direction: FlexDirection::Column,
                width: percent(100),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
            crate::sand_store::SandCredits(super::CREDITS),
        ))
        .id();
    let status =
        crate::edit_mode::label(world, owner, "Choose a Fiote and connect its agent.", 14.0);
    crate::description::button(world, owner, owner, "Manage Fiote", management::Open);
    let content = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    world.entity_mut(owner).insert(Panel {
        view_uid: binding.uid.clone(),
        pending_command: None,
        binding: binding.clone(),
        status,
        content,
        step: if automatic {
            Step::Manage
        } else {
            Step::Closed
        },
        provider: None,
        method: None,
        labels: Vec::new(),
        selection: 0,
        choices: Vec::new(),
        fields: Vec::new(),
        pending: None,
        saved: None,
        poll: std::time::Instant::now(),
        automatic,
    });
    request(
        world,
        owner,
        if automatic {
            FioteRequest::Prepare {
                record: binding.uid,
            }
        } else {
            FioteRequest::Inspect {
                record: binding.uid,
            }
        },
    );
}

fn open_setup(world: &mut World, owner: Entity) -> bool {
    let panel = world.get::<Panel>(owner).unwrap();
    if panel.automatic {
        return false;
    }
    let uid = panel.binding.uid.clone();
    let mut cursor = Some(panel.binding.area);
    while let Some(entity) = cursor {
        if world.get::<crate::workspace::Workspaces>(entity).is_some() {
            crate::full_record::open_fiote(world, entity, &uid);
            return true;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

fn send(world: &World, entity: Entity, request: FioteRequest) -> Result<String, String> {
    if crate::laboratory::suspended(world, entity) {
        return Err("Workspace is suspended.".into());
    }
    let id = nucleus::new_uid("fiote");
    world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .ok_or("The local Cell is not connected.")?
        .outgoing
        .try_send(cell::ClientMessage::Fiote {
            id: id.clone(),
            request,
        })
        .map_err(|_| "The local Cell is busy or disconnected.")?;
    Ok(id)
}

fn request(world: &mut World, owner: Entity, message: FioteRequest) {
    let status = world.get::<Panel>(owner).unwrap().status;
    match send(world, owner, message) {
        Ok(id) => {
            world.get_mut::<Panel>(owner).unwrap().pending = Some(id);
            world.get_mut::<Text>(status).unwrap().0 = "Connecting…".into();
        }
        Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
    }
}

fn show(world: &mut World, owner: Entity, step: Step) {
    let Some(panel) = world.get::<Panel>(owner) else {
        return;
    };
    let (content, provider, method, saved) = (
        panel.content,
        panel.provider.clone(),
        panel.method.clone(),
        panel.saved.clone(),
    );
    if let Some(children) = world.get::<Children>(content) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.despawn(child);
        }
    }
    let mut fields = Vec::new();
    let mut choices = Vec::new();
    let labels: Vec<String> = match step {
        Step::Providers => saved
            .as_ref()
            .map(|saved| {
                saved
                    .providers
                    .iter()
                    .map(|provider| provider.label.clone())
                    .collect()
            })
            .unwrap_or_default(),
        Step::Methods => provider
            .as_ref()
            .map(|provider| {
                provider
                    .auth_methods
                    .iter()
                    .map(|method| method.label.clone())
                    .collect()
            })
            .unwrap_or_default(),
        Step::Agent => saved
            .as_ref()
            .and_then(|saved| saved.agent_info.as_ref())
            .and_then(|info| info["providers"].as_array())
            .map(|providers| {
                providers
                    .iter()
                    .filter_map(|provider| provider["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    match step {
        Step::Providers | Step::Methods => {
            if step == Step::Providers {
                crate::description::button(
                    world,
                    content,
                    owner,
                    "Connect an agent · ACP",
                    agent::Open,
                );
            }
            crate::edit_mode::label(
                world,
                content,
                if step == Step::Providers {
                    "Choose a provider · ↑ ↓ Enter"
                } else {
                    "Choose how to connect · ↑ ↓ Enter"
                },
                16.0,
            );
            let list = world
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        max_height: px(248),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(content),
                ))
                .id();
            crate::scroll_sand::attach(world, list);
            for (index, label) in labels.iter().enumerate() {
                let button = crate::description::button(world, list, owner, label, Choose(index));
                let mut node = world.get_mut::<Node>(button).unwrap();
                node.height = px(32);
                node.flex_shrink = 0.0;
                choices.push(world.get::<Children>(button).unwrap()[0]);
            }
        }
        Step::Manage => management::show(world, owner, content, saved.as_ref()),
        Step::Agent => {
            agent::show(
                world,
                owner,
                content,
                &mut fields,
                &mut choices,
                saved.as_ref(),
            );
        }
        Step::Credentials => {
            let Some(provider) = provider.as_ref() else {
                return;
            };
            let Some(method) = method.as_ref() else {
                return;
            };
            crate::edit_mode::label(world, content, &provider.label, 16.0);
            fields.push(field(
                world,
                content,
                if provider.model_optional {
                    "Model · optional"
                } else {
                    "Model"
                },
                false,
            ));
            let key_group = world
                .spawn((
                    Node {
                        display: if method.kind == FioteAuthKind::ApiKey {
                            Display::Flex
                        } else {
                            Display::None
                        },
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    ChildOf(content),
                ))
                .id();
            fields.push(field(world, key_group, "API key", true));
            let vault_group = world
                .spawn((
                    Node {
                        display: if method.kind == FioteAuthKind::None {
                            Display::None
                        } else {
                            Display::Flex
                        },
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    ChildOf(content),
                ))
                .id();
            fields.push(field(
                world,
                vault_group,
                if saved.as_ref().is_some_and(|saved| saved.vault_exists) {
                    "Vault password"
                } else {
                    "Choose a password for the provider vault"
                },
                true,
            ));
            let options = world
                .spawn((
                    Node {
                        display: Display::None,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                    ChildOf(content),
                ))
                .id();
            crate::description::button(world, content, owner, "Options", Options(options));
            let endpoint_group = world
                .spawn((
                    Node {
                        display: if provider.endpoint.is_empty() {
                            Display::None
                        } else {
                            Display::Flex
                        },
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    ChildOf(options),
                ))
                .id();
            fields.push(field(
                world,
                endpoint_group,
                "Endpoint · blank uses the adapter default",
                false,
            ));
            fields.push(field(
                world,
                options,
                "Folder for new files · optional",
                false,
            ));
            crate::description::button(
                world,
                content,
                owner,
                if method.kind == FioteAuthKind::Browser {
                    "Continue in browser"
                } else {
                    "Connect"
                },
                Continue,
            );
            world.resource_mut::<InputFocus>().set(
                if provider.model_optional && method.kind != FioteAuthKind::None {
                    fields[2]
                } else {
                    fields[0]
                },
                FocusCause::Navigated,
            );
        }
        Step::Unlock => {
            fields.push(field(
                world,
                content,
                "Unlock the provider vault for this Cell session",
                true,
            ));
            crate::description::button(world, content, owner, "Unlock", Continue);
            crate::description::button(world, content, owner, "Choose another provider", Back);
            world
                .resource_mut::<InputFocus>()
                .set(fields[0], FocusCause::Navigated);
        }
        Step::Browser => {
            crate::edit_mode::label(world, content, "Finish signing in in your browser.", 14.0);
            crate::description::button(world, content, owner, "Open browser again", OpenBrowser);
        }
        Step::Closed => {}
    }
    if step != Step::Closed {
        crate::description::button(
            world,
            content,
            owner,
            if matches!(step, Step::Agent | Step::Manage) {
                "Close"
            } else {
                "Cancel"
            },
            Close,
        );
    }
    let mut panel = world.get_mut::<Panel>(owner).unwrap();
    panel.step = step;
    panel.fields = fields;
    panel.choices = choices;
    panel.labels = labels;
    if matches!(step, Step::Providers | Step::Methods | Step::Agent) {
        panel.selection = 0;
    }
    world.resource_mut::<Active>().0 = (step != Step::Closed).then_some(owner);
    if matches!(step, Step::Providers | Step::Methods | Step::Agent) {
        world
            .resource_mut::<InputFocus>()
            .set(owner, FocusCause::Navigated);
        highlight(world, owner);
    }
}

fn highlight(world: &mut World, owner: Entity) {
    let panel = world.get::<Panel>(owner).unwrap();
    let (selection, choices, labels) =
        (panel.selection, panel.choices.clone(), panel.labels.clone());
    if let Some(list) = choices
        .get(selection)
        .and_then(|label| world.get::<ChildOf>(*label))
        .and_then(|button| world.get::<ChildOf>(button.parent()))
        .map(ChildOf::parent)
        && let Some(mut position) = world.get_mut::<ScrollPosition>(list)
    {
        let top = selection as f32 * 36.0;
        position.0.y = position.0.y.min(top).max(top + 32.0 - 248.0).max(0.0);
    }
    for (index, label) in choices.into_iter().enumerate() {
        world.get_mut::<Text>(label).unwrap().0 = format!(
            "{}{}",
            if index == selection { "› " } else { "  " },
            labels[index]
        );
    }
}

#[derive(Clone)]
struct Choose(usize);
impl Action for Choose {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut panel) = world.get_mut::<Panel>(owner) else {
            return;
        };
        if panel.pending.is_some() {
            return;
        }
        let next = if panel.step == Step::Providers {
            let Some(provider) = panel
                .saved
                .as_ref()
                .and_then(|saved| saved.providers.get(self.0))
                .cloned()
            else {
                return;
            };
            let next = if provider.auth_methods.len() == 1 {
                panel.method = provider.auth_methods.first().cloned();
                Step::Credentials
            } else {
                Step::Methods
            };
            panel.provider = Some(provider);
            next
        } else {
            panel.method = panel
                .provider
                .as_ref()
                .and_then(|provider| provider.auth_methods.get(self.0))
                .cloned();
            Step::Credentials
        };
        show(world, owner, next);
    }
}

#[derive(Clone)]
struct Back;
impl Action for Back {
    fn apply(&self, world: &mut World, owner: Entity) {
        show(world, owner, Step::Providers);
    }
}
#[derive(Clone)]
struct Options(Entity);
impl Action for Options {
    fn apply(&self, world: &mut World, _: Entity) {
        if let Some(mut node) = world.get_mut::<Node>(self.0) {
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}
#[derive(Clone)]
struct Close;
impl Action for Close {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        if panel.step == Step::Browser || panel.pending.is_some() {
            let record = panel.binding.uid.clone();
            request(world, owner, FioteRequest::BrowserCancel { record });
        }
        show(world, owner, Step::Closed);
    }
}

fn value(world: &World, entity: Entity) -> Result<String, String> {
    let text = world
        .get::<EditableText>(entity)
        .ok_or("Field is closed.")?;
    if text.is_composing() || crate::record_view::pending_text(text) {
        return Err("Finish editing this field first.".into());
    }
    Ok(text.value().to_string())
}

#[derive(Clone)]
struct Continue;
impl Action for Continue {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        if panel.pending.is_some() {
            return;
        }
        let status = panel.status;
        let result = (|| {
            let record = panel.binding.uid.clone();
            let fields = &panel.fields;
            if panel.step == Step::Unlock {
                return Ok(FioteRequest::Unlock {
                    record,
                    password: FioteSecret(value(world, fields[0])?),
                });
            }
            let provider = panel.provider.as_ref().ok_or("Choose a provider.")?;
            let method = panel.method.as_ref().ok_or("Choose a login method.")?;
            let settings = FioteSettings {
                enabled: true,
                provider: provider.id.clone(),
                auth_method: method.id.clone(),
                model: value(world, fields[0])?,
                endpoint: value(world, fields[3])?,
                directory: value(world, fields[4])?.into(),
            };
            if method.kind == FioteAuthKind::Browser {
                return Ok(FioteRequest::BrowserStart {
                    record,
                    password: FioteSecret(value(world, fields[2])?),
                    settings,
                });
            }
            let key = value(world, fields[1])?;
            Ok::<_, String>(FioteRequest::Configure {
                record,
                settings,
                api_key: (!key.is_empty()).then_some(FioteSecret(key)),
                password: if method.kind == FioteAuthKind::None {
                    None
                } else {
                    Some(FioteSecret(value(world, fields[2])?))
                },
            })
        })();
        match result {
            Ok(message) => {
                request(world, owner, message);
                let fields = world.get::<Panel>(owner).unwrap().fields.clone();
                for field in fields {
                    if world.get::<SecretField>(field).is_some() {
                        world
                            .get_mut::<EditableText>(field)
                            .unwrap()
                            .editor
                            .set_text("");
                    }
                }
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

#[derive(Clone)]
struct OpenBrowser;
impl Action for OpenBrowser {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        let Some(url) = panel.saved.as_ref().and_then(|saved| {
            saved.login_url.clone().or_else(|| {
                saved.agent_info.as_ref().and_then(|info| {
                    info["deviceCode"]["verificationUri"]
                        .as_str()
                        .map(str::to_owned)
                })
            })
        }) else {
            return;
        };
        let result = if cfg!(target_os = "windows") {
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", &url])
                .spawn()
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&url).spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&url).spawn()
        };
        if result.is_err() {
            let status = panel.status;
            world.get_mut::<Text>(status).unwrap().0 =
                format!("Open this address in your browser: {url}");
        }
    }
}

pub fn command(world: &mut World, binding: &RecordBinding, text: &str) -> bool {
    if binding.source != Source::Local {
        return false;
    }
    let owner = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel.view_uid == binding.uid
                && panel.binding.area == binding.area
                && panel.binding.source == binding.source
        })
        .map(|(entity, _)| entity);
    let Some(owner) = owner else { return false };
    if text.trim() == "/lock" {
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(world, owner, FioteRequest::Lock { record });
        return true;
    }
    if text.trim_start().starts_with("/login") {
        if open_setup(world, owner) {
            return true;
        }
        let panel = world.get::<Panel>(owner).unwrap();
        if panel.pending.is_none() {
            let step = Step::Agent;
            show(world, owner, step);
        }
        return true;
    }
    false
}

pub fn ready(world: &mut World, binding: &RecordBinding) -> bool {
    let owner = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel.view_uid == binding.uid
                && panel.binding.area == binding.area
                && panel.binding.source == binding.source
        })
        .map(|(entity, _)| entity);
    let Some(owner) = owner else { return true };
    let panel = world.get::<Panel>(owner).unwrap();
    let fiote = panel.automatic
        || panel.saved.as_ref().is_some_and(|saved| {
            (saved.settings.enabled && saved.record == binding.uid)
                || saved
                    .fiotes
                    .iter()
                    .any(|choice| choice.record == binding.uid)
        });
    let ready = !fiote
        || panel.saved.as_ref().is_some_and(|saved| {
            !saved.tool_connections.is_empty()
                || (saved.settings.enabled && (!saved.locked || !saved.requires_credential))
        });
    if !ready && panel.pending.is_none() && !open_setup(world, owner) {
        show(world, owner, Step::Agent);
    }
    ready
}

pub fn thread_controls(world: &mut World, parent: Entity, thread: &str, binding: &RecordBinding) {
    if binding.source != Source::Local {
        return;
    }
    let owner = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let status = crate::thread_castle::message_view::status(world, owner);
    let bar = world
        .spawn((
            Node {
                column_gap: px(6),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let details = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let toggle_details = crate::description::button(
        world,
        bar,
        details,
        "⋯",
        crate::thread_castle::message_view::Details,
    );
    world
        .entity_mut(toggle_details)
        .insert(crate::sand::Borderless);
    let stop = crate::description::button(world, bar, owner, "Stop", Stop);
    world.entity_mut(stop).insert(crate::sand::Borderless);
    world.get_mut::<Node>(stop).unwrap().display = Display::None;

    let instruction_view = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                max_height: px(300),
                ..default()
            },
            ChildOf(details),
        ))
        .id();
    crate::scroll_sand::attach(world, instruction_view);
    let instructions = crate::edit_mode::label(
        world,
        instruction_view,
        "Instructions are pinned when this session first runs.",
        13.0,
    );
    crate::description::button(
        world,
        details,
        owner,
        "View session instructions",
        management::ViewInstructions,
    );
    crate::description::button(
        world,
        details,
        owner,
        "Apply current instructions",
        management::Refresh,
    );
    let toggle = crate::description::button(world, details, owner, "Open agent tools", AgentTools);
    let copy = crate::description::button(world, details, owner, "Copy MCP connection", CopyTools);
    world.get_mut::<Node>(copy).unwrap().display = Display::None;
    world.entity_mut(owner).insert(ThreadControl {
        area: binding.area,
        view_uid: binding.uid.clone(),
        record: binding.uid.clone(),
        thread: thread.into(),
        status,
        pending: None,
        tools_request: false,
        inspecting: false,
        instructions,
        poll: std::time::Instant::now(),
        toggle,
        copy,
        stop,
        connection: None,
    });
}

#[derive(Clone)]
struct AgentTools;
impl Action for AgentTools {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        if control.pending.is_some() && !control.inspecting {
            return;
        }
        let request = if control.connection.is_some() {
            FioteRequest::CloseTools {
                record: control.record.clone(),
                thread: control.thread.clone(),
            }
        } else {
            FioteRequest::OpenTools {
                record: control.record.clone(),
                thread: control.thread.clone(),
            }
        };
        let status = control.status;
        match send(world, owner, request) {
            Ok(id) => {
                let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
                control.pending = Some(id);
                control.inspecting = false;
                control.tools_request = true;
                world.get_mut::<Text>(status).unwrap().0 = "Connecting…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

#[derive(Clone)]
struct CopyTools;
impl Action for CopyTools {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        let Some(connection) = &control.connection else {
            return;
        };
        let status = control.status;
        let value = serde_json::json!({
            "type":"http", "name":"lince", "url":connection.url,
            "headers":[{"name":"Authorization","value":format!("Bearer {}", connection.token.0)}]
        })
        .to_string();
        let copied = world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .is_some_and(|mut clipboard| clipboard.set_text(value).is_ok());
        world.get_mut::<Text>(status).unwrap().0 = if copied {
            "Copied MCP connection. It grants your local Lince access until closed, locked, or the Cell stops."
        } else { "Could not copy the connection." }.into();
    }
}

fn update_connections(world: &mut World, status: &FioteStatus) {
    let panels: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .filter(|(_, panel)| panel.binding.uid == status.record && panel.pending.is_none())
        .map(|(entity, _)| entity)
        .collect();
    for owner in panels {
        let mut panel = world.get_mut::<Panel>(owner).unwrap();
        panel.saved = Some(status.clone());
        let close_picker = !status.tool_connections.is_empty() && panel.step == Step::Providers;
        if close_picker {
            show(world, owner, Step::Closed);
        }
    }
    let controls: Vec<_> = world
        .query::<(Entity, &ThreadControl)>()
        .iter(world)
        .filter(|(_, control)| control.record == status.record)
        .map(|(entity, _)| entity)
        .collect();
    for owner in controls {
        let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
        control.connection = status
            .tool_connections
            .iter()
            .find(|connection| connection.thread == control.thread)
            .cloned();
        let opened = control.connection.is_some();
        let (toggle, copy, stop) = (control.toggle, control.copy, control.stop);
        let running = status.running.contains(&control.thread);
        world.get_mut::<Node>(stop).unwrap().display = if running {
            Display::Flex
        } else {
            Display::None
        };
        let label = world.get::<Children>(toggle).unwrap()[0];
        world.get_mut::<Text>(label).unwrap().0 = if opened {
            "Close agent tools"
        } else {
            "Open agent tools"
        }
        .into();
        world.get_mut::<Node>(copy).unwrap().display =
            if opened { Display::Flex } else { Display::None };
    }
}
#[derive(Clone)]
struct Stop;
impl Action for Stop {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        if control.pending.is_some() && !control.inspecting {
            return;
        }
        let (thread, status) = (control.thread.clone(), control.status);
        match send(world, owner, FioteRequest::Stop { thread }) {
            Ok(id) => {
                let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
                control.pending = Some(id);
                control.inspecting = false;
                control.tools_request = false;
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

fn apply_status(world: &mut World, owner: Entity, saved: FioteStatus) {
    let refreshed = agent::refresh(world, owner, &saved);
    if world
        .entity_mut(owner)
        .take::<agent::OpenAfterSetup>()
        .is_some()
    {
        let binding = RecordBinding {
            uid: saved.record.clone(),
            ..world.get::<Panel>(owner).unwrap().binding.clone()
        };
        crate::full_record::Open(binding).apply(world, owner);
    }
    let panel = world.get::<Panel>(owner).unwrap();
    if panel.automatic && panel.binding.uid != saved.record {
        let area = panel.binding.area;
        if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(area) {
            if let Some(config) = &mut area.protein {
                config.draft.query["where"] = serde_json::json!([{"uid_eq":saved.record}]);
            }
        }
    }
    let panel = world.get::<Panel>(owner).unwrap();
    let step = panel.step;
    let first = panel.saved.is_none() && panel.automatic;
    let agent_changed = panel
        .saved
        .as_ref()
        .is_none_or(|previous| previous.agent_info != saved.agent_info);
    let label = panel.status;
    let next = if matches!(step, Step::Agent | Step::Manage) {
        step
    } else if saved.login_pending {
        Step::Browser
    } else if saved.settings.enabled && (!saved.locked || !saved.requires_credential) {
        Step::Closed
    } else if first {
        Step::Manage
    } else {
        step
    };
    let browser = next == Step::Browser && step != Step::Browser;
    world.get_mut::<Text>(label).unwrap().0 = if let Some(agent) = &saved.agent {
        format!("Agent · {} · /login", agent.command.display())
    } else if saved.settings.enabled {
        format!(
            "{} · {} · /login",
            saved
                .providers
                .iter()
                .find(|provider| provider.id == saved.settings.provider)
                .map_or("Provider", |provider| provider.label.as_str()),
            if saved.locked && saved.requires_credential {
                "vault locked"
            } else {
                "connected"
            }
        )
    } else {
        "Choose a Fiote and connect its agent.".into()
    };
    let mut panel = world.get_mut::<Panel>(owner).unwrap();
    panel.pending = None;
    panel.binding.uid = saved.record.clone();
    let deferred = panel.pending_command.take();
    let binding = RecordBinding {
        uid: panel.view_uid.clone(),
        ..panel.binding.clone()
    };
    panel.saved = Some(saved);
    if next != step
        || first
        || (step == Step::Agent && (agent_changed || refreshed))
        || step == Step::Manage
    {
        show(world, owner, next);
    }
    if browser {
        OpenBrowser.apply(world, owner);
    }
    if let Some(text) = deferred {
        command(world, &binding, &text);
    }
}

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let events: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for event in events {
        let (id, result) = match event {
            cell::ServerMessage::Fiote { id, status } => (id, Ok(status)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owners: Vec<_> = world
            .query::<(Entity, &Panel)>()
            .iter(world)
            .filter(|(_, panel)| {
                panel.pending.as_deref() == Some(&id)
                    || (id == crate::cell_bridge::CONNECTION && panel.pending.is_some())
            })
            .map(|(entity, _)| entity)
            .collect();
        for owner in owners {
            match &result {
                Ok(status) => apply_status(world, owner, status.clone()),
                Err(error) => {
                    world.entity_mut(owner).remove::<agent::OpenAfterSetup>();
                    world.entity_mut(owner).remove::<agent::RefreshDraft>();
                    let mut panel = world.get_mut::<Panel>(owner).unwrap();
                    panel.pending = None;
                    panel.pending_command = None;
                    let (label, browser) = (panel.status, panel.step == Step::Browser);
                    if browser {
                        show(world, owner, Step::Providers);
                    }
                    world.get_mut::<Text>(label).unwrap().0 = error.clone();
                }
            }
        }
        if let Ok(status) = &result {
            let views: Vec<_> = world
                .query::<&ThreadControl>()
                .iter(world)
                .filter(|control| control.pending.as_deref() == Some(&id))
                .map(|control| (control.area, control.view_uid.clone()))
                .collect();
            for mut panel in world.query::<&mut Panel>().iter_mut(world) {
                if panel.pending.is_none()
                    && panel.step == Step::Closed
                    && views
                        .iter()
                        .any(|(area, uid)| *area == panel.binding.area && *uid == panel.view_uid)
                {
                    panel.binding.uid = status.record.clone();
                    panel.saved = Some(status.clone());
                }
            }
            for mut control in world.query::<&mut ThreadControl>().iter_mut(world) {
                if control.pending.as_deref() == Some(&id) {
                    control.record = status.record.clone();
                }
            }
            update_connections(world, status);
            management::instructions(world, status);
            agent::activity(world, status);
        }
        let controls: Vec<_> = world
            .query::<(Entity, &ThreadControl)>()
            .iter(world)
            .filter(|(_, control)| control.pending.as_deref() == Some(&id))
            .map(|(entity, _)| entity)
            .collect();
        for owner in controls {
            let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
            control.pending = None;
            let label = control.status;
            if control.inspecting {
                control.inspecting = false;
                if let Ok(saved) = &result {
                    let text = saved
                        .tasks
                        .iter()
                        .find(|task| task.thread == control.thread)
                        .map(|task| format!("{} · {}", task.state, task.detail))
                        .unwrap_or_else(|| {
                            if saved.running.contains(&control.thread) {
                                "Fiote is working…".into()
                            } else {
                                String::new()
                            }
                        });
                    let text = saved
                        .agent_activity
                        .iter()
                        .find(|activity| {
                            activity.thread == control.thread && activity.permission.is_none()
                        })
                        .map(|activity| activity.title.clone())
                        .unwrap_or(text);
                    world.get_mut::<Text>(label).unwrap().0 = text;
                }
                continue;
            }
            let success = if control.tools_request {
                if control.connection.is_some() {
                    "Agent tools open. Copy the connection for your agent; /lock also closes it."
                } else {
                    "Agent tools closed."
                }
            } else {
                "Session request completed."
            };
            world.get_mut::<Text>(label).unwrap().0 = result
                .as_ref()
                .map(|_| success.into())
                .unwrap_or_else(|error| error.clone());
        }
    }
}

fn keyboard(world: &mut World) {
    let Some(owner) = world.resource::<Active>().0 else {
        return;
    };
    let Some(panel) = world.get::<Panel>(owner) else {
        world.resource_mut::<Active>().0 = None;
        return;
    };
    if panel.pending.is_some() || crate::laboratory::suspended(world, owner) {
        return;
    }
    let focus = world.resource::<InputFocus>().get();
    if focus != Some(owner) && !panel.fields.iter().any(|field| Some(*field) == focus) {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    let (up, down, enter, escape) = (
        keys.just_pressed(KeyCode::ArrowUp),
        keys.just_pressed(KeyCode::ArrowDown),
        keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
        keys.just_pressed(KeyCode::Escape),
    );
    let step = panel.step;
    if escape {
        Close.apply(world, owner);
    } else if matches!(step, Step::Providers | Step::Methods)
        || (step == Step::Agent && focus == Some(owner))
    {
        let mut panel = world.get_mut::<Panel>(owner).unwrap();
        if panel.choices.is_empty() {
            return;
        }
        if up {
            panel.selection = (panel.selection + panel.choices.len() - 1) % panel.choices.len();
        }
        if down {
            panel.selection = (panel.selection + 1) % panel.choices.len();
        }
        let selection = panel.selection;
        highlight(world, owner);
        if enter {
            if step == Step::Agent {
                agent::choose(world, owner, selection);
            } else {
                Choose(selection).apply(world, owner);
            }
        }
    } else if enter && matches!(step, Step::Credentials | Step::Unlock) {
        Continue.apply(world, owner);
    }
}

fn poll(world: &mut World) {
    let owners: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .filter(|(_, panel)| {
            panel.pending.is_none()
                && panel.step != Step::Manage
                && (((panel.step == Step::Browser
                    || panel.saved.as_ref().is_some_and(|saved| {
                        !saved.running.is_empty()
                            || saved
                                .agent_info
                                .as_ref()
                                .is_some_and(|info| info["loginPending"] == true)
                    }))
                    && panel.poll.elapsed().as_secs() >= 1)
                    || (panel
                        .saved
                        .as_ref()
                        .is_some_and(|saved| !saved.tool_connections.is_empty())
                        && panel.poll.elapsed().as_secs() >= 5))
        })
        .map(|(entity, panel)| (entity, panel.binding.uid.clone(), panel.step))
        .collect();
    for (owner, record, step) in owners {
        world.get_mut::<Panel>(owner).unwrap().poll = std::time::Instant::now();
        let update = if step == Step::Browser {
            FioteRequest::BrowserPoll { record }
        } else {
            FioteRequest::Inspect { record }
        };
        request(world, owner, update);
    }
}
fn masks(fields: Query<&EditableText>, mut masks: Query<(&Mask, &mut Text)>) {
    for (mask, mut text) in &mut masks {
        let count = fields
            .get(mask.0)
            .map_or(0, |field| field.value().chars().count());
        text.set_if_neq(Text::new("•".repeat(count.min(32))));
    }
}
fn protect_keys(mut fields: Query<&mut EditableText, With<SecretField>>) {
    for mut field in &mut fields {
        field
            .pending_edits
            .retain(|edit| !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut));
    }
}

mod agent;
mod management;
#[cfg(test)]
mod tests;

fn poll_threads(world: &mut World) {
    let controls: Vec<_> = world
        .query::<(Entity, &ThreadControl)>()
        .iter(world)
        .filter(|(owner, control)| {
            control.pending.is_none()
                && control.poll.elapsed().as_secs() >= 1
                && displayed(world, *owner)
        })
        .map(|(owner, control)| (owner, control.thread.clone()))
        .collect();
    for (owner, thread) in controls {
        if let Ok(id) = send(world, owner, FioteRequest::InspectThread { thread }) {
            let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
            control.pending = Some(id);
            control.inspecting = true;
            control.poll = std::time::Instant::now();
        }
    }
}

fn displayed(world: &World, mut entity: Entity) -> bool {
    loop {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
        {
            return false;
        }
        match world.get::<ChildOf>(entity) {
            Some(parent) => entity = parent.parent(),
            None => return true,
        }
    }
}

pub fn thread_command(
    world: &mut World,
    binding: &RecordBinding,
    thread: &str,
    text: &str,
) -> bool {
    if binding.source != Source::Local
        || !(text.trim() == "/lock" || text.trim_start().starts_with("/login"))
    {
        return false;
    }
    let owner = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| panel.view_uid == binding.uid && panel.binding.area == binding.area)
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    world.get_mut::<Panel>(owner).unwrap().pending_command = Some(text.to_string());
    request(
        world,
        owner,
        FioteRequest::InspectThread {
            thread: thread.to_string(),
        },
    );
    true
}
