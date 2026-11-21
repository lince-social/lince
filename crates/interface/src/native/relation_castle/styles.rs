// non-lean-yet
// Scope: mod preparation, mod tests, mod ui, struct Settings.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (366, 381).

mod preparation;
#[cfg(test)]
mod tests;
mod ui;

use crate::native::tokens::{Token, TokenValue};
use bevy::prelude::*;
use nucleus::DecimalValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

pub(crate) use ui::{controls, inputs};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub enabled: bool,
    pub target: Target,
    pub mode: Mode,
    pub conditions: Vec<Condition>,
    pub colors: Colors,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    #[default]
    Card,
    Link,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    All,
    Any,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordSide {
    #[default]
    Record,
    Source,
    Destination,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Outgoing,
    Incoming,
    #[default]
    Either,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    #[default]
    Any,
    Unitless,
    Exact(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    Quantity {
        record: RecordSide,
        comparison: Comparison,
        unit: Unit,
    },
    Assertion {
        record: RecordSide,
        predicate: String,
        family: bool,
        direction: Direction,
        present: bool,
        quantity: Option<Comparison>,
        unit: Unit,
    },
    Link {
        predicate: String,
        family: bool,
        quantity: Option<Comparison>,
        unit: Unit,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operator {
    Equal,
    NotEqual,
    Greater,
    #[default]
    AtLeast,
    Less,
    AtMost,
    Between,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub operator: Operator,
    pub value: String,
    pub upper: String,
    pub include_lower: bool,
    pub include_upper: bool,
}

impl Default for Comparison {
    fn default() -> Self {
        Self {
            operator: Operator::AtLeast,
            value: "0".into(),
            upper: "10".into(),
            include_lower: true,
            include_upper: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Color {
    Token(Token),
    Specific([u8; 4]),
}

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Colors {
    pub background: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
    pub link: Option<Color>,
}

fn decimal(value: &str) -> Option<DecimalValue> {
    (value.len() <= 64)
        .then(|| DecimalValue::parse_inferred(value.trim()).ok())
        .flatten()
}

impl Comparison {
    pub fn valid(&self) -> bool {
        preparation::ComparisonInput::new(self).valid()
    }

    #[cfg(test)]
    fn matches(&self, value: &Value) -> bool {
        preparation::ComparisonInput::new(self).matches(value)
    }
}

fn identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.contains('\0')
}

impl Unit {
    fn valid(&self) -> bool {
        match self {
            Self::Exact(unit) => identifier(unit),
            _ => true,
        }
    }
}

impl Condition {
    fn valid(&self, target: Target) -> bool {
        let side_valid = |side: RecordSide| match target {
            Target::Card => side == RecordSide::Record,
            Target::Link => side != RecordSide::Record,
        };
        match self {
            Self::Quantity {
                record,
                comparison,
                unit,
            } => side_valid(*record) && comparison.valid() && unit.valid(),
            Self::Assertion {
                record,
                predicate,
                quantity,
                unit,
                ..
            } => {
                side_valid(*record)
                    && identifier(predicate)
                    && quantity.as_ref().is_none_or(Comparison::valid)
                    && unit.valid()
            }
            Self::Link {
                predicate,
                quantity,
                unit,
                ..
            } => {
                target == Target::Link
                    && identifier(predicate)
                    && quantity.as_ref().is_none_or(Comparison::valid)
                    && unit.valid()
            }
        }
    }

    fn family(&self) -> Option<&str> {
        match self {
            Self::Assertion {
                predicate,
                family: true,
                ..
            }
            | Self::Link {
                predicate,
                family: true,
                ..
            } => Some(predicate),
            _ => None,
        }
    }
}

impl Colors {
    fn valid(&self, target: Target) -> bool {
        let values = [&self.background, &self.text, &self.border, &self.link];
        values.iter().any(|value| value.is_some())
            && values.into_iter().flatten().all(|color| match color {
                Color::Token(token) => matches!(token.definition().dark, TokenValue::Color(_)),
                Color::Specific(_) => true,
            })
            && match target {
                Target::Card => self.link.is_none(),
                Target::Link => {
                    self.background.is_none() && self.text.is_none() && self.border.is_none()
                }
            }
    }

    fn fill(&mut self, colors: &Self) {
        for (destination, source) in [
            (&mut self.background, &colors.background),
            (&mut self.text, &colors.text),
            (&mut self.border, &colors.border),
            (&mut self.link, &colors.link),
        ] {
            if destination.is_none() {
                destination.clone_from(source);
            }
        }
    }
}

impl Settings {
    pub fn valid(&self) -> bool {
        self.rules.len() <= 128
            && self.family_names().len() <= 128
            && self.rules.iter().all(|rule| {
                rule.name.len() <= 128
                    && !rule.name.contains('\0')
                    && rule.conditions.len() <= 32
                    && rule
                        .conditions
                        .iter()
                        .all(|condition| condition.valid(rule.target))
                    && rule.colors.valid(rule.target)
            })
    }

    fn family_names(&self) -> BTreeSet<&str> {
        self.rules
            .iter()
            .flat_map(|rule| &rule.conditions)
            .filter_map(Condition::family)
            .map(str::trim)
            .filter(|predicate| *predicate != "*")
            .collect()
    }

    pub(crate) fn families(&self) -> Vec<String> {
        self.family_names().into_iter().map(str::to_owned).collect()
    }

    #[cfg(test)]
    fn evaluate(
        &self,
        target: Target,
        record: &Value,
        source: &Value,
        destination: &Value,
        link: &Value,
    ) -> Colors {
        preparation::Rules::new(self, target).evaluate(record, source, destination, link)
    }
}

fn set(world: &mut World, entity: Entity, colors: Colors) {
    if colors == Colors::default() {
        world.entity_mut(entity).remove::<Colors>();
    } else if let Some(mut current) = world.get_mut::<Colors>(entity) {
        current.set_if_neq(colors);
    } else {
        world.entity_mut(entity).insert(colors);
    }
}

pub(super) fn apply(
    world: &mut World,
    owner: Entity,
    config: &crate::native::protein_area::Config,
    rows: &[Value],
    records: &HashMap<String, Entity>,
) {
    let _application = nucleus::performance::Span::new("relation-style-application");
    let empty = Settings::default();
    let settings = if config.relations && config.enabled {
        &config.relation_styles
    } else {
        &empty
    };
    let cards = preparation::Rules::new(settings, Target::Card);
    let links = preparation::Rules::new(settings, Target::Link);
    let data: HashMap<_, _> = if cards.needs_rows || links.needs_rows {
        rows.iter()
            .filter_map(|row| row["uid"].as_str().map(|uid| (uid, row)))
            .collect()
    } else {
        HashMap::new()
    };
    let assertions: HashMap<_, _> = if links.needs_assertions {
        rows.iter()
            .flat_map(|row| {
                row["relation_context"]["assertions"]
                    .as_array()
                    .into_iter()
                    .flatten()
            })
            .filter_map(|row| row["uid"].as_str().map(|uid| (uid, row)))
            .collect()
    } else {
        HashMap::new()
    };
    nucleus::performance::add("relation-style-records-indexed", data.len() as u64);
    nucleus::performance::add("relation-style-assertions-indexed", assertions.len() as u64);
    for (uid, entity) in records {
        let row = data.get(uid.as_str()).copied().unwrap_or(&Value::Null);
        set(
            world,
            *entity,
            cards.evaluate(row, &Value::Null, &Value::Null, &Value::Null),
        );
    }
    let targets: Vec<_> = world
        .query::<(
            Entity,
            &super::RelationLink,
            &crate::native::arrow_sand::ArrowSand,
        )>()
        .iter(world)
        .filter(|(_, link, _)| link.owner == owner)
        .map(|(entity, _, _)| entity)
        .collect();
    for entity in targets {
        let arrow = world
            .get::<crate::native::arrow_sand::ArrowSand>(entity)
            .unwrap();
        let source = world
            .get::<crate::native::protein_area::RecordBinding>(arrow.from)
            .and_then(|binding| data.get(binding.uid.as_str()))
            .copied()
            .unwrap_or(&Value::Null);
        let destination = world
            .get::<crate::native::protein_area::RecordBinding>(arrow.to)
            .and_then(|binding| data.get(binding.uid.as_str()))
            .copied()
            .unwrap_or(&Value::Null);
        let link = assertions
            .get(
                world
                    .get::<super::RelationLink>(entity)
                    .unwrap()
                    .uid
                    .as_str(),
            )
            .copied()
            .unwrap_or(&Value::Null);
        set(
            world,
            entity,
            links.evaluate(&Value::Null, source, destination, link),
        );
    }
}

pub(crate) fn override_value(world: &World, entity: Entity, token: Token) -> Option<TokenValue> {
    if !matches!(
        token,
        Token::SandBackground
            | Token::Surface
            | Token::SandInk
            | Token::Ink
            | Token::SandBorder
            | Token::Accent
            | Token::BorderWidth
    ) {
        return None;
    }
    let mut ancestor = Some(entity);
    while let Some(current) = ancestor {
        if let Some(colors) = world.get::<Colors>(current) {
            let color = match token {
                Token::SandBackground | Token::Surface => colors.background.as_ref(),
                Token::SandInk | Token::Ink => colors.text.as_ref().or(colors.link.as_ref()),
                Token::SandBorder => colors.border.as_ref(),
                Token::Accent => colors.link.as_ref(),
                Token::BorderWidth if colors.border.is_some() && current == entity => {
                    return Some(TokenValue::Number(
                        crate::native::token_style::resolve_base(world, current, token)
                            .0
                            .number()
                            .max(1.0),
                    ));
                }
                _ => None,
            };
            if let Some(color) = color {
                return Some(match color {
                    Color::Specific(rgba) => TokenValue::Color(*rgba),
                    Color::Token(reference) => {
                        crate::native::token_style::resolve_base(world, current, *reference).0
                    }
                });
            }
        }
        ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    None
}
