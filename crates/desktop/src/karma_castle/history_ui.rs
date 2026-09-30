use bevy::prelude::*;
use serde_json::Value;

use super::*;

#[derive(Component)]
struct History {
    output: Entity,
    pending: Option<String>,
}

#[derive(Clone)]
struct Inspect(String);

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let output = crate::edit_mode::label(world, parent, "", 13.0);
    world.entity_mut(owner).insert(History {
        output,
        pending: None,
    });
}

pub(super) fn button(world: &mut World, owner: Entity, parent: Entity, rule: &str) {
    crate::castle_feed::button(world, parent, owner, "History", Inspect(rule.into()));
}

impl crate::actions::Action for Inspect {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        let id = nucleus::new_uid("karma-history");
        let result = send(
            world,
            ClientMessage::Act {
                id: id.clone(),
                action: engine::actions::Action::InspectKarmaRuleHistory {
                    rule: self.0.clone(),
                    limit: 10,
                },
            },
        );
        let output = world.get::<History>(owner).unwrap().output;
        match result {
            Ok(()) => {
                world.get_mut::<History>(owner).unwrap().pending = Some(id);
                world.get_mut::<Text>(output).unwrap().0 = "Reading Rule history…".into();
            }
            Err(error) => world.get_mut::<Text>(output).unwrap().0 = error,
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &History)>()
        .iter(world)
        .find(|(_, history)| history.pending.as_ref() == Some(id))
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    let mut history = world.get_mut::<History>(owner).unwrap();
    history.pending = None;
    let output = history.output;
    let text = match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => describe(data),
        ServerMessage::Error { message, .. } => message.clone(),
        _ => "No Rule history returned".into(),
    };
    world.get_mut::<Text>(output).unwrap().0 = text;
    true
}

fn quantity(value: &Value) -> String {
    serde_json::from_value::<nucleus::DecimalValue>(value.clone())
        .map_or_else(|_| "unavailable".into(), |value| value.to_string())
}

fn describe(data: &Value) -> String {
    let applications = data["applications"].as_array().cloned().unwrap_or_default();
    if applications.is_empty() {
        return "This Rule has no recorded applications".into();
    }
    applications
        .iter()
        .map(|application| {
            let evidence = &application["evidence"];
            let evaluation = &evidence["evaluation"];
            let mut lines = vec![format!(
                "{} · revision {} · {}",
                application["at"].as_str().unwrap_or(""),
                application["revision"],
                application["status"].as_str().unwrap_or("unavailable")
            )];
            if let Some(source) = evidence["source"].as_str() {
                lines.push(format!("Condition: {source}"));
            }
            if let Some(readings) = evaluation["readings"].as_array() {
                for reading in readings {
                    lines.push(format!(
                        "{}(@{}) = {}",
                        reading["function"].as_str().unwrap_or("reading"),
                        reading["reference"].as_str().unwrap_or(""),
                        reading["error"]
                            .as_str()
                            .map_or_else(|| quantity(&reading["value"]), str::to_owned)
                    ));
                }
            }
            if !evaluation["computed"].is_null() {
                lines.push(format!(
                    "Computed {} · Threshold {} · carried {}",
                    quantity(&evaluation["computed"]),
                    if evaluation["gate_passed"] == true {
                        "passed"
                    } else {
                        "blocked"
                    },
                    quantity(&evaluation["carried"])
                ));
            }
            if let Some(reason) = application["reason"]
                .as_str()
                .or_else(|| application["unavailable"].as_str())
            {
                lines.push(reason.into());
            }
            if let Some(effects) = application["effects"].as_array() {
                for effect in effects {
                    lines.push(format!(
                        "Consequence {} · attempt {} · {}",
                        effect["position"],
                        effect["attempt"],
                        effect["status"].as_str().unwrap_or("unavailable")
                    ));
                    if let Some(reason) = effect["reason"].as_str() {
                        lines.push(reason.into());
                    }
                }
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_history_describes_saved_decisions_and_partial_failures() {
        let value = serde_json::json!({"applications":[{"at":"2030-01-01","revision":3,"status":"blocked","evidence":{"source":"@stock * 2","evaluation":{"computed":{"value":"4","scale":0},"gate_passed":false,"carried":null,"readings":[{"function":"quantity","reference":"r_stock","value":{"value":"2","scale":0}}]}}},{"revision":4,"status":"failed","reason":"price is unavailable","evidence":{"evaluation":{"readings":[{"function":"extension","reference":"r_stock","error":"missing property"}]}}}]});
        let description = describe(&value);
        assert!(description.contains("quantity(@r_stock) = 2"));
        assert!(description.contains("Computed 4 · Threshold blocked"));
        assert!(description.contains("price is unavailable"));
        assert!(description.contains("missing property"));
    }
}
