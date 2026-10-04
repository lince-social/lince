use super::*;

fn display(value: &Value) -> String {
    let text = if let Some(text) = value.as_str() {
        text.to_owned()
    } else {
        value.to_string()
    };
    let mut visible = text.chars().take(2048).collect::<String>();
    if text.chars().count() > 2048 {
        visible.push_str("…");
    }
    visible
}

pub(super) fn change(world: &mut World, parent: Entity, value: &Value) {
    let Some(operation) = value["operation"].as_str() else {
        return;
    };
    let description = match operation {
        "add" => format!(
            "Add placement {} · {}",
            display(&value["element"]["id"]),
            value["element"]["component"]["state"]["kind"]
                .as_str()
                .or_else(|| value["element"]["component"]["composition"]["name"].as_str())
                .unwrap_or("component")
        ),
        "configure" => format!("Replace configuration of {}", display(&value["element"])),
        "move" => format!(
            "Move {} to {}",
            display(&value["element"]),
            display(&value["position"])
        ),
        "resize" => format!(
            "Resize {} to {}",
            display(&value["element"]),
            display(&value["size"])
        ),
        "remove" => format!("Remove placement {}", display(&value["element"])),
        "rename" => format!("Rename workspace to {}", display(&value["name"])),
        "policy" => {
            "Change workspace admission and Record ceiling; suspend consequential controls".into()
        }
        "area" => format!("Change Area {} recipe", display(&value["element"])),
        "invoke" => format!(
            "Use declared control {} · {}",
            display(&value["element"]),
            value["event"].as_str().unwrap_or("press")
        ),
        "create_record" => format!(
            "Create Record {} · {}",
            display(&value["draft"]["uid"]),
            display(&value["draft"]["head"])
        ),
        "restore_record" => format!("Restore Record {}", display(&value["record"])),
        "delete_record" => format!("Delete Record {}", display(&value["record"])),
        _ => format!("Edit Record {}", display(&value["record"])),
    };
    label(world, parent, &description);
    if let Some(changes) = value["changes"].as_object() {
        for (property, value) in changes {
            if !value.is_null() && value != &json!([]) {
                label(world, parent, &format!("{property}: {}", display(value)));
            }
        }
    }
    for edit in value["edits"].as_array().into_iter().flatten() {
        if let Some(fields) = edit.as_object() {
            for (property, value) in fields {
                label(world, parent, &format!("{property}: {}", display(value)));
            }
        }
    }
}

fn difference(world: &mut World, parent: Entity, before: &Value, after: &Value) {
    if before == after {
        return;
    }
    if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
        let keys = before
            .keys()
            .chain(after.keys())
            .collect::<std::collections::BTreeSet<_>>();
        for key in keys {
            let old = before.get(key).unwrap_or(&Value::Null);
            let new = after.get(key).unwrap_or(&Value::Null);
            if old != new {
                label(
                    world,
                    parent,
                    &format!("{key}\nBefore: {}\nAfter: {}", display(old), display(new)),
                );
            }
        }
    } else {
        label(
            world,
            parent,
            &format!("Before: {}\nAfter: {}", display(before), display(after)),
        );
    }
}

pub(super) fn preview(world: &mut World, owner: Entity, data: &Value) {
    let parent = world.get::<WorkspaceSync>(owner).unwrap().history;
    panel::clear(world, parent);
    label(
        world,
        parent,
        &format!(
            "Preview against current revision {} · {}",
            data["base_revision"],
            if data["requires_review"] == true {
                "review required"
            } else {
                "automatic if still independent"
            }
        ),
    );
    label(
        world,
        parent,
        data["explanation"]
            .as_str()
            .unwrap_or("Current authority is checked again before acceptance."),
    );
    if let (Ok(before), Ok(after)) = (
        serde_json::from_value::<Layout>(data["layout_before"].clone()),
        serde_json::from_value::<Layout>(data["layout_after"].clone()),
    ) {
        for element in &before.elements {
            if !after.elements.iter().any(|new| new.id == element.id) {
                label(world, parent, &format!("Remove placement {}", element.id));
            }
        }
        for element in &after.elements {
            if let Some(old) = before.elements.iter().find(|old| old.id == element.id) {
                if old.geometry != element.geometry || old.component != element.component {
                    label(world, parent, &format!("Placement {}", element.id));
                    difference(
                        world,
                        parent,
                        &json!({"position":old.geometry.position,"size":old.geometry.size,"component":old.component}),
                        &json!({"position":element.geometry.position,"size":element.geometry.size,"component":element.component}),
                    );
                }
            } else {
                label(world, parent, &format!("Add placement {}", element.id));
                difference(
                    world,
                    parent,
                    &Value::Null,
                    &serde_json::to_value(element).unwrap(),
                );
            }
        }
        difference(
            world,
            parent,
            &json!({"Area recipes":before.areas,"Suspended Areas":before.disabled_areas,"Suspended controls":before.disabled_controls}),
            &json!({"Area recipes":after.areas,"Suspended Areas":after.disabled_areas,"Suspended controls":after.disabled_controls}),
        );
    }
    for consequence in data["consequences"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "Record {} · {}",
                display(&consequence["record"]),
                if consequence["created"] == true {
                    "created"
                } else if consequence["deleted"] == true {
                    "deleted"
                } else {
                    "changed"
                }
            ),
        );
        difference(world, parent, &consequence["before"], &consequence["after"]);
        difference(
            world,
            parent,
            &json!({"assertions":consequence["assertion_details_before"]}),
            &json!({"assertions":consequence["assertion_details_after"]}),
        );
    }
    for affected in data["affected_actors"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "Person {} · view {} → {} · edit {} → {}",
                affected["person"],
                affected["view_before"],
                affected["view_after"],
                affected["edit_before"],
                affected["edit_after"]
            ),
        );
    }
    if data.get("layout_after").is_none() && data.get("affected_actors").is_none() {
        label(world, parent, &display(data));
    }
}
