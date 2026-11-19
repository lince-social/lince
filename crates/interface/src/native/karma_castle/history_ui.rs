use bevy::prelude::*;
use serde_json::Value;

use super::*;

#[derive(Component)]
pub(super) struct History {
    output: Entity,
    pending: Option<String>,
}

impl replies::Pending for History {
    fn pending_id(&self) -> Option<&str> {
        self.pending.as_deref()
    }
}

#[derive(Clone)]
struct Inspect(String);

pub(super) fn inspect(world: &mut World, owner: Entity, uid: &str) {
    Inspect(uid.into()).apply(world, owner);
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let output = crate::native::edit_mode::label(world, parent, "", 13.0);
    world.entity_mut(owner).insert(History {
        output,
        pending: None,
    });
}

impl crate::native::actions::Action for Inspect {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        let id = nucleus::new_uid("karma-history");
        let result = send(
            world,
            owner,
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
                let mut text = world.get_mut::<Text>(output).unwrap();
                if text.0 != "Reading Rule history…" {
                    "Reading Rule history…".clone_into(&mut text.0);
                }
            }
            Err(error) => {
                let mut text = world.get_mut::<Text>(output).unwrap();
                if text.0 != error {
                    text.0 = error;
                }
            }
        }
    }
}

pub(super) fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
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
    let mut current = world.get_mut::<Text>(output).unwrap();
    if current.0 != text {
        current.0 = text;
    }
}

fn quantity(value: &Value) -> String {
    nucleus::DecimalValue::deserialize(value)
        .map_or_else(|_| "unavailable".into(), |value| value.to_string())
}

fn describe(data: &Value) -> String {
    let _describe = nucleus::performance::Span::new("karma-history-description");
    let applications = data["applications"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
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
