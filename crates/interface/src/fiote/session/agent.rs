use super::*;
use serde_json::json;

mod options;

#[derive(Component, Clone)]
struct Draft(cell::FioteAgentConfig);

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
    choices: &mut Vec<Entity>,
    saved: Option<&FioteStatus>,
) {
    let configured = saved.and_then(|saved| saved.agent.clone());
    let draft = world
        .get::<Draft>(owner)
        .map(|draft| draft.0.clone())
        .or(configured)
        .unwrap_or_else(|| cell::FioteAgentConfig {
            require_vault: false,
            command: "goose".into(),
            args: vec!["acp".into()],
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
    world.entity_mut(owner).insert(Draft(draft));
    crate::description::button(world, content, owner, "Change provider / sign in", Discover);
    crate::description::button(
        world,
        content,
        owner,
        "Save working directory",
        options::SaveDirectory,
    );
    options::show(world, owner, content, saved);

    let Some(info) = saved.and_then(|saved| saved.agent_info.as_ref()) else {
        return;
    };
    if let Some(error) = info["setupError"].as_str() {
        crate::edit_mode::label(world, content, error, 14.0);
    }
    if info["selectedProvider"].is_null() {
        if let Some(providers) = info["providers"].as_array() {
            let list = world
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        max_height: px(240),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(content),
                ))
                .id();
            crate::scroll_sand::attach(world, list);
            for provider in providers {
                let (Some(id), Some(name)) =
                    (provider["providerId"].as_str(), provider["name"].as_str())
                else {
                    continue;
                };
                let button =
                    crate::description::button(world, list, owner, name, ChooseProvider(id.into()));
                let mut node = world.get_mut::<Node>(button).unwrap();
                node.height = px(32);
                node.flex_shrink = 0.0;
                choices.push(world.get::<Children>(button).unwrap()[0]);
            }
        }
    }
    let selected = &info["selectedProvider"];
    crate::description::button(world, content, owner, "Finish · open Record", Connect);
    if let Some(name) = selected["name"].as_str() {
        crate::edit_mode::label(world, content, &format!("Connection: {name}"), 16.0);
        if let Some(description) = selected["description"].as_str() {
            crate::edit_mode::label(world, content, description, 14.0);
        }
    }
    let login = info.get("loginAgent").unwrap_or(info);
    if login["agentInfo"]["name"] != "goose" {
        if let Some(methods) = login["authMethods"].as_array() {
            for method in methods {
                let (Some(id), Some(name)) = (method["id"].as_str(), method["name"].as_str())
                else {
                    continue;
                };
                if method["type"] != "terminal" {
                    crate::description::button(
                        world,
                        content,
                        owner,
                        name,
                        Authenticate(id.into()),
                    );
                } else if let Some(description) = method["description"].as_str() {
                    crate::edit_mode::label(world, content, description, 14.0);
                }
            }
        }
    }
    if selected["acp"] != true {
        let definitions = selected["fields"].as_array();
        if let Some(definitions) = definitions {
            for definition in definitions {
                let input = field(
                    world,
                    content,
                    definition["label"].as_str().unwrap_or("Provider field"),
                    definition["secret"] == true,
                );
                if let Some(text) = definition["defaultValue"].as_str() {
                    world
                        .get_mut::<EditableText>(input)
                        .unwrap()
                        .editor
                        .set_text(text);
                }
                fields.push(input);
            }
        }
        if definitions.is_some_and(|fields| !fields.is_empty())
            || selected["setupMethod"]
                .as_str()
                .is_some_and(|method| method.contains("oauth"))
        {
            crate::description::button(world, content, owner, "Connect provider", ProviderLogin);
        }
    }
    if info["loginPending"] == true {
        crate::edit_mode::label(
            world,
            content,
            "Finish the agent's login in your browser. Your account stays signed in until it expires or you sign out.",
            14.0,
        );
        if let (Some(code), Some(url)) = (
            info["deviceCode"]["userCode"].as_str(),
            info["deviceCode"]["verificationUri"].as_str(),
        ) {
            crate::edit_mode::label(
                world,
                content,
                &format!("Open {url}\nEnter code: {code}"),
                16.0,
            );
            crate::description::button(world, content, owner, "Open sign-in page", OpenBrowser);
        }
    }
    if let Some(result) = info["loginResult"].as_str() {
        crate::edit_mode::label(world, content, result, 14.0);
    }
    crate::edit_mode::label(
        world,
        content,
        "Goose or the selected agent stores and refreshes your login.",
        14.0,
    );
}

pub(super) fn choose(world: &mut World, owner: Entity, index: usize) {
    let provider = world
        .get::<Panel>(owner)
        .and_then(|panel| panel.saved.as_ref())
        .and_then(|saved| saved.agent_info.as_ref())
        .and_then(|info| info["providers"].as_array())
        .and_then(|providers| providers.get(index))
        .and_then(|provider| provider["providerId"].as_str())
        .map(str::to_string);
    if let Some(provider) = provider {
        ChooseProvider(provider).apply(world, owner);
    }
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
                    FioteRequest::AgentConfigure { record, config }
                } else {
                    FioteRequest::AgentDiscover { record, config }
                },
            );
            if configure && world.get::<Panel>(owner).unwrap().pending.is_some() {
                super::show(world, owner, Step::Manage);
            }
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
struct ChooseProvider(String);
impl Action for ChooseProvider {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        if panel.pending.is_some() {
            return;
        }
        let status = panel.status;
        let current = match config(world, owner) {
            Ok(current) => current,
            Err(error) => {
                world.get_mut::<Text>(status).unwrap().0 = error;
                return;
            }
        };
        if world.get::<Draft>(owner).is_some_and(|draft| {
            draft.0.command != current.command
                || draft.0.args != current.args
                || draft.0.directory != current.directory
                || draft.0.environment != current.environment
        }) {
            world.get_mut::<Text>(status).unwrap().0 =
                "Discover connections again after changing the agent or working folder.".into();
            return;
        }
        let record = panel.binding.uid.clone();
        request(
            world,
            owner,
            FioteRequest::AgentProvider {
                record,
                provider: self.0.clone(),
            },
        );
        if world.get::<Panel>(owner).unwrap().pending.is_some() {
            world.entity_mut(owner).insert(RefreshDraft);
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
struct ProviderLogin;
impl Action for ProviderLogin {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        if panel.pending.is_some() {
            return;
        }
        let status = panel.status;
        let result = (|| {
            let entry = &panel
                .saved
                .as_ref()
                .ok_or("Discover the agent first.")?
                .agent_info
                .as_ref()
                .ok_or("Choose a connection first.")?["selectedProvider"];
            let mut values = serde_json::Map::new();
            let definitions = entry["fields"].as_array().cloned().unwrap_or_default();
            for (index, definition) in definitions.iter().enumerate() {
                values.insert(
                    definition["key"]
                        .as_str()
                        .ok_or("Invalid provider field.")?
                        .into(),
                    value(world, panel.fields[4 + index])?.into(),
                );
            }
            Ok::<_, String>(FioteRequest::AgentProviderLogin {
                record: panel.binding.uid.clone(),
                fields: FioteSecret(json!(values).to_string()),
                password: None,
            })
        })();
        match result {
            Ok(message) => {
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
                request(world, owner, message);
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
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
