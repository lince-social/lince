use super::*;
use protein::authority::{
    AssertionGrant, AssertionProperty, AssertionRole, AssertionTarget, ExtensionProperty, Property,
    RolePolicy,
};

#[derive(Clone)]
enum Edit {
    Property(Property),
    Extension(Entity, Entity),
    Assertion {
        predicate: Entity,
        target: Entity,
        identity: bool,
        remove: bool,
        quantity: bool,
        unit: bool,
    },
}

#[derive(Clone)]
struct Modify {
    raw: Entity,
    ceiling: bool,
    grant: Entity,
    message: Entity,
    edit: Edit,
}

impl Action for Modify {
    fn apply(&self, world: &mut World, _: Entity) {
        let result = self.update(world);
        if let Some(mut text) = world.get_mut::<Text>(self.message) {
            text.0 = match result {
                Ok(()) => "Policy draft changed. Review it and save/propose to apply.".into(),
                Err(error) => error,
            };
        }
    }
}

impl Modify {
    fn update(&self, world: &mut World) -> Result<(), String> {
        let mut value: Value = serde_json::from_str(&panel::value(world, self.raw)?)
            .map_err(|error| error.to_string())?;
        let chosen = if self.ceiling {
            &mut value["ceiling"]
        } else {
            &mut value
        };
        let mut policy: RolePolicy =
            serde_json::from_value(chosen.clone()).map_err(|error| error.to_string())?;
        let index = panel::value(world, self.grant)?
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .ok_or("Choose a grant number starting at 1")?;
        let grant = policy
            .grants
            .get_mut(index)
            .ok_or("Choose an existing grant")?;
        match &self.edit {
            Edit::Property(property) => {
                if !grant.properties.remove(property) {
                    grant.properties.insert(property.clone());
                }
            }
            Edit::Extension(namespace, field) => {
                let namespace = panel::value(world, *namespace)?.trim().to_owned();
                let field = panel::value(world, *field)?.trim().to_owned();
                if namespace.is_empty()
                    || namespace.len() > 128
                    || field.is_empty()
                    || field.len() > 128
                {
                    return Err(
                        "Enter an extension namespace and field, each within 128 bytes".into(),
                    );
                }
                let property = Property::Extension(ExtensionProperty { namespace, field });
                if !grant.properties.remove(&property) {
                    grant.properties.insert(property);
                }
            }
            Edit::Assertion {
                predicate,
                target,
                identity,
                remove,
                quantity,
                unit,
            } => {
                let predicate_uid = panel::value(world, *predicate)?.trim().to_owned();
                let target = panel::value(world, *target)?.trim().to_owned();
                if !nucleus::valid_uid(&predicate_uid, "c")
                    || !target.is_empty() && target != "*" && !nucleus::valid_uid(&target, "r")
                {
                    return Err(
                        "Use a Concept UID and a target Record UID, * or empty for unary".into(),
                    );
                }
                if *identity && !target.is_empty() {
                    return Err("Identity assertions must be unary".into());
                }
                let target = if target.is_empty() {
                    AssertionTarget::Unary
                } else if target == "*" {
                    AssertionTarget::AnyReadableRecord
                } else {
                    AssertionTarget::Record(target)
                };
                let mut properties = std::collections::BTreeSet::new();
                if !identity && *quantity {
                    properties.insert(AssertionProperty::Quantity);
                }
                if !identity && *unit {
                    properties.insert(AssertionProperty::Unit);
                }
                let rules = if *remove {
                    &mut grant.assertions_remove
                } else {
                    &mut grant.assertions_add
                };
                if rules.len() >= 128 {
                    return Err("Keep each grant within 128 assertion rules".into());
                }
                rules.push(AssertionGrant {
                    predicate_uid,
                    target,
                    role: if *identity {
                        AssertionRole::Identity
                    } else {
                        AssertionRole::Ordinary
                    },
                    properties,
                });
            }
        }
        *chosen = serde_json::to_value(policy).map_err(|error| error.to_string())?;
        *world
            .get_mut::<EditableText>(self.raw)
            .ok_or("Policy editor unavailable")? =
            draft_editor(&serde_json::to_string_pretty(&value).unwrap());
        Ok(())
    }
}

pub(crate) fn mount(world: &mut World, parent: Entity, owner: Entity, raw: Entity, ceiling: bool) {
    label(
        world,
        parent,
        "Advanced grant controls · changes remain a draft until saved",
    );
    let grant = panel::field(world, parent, "Grant number", "1");
    let namespace = panel::field(world, parent, "Extension namespace", "");
    let field = panel::field(world, parent, "Extension field", "");
    let message = label(world, parent, "");
    let button = |world: &mut World, title: &str, edit| {
        panel::button(
            world,
            parent,
            owner,
            title,
            Modify {
                raw,
                ceiling,
                grant,
                message,
                edit,
            },
        );
    };
    button(
        world,
        "Toggle extension field authority",
        Edit::Extension(namespace, field),
    );
    for (title, property) in [
        ("Toggle head authority", Property::Head),
        ("Toggle body authority", Property::Body),
        ("Toggle slug authority", Property::Slug),
        ("Toggle unit authority", Property::Unit),
    ] {
        button(world, title, Edit::Property(property));
    }
    let predicate = panel::field(world, parent, "Assertion Concept UID", "");
    let target = panel::field(
        world,
        parent,
        "Assertion target (empty = unary, Record UID, * = readable)",
        "",
    );
    for (title, identity, remove, quantity, unit) in [
        ("Allow adding identity", true, false, false, false),
        ("Allow removing identity", true, true, false, false),
        (
            "Allow adding ordinary assertion",
            false,
            false,
            false,
            false,
        ),
        (
            "Allow removing ordinary assertion",
            false,
            true,
            false,
            false,
        ),
        (
            "Allow ordinary assertion quantity and unit changes",
            false,
            false,
            true,
            true,
        ),
    ] {
        button(
            world,
            title,
            Edit::Assertion {
                predicate,
                target,
                identity,
                remove,
                quantity,
                unit,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(world: &mut World, caption: &str, value: &str) {
        let field = world
            .query::<(Entity, &EditableText, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .find(|(_, _, node)| node.label() == Some(caption))
            .map(|(entity, _, _)| entity)
            .unwrap();
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(value);
    }

    fn click(world: &mut World, caption: &str) {
        let (owner, actions) = world
            .query::<(
                &crate::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(world)
            .find(|(_, node)| node.label() == Some(caption))
            .map(|(button, _)| (button.target, button.actions.clone()))
            .unwrap();
        actions.run(world, owner);
    }

    #[test]
    fn role_and_workspace_forms_edit_extension_fields_and_refuse_binary_identity_rules() {
        for ceiling in [false, true] {
            let mut app = crate::sand_panel::tests::app();
            let world = app.world_mut();
            let owner = world.spawn(Node::default()).id();
            let policy = json!({"read":{"all":[]},"grants":[{"operation":"update","selector":{"kind_eq":"plain"},"properties":[],"assertions_add":[],"assertions_remove":[]}]});
            let value = if ceiling {
                json!({"ceiling":policy,"required_capabilities":[]})
            } else {
                policy
            };
            let raw = panel::field(world, owner, "Raw policy", &value.to_string());
            mount(world, owner, owner, raw, ceiling);
            input(world, "Extension namespace", "audit");
            input(world, "Extension field", "color");
            click(world, "Toggle extension field authority");
            let value: Value = serde_json::from_str(&panel::value(world, raw).unwrap()).unwrap();
            let policy = if ceiling { &value["ceiling"] } else { &value };
            assert_eq!(
                policy["grants"][0]["properties"][0],
                json!({"extension":{"namespace":"audit","field":"color"}})
            );
            input(world, "Assertion Concept UID", &nucleus::new_uid("c"));
            input(
                world,
                "Assertion target (empty = unary, Record UID, * = readable)",
                &nucleus::new_uid("r"),
            );
            click(world, "Allow adding identity");
            assert_eq!(
                serde_json::from_str::<Value>(&panel::value(world, raw).unwrap()).unwrap(),
                value
            );
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == "Identity assertions must be unary")
            );
            input(
                world,
                "Assertion target (empty = unary, Record UID, * = readable)",
                "",
            );
            click(world, "Allow adding identity");
            let value: Value = serde_json::from_str(&panel::value(world, raw).unwrap()).unwrap();
            let policy = if ceiling { &value["ceiling"] } else { &value };
            assert_eq!(policy["grants"][0]["assertions_add"][0]["role"], "identity");
            assert_eq!(
                policy["grants"][0]["assertions_add"][0]["properties"],
                json!([])
            );
        }
    }
}
