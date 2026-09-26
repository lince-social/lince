use super::*;

#[derive(Clone)]
pub(super) enum Kind {
    Text,
    Number,
    Filter,
    Scope,
    Choice(Vec<(String, Value)>),
}

pub(super) struct Field<'a>(pub &'a str, pub &'a str, pub Kind, pub Value);

#[derive(Component)]
pub(super) struct Input {
    pub caption: String,
    pub kind: Kind,
    pub value: Value,
    pub text: Option<Entity>,
    pub label: Entity,
}

#[derive(Component)]
pub(super) struct Form {
    pub owner: Entity,
    pub payload: Value,
    pub inputs: Vec<(String, Entity)>,
    pub output: Entity,
    pub confirmation: Option<String>,
    pub pending: Option<String>,
}

pub(super) fn form(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    caption: &str,
    payload: Value,
    fields: Vec<Field<'_>>,
    confirmation: Option<&str>,
) -> Entity {
    let container = panel::column(world, parent);
    let mut inputs = Vec::new();
    for Field(key, caption, kind, value) in fields {
        let field = panel::column(world, container);
        let label = label(world, field, caption);
        let text = match &kind {
            Kind::Choice(options) => {
                let row = panel::row(world, field);
                for (name, value) in options {
                    panel::button(world, row, field, name, Choose(value.clone()));
                }
                None
            }
            Kind::Scope => {
                let row = panel::row(world, field);
                for (name, mode) in [
                    ("All fields", "all"),
                    ("Named fields", "some"),
                    ("Minimum only", "none"),
                ] {
                    panel::button(world, row, field, name, Choose(json!(mode)));
                }
                Some(panel::field(
                    world,
                    field,
                    "Field names, separated by commas",
                    &scope_text(&value),
                ))
            }
            _ => Some(panel::field(
                world,
                field,
                caption,
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            )),
        };
        let value = if matches!(kind, Kind::Scope) {
            json!(if value.is_null() {
                "all"
            } else if value.as_array().is_some_and(Vec::is_empty) {
                "none"
            } else {
                "some"
            })
        } else {
            value
        };
        world.entity_mut(field).insert(Input {
            caption: caption.into(),
            kind,
            value,
            text,
            label,
        });
        show_choice(world, field);
        inputs.push((key.into(), field));
    }
    let output = panel::column(world, container);
    panel::button(world, container, container, caption, Submit);
    world.entity_mut(container).insert(Form {
        owner,
        payload,
        inputs,
        output,
        confirmation: confirmation.map(str::to_owned),
        pending: None,
    });
    container
}

fn scope_text(value: &Value) -> String {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

#[derive(Clone)]
struct Choose(Value);
impl Action for Choose {
    fn apply(&self, world: &mut World, entity: Entity) {
        if let Some(mut input) = world.get_mut::<Input>(entity) {
            input.value = self.0.clone();
        }
        show_choice(world, entity);
    }
}

fn show_choice(world: &mut World, entity: Entity) {
    let input = world.get::<Input>(entity).unwrap();
    let caption = input.caption.clone();
    let selected = match &input.kind {
        Kind::Choice(options) => options
            .iter()
            .find(|(_, value)| *value == input.value)
            .map(|(name, _)| name.clone()),
        Kind::Scope => Some(
            match input.value.as_str() {
                Some("all") => "All fields",
                Some("none") => "Minimum only",
                _ => "Named fields",
            }
            .into(),
        ),
        _ => None,
    };
    let (label, text, enabled) = (input.label, input.text, input.value == "some");
    if let Some(selected) = selected {
        panel::status(world, label, format!("{caption}: {selected}"));
        if let Some(text) = text {
            world.get_mut::<Node>(text).unwrap().display = if enabled {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}

pub(super) fn payload(world: &World, entity: Entity) -> Result<Value, String> {
    let form = world.get::<Form>(entity).ok_or("This form is closed")?;
    let mut payload = form.payload.clone();
    for (path, entity) in &form.inputs {
        let input = world.get::<Input>(*entity).ok_or("This field is closed")?;
        let text = input
            .text
            .map(|entity| panel::value(world, entity))
            .transpose()?
            .unwrap_or_default();
        let value = match &input.kind {
            Kind::Text => json!(text.trim()),
            Kind::Number => json!(
                text.trim()
                    .parse::<u32>()
                    .map_err(|_| "Enter a non-negative whole number")?
            ),
            Kind::Choice(_) => input.value.clone(),
            Kind::Scope => scope(&input.value, &text)?,
            Kind::Filter => {
                if !text.trim().is_empty() {
                    let predicate: protein::Predicate = serde_json::from_str(text.trim())
                        .map_err(|e| format!("Invalid filter: {e}"))?;
                    let query: protein::Protein =
                        serde_json::from_value(json!({"source":"record", "where":[predicate]}))
                            .map_err(|e| e.to_string())?;
                    protein::validate(&query).map_err(|e| e.to_string())?;
                }
                json!(text.trim())
            }
        };
        *payload.pointer_mut(path).ok_or("Invalid form field")? = value;
    }
    Ok(payload)
}

pub(super) fn scope(mode: &Value, text: &str) -> Result<Value, String> {
    match mode.as_str() {
        Some("all") => Ok(Value::Null),
        Some("none") => Ok(json!([])),
        Some("some") => {
            let mut fields: Vec<_> = text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            fields.sort_unstable();
            fields.dedup();
            if fields.is_empty() {
                return Err("Name at least one field, or choose Minimum only".into());
            }
            Ok(json!(fields))
        }
        _ => Err("Choose a sharing limit".into()),
    }
}

#[derive(Clone)]
pub(super) struct Submit;
impl Action for Submit {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        let output = form.output;
        let result = payload(world, entity).and_then(|payload| {
            if let Some(question) = world
                .get::<Form>(entity)
                .and_then(|form| form.confirmation.clone())
            {
                panel::clear(world, output);
                label(world, output, &question);
                panel::button(world, output, entity, "Confirm", Confirm(payload));
                panel::button(world, output, output, "Cancel", Clear);
                Ok(())
            } else {
                dispatch(world, entity, payload)
            }
        });
        if let Err(error) = result {
            report(world, output, &error);
        }
    }
}

#[derive(Clone)]
struct Confirm(Value);
impl Action for Confirm {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        let output = form.output;
        if let Err(error) = dispatch(world, entity, self.0.clone()) {
            report(world, output, &error);
        }
    }
}

#[derive(Clone)]
struct Clear;
impl Action for Clear {
    fn apply(&self, world: &mut World, entity: Entity) {
        panel::clear(world, entity);
    }
}

pub(super) fn boolean() -> Kind {
    Kind::Choice(vec![
        ("On".into(), json!(true)),
        ("Off".into(), json!(false)),
    ])
}

pub(super) fn choices(values: &[&str]) -> Kind {
    Kind::Choice(values.iter().map(|s| ((*s).into(), json!(s))).collect())
}

pub(super) fn input_text(world: &World, form: Entity, path: &str) -> Option<Entity> {
    let form = world.get::<Form>(form)?;
    let entity = form.inputs.iter().find(|(key, _)| key == path)?.1;
    world.get::<Input>(entity)?.text
}
