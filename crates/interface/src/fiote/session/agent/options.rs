use super::*;
use serde_json::Value;

pub(super) fn show(world: &mut World, owner: Entity, content: Entity, saved: Option<&FioteStatus>) {
    crate::edit_mode::label(world, content, "Model and response settings", 18.0);
    crate::edit_mode::label(
        world,
        content,
        "Saved choices apply to this Fiote's next replies. Stop its running threads before changing settings.",
        14.0,
    );
    crate::description::button(world, content, owner, "Save and load choices", Load(false));
    crate::description::button(
        world,
        content,
        owner,
        "Reset choices to agent defaults",
        Load(true),
    );
    let Some(options) = saved
        .and_then(|saved| saved.agent_info.as_ref())
        .and_then(|info| info["configOptions"].as_array())
    else {
        crate::edit_mode::label(
            world,
            content,
            "Connect your agent, then load its available provider, model, thinking level and speed choices.",
            14.0,
        );
        return;
    };
    for option in options {
        let Some(id) = option["id"].as_str() else {
            continue;
        };
        let name = option["name"].as_str().unwrap_or(id);
        let choices = choices(option);
        if choices.is_empty() {
            continue;
        }
        let selected = choices
            .iter()
            .find(|(_, value)| value == &option["currentValue"])
            .map(|(name, _)| name.as_str())
            .unwrap_or("Agent default");
        crate::edit_mode::label(world, content, name, 14.0);
        let toggle = crate::dropdown::spawn(
            world,
            content,
            owner,
            name,
            selected,
            choices
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        crate::actions![Select {
                            option: id.into(),
                            value: value.clone(),
                        }],
                    )
                })
                .collect(),
        );
        let menu = world.get::<crate::dropdown::Dropdown>(toggle).unwrap().menu;
        world.get_mut::<Node>(menu).unwrap().max_height = px(240);
        crate::scroll_sand::attach(world, menu);
        if let Some(description) = option["description"].as_str() {
            crate::edit_mode::label(world, content, description, 13.0);
        }
    }
    for (label, hints) in [
        ("Provider", &["provider"][..]),
        ("Model", &["model"][..]),
        (
            "Thinking level",
            &["thought_level", "thinking", "reasoning", "effort"][..],
        ),
        (
            "Fast / normal",
            &["speed", "fast", "service_tier", "service tier"][..],
        ),
    ] {
        if !options.iter().any(|option| {
            let title = format!(
                "{} {}",
                option["id"].as_str().unwrap_or(""),
                option["name"].as_str().unwrap_or("")
            )
            .to_lowercase();
            !choices(option).is_empty()
                && hints
                    .iter()
                    .any(|hint| title.contains(hint) || option["category"].as_str() == Some(*hint))
        }) {
            let message = if label == "Provider"
                && saved
                    .and_then(|saved| saved.agent_info.as_ref())
                    .and_then(|info| info["providers"].as_array())
                    .is_some_and(|providers| !providers.is_empty())
            {
                "Provider: use Change provider / sign in.".into()
            } else {
                format!("{label}: not offered by this agent.")
            };
            crate::edit_mode::label(world, content, &message, 14.0);
        }
    }
}

fn choices(option: &Value) -> Vec<(String, Value)> {
    if option["type"] == "boolean" {
        return vec![("Off".into(), false.into()), ("On".into(), true.into())];
    }
    if option["type"] != "select" {
        return Vec::new();
    }
    let mut choices = Vec::new();
    for entry in option["options"].as_array().into_iter().flatten() {
        if let Some(group) = entry["options"].as_array() {
            for choice in group {
                if let (Some(name), Some(value)) =
                    (choice["name"].as_str(), choice["value"].as_str())
                {
                    choices.push((
                        format!("{} · {name}", entry["name"].as_str().unwrap_or("Models")),
                        value.into(),
                    ));
                }
            }
        } else if let (Some(name), Some(value)) = (entry["name"].as_str(), entry["value"].as_str())
        {
            choices.push((name.into(), value.into()));
        }
    }
    choices
}

fn submit(
    world: &mut World,
    owner: Entity,
    build: impl FnOnce(cell::FioteAgentConfig, &Panel) -> Result<FioteRequest, String>,
) {
    let Some(panel) = world.get::<Panel>(owner) else {
        return;
    };
    if panel.pending.is_some() {
        return;
    }
    let status = panel.status;
    let result = if panel
        .saved
        .as_ref()
        .is_some_and(|saved| !saved.running.is_empty())
    {
        Err("Stop this Fiote before changing its settings.".into())
    } else {
        config(world, owner).and_then(|config| build(config, panel))
    };
    match result {
        Ok(message) => {
            request(world, owner, message);
            if world.get::<Panel>(owner).unwrap().pending.is_some() {
                world.entity_mut(owner).insert(RefreshDraft);
            }
        }
        Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
    }
}

#[derive(Clone)]
struct Load(bool);
impl Action for Load {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, |mut config, panel| {
            if self.0 {
                config.options.clear();
            }
            Ok(FioteRequest::AgentOptions {
                record: panel.binding.uid.clone(),
                config,
            })
        });
    }
}

#[derive(Clone)]
pub(super) struct SaveDirectory;
impl Action for SaveDirectory {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, |config, panel| {
            Ok(FioteRequest::AgentConfigure {
                record: panel.binding.uid.clone(),
                config,
            })
        });
    }
}

#[derive(Clone)]
struct Select {
    option: String,
    value: Value,
}
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        submit(world, owner, |config, panel| {
            if panel.saved.as_ref().and_then(|saved| saved.agent.as_ref()) != Some(&config) {
                return Err(
                    "Save and load choices after editing the connection or working directory."
                        .into(),
                );
            }
            Ok(FioteRequest::AgentSetOption {
                record: panel.binding.uid.clone(),
                option: self.option.clone(),
                value: self.value.clone(),
            })
        });
    }
}

#[cfg(test)]
mod tests;
