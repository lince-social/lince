use crate::app::{ButtonIntent, Input, Mobile};
use bevy::{prelude::*, text::EditableText};

pub fn command(world: &mut World, encoded: &str) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let Ok(bytes) = STANDARD.decode(encoded) else {
        return;
    };
    let Ok(request) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let result = match request["command"].as_str().unwrap_or_default() {
        "snapshot" => Ok(()),
        "press" => {
            let title = request["label"].as_str().unwrap_or_default();
            let buttons = buttons(world);
            let found: Vec<_> = buttons
                .into_iter()
                .filter(|(_, label)| label == title)
                .collect();
            if let [(entity, _)] = found.as_slice() {
                let action = world
                    .get::<ButtonIntent>(*entity)
                    .expect("button")
                    .0
                    .clone();
                crate::app::queue_intent(world, action);
                Ok(())
            } else {
                Err(format!(
                    "Expected one button named {title}, found {}",
                    found.len()
                ))
            }
        }
        "edit" => {
            let key = request["key"].as_str().unwrap_or_default();
            let value = request["value"].as_str().unwrap_or_default();
            let mut matched = false;
            for (input, mut text) in world.query::<(&Input, &mut EditableText)>().iter_mut(world) {
                if input.key == key {
                    text.editor.set_text(value);
                    matched = true;
                }
            }
            if matched {
                crate::app::capture(world);
                Ok(())
            } else {
                Err(format!("Field {key} is unavailable"))
            }
        }
        _ => Err("Unknown smoke command".into()),
    };
    let buttons = buttons(world)
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>();
    let fields: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.key != "login/password")
        .map(|(input, text)| serde_json::json!({"key":input.key,"value":text.value().to_string()}))
        .collect();
    let state = world.resource::<Mobile>();
    bevy::log::info!(
        "LINCE_SMOKE {}",
        serde_json::json!({"id":request["id"],"error":result.err(),"ready":state.ready,"setup":state.setup_required,"page":format!("{:?}",state.navigation.current),"status":state.status,"buttons":buttons,"fields":fields,"record":state.rows.get("record").and_then(|rows| rows.first()).map(|row| serde_json::json!({"uid":row["uid"],"head":row["head"]}))})
    );
}

fn buttons(world: &mut World) -> Vec<(Entity, String)> {
    world
        .query::<(Entity, &Children, &ButtonIntent)>()
        .iter(world)
        .map(|(entity, children, _)| {
            (
                entity,
                children
                    .iter()
                    .filter_map(|child| world.get::<Text>(child))
                    .map(|text| text.0.as_str())
                    .collect::<String>(),
            )
        })
        .collect()
}
