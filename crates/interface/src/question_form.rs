use crate::actions::Action;
use bevy::{prelude::*, text::EditableText};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "regex",
        author: "The regex contributors",
        license: include_str!("../licenses/regex-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "url",
        author: "The rust-url contributors",
        license: include_str!("../licenses/url-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "chrono",
        author: "The Chrono developers",
        license: include_str!("../licenses/chrono.txt"),
    },
];

#[derive(Component)]
struct Form {
    schema: Value,
    fields: BTreeMap<String, Entity>,
}

#[derive(Component)]
struct Field {
    value: Option<Value>,
    input: Option<Entity>,
    label: Entity,
    multiple: bool,
}

pub(crate) fn create(
    world: &mut World,
    parent: Entity,
    schema: &Value,
    initial: Option<&Value>,
) -> Result<Entity, String> {
    nucleus::question::validate_schema(schema)?;
    let owner = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            ChildOf(parent),
            crate::sand_store::SandCredits(CREDITS),
        ))
        .id();
    let mut fields = BTreeMap::new();
    for (name, field) in schema["properties"].as_object().unwrap() {
        let required = schema["required"]
            .as_array()
            .is_some_and(|fields| fields.contains(&Value::from(name.clone())));
        let title = field["title"].as_str().unwrap_or(name);
        crate::edit_mode::label(
            world,
            owner,
            &format!("{title}{}", if required { " *" } else { " (optional)" }),
            14.0,
        );
        if let Some(description) = field["description"].as_str() {
            crate::edit_mode::label(world, owner, description, 12.0);
        }
        let entity = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ChildOf(owner),
            ))
            .id();
        let label = crate::edit_mode::label(world, entity, "", 12.0);
        let initial_value = initial
            .and_then(|values| values.get(name))
            .or_else(|| field.get("default"))
            .cloned();
        let multiple = field["type"] == "array";
        let choices = if field["type"] == "boolean" {
            vec![("Yes".into(), json!(true)), ("No".into(), json!(false))]
        } else {
            nucleus::question::choices(if multiple { &field["items"] } else { field })
                .into_iter()
                .map(|(label, value)| (label, Value::from(value)))
                .collect()
        };
        let input = if !choices.is_empty() {
            for (title, value) in choices {
                crate::description::button(world, entity, entity, &title, Choose(Some(value)));
            }
            None
        } else {
            let value = initial_value
                .as_ref()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string())
                })
                .unwrap_or_default();
            let input = world
                .spawn((
                    crate::sand::text_editor(
                        &value,
                        world.resource::<crate::theme::Typography>(),
                        0,
                    ),
                    ChildOf(entity),
                ))
                .insert(Node {
                    width: percent(100),
                    max_height: px(120),
                    ..default()
                })
                .id();
            world.get_mut::<EditableText>(input).unwrap().max_characters = Some(8192);
            Some(input)
        };
        world.entity_mut(entity).insert(Field {
            value: initial_value,
            input,
            label,
            multiple,
        });
        if input.is_none() {
            crate::description::button(world, entity, entity, "Clear selection", Choose(None));
            refresh(world, entity);
        }
        fields.insert(name.clone(), entity);
    }
    world.entity_mut(owner).insert(Form {
        schema: schema.clone(),
        fields,
    });
    Ok(owner)
}

pub(crate) fn answers(world: &World, owner: Entity) -> Result<Value, String> {
    let form = world.get::<Form>(owner).ok_or("The question is closed.")?;
    let mut values = serde_json::Map::new();
    for (name, entity) in &form.fields {
        let field = world
            .get::<Field>(*entity)
            .ok_or("The question field is closed.")?;
        let value = if let Some(input) = field.input {
            let text = world
                .get::<EditableText>(input)
                .ok_or("The answer is unavailable.")?
                .value()
                .to_string();
            let required = form.schema["required"]
                .as_array()
                .is_some_and(|fields| fields.contains(&Value::from(name.clone())));
            if text.is_empty() && !required {
                None
            } else if form.schema["properties"][name]["type"] == "string" {
                Some(text.into())
            } else {
                Some(
                    serde_json::from_str(&text)
                        .map_err(|_| format!("Enter a number for {name}."))?,
                )
            }
        } else {
            field.value.clone()
        };
        if let Some(value) = value {
            values.insert(name.clone(), value);
        }
    }
    let answers = Value::Object(values);
    nucleus::question::validate_answers(&form.schema, &answers)?;
    Ok(answers)
}

fn refresh(world: &mut World, entity: Entity) {
    let Some(field) = world.get::<Field>(entity) else {
        return;
    };
    let (label, value) = (
        field.label,
        field
            .value
            .as_ref()
            .map(|value| format!("Selected: {value}"))
            .unwrap_or_else(|| "No selection".into()),
    );
    if let Some(mut text) = world.get_mut::<Text>(label) {
        text.0 = value;
    }
}

#[derive(Clone)]
struct Choose(Option<Value>);
impl Action for Choose {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(mut field) = world.get_mut::<Field>(entity) else {
            return;
        };
        if field.multiple && self.0.is_some() {
            let value = self.0.as_ref().unwrap();
            let mut selected = field
                .value
                .as_ref()
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if selected.contains(value) {
                selected.retain(|item| item != value);
            } else {
                selected.push(value.clone());
            }
            field.value = Some(selected.into());
        } else {
            field.value = self.0.clone();
        }
        refresh(world, entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_preserve_edits_validate_required_fields_and_collect_choices() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        let parent = world.spawn(Node::default()).id();
        let schema = json!({"type":"object","properties":{
            "name":{"type":"string","minLength":2},
            "count":{"type":"integer","minimum":1,"default":2},
            "agree":{"type":"boolean"},
            "choices":{"type":"array","items":{"type":"string","enum":["left","right"]},"minItems":1}
        },"required":["name","count","agree","choices"]});
        let form = create(&mut world, parent, &schema, None).unwrap();
        assert!(answers(&world, form).is_err());
        let fields = world.get::<Form>(form).unwrap().fields.clone();
        let input = world.get::<Field>(fields["name"]).unwrap().input.unwrap();
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("Edited name");
        Choose(Some(false.into())).apply(&mut world, fields["agree"]);
        Choose(Some("left".into())).apply(&mut world, fields["choices"]);
        Choose(Some("right".into())).apply(&mut world, fields["choices"]);
        Choose(Some("left".into())).apply(&mut world, fields["choices"]);
        assert_eq!(
            answers(&world, form).unwrap(),
            json!({"name":"Edited name","count":2,"agree":false,"choices":["right"]})
        );
        Choose(None).apply(&mut world, fields["agree"]);
        assert!(answers(&world, form).is_err());
    }
}
