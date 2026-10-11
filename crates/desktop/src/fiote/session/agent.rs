use super::*;

pub(super) mod options;

#[derive(Component, Clone)]
struct Draft(cell::FioteAgentConfig);

#[derive(Component)]
struct Directories(Entity);

#[derive(Component)]
struct Activity {
    owner: Entity,
    permission: Option<String>,
}

#[derive(Clone)]
pub(super) struct Open;

#[derive(Component)]
pub(super) struct OpenAfterSetup;

#[derive(Component)]
pub(super) struct RefreshDraft;

pub(super) fn refresh(world: &mut World, owner: Entity, saved: &FioteStatus) -> bool {
    if world
        .get::<Panel>(owner)
        .is_some_and(|panel| panel.binding.uid != saved.record)
    {
        world.entity_mut(owner).remove::<Draft>();
    }
    if world.entity_mut(owner).take::<RefreshDraft>().is_none() {
        return false;
    }
    if let Some(config) = &saved.agent {
        world.entity_mut(owner).insert(Draft(config.clone()));
    }
    true
}
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        super::show(world, owner, Step::Agent);
    }
}

fn config(world: &World, owner: Entity) -> Result<cell::FioteAgentConfig, String> {
    let panel = world
        .get::<Panel>(owner)
        .ok_or("The agent panel is closed.")?;
    let fields = &panel.fields;
    let previous = world.get::<Draft>(owner).map(|draft| draft.0.clone());
    Ok(cell::FioteAgentConfig {
        require_vault: false,
        additional_directories: world
            .get::<Directories>(owner)
            .map(|field| crate::directory_list::paths(world, field.0))
            .unwrap_or_default(),
        command: value(world, fields[0])?.into(),
        args: serde_json::from_str(&value(world, fields[1])?)
            .map_err(|_| "Arguments must be a JSON array of strings.")?,
        directory: value(world, fields[2])?.into(),
        environment: serde_json::from_str(&value(world, fields[3])?)
            .map_err(|_| "Environment must be a JSON object of strings.")?,
        session_meta: previous
            .as_ref()
            .map(|config| config.session_meta.clone())
            .unwrap_or_default(),
        options: previous.map(|config| config.options).unwrap_or_default(),
    })
}

pub(super) fn show(
    world: &mut World,
    owner: Entity,
    content: Entity,
    fields: &mut Vec<Entity>,
    saved: Option<&FioteStatus>,
) {
    let configured = saved.and_then(|saved| saved.agent.clone());
    let draft = world
        .get::<Draft>(owner)
        .map(|draft| draft.0.clone())
        .or(configured)
        .unwrap_or_else(|| cell::FioteAgentConfig {
            require_vault: false,
            additional_directories: Vec::new(),
            command: Default::default(),
            args: Vec::new(),
            directory: std::env::current_dir().unwrap_or_default(),
            environment: Default::default(),
            session_meta: Default::default(),
            options: Default::default(),
        });
    crate::edit_mode::label(world, content, "Connection", 20.0);
    let advanced = world
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
    crate::description::button(world, content, owner, "Agent options", Advanced(advanced));
    for (label, text) in [
        ("Executable", draft.command.display().to_string()),
        (
            "Arguments · JSON array",
            serde_json::to_string(&draft.args).unwrap(),
        ),
        (
            "Working directory · code tools can edit files here",
            draft.directory.display().to_string(),
        ),
        (
            "Environment · JSON object, no secrets",
            serde_json::to_string(&draft.environment).unwrap(),
        ),
    ] {
        let input = field(
            world,
            if label.starts_with("Working directory") {
                content
            } else {
                advanced
            },
            label,
            false,
        );
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text(&text);
        fields.push(input);
    }
    let directories = crate::directory_list::create(world, content, &draft.additional_directories);
    world
        .entity_mut(owner)
        .insert((Draft(draft), Directories(directories)));
    crate::description::button(world, content, owner, "Check connection · no tokens", Check);
    connection_status(world, content, saved);
    crate::description::button(world, content, owner, "Discover agent / sign in", Discover);
    crate::description::button(
        world,
        content,
        owner,
        "Save directory defaults",
        options::SaveDirectory,
    );
    let Some(info) = saved.and_then(|saved| saved.agent_info.as_ref()) else {
        crate::edit_mode::label(
            world,
            content,
            "Discover the agent to sign in, or check an agent you already configured.",
            14.0,
        );
        return;
    };
    if let Some(error) = info["setupError"].as_str() {
        crate::edit_mode::label(world, content, error, 14.0);
    }
    if let Some(methods) = info["authMethods"].as_array() {
        for method in methods {
            let (Some(id), Some(name)) = (method["id"].as_str(), method["name"].as_str()) else {
                continue;
            };
            crate::description::button(world, content, owner, name, Authenticate(id.into()));
        }
    }
    if info["agentCapabilities"]["auth"]["logout"].is_object() {
        crate::description::button(world, content, owner, "Sign out of agent", Logout);
    } else {
        crate::edit_mode::label(
            world,
            content,
            "This agent does not offer logout here. Use its account settings to sign out.",
            12.0,
        );
    }
    if info["loginPending"] == true {
        crate::description::button(world, content, owner, "Cancel sign-in", CancelLogin);
        crate::edit_mode::label(
            world,
            content,
            if info["terminalLogin"].is_string() {
                "Complete sign-in in the private terminal below. Follow its browser instructions if it provides a link."
            } else {
                "Finish the agent's login in your browser. Your account stays signed in until it expires or you sign out."
            },
            14.0,
        );
    }
    if let Some(result) = info["loginResult"].as_str() {
        crate::edit_mode::label(world, content, result, 14.0);
    }
    crate::edit_mode::label(
        world,
        content,
        "The selected agent stores and refreshes its own login.",
        14.0,
    );
    options::show(world, owner, content, saved);
    crate::description::button(
        world,
        content,
        owner,
        "Check and open conversation",
        Connect,
    );
}

fn connection_status(world: &mut World, content: Entity, saved: Option<&FioteStatus>) {
    if let Some(check) = saved
        .and_then(|saved| saved.agent_info.as_ref())
        .and_then(|info| info.get("connectionCheck"))
    {
        crate::edit_mode::label(
            world,
            content,
            if check["ready"] == true {
                "Last check: session ready"
            } else {
                "Last check: connection needs attention"
            },
            16.0,
        );
        for (key, label) in [
            ("agent", "Agent"),
            ("login", "Login"),
            ("model", "Model"),
            ("session", "Session"),
        ] {
            if let Some(value) = check[key].as_str() {
                crate::edit_mode::label(world, content, &format!("{label}: {value}"), 14.0);
            }
        }
        if let Some(detail) = check["detail"].as_str().filter(|detail| !detail.is_empty()) {
            crate::edit_mode::label(world, content, detail, 14.0);
        }
    } else {
        crate::edit_mode::label(world, content, "Connection not checked", 16.0);
    }
    crate::edit_mode::label(
        world,
        content,
        "Checks agent startup and session access without sending a message. No model tokens are used. Generation and quota are not tested. Check again after changing settings.",
        14.0,
    );
}

fn submit(world: &mut World, owner: Entity, configure: bool) {
    let Some(panel) = world.get::<Panel>(owner) else {
        return;
    };
    if panel.pending.is_some() {
        return;
    }
    let record = panel.binding.uid.clone();
    let status = panel.status;
    match config(world, owner) {
        Ok(config) => {
            world.entity_mut(owner).insert(Draft(config.clone()));
            request(
                world,
                owner,
                if configure {
                    FioteRequest::AgentCheck { record, config }
                } else {
                    FioteRequest::AgentDiscover { record, config }
                },
            );
        }
        Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
    }
}

#[derive(Clone)]
struct Discover;
impl Action for Discover {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, false);
    }
}
#[derive(Clone)]
struct Check;
impl Action for Check {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, true);
    }
}

#[derive(Clone)]
struct CancelLogin;
impl Action for CancelLogin {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(panel) = world
            .get::<Panel>(owner)
            .filter(|panel| panel.pending.is_none())
        {
            request(
                world,
                owner,
                FioteRequest::AgentCancelLogin {
                    record: panel.binding.uid.clone(),
                },
            );
        }
    }
}
#[derive(Clone)]
struct Connect;
impl Action for Connect {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, true);
        if world
            .get::<Panel>(owner)
            .is_some_and(|panel| panel.pending.is_some())
        {
            world.entity_mut(owner).insert(OpenAfterSetup);
        }
    }
}
#[derive(Clone)]
struct Authenticate(String);
impl Action for Authenticate {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        if panel.pending.is_some() {
            return;
        }
        request(
            world,
            owner,
            FioteRequest::AgentAuthenticate {
                record: panel.binding.uid.clone(),
                method: self.0.clone(),
            },
        );
    }
}

#[derive(Clone)]
struct Logout;
impl Action for Logout {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        let record = panel.binding.uid.clone();
        request(world, owner, FioteRequest::AgentLogout { record });
    }
}
pub(super) fn activity(world: &mut World, saved: &FioteStatus) {
    let controls: Vec<_> = world
        .query::<(Entity, &ThreadControl)>()
        .iter(world)
        .filter(|(_, control)| control.record == saved.record)
        .map(|(owner, control)| (owner, control.thread.clone(), control.status))
        .collect();
    for (owner, thread, label) in controls {
        if let Some(item) = saved
            .agent_activity
            .iter()
            .find(|item| item.thread == thread && item.permission.is_none())
        {
            world.get_mut::<Text>(label).unwrap().0 = item.title.clone();
        }
        let pending = saved
            .agent_activity
            .iter()
            .find(|item| item.thread == thread && item.permission.is_some())
            .and_then(|item| item.permission.as_ref());
        let previous = world
            .query::<(Entity, &Activity)>()
            .iter(world)
            .find(|(_, panel)| panel.owner == owner)
            .map(|(entity, panel)| (entity, panel.permission.clone()));
        if previous.as_ref().and_then(|(_, id)| id.as_deref())
            == pending.map(|request| request.id.as_str())
        {
            continue;
        }
        if let Some((previous, _)) = previous {
            world.despawn(previous);
        }
        let Some(pending) = pending else { continue };
        let panel = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    ..default()
                },
                ChildOf(owner),
                Activity {
                    owner,
                    permission: Some(pending.id.clone()),
                },
            ))
            .id();
        crate::edit_mode::label(world, panel, &pending.title, 16.0);
        crate::edit_mode::label(world, panel, &pending.details, 13.0);
        for option in &pending.options {
            crate::description::button(
                world,
                panel,
                owner,
                &option.label,
                Answer {
                    request: pending.id.clone(),
                    option: Some(option.id.clone()),
                },
            );
        }
        crate::description::button(
            world,
            panel,
            owner,
            "Cancel tool",
            Answer {
                request: pending.id.clone(),
                option: None,
            },
        );
    }
}

#[derive(Clone)]
struct Answer {
    request: String,
    option: Option<String>,
}
impl Action for Answer {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        let status = control.status;
        if let Err(error) = send(
            world,
            owner,
            FioteRequest::AgentPermission {
                record: control.record.clone(),
                thread: control.thread.clone(),
                request: self.request.clone(),
                option: self.option.clone(),
            },
        ) {
            world.get_mut::<Text>(status).unwrap().0 = error;
        }
    }
}

#[derive(Clone)]
struct Advanced(Entity);
impl Action for Advanced {
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
