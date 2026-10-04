use super::*;
use serde_json::Value;

#[derive(Component, Default)]
struct Choices(Option<Value>);

pub(super) fn invalidate(world: &mut World, owner: Entity) {
    if let Some(content) = world
        .get::<ThreadControl>(owner)
        .map(|control| control.choices)
    {
        if let Some(mut choices) = world.get_mut::<Choices>(content) {
            choices.0 = None;
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
            Choices::default(),
        ))
        .id();
    crate::description::button(
        world,
        content,
        owner,
        "Load conversation choices · no tokens",
        Load,
    );
    crate::description::button(
        world,
        content,
        owner,
        "Reset conversation choices to Fiote defaults",
        Reset,
    );
    content
}

pub(super) fn update(world: &mut World, owner: Entity, saved: &FioteStatus) {
    let Some(control) = world.get::<ThreadControl>(owner) else {
        return;
    };
    let content = control.choices;
    let thread = control.thread.clone();
    if saved.agent.is_none() {
        if world
            .get::<Choices>(content)
            .is_some_and(|choices| choices.0 != Some(Value::from("direct")))
        {
            world.get_mut::<Choices>(content).unwrap().0 = Some(Value::from("direct"));
            world.entity_mut(content).despawn_children();
            crate::edit_mode::label(
                world,
                content,
                "Direct-model settings are changed in Fiote setup. Live session choices and agent commands are advertised by ACP agents. Attachment support depends on the connection and model; unsupported media is reported before sending.",
                13.0,
            );
        }
        crate::message_commands::update(world, &thread, Vec::new());
        return;
    }
    let commands = saved
        .agent_session
        .as_ref()
        .filter(|session| session.connected && session.thread == thread)
        .map(|session| {
            session
                .state
                .commands
                .iter()
                .filter(|command| valid_name(&command.name))
                .map(|command| {
                    let input = serde_json::to_value(&command.input).unwrap_or_default();
                    crate::message_commands::Command {
                        name: command.name.clone(),
                        description: command.description.clone(),
                        hint: input["hint"].as_str().unwrap_or_default().into(),
                        target: format!(
                            "Agent · {}",
                            saved
                                .fiotes
                                .iter()
                                .find(|choice| choice.record == saved.record)
                                .map(|choice| choice.title.as_str())
                                .unwrap_or(&saved.record)
                        ),
                        insert: format!("/agent:{}", command.name),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    crate::message_commands::update(world, &thread, commands);
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
    let value = serde_json::json!({"state":{"options": session.state.options}, "pending":session.pending, "connected":session.connected, "directories":session.directories, "supports_directories":session.supports_directories, "capabilities":session.prompt_capabilities});
    if world
        .get::<Choices>(content)
        .is_some_and(|choices| choices.0.as_ref() == Some(&value))
    {
        return;
    }
    world.get_mut::<Choices>(content).unwrap().0 = Some(value.clone());
    world.entity_mut(content).despawn_children();
    crate::edit_mode::label(world, content, "This conversation", 16.0);
    crate::description::button(
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
    crate::edit_mode::label(
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
    crate::edit_mode::label(
        world,
        content,
        "Changes during a reply apply at the next turn. Fiote defaults stay unchanged.",
        13.0,
    );
    if !session.connected {
        crate::edit_mode::label(
            world,
            content,
            "Agent disconnected. Reload the conversation choices.",
            13.0,
        );
        crate::description::button(world, content, owner, "Reload choices · no tokens", Load);
        return;
    }
    if session.supports_directories {
        let input = crate::directory_list::create(world, content, &session.directories);
        crate::description::button(
            world,
            content,
            owner,
            "Apply directories and reload session",
            Directories(input),
        );
    } else {
        crate::edit_mode::label(
            world,
            content,
            "Additional directories are unavailable for this agent.",
            13.0,
        );
    }
    for option in value["state"]["options"].as_array().into_iter().flatten() {
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
            crate::edit_mode::label(
                world,
                content,
                "Provider changes use this agent's session controls. Complete any required provider sign-in in Fiote setup, then retry.",
                12.0,
            );
        }
        if choices.is_empty() {
            continue;
        }
        crate::edit_mode::label(world, content, name, 13.0);
        crate::dropdown::spawn(
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
                        crate::actions![Select {
                            option: id.into(),
                            value
                        }],
                    )
                })
                .collect(),
        );
        if let Some(value) = session.pending.get(id) {
            crate::edit_mode::label(world, content, &format!("Next turn: {value}"), 13.0);
        }
    }
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
        let directories = crate::directory_list::paths(world, self.0);
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
