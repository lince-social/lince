use super::*;
use serde_json::{Value, json};

#[derive(Component)]
struct Card {
    parent: Entity,
    record: String,
    request: String,
    form: Option<Entity>,
    url: Option<String>,
    status: Entity,
    pending: Option<String>,
}

pub(super) fn sync(world: &mut World, saved: &FioteStatus) {
    let mut parents: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .filter(|(_, panel)| panel.binding.uid == saved.record && panel.step != Step::Closed)
        .map(|(owner, panel)| (owner, panel.content, None))
        .collect();
    parents.extend(
        world
            .query::<(Entity, &ThreadControl)>()
            .iter(world)
            .filter(|(_, control)| control.record == saved.record)
            .map(|(owner, control)| (owner, owner, Some(control.thread.clone()))),
    );
    for (owner, parent, thread) in parents {
        let requests: Vec<_> = saved
            .questions
            .iter()
            .filter(|question| question.thread == thread)
            .map(|question| &question.request)
            .collect();
        let old: Vec<_> = world
            .query::<(Entity, &Card)>()
            .iter(world)
            .filter(|(_, card)| card.parent == owner)
            .map(|(entity, card)| (entity, card.request.clone()))
            .collect();
        for (entity, id) in &old {
            if !requests.iter().any(|request| request.id == *id) {
                world.despawn(*entity);
            }
        }
        for request in requests {
            if old.iter().any(|(_, id)| *id == request.id) {
                continue;
            }
            let card = world
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(5),
                        ..default()
                    },
                    ChildOf(parent),
                ))
                .id();
            crate::edit_mode::label(world, card, "Private agent interaction", 16.0);
            crate::accessibility::question(world, card, &request.prompt);
            crate::edit_mode::label(world, card, &request.prompt, 14.0);
            let form =
                request.schema.as_ref().and_then(|schema| {
                    match crate::question_form::create(world, card, schema, None) {
                        Ok(form) => Some(form),
                        Err(error) => {
                            crate::edit_mode::label(world, card, &error, 13.0);
                            None
                        }
                    }
                });
            let url = request.url.as_ref().map(|url| url.0.clone());
            if let Some(url) = &url {
                let host = url
                    .split_once("://")
                    .map(|(_, remainder)| {
                        remainder.split(['/', '?', '#']).next().unwrap_or_default()
                    })
                    .unwrap_or_default();
                crate::edit_mode::label(
                    world,
                    card,
                    &format!(
                        "Destination: {host}\nOpening the page does not confirm authorization. The agent must verify completion."
                    ),
                    13.0,
                );
                crate::description::button(world, card, card, "Open requested page", Open);
                crate::description::button(world, card, card, "Copy private address", Copy);
            }
            let status = crate::edit_mode::label(world, card, "", 12.0);
            crate::accessibility::status(world, status);
            if form.is_some() || url.is_some() {
                crate::description::button(
                    world,
                    card,
                    card,
                    if form.is_some() {
                        "Submit answers"
                    } else {
                        "Continue after browser interaction"
                    },
                    Respond("accept"),
                );
            }
            crate::description::button(world, card, card, "Decline", Respond("decline"));
            crate::description::button(world, card, card, "Cancel", Respond("cancel"));
            world.entity_mut(card).insert(Card {
                parent: owner,
                record: saved.record.clone(),
                request: request.id.clone(),
                form,
                url,
                status,
                pending: None,
            });
        }
    }
}

pub(super) fn receive(world: &mut World, id: &str, result: &Result<FioteStatus, String>) {
    let cards: Vec<_> = world
        .query::<(Entity, &Card)>()
        .iter(world)
        .filter(|(_, card)| card.pending.as_deref() == Some(id))
        .map(|(entity, card)| (entity, card.status))
        .collect();
    for (entity, status) in cards {
        world.get_mut::<Card>(entity).unwrap().pending = None;
        world.get_mut::<Text>(status).unwrap().0 = result
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "Response delivered to the agent".into());
    }
    if let Ok(saved) = result {
        sync(world, saved);
    }
}

#[derive(Clone)]
struct Respond(&'static str);
impl Action for Respond {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(card) = world.get::<Card>(owner) else {
            return;
        };
        if card.pending.is_some() {
            return;
        }
        let status = card.status;
        let mut answer = json!({"action":self.0});
        if self.0 == "accept" {
            if let Some(form) = card.form {
                match crate::question_form::answers(world, form) {
                    Ok(content) => answer["content"] = content,
                    Err(error) => {
                        world.get_mut::<Text>(status).unwrap().0 = error;
                        return;
                    }
                }
            }
        }
        let answer = match serde_json::from_value::<Value>(answer).and_then(serde_json::from_value)
        {
            Ok(answer) => answer,
            Err(error) => {
                world.get_mut::<Text>(status).unwrap().0 = error.to_string();
                return;
            }
        };
        match send(
            world,
            owner,
            FioteRequest::AgentQuestionAnswer {
                record: card.record.clone(),
                request: card.request.clone(),
                answer,
            },
        ) {
            Ok(id) => {
                world.get_mut::<Card>(owner).unwrap().pending = Some(id);
                world.get_mut::<Text>(status).unwrap().0 = "Submitting…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

#[derive(Clone)]
struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(card) = world.get::<Card>(owner) else {
            return;
        };
        let Some(url) = &card.url else { return };
        let result = if cfg!(target_os = "windows") {
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", url])
                .spawn()
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(url).spawn()
        } else {
            std::process::Command::new("xdg-open").arg(url).spawn()
        };
        let status = card.status;
        world.get_mut::<Text>(status).unwrap().0 = if result.is_ok() {
            "Browser opened. Complete the interaction there, then continue.".into()
        } else {
            "Could not open the browser. Copy the private address and open it yourself.".into()
        };
    }
}

#[derive(Clone)]
struct Copy;
impl Action for Copy {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some((url, status)) = world
            .get::<Card>(owner)
            .and_then(|card| card.url.clone().map(|url| (url, card.status)))
        else {
            return;
        };
        let result = world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .ok_or_else(|| "Clipboard is unavailable.".to_string())
            .and_then(|mut clipboard| clipboard.set_text(url).map_err(|error| error.to_string()));
        world.get_mut::<Text>(status).unwrap().0 = result
            .err()
            .unwrap_or_else(|| "Private address copied".into());
    }
}
