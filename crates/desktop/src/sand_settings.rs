use crate::actions::Action;
use bevy::prelude::*;
pub use lince_interface::settings::{Control, Definition, Value, Values};

#[derive(Component, Clone)]
pub struct Declaration {
    pub name: String,
    pub settings: Vec<Definition>,
}

#[derive(Component, Clone, Default)]
pub struct Configuration(pub Values);

#[derive(EntityEvent, Clone)]
pub struct Changed {
    pub entity: Entity,
    pub id: String,
    pub value: Value,
}

pub fn declare(
    world: &mut World,
    entity: Entity,
    declaration: Declaration,
    values: Values,
) -> bool {
    if !values.valid(&declaration.settings)
        || declaration.name.trim().is_empty()
        || declaration.name.chars().count() > 80
        || declaration.name.chars().any(char::is_control)
        || declaration
            .settings
            .iter()
            .enumerate()
            .any(|(index, setting)| {
                declaration.settings[..index]
                    .iter()
                    .any(|other| other.id == setting.id)
            })
    {
        return false;
    }
    world
        .entity_mut(entity)
        .insert((declaration, Configuration(values)));
    true
}

pub fn set(world: &mut World, entity: Entity, id: &str, value: Option<Value>) -> bool {
    let Some(definition) = world
        .get::<Declaration>(entity)
        .and_then(|declaration| declaration.settings.iter().find(|setting| setting.id == id))
        .cloned()
    else {
        return false;
    };
    if value
        .as_ref()
        .is_some_and(|value| !definition.accepts(value))
    {
        return false;
    }
    let Some(mut configuration) = world.get_mut::<Configuration>(entity) else {
        return false;
    };
    if let Some(value) = value {
        configuration.0.0.insert(id.into(), value);
    } else {
        configuration.0.0.remove(id);
    }
    let value = configuration.0.resolve(&definition);
    world.trigger(Changed {
        entity,
        id: id.into(),
        value,
    });
    true
}

#[derive(Clone)]
struct Set {
    sand: Entity,
    id: String,
    value: Option<Value>,
    root: Entity,
}

impl Action for Set {
    fn apply(&self, world: &mut World, _: Entity) {
        if set(world, self.sand, &self.id, self.value.clone()) {
            crate::edit_mode::render_panel(world, self.root);
        }
    }
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity, sand: Entity) {
    let Some(declaration) = world.get::<Declaration>(sand).cloned() else {
        return;
    };
    let values = world
        .get::<Configuration>(sand)
        .map(|config| config.0.clone())
        .unwrap_or_default();
    crate::edit_mode::label(
        world,
        panel,
        &format!("{} settings", declaration.name),
        18.0,
    );
    for definition in declaration.settings {
        let heading = crate::sand_panel::row(world, panel);
        crate::edit_mode::label(world, heading, &definition.label, 14.0);
        crate::sand_panel::button(
            world,
            heading,
            root,
            "Reset",
            Set {
                sand,
                id: definition.id.clone(),
                value: None,
                root,
            },
        );
        let current = values.resolve(&definition);
        let action = |value| Set {
            sand,
            id: definition.id.clone(),
            value: Some(value),
            root,
        };
        match definition.control {
            Control::Number { min, max, step } => {
                let Value::Number(value) = current else {
                    continue;
                };
                let slider = crate::slider::spawn(
                    world,
                    panel,
                    &definition.label,
                    crate::slider::SliderSand {
                        start: min,
                        end: max,
                        step,
                        decimals: if step >= 1.0 { 0 } else { 6 },
                    },
                    value,
                    "",
                )
                .unwrap();
                let id = definition.id;
                world.entity_mut(slider).observe(
                    move |event: On<crate::slider::SliderChanged>, mut commands: Commands| {
                        let value = event.value;
                        let id = id.clone();
                        commands.queue(move |world: &mut World| {
                            set(world, sand, &id, Some(Value::Number(value)));
                        });
                    },
                );
            }
            Control::Toggle => {
                let Value::Toggle(value) = current else {
                    continue;
                };
                crate::sand_panel::button(
                    world,
                    panel,
                    root,
                    if value { "On" } else { "Off" },
                    action(Value::Toggle(!value)),
                );
            }
            Control::Choice(ref choices) => {
                let Value::Choice(value) = current else {
                    continue;
                };
                crate::dropdown::spawn(
                    world,
                    panel,
                    root,
                    &definition.label,
                    &value,
                    choices
                        .iter()
                        .map(|choice| {
                            (
                                choice.clone(),
                                crate::actions![action(Value::Choice(choice.clone()))],
                            )
                        })
                        .collect(),
                );
            }
        }
    }
}

pub(crate) fn text_definitions() -> Vec<Definition> {
    vec![
        Definition {
            id: "wrap".into(),
            label: "Wrap text".into(),
            default: Value::Toggle(true),
            control: Control::Toggle,
        },
        Definition {
            id: "overflow".into(),
            label: "Overflow".into(),
            default: Value::Choice("Scroll".into()),
            control: Control::Choice(vec!["Scroll".into(), "Grow".into()]),
        },
    ]
}

pub(crate) fn text(world: &mut World, entity: Entity) {
    let area = world.get::<crate::sand_text::SandText>(entity).unwrap();
    let mut values = area.settings.clone();
    values.0.insert(
        "overflow".into(),
        Value::Choice(
            match area.overflow {
                crate::sand_text::TextOverflow::Scroll => "Scroll",
                crate::sand_text::TextOverflow::Grow => "Grow",
            }
            .into(),
        ),
    );
    declare(
        world,
        entity,
        Declaration {
            name: "Text Sand".into(),
            settings: text_definitions(),
        },
        values,
    );
    world.entity_mut(entity).observe(text_changed);
    apply_text(world, entity);
}

fn text_changed(event: On<Changed>, mut commands: Commands) {
    let entity = event.entity;
    commands.queue(move |world: &mut World| {
        apply_text(world, entity);
    });
}

fn apply_text(world: &mut World, entity: Entity) {
    let Some(values) = world
        .get::<Configuration>(entity)
        .map(|configuration| configuration.0.clone())
    else {
        return;
    };
    let definitions = text_definitions();
    let wrap = values.resolve(&definitions[0]) == Value::Toggle(true);
    if let Some(mut area) = world.get_mut::<crate::sand_text::SandText>(entity) {
        area.settings = values.clone();
        area.overflow = if values.resolve(&definitions[1]) == Value::Choice("Grow".into()) {
            crate::sand_text::TextOverflow::Grow
        } else {
            crate::sand_text::TextOverflow::Scroll
        };
    }
    world
        .entity_mut(entity)
        .insert(TextLayout::linebreak(if wrap {
            bevy::text::LineBreak::WordBoundary
        } else {
            bevy::text::LineBreak::NoWrap
        }));
}

pub(crate) fn select(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<bevy::picking::pointer::PointerInput>>,
) {
    use bevy::picking::pointer::{PointerAction, PointerButton, PointerId, PointerInput};
    let Some(events) = world.get_resource::<Messages<PointerInput>>() else {
        return;
    };
    let pressed = cursor.read(events).any(|event| {
        event.pointer_id == PointerId::Mouse
            && matches!(event.action, PointerAction::Press(PointerButton::Primary))
    });
    if !pressed {
        return;
    }
    let mut entity = world
        .get_resource::<bevy::picking::hover::HoverMap>()
        .and_then(|hover| hover.get(&PointerId::Mouse))
        .and_then(|hits| {
            hits.iter()
                .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
                .map(|(entity, _)| *entity)
        });
    let mut target = None;
    while let Some(current) = entity {
        if world
            .get::<crate::inspection::InspectionExcluded>(current)
            .is_some()
        {
            return;
        }
        if target.is_none() && world.get::<Declaration>(current).is_some() {
            target = Some(current);
        }
        if let Some(sand) = target
            && crate::edit_mode::customizing(world, current)
        {
            if world.get::<crate::customization::Scope>(current)
                != Some(&crate::customization::Scope::Sand(sand))
            {
                world
                    .entity_mut(current)
                    .insert(crate::customization::Scope::Sand(sand));
                crate::edit_mode::render_panel(world, current);
            }
            return;
        }
        entity = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
}
