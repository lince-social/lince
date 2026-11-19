use super::*;
use crate::native::retained::Presentation;
use cell::fiote_communication::acp::{AvailableCommandInput, SessionConfigOption};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

enum Choices {
    Direct,
    Agent(SessionChoices),
}

struct SessionChoices {
    options: Vec<SessionConfigOption>,
    pending: BTreeMap<String, Value>,
    connected: bool,
    directories: Vec<PathBuf>,
    supports_directories: bool,
    capabilities: [bool; 3],
}

pub(super) fn invalidate(world: &mut World, owner: Entity) {
    if let Some(content) = world
        .get::<ThreadControl>(owner)
        .map(|control| control.choices)
    {
        if let Ok(mut content) = world.get_entity_mut(content) {
            content.remove::<Presentation<Choices>>();
        }
    }
}

pub(super) fn create(world: &mut World, parent: Entity, owner: Entity) -> Entity {
    let content = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::native::description::button(
        world,
        content,
        owner,
        "Load conversation choices · no tokens",
        Load,
    );
    crate::native::description::button(
        world,
        content,
        owner,
        "Reset conversation choices to Fiote defaults",
        Reset,
    );
    content
}

pub(super) fn update(world: &mut World, owner: Entity, saved: &FioteStatus) {
    let _presentation = nucleus::performance::Span::new("fiote-live-choices-presentation");
    let Some(control) = world.get::<ThreadControl>(owner) else {
        return;
    };
    let content = control.choices;
    let thread = control.thread.clone();
    if saved.agent.is_none() {
        if !Presentation::matches(world, content, |choices: &Choices| {
            matches!(choices, Choices::Direct)
        }) {
            world.entity_mut(content).despawn_children();
            crate::native::edit_mode::label(
                world,
                content,
                "Direct-model settings are changed in Fiote setup. Live session choices and agent commands are advertised by ACP agents. Attachment support depends on the connection and model; unsupported media is reported before sending.",
                13.0,
            );
            Presentation::remember(world, content, Choices::Direct);
            nucleus::performance::add("fiote-live-choices-rebuilt", 1);
        } else {
            nucleus::performance::add("fiote-live-choices-retained", 1);
        }
        crate::native::message_commands::update(world, &thread, Vec::new());
        return;
    }
    let mut commands = saved
        .agent_session
        .as_ref()
        .filter(|session| session.connected && session.thread == thread)
        .map(|session| {
            session
                .state
                .commands
                .iter()
                .filter(|command| valid_name(&command.name))
                .map(|command| crate::native::message_commands::AgentCommand {
                    name: &command.name,
                    description: &command.description,
                    hint: match command.input.as_ref() {
                        Some(AvailableCommandInput::Unstructured(input)) => &input.hint,
                        _ => "",
                    },
                })
        })
        .into_iter()
        .flatten()
        .peekable();
    if commands.peek().is_some() {
        let title = saved
            .fiotes
            .iter()
            .find(|choice| choice.record == saved.record)
            .map(|choice| choice.title.as_str())
            .unwrap_or(&saved.record);
        crate::native::message_commands::update_agent(world, &thread, title, commands);
    } else {
        crate::native::message_commands::update(world, &thread, Vec::new());
    }
    let Some(control) = world.get::<ThreadControl>(owner) else {
        return;
    };
    let Some(session) = saved
        .agent_session
        .as_ref()
        .filter(|session| session.thread == control.thread)
    else {
        return;
    };
    let capabilities =
        ["image", "audio", "embeddedContext"].map(|key| session.prompt_capabilities[key] == true);
    if Presentation::matches(world, content, |choices: &Choices| {
        let Choices::Agent(previous) = choices else {
            return false;
        };
        previous.connected == session.connected
            && previous.capabilities == capabilities
            && (!session.connected
                || (previous.options == session.state.options
                    && previous.pending == session.pending
                    && previous.supports_directories == session.supports_directories
                    && (!session.supports_directories
                        || previous.directories == session.directories)))
    }) {
        nucleus::performance::add("fiote-live-choices-retained", 1);
        return;
    }
    let choices = Choices::Agent(SessionChoices {
        options: if session.connected {
            session.state.options.clone()
        } else {
            Vec::new()
        },
        pending: if session.connected {
            session.pending.clone()
        } else {
            BTreeMap::new()
        },
        connected: session.connected,
        directories: if session.connected && session.supports_directories {
            session.directories.clone()
        } else {
            Vec::new()
        },
        supports_directories: session.supports_directories,
        capabilities,
    });
    nucleus::performance::add("fiote-live-choices-rebuilt", 1);
    world.entity_mut(content).despawn_children();
    crate::native::edit_mode::label(world, content, "This conversation", 16.0);
    crate::native::description::button(
        world,
        content,
        owner,
        "Reset conversation choices to Fiote defaults",
        Reset,
    );
    let capability = |key: &str| {
        if session.prompt_capabilities[key] == true {
            "advertised by agent"
        } else {
            "unavailable"
        }
    };
    crate::native::edit_mode::label(
        world,
        content,
        &format!(
            "Images: {} · audio: {} · file contents: {}. Actual model acceptance is unverified until used. References identify resources; they do not grant access.",
            capability("image"),
            capability("audio"),
            capability("embeddedContext")
        ),
        12.0,
    );
    crate::native::edit_mode::label(
        world,
        content,
        "Changes during a reply apply at the next turn. Fiote defaults stay unchanged.",
        13.0,
    );
    if !session.connected {
        crate::native::edit_mode::label(
            world,
            content,
            "Agent disconnected. Reload the conversation choices.",
            13.0,
        );
        crate::native::description::button(
            world,
            content,
            owner,
            "Reload choices · no tokens",
            Load,
        );
        Presentation::remember(world, content, choices);
        return;
    }
    if session.supports_directories {
        let input = crate::native::directory_list::create(world, content, &session.directories);
        crate::native::description::button(
            world,
            content,
            owner,
            "Apply directories and reload session",
            Directories(input),
        );
    } else {
        crate::native::edit_mode::label(
            world,
            content,
            "Additional directories are unavailable for this agent.",
            13.0,
        );
    }
    let options = serde_json::json!(session.state.options);
    for option in options.as_array().into_iter().flatten() {
        let Some(id) = option["id"].as_str() else {
            continue;
        };
        let name = option["name"].as_str().unwrap_or(id);
        let choices = agent::options::choices(option);
        let selected = choices
            .iter()
            .find(|(_, value)| value == &option["currentValue"])
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "Unverified".into());
        if id == "provider" {
            crate::native::edit_mode::label(
                world,
                content,
                "Provider changes use this agent's session controls. Complete any required provider sign-in in Fiote setup, then retry.",
                12.0,
            );
        }
        if choices.is_empty() {
            continue;
        }
        crate::native::edit_mode::label(world, content, name, 13.0);
        crate::native::dropdown::spawn(
            world,
            content,
            owner,
            name,
            &selected,
            choices
                .into_iter()
                .map(|(name, value)| {
                    (
                        name,
                        crate::native::actions![Select {
                            option: id.into(),
                            value
                        }],
                    )
                })
                .collect(),
        );
        if let Some(value) = session.pending.get(id) {
            crate::native::edit_mode::label(world, content, &format!("Next turn: {value}"), 13.0);
        }
    }
    Presentation::remember(world, content, choices);
}

fn submit(world: &mut World, owner: Entity, build: impl FnOnce(String) -> FioteRequest) {
    let Some(control) = world.get::<ThreadControl>(owner) else {
        return;
    };
    if control.pending.is_some() && !control.inspecting {
        return;
    }
    let request = build(control.thread.clone());
    let status = control.status;
    match send(world, owner, request) {
        Ok(id) => {
            let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
            control.pending = Some(id);
            control.inspecting = false;
            control.tools_request = false;
            world.get_mut::<Text>(status).unwrap().0 = "Updating conversation settings…".into();
        }
        Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
    }
}

#[derive(Clone)]
struct Load;
impl Action for Load {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, |thread| FioteRequest::SessionOptions {
            thread,
        });
    }
}

#[derive(Clone)]
struct Reset;
impl Action for Reset {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, |thread| FioteRequest::SessionReset { thread });
    }
}

#[derive(Clone)]
struct Select {
    option: String,
    value: Value,
}
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, |thread| FioteRequest::SessionSetOption {
            thread,
            option: self.option.clone(),
            value: self.value.clone(),
        });
    }
}

#[derive(Clone)]
struct Directories(Entity);
impl Action for Directories {
    fn apply(&self, world: &mut World, owner: Entity) {
        let directories = crate::native::directory_list::paths(world, self.0);
        submit(world, owner, |thread| FioteRequest::SessionDirectories {
            thread,
            directories,
        });
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
}
