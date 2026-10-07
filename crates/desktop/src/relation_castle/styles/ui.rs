use super::*;
use crate::{actions::Action, edit_mode::label, sand_panel};
use bevy::text::EditableText;

#[derive(Component, Clone)]
struct Editor {
    draft: Settings,
    message: String,
}

#[derive(Clone, Copy)]
enum Kind {
    Quantity,
    Assertion,
    Link,
}

#[derive(Clone, Copy)]
enum Property {
    Background,
    Text,
    Border,
    Link,
}

impl Property {
    fn name(self) -> &'static str {
        match self {
            Self::Background => "Card background",
            Self::Text => "Card text",
            Self::Border => "Card border",
            Self::Link => "Link and label",
        }
    }
    fn color(self, colors: &Colors) -> &Option<Color> {
        match self {
            Self::Background => &colors.background,
            Self::Text => &colors.text,
            Self::Border => &colors.border,
            Self::Link => &colors.link,
        }
    }
    fn color_mut(self, colors: &mut Colors) -> &mut Option<Color> {
        match self {
            Self::Background => &mut colors.background,
            Self::Text => &mut colors.text,
            Self::Border => &mut colors.border,
            Self::Link => &mut colors.link,
        }
    }
}

#[derive(Clone, Copy)]
enum Field {
    Name,
    Predicate(usize),
    Unit(usize),
    Lower(usize),
    Upper(usize),
}

#[derive(Component)]
struct Input {
    owner: Entity,
    rule: usize,
    field: Field,
    observed: String,
}

#[derive(Component)]
struct Status(Entity);

#[derive(Clone)]
enum Command {
    Open,
    Apply,
    Discard,
    Add(Target),
    Remove(usize),
    Move(usize, bool),
    Enabled(usize),
    Mode(usize),
    AddCondition(usize),
    RemoveCondition(usize, usize),
    Kind(usize, usize, Kind),
    Side(usize, usize, RecordSide),
    Family(usize, usize),
    Direction(usize, usize, Direction),
    Present(usize, usize),
    Quantity(usize, usize),
    Operator(usize, usize, Operator),
    LowerInclusive(usize, usize),
    UpperInclusive(usize, usize),
    Unit(usize, usize, Unit),
    Color(usize, Property, Option<Color>),
}

fn allowed(world: &World, owner: Entity) -> Option<Entity> {
    let root = world.get::<ChildOf>(owner)?.parent();
    (crate::area_panel::owns(world, root, owner)
        && world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled && mode.areas))
    .then_some(root)
}

fn condition(kind: Kind, target: Target) -> Condition {
    let record = if target == Target::Card {
        RecordSide::Record
    } else {
        RecordSide::Source
    };
    match kind {
        Kind::Quantity => Condition::Quantity {
            record,
            comparison: default(),
            unit: Unit::Any,
        },
        Kind::Assertion => Condition::Assertion {
            record,
            predicate: "*".into(),
            family: false,
            direction: Direction::Either,
            present: true,
            quantity: None,
            unit: Unit::Any,
        },
        Kind::Link => Condition::Link {
            predicate: "*".into(),
            family: false,
            quantity: None,
            unit: Unit::Any,
        },
    }
}

fn comparison_mut(condition: &mut Condition) -> Option<&mut Comparison> {
    match condition {
        Condition::Quantity { comparison, .. } => Some(comparison),
        Condition::Assertion { quantity, .. } | Condition::Link { quantity, .. } => {
            quantity.as_mut()
        }
    }
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = allowed(world, owner) else {
            return;
        };
        let Some(mut config) =
            crate::protein_area::configuration(world, owner).filter(|config| config.relations)
        else {
            return;
        };
        if matches!(self, Self::Open) {
            world.entity_mut(owner).insert(Editor {
                draft: config.relation_styles,
                message: String::new(),
            });
        } else if matches!(self, Self::Discard) {
            world.entity_mut(owner).remove::<Editor>();
        } else if matches!(self, Self::Apply) {
            let Some(editor) = world.get::<Editor>(owner) else {
                return;
            };
            if !editor.draft.valid() {
                world.get_mut::<Editor>(owner).unwrap().message =
                    "Check quantities, range endpoints, units and color choices before applying."
                        .into();
            } else {
                config.relation_styles = editor.draft.clone();
                crate::protein_area::set_configuration(world, owner, Some(config));
                world.entity_mut(owner).remove::<Editor>();
            }
        } else if let Some(mut editor) = world.get_mut::<Editor>(owner) {
            editor.message.clear();
            match self {
                Self::Add(target) if editor.draft.rules.len() < 128 => {
                    editor.draft.rules.push(Rule {
                        name: if *target == Target::Card {
                            "Card color"
                        } else {
                            "Link color"
                        }
                        .into(),
                        enabled: true,
                        target: *target,
                        mode: Mode::All,
                        conditions: vec![condition(
                            if *target == Target::Card {
                                Kind::Quantity
                            } else {
                                Kind::Link
                            },
                            *target,
                        )],
                        colors: if *target == Target::Card {
                            Colors {
                                background: Some(Color::Token(Token::Warning)),
                                ..default()
                            }
                        } else {
                            Colors {
                                link: Some(Color::Token(Token::Accent)),
                                ..default()
                            }
                        },
                    });
                }
                Self::Remove(rule) if *rule < editor.draft.rules.len() => {
                    editor.draft.rules.remove(*rule);
                }
                Self::Move(rule, down) => {
                    let other = if *down {
                        rule.checked_add(1)
                    } else {
                        rule.checked_sub(1)
                    };
                    if let Some(other) = other.filter(|other| {
                        *other < editor.draft.rules.len() && *rule < editor.draft.rules.len()
                    }) {
                        editor.draft.rules.swap(*rule, other);
                    }
                }
                _ => edit(&mut editor.draft, self),
            }
        }
        crate::edit_mode::render_panel(world, root);
    }
}

fn edit(settings: &mut Settings, command: &Command) {
    let index = match command {
        Command::Enabled(rule)
        | Command::Mode(rule)
        | Command::AddCondition(rule)
        | Command::RemoveCondition(rule, _)
        | Command::Kind(rule, _, _)
        | Command::Side(rule, _, _)
        | Command::Family(rule, _)
        | Command::Direction(rule, _, _)
        | Command::Present(rule, _)
        | Command::Quantity(rule, _)
        | Command::Operator(rule, _, _)
        | Command::LowerInclusive(rule, _)
        | Command::UpperInclusive(rule, _)
        | Command::Unit(rule, _, _)
        | Command::Color(rule, _, _) => *rule,
        _ => return,
    };
    let Some(rule) = settings.rules.get_mut(index) else {
        return;
    };
    match command {
        Command::Enabled(_) => {
            rule.enabled = !rule.enabled;
            return;
        }
        Command::Mode(_) => {
            rule.mode = if rule.mode == Mode::All {
                Mode::Any
            } else {
                Mode::All
            };
            return;
        }
        Command::AddCondition(_) => {
            if rule.conditions.len() < 32 {
                rule.conditions.push(condition(Kind::Quantity, rule.target));
            }
            return;
        }
        Command::RemoveCondition(_, index) => {
            if *index < rule.conditions.len() {
                rule.conditions.remove(*index);
            }
            return;
        }
        Command::Color(_, property, color) => {
            property.color_mut(&mut rule.colors).clone_from(color);
            return;
        }
        _ => {}
    }
    let index = match command {
        Command::Kind(_, index, _)
        | Command::Side(_, index, _)
        | Command::Family(_, index)
        | Command::Direction(_, index, _)
        | Command::Present(_, index)
        | Command::Quantity(_, index)
        | Command::Operator(_, index, _)
        | Command::LowerInclusive(_, index)
        | Command::UpperInclusive(_, index)
        | Command::Unit(_, index, _) => *index,
        _ => return,
    };
    let Some(current) = rule.conditions.get_mut(index) else {
        return;
    };
    match command {
        Command::Kind(_, _, kind) => *current = condition(*kind, rule.target),
        Command::Side(_, _, side) => match current {
            Condition::Quantity { record, .. } | Condition::Assertion { record, .. } => {
                *record = *side
            }
            _ => {}
        },
        Command::Family(_, _) => match current {
            Condition::Assertion { family, .. } | Condition::Link { family, .. } => {
                *family = !*family
            }
            _ => {}
        },
        Command::Direction(_, _, value) => {
            if let Condition::Assertion { direction, .. } = current {
                *direction = *value;
            }
        }
        Command::Present(_, _) => {
            if let Condition::Assertion { present, .. } = current {
                *present = !*present;
            }
        }
        Command::Quantity(_, _) => match current {
            Condition::Assertion { quantity, .. } | Condition::Link { quantity, .. } => {
                *quantity = if quantity.is_some() {
                    None
                } else {
                    Some(default())
                }
            }
            _ => {}
        },
        Command::Operator(_, _, operator) => {
            if let Some(comparison) = comparison_mut(current) {
                comparison.operator = *operator;
            }
        }
        Command::LowerInclusive(_, _) => {
            if let Some(comparison) = comparison_mut(current) {
                comparison.include_lower = !comparison.include_lower;
            }
        }
        Command::UpperInclusive(_, _) => {
            if let Some(comparison) = comparison_mut(current) {
                comparison.include_upper = !comparison.include_upper;
            }
        }
        Command::Unit(_, _, value) => match current {
            Condition::Quantity { unit, .. }
            | Condition::Assertion { unit, .. }
            | Condition::Link { unit, .. } => unit.clone_from(value),
        },
        _ => {}
    }
}

fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    rule: usize,
    field: Field,
    title: &str,
    value: &str,
) {
    label(world, parent, title, 13.0);
    let bundle = crate::sand::text_editor(value, world.resource::<crate::theme::Typography>(), 0);
    let entity = world
        .spawn((
            bundle,
            ChildOf(parent),
            Input {
                owner,
                rule,
                field,
                observed: value.into(),
            },
        ))
        .id();
    crate::accessibility::input(world, entity, title, false);
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.max_characters = Some(128);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = percent(100);
    node.min_height = px(30);
    node.flex_shrink = 0.0;
}

pub(crate) fn inputs(world: &mut World) {
    let updates: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter_map(|(entity, field, text)| {
            let value = text.value().to_string();
            (value != field.observed).then_some((
                entity,
                field.owner,
                field.rule,
                field.field,
                value,
            ))
        })
        .collect();
    for (entity, owner, rule, field, value) in updates {
        if allowed(world, owner).is_none() {
            continue;
        }
        if let Some(mut editor) = world.get_mut::<Editor>(owner) {
            editor.message.clear();
            if let Some(rule) = editor.draft.rules.get_mut(rule) {
                match field {
                    Field::Name => rule.name.clone_from(&value),
                    Field::Predicate(index) => {
                        if let Some(
                            Condition::Assertion { predicate, .. }
                            | Condition::Link { predicate, .. },
                        ) = rule.conditions.get_mut(index)
                        {
                            predicate.clone_from(&value);
                        }
                    }
                    Field::Unit(index) => {
                        if let Some(current) = rule.conditions.get_mut(index) {
                            match current {
                                Condition::Quantity { unit, .. }
                                | Condition::Assertion { unit, .. }
                                | Condition::Link { unit, .. } => {
                                    *unit = Unit::Exact(value.clone())
                                }
                            }
                        }
                    }
                    Field::Lower(index) | Field::Upper(index) => {
                        if let Some(comparison) =
                            rule.conditions.get_mut(index).and_then(comparison_mut)
                        {
                            match field {
                                Field::Lower(_) => comparison.value.clone_from(&value),
                                _ => comparison.upper.clone_from(&value),
                            }
                        }
                    }
                }
            }
        }
        world.get_mut::<Input>(entity).unwrap().observed = value;
    }
    let statuses: Vec<_> = world
        .query::<(Entity, &Status)>()
        .iter(world)
        .map(|(entity, status)| {
            let message = world
                .get::<Editor>(status.0)
                .map_or(String::new(), |editor| {
                    if !editor.message.is_empty() {
                        editor.message.clone()
                    } else if !editor.draft.valid() {
                        "Some rules are incomplete or have invalid quantities or ranges.".into()
                    } else {
                        "Ready to apply. Matching colors update with Record data and theme tokens."
                            .into()
                    }
                });
            (entity, message)
        })
        .collect();
    for (entity, message) in statuses {
        world
            .get_mut::<Text>(entity)
            .unwrap()
            .set_if_neq(Text::new(message));
    }
}

fn dropdown(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    name: &str,
    selected: &str,
    choices: Vec<(&str, Command)>,
) {
    crate::dropdown::spawn(
        world,
        parent,
        owner,
        name,
        selected,
        choices
            .into_iter()
            .map(|(label, command)| (label.into(), crate::actions![command]))
            .collect(),
    );
}

pub(crate) fn controls(world: &mut World, parent: Entity, owner: Entity, settings: &Settings) {
    label(world, parent, "Relation color rules", 18.0);
    label(
        world,
        parent,
        "Rules run top to bottom. The first match for each color wins; unmatched colors use the underlying style.",
        13.0,
    );
    let Some(editor) = world.get::<Editor>(owner).cloned() else {
        for rule in &settings.rules {
            label(
                world,
                parent,
                &format!(
                    "{} · {} · {}",
                    rule.name,
                    if rule.target == Target::Card {
                        "Card"
                    } else {
                        "Link"
                    },
                    if rule.enabled { "on" } else { "off" }
                ),
                13.0,
            );
        }
        sand_panel::button(world, parent, owner, "Configure color rules", Command::Open);
        return;
    };
    let actions = sand_panel::row(world, parent);
    sand_panel::button(
        world,
        actions,
        owner,
        "Add card rule",
        Command::Add(Target::Card),
    );
    sand_panel::button(
        world,
        actions,
        owner,
        "Add link rule",
        Command::Add(Target::Link),
    );
    for (index, rule) in editor.draft.rules.iter().enumerate() {
        let group = sand_panel::column(world, parent);
        label(
            world,
            group,
            &format!(
                "{} · {}",
                index + 1,
                if rule.target == Target::Card {
                    "Card"
                } else {
                    "Link"
                }
            ),
            16.0,
        );
        input(
            world,
            group,
            owner,
            index,
            Field::Name,
            "Rule name",
            &rule.name,
        );
        let actions = sand_panel::row(world, group);
        sand_panel::button(
            world,
            actions,
            owner,
            if rule.enabled { "Enabled" } else { "Disabled" },
            Command::Enabled(index),
        );
        sand_panel::button(
            world,
            actions,
            owner,
            "Move up",
            Command::Move(index, false),
        );
        sand_panel::button(
            world,
            actions,
            owner,
            "Move down",
            Command::Move(index, true),
        );
        sand_panel::button(world, actions, owner, "Delete rule", Command::Remove(index));
        sand_panel::button(
            world,
            group,
            owner,
            if rule.mode == Mode::All {
                "Match all conditions"
            } else {
                "Match any condition"
            },
            Command::Mode(index),
        );
        if rule.conditions.is_empty() {
            label(world, group, "This rule always matches.", 13.0);
        }
        for (condition_index, condition) in rule.conditions.iter().enumerate() {
            condition_controls(
                world,
                group,
                owner,
                index,
                condition_index,
                rule.target,
                condition,
            );
        }
        sand_panel::button(
            world,
            group,
            owner,
            "Add condition",
            Command::AddCondition(index),
        );
        let properties: &[Property] = if rule.target == Target::Card {
            &[Property::Background, Property::Text, Property::Border]
        } else {
            &[Property::Link]
        };
        for property in properties {
            color_controls(
                world,
                group,
                owner,
                index,
                *property,
                property.color(&rule.colors),
            );
        }
    }
    let message = label(world, parent, &editor.message, 13.0);
    world.entity_mut(message).insert(Status(owner));
    let actions = sand_panel::row(world, parent);
    sand_panel::button(world, actions, owner, "Apply color rules", Command::Apply);
    sand_panel::button(world, actions, owner, "Discard changes", Command::Discard);
}

fn condition_controls(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    rule: usize,
    index: usize,
    target: Target,
    condition: &Condition,
) {
    let group = sand_panel::column(world, parent);
    let (kind, side, predicate, family, comparison, unit) = match condition {
        Condition::Quantity {
            record,
            comparison,
            unit,
        } => (
            "Record quantity",
            Some(*record),
            None,
            false,
            Some(comparison),
            unit,
        ),
        Condition::Assertion {
            record,
            predicate,
            family,
            quantity,
            unit,
            ..
        } => (
            "Record assertion or link",
            Some(*record),
            Some(predicate),
            *family,
            quantity.as_ref(),
            unit,
        ),
        Condition::Link {
            predicate,
            family,
            quantity,
            unit,
        } => (
            "This link's assertion",
            None,
            Some(predicate),
            *family,
            quantity.as_ref(),
            unit,
        ),
    };
    let mut kinds = vec![
        (
            "Record quantity",
            Command::Kind(rule, index, Kind::Quantity),
        ),
        (
            "Record assertion or link",
            Command::Kind(rule, index, Kind::Assertion),
        ),
    ];
    if target == Target::Link {
        kinds.push((
            "This link's assertion",
            Command::Kind(rule, index, Kind::Link),
        ));
    }
    dropdown(world, group, owner, "Condition", kind, kinds);
    if target == Target::Link
        && let Some(side) = side
    {
        dropdown(
            world,
            group,
            owner,
            "Endpoint",
            if side == RecordSide::Source {
                "Source Record"
            } else {
                "Destination Record"
            },
            vec![
                (
                    "Source Record",
                    Command::Side(rule, index, RecordSide::Source),
                ),
                (
                    "Destination Record",
                    Command::Side(rule, index, RecordSide::Destination),
                ),
            ],
        );
    }
    if let Some(predicate) = predicate {
        input(
            world,
            group,
            owner,
            rule,
            Field::Predicate(index),
            "Concept name or UID (* matches any)",
            predicate,
        );
        sand_panel::button(
            world,
            group,
            owner,
            if family {
                "Concept family and counts-as"
            } else {
                "Exact concept"
            },
            Command::Family(rule, index),
        );
    }
    if let Condition::Assertion {
        direction, present, ..
    } = condition
    {
        let name = match direction {
            Direction::Outgoing => "Outgoing",
            Direction::Incoming => "Incoming",
            Direction::Either => "Either direction",
        };
        dropdown(
            world,
            group,
            owner,
            "Assertion direction",
            name,
            vec![
                (
                    "Outgoing",
                    Command::Direction(rule, index, Direction::Outgoing),
                ),
                (
                    "Incoming",
                    Command::Direction(rule, index, Direction::Incoming),
                ),
                (
                    "Either direction",
                    Command::Direction(rule, index, Direction::Either),
                ),
            ],
        );
        sand_panel::button(
            world,
            group,
            owner,
            if *present {
                "Has a matching assertion"
            } else {
                "Has no matching assertion"
            },
            Command::Present(rule, index),
        );
    }
    if !matches!(condition, Condition::Quantity { .. }) {
        sand_panel::button(
            world,
            group,
            owner,
            if comparison.is_some() {
                "Quantity comparison: on"
            } else {
                "Quantity comparison: off"
            },
            Command::Quantity(rule, index),
        );
    }
    if let Some(comparison) = comparison {
        let operators = [
            ("Equals", Operator::Equal),
            ("Does not equal", Operator::NotEqual),
            ("Higher than", Operator::Greater),
            ("At least", Operator::AtLeast),
            ("Lower than", Operator::Less),
            ("At most", Operator::AtMost),
            ("Between", Operator::Between),
        ];
        let selected = operators
            .iter()
            .find(|(_, operator)| *operator == comparison.operator)
            .unwrap()
            .0;
        dropdown(
            world,
            group,
            owner,
            "Quantity comparison",
            selected,
            operators
                .into_iter()
                .map(|(name, operator)| (name, Command::Operator(rule, index, operator)))
                .collect(),
        );
        input(
            world,
            group,
            owner,
            rule,
            Field::Lower(index),
            if comparison.operator == Operator::Between {
                "From"
            } else {
                "Quantity"
            },
            &comparison.value,
        );
        if comparison.operator == Operator::Between {
            input(
                world,
                group,
                owner,
                rule,
                Field::Upper(index),
                "To",
                &comparison.upper,
            );
            sand_panel::button(
                world,
                group,
                owner,
                if comparison.include_lower {
                    "Include lower endpoint"
                } else {
                    "Exclude lower endpoint"
                },
                Command::LowerInclusive(rule, index),
            );
            sand_panel::button(
                world,
                group,
                owner,
                if comparison.include_upper {
                    "Include upper endpoint"
                } else {
                    "Exclude upper endpoint"
                },
                Command::UpperInclusive(rule, index),
            );
        }
    }
    let selected = match unit {
        Unit::Any => "Any unit",
        Unit::Unitless => "Unitless only",
        Unit::Exact(_) => "Specific unit",
    };
    dropdown(
        world,
        group,
        owner,
        "Unit filter",
        selected,
        vec![
            ("Any unit", Command::Unit(rule, index, Unit::Any)),
            ("Unitless only", Command::Unit(rule, index, Unit::Unitless)),
            (
                "Specific unit",
                Command::Unit(rule, index, Unit::Exact(String::new())),
            ),
        ],
    );
    if let Unit::Exact(unit) = unit {
        input(
            world,
            group,
            owner,
            rule,
            Field::Unit(index),
            "Unit name or UID (no conversion)",
            unit,
        );
    }
    sand_panel::button(
        world,
        group,
        owner,
        "Remove condition",
        Command::RemoveCondition(rule, index),
    );
}

fn color_controls(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    rule: usize,
    property: Property,
    color: &Option<Color>,
) {
    let mut choices = vec![
        (
            "Unchanged".to_string(),
            crate::actions![Command::Color(rule, property, None)],
        ),
        (
            "Specific color".into(),
            crate::actions![Command::Color(
                rule,
                property,
                Some(Color::Specific([99, 102, 241, 255]))
            )],
        ),
    ];
    for definition in crate::tokens::TOKENS
        .iter()
        .filter(|definition| matches!(definition.dark, TokenValue::Color(_)))
    {
        choices.push((
            definition.name.into(),
            crate::actions![Command::Color(
                rule,
                property,
                Some(Color::Token(definition.token))
            )],
        ));
    }
    let selected = match color {
        None => "Unchanged",
        Some(Color::Specific(_)) => "Specific color",
        Some(Color::Token(token)) => token.definition().name,
    };
    crate::dropdown::spawn(world, parent, owner, property.name(), selected, choices);
    if let Some(Color::Specific(rgba)) = color {
        let picker = crate::color_picker::spawn(world, parent, property.name(), *rgba, true);
        world.entity_mut(picker).observe(
            move |event: On<crate::color_picker::ColorChanged>, mut commands: Commands| {
                let rgba = event.rgba;
                commands.queue(move |world: &mut World| {
                    if allowed(world, owner).is_some()
                        && let Some(mut editor) = world.get_mut::<Editor>(owner)
                        && let Some(rule) = editor.draft.rules.get_mut(rule)
                    {
                        *property.color_mut(&mut rule.colors) = Some(Color::Specific(rgba));
                    }
                });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (App, Entity, Entity) {
        let (mut app, root) = crate::edit_mode::tests::fixture();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        crate::edit_mode::EditAction::Areas.apply(app.world_mut(), root);
        let mut area = crate::area::InfluenceArea::new(
            crate::area::AreaShape::Circle,
            bevy::math::DVec2::ZERO,
            bevy::math::DVec2::splat(1200.0),
        );
        area.protein = Some(crate::relation_castle::config());
        let workspace = app
            .world()
            .get::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let owner = crate::area::spawn_area(app.world_mut(), root, workspace, area).unwrap();
        app.world_mut()
            .get_mut::<crate::area_panel::AreaEditor>(root)
            .unwrap()
            .selected = Some(owner);
        Command::Open.apply(app.world_mut(), owner);
        (app, root, owner)
    }

    fn lower(world: &mut World, owner: Entity, value: &str) {
        let entity = world
            .query::<(Entity, &Input)>()
            .iter(world)
            .find(|(_, input)| input.owner == owner && matches!(input.field, Field::Lower(0)))
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
        inputs(world);
    }

    #[test]
    fn editor_validates_applies_persists_and_discards_drafts() {
        let (mut app, _, owner) = fixture();
        Command::Add(Target::Card).apply(app.world_mut(), owner);
        lower(app.world_mut(), owner, "invalid");
        Command::Apply.apply(app.world_mut(), owner);
        assert!(app.world().get::<Editor>(owner).is_some());
        assert!(
            crate::protein_area::configuration(app.world(), owner)
                .unwrap()
                .relation_styles
                .rules
                .is_empty()
        );
        lower(app.world_mut(), owner, "5");
        Command::Apply.apply(app.world_mut(), owner);
        assert!(app.world().get::<Editor>(owner).is_none());
        let saved = crate::protein_area::configuration(app.world(), owner).unwrap();
        assert_eq!(saved.relation_styles.rules.len(), 1);
        assert!(saved.valid());
        let loaded: crate::protein_area::Config =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(loaded.relation_styles, saved.relation_styles);
        Command::Open.apply(app.world_mut(), owner);
        Command::Add(Target::Link).apply(app.world_mut(), owner);
        Command::Move(1, false).apply(app.world_mut(), owner);
        assert_eq!(
            app.world().get::<Editor>(owner).unwrap().draft.rules[0].target,
            Target::Link
        );
        Command::Discard.apply(app.world_mut(), owner);
        assert_eq!(
            crate::protein_area::configuration(app.world(), owner)
                .unwrap()
                .relation_styles,
            saved.relation_styles
        );
    }

    #[test]
    fn custom_color_picker_changes_the_draft_and_readonly_actions_are_ignored() {
        let (mut app, root, owner) = fixture();
        Command::Add(Target::Card).apply(app.world_mut(), owner);
        Command::Color(
            0,
            Property::Background,
            Some(Color::Specific([99, 102, 241, 255])),
        )
        .apply(app.world_mut(), owner);
        let picker = app
            .world_mut()
            .query::<(Entity, &crate::color_picker::ColorPicker)>()
            .iter(app.world())
            .find(|(_, picker)| {
                app.world()
                    .get::<EditableText>(picker.editor)
                    .unwrap()
                    .value()
                    == TokenValue::Color([99, 102, 241, 255]).display().as_str()
            })
            .unwrap()
            .0;
        app.world_mut().trigger(crate::color_picker::ColorChanged {
            entity: picker,
            rgba: [11, 22, 33, 44],
        });
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<Editor>(owner).unwrap().draft.rules[0]
                .colors
                .background,
            Some(Color::Specific([11, 22, 33, 44]))
        );
        let before = app.world().get::<Editor>(owner).unwrap().draft.clone();
        app.world_mut()
            .get_mut::<crate::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = false;
        Command::Add(Target::Link).apply(app.world_mut(), owner);
        Command::Apply.apply(app.world_mut(), owner);
        lower(app.world_mut(), owner, "100");
        app.world_mut().trigger(crate::color_picker::ColorChanged {
            entity: picker,
            rgba: [1, 2, 3, 4],
        });
        app.world_mut().flush();
        assert_eq!(app.world().get::<Editor>(owner).unwrap().draft, before);
        assert!(
            crate::protein_area::configuration(app.world(), owner)
                .unwrap()
                .relation_styles
                .rules
                .is_empty()
        );
    }
}
