#[cfg(test)]
mod tests;
mod ui;

use crate::tokens::{Token, TokenValue};
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
        let Some(lower) = decimal(&self.value) else {
            return false;
        };
        if self.operator != Operator::Between {
            return true;
        }
        decimal(&self.upper).is_some_and(|upper| {
            let order = lower.exact_numeric_cmp(upper);
            order.is_lt() || (order.is_eq() && self.include_lower && self.include_upper)
        })
    }

    fn matches(&self, value: &Value) -> bool {
        let Some(value) = value.as_str().and_then(decimal) else {
            return false;
        };
        let Some(lower) = decimal(&self.value) else {
            return false;
        };
        let order = value.exact_numeric_cmp(lower);
        match self.operator {
            Operator::Equal => order.is_eq(),
            Operator::NotEqual => !order.is_eq(),
            Operator::Greater => order.is_gt(),
            Operator::AtLeast => !order.is_lt(),
            Operator::Less => order.is_lt(),
            Operator::AtMost => !order.is_gt(),
            Operator::Between => decimal(&self.upper).is_some_and(|upper| {
                let end = value.exact_numeric_cmp(upper);
                (order.is_gt() || (self.include_lower && order.is_eq()))
                    && (end.is_lt() || (self.include_upper && end.is_eq()))
            }),
        }
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
    fn matches(&self, entry: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Unitless => entry["unit"].is_null(),
            Self::Exact(unit) => {
                entry["unit"].as_str() == Some(unit.trim())
                    || entry["unit_name"].as_str() == Some(unit.trim())
                    || entry["relation_context"]["unit_name"].as_str() == Some(unit.trim())
            }
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
            && self.families().len() <= 128
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

    pub(crate) fn families(&self) -> Vec<String> {
        self.rules
            .iter()
            .flat_map(|rule| &rule.conditions)
            .filter_map(Condition::family)
            .map(str::trim)
            .filter(|predicate| *predicate != "*")
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn evaluate(
        &self,
        target: Target,
        record: &Value,
        source: &Value,
        destination: &Value,
        link: &Value,
    ) -> Colors {
        let mut colors = Colors::default();
        let row = |side| match side {
            RecordSide::Record => record,
            RecordSide::Source => source,
            RecordSide::Destination => destination,
        };
        for rule in self
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.target == target)
        {
            let matches = |condition: &Condition| match condition {
                Condition::Quantity {
                    record,
                    comparison,
                    unit,
                } => comparison.matches(&row(*record)["quantity"]) && unit.matches(row(*record)),
                Condition::Assertion {
                    record,
                    predicate,
                    family,
                    direction,
                    present,
                    quantity,
                    unit,
                } => {
                    let record = row(*record);
                    let uid = record["uid"].as_str();
                    let found = record["relation_context"]["assertions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|assertion| {
                            let incident = match direction {
                                Direction::Outgoing => {
                                    uid.is_some() && assertion["from"].as_str() == uid
                                }
                                Direction::Incoming => {
                                    assertion["to"].as_str() == uid && uid.is_some()
                                }
                                Direction::Either => {
                                    uid.is_some()
                                        && (assertion["from"].as_str() == uid
                                            || assertion["to"].as_str() == uid)
                                }
                            };
                            incident
                                && assertion_matches(
                                    assertion,
                                    predicate,
                                    *family,
                                    quantity.as_ref(),
                                    unit,
                                )
                        });
                    record["relation_context"]["assertions"].is_array() && found == *present
                }
                Condition::Link {
                    predicate,
                    family,
                    quantity,
                    unit,
                } => assertion_matches(link, predicate, *family, quantity.as_ref(), unit),
            };
            let matched = if rule.conditions.is_empty() {
                true
            } else {
                match rule.mode {
                    Mode::All => rule.conditions.iter().all(matches),
                    Mode::Any => rule.conditions.iter().any(matches),
                }
            };
            if matched {
                colors.fill(&rule.colors);
            }
        }
        colors
    }
}

fn assertion_matches(
    assertion: &Value,
    predicate: &str,
    family: bool,
    quantity: Option<&Comparison>,
    unit: &Unit,
) -> bool {
    let predicate = predicate.trim();
    !assertion.is_null()
        && (predicate == "*"
            || if family {
                assertion["families"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|value| value.as_str() == Some(predicate))
            } else {
                assertion["predicate_uid"].as_str() == Some(predicate)
                    || assertion["predicate"].as_str() == Some(predicate)
            })
        && quantity.is_none_or(|comparison| comparison.matches(&assertion["quantity"]))
        && unit.matches(assertion)
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
    config: &crate::protein_area::Config,
    rows: &[Value],
    records: &HashMap<String, Entity>,
) {
    let empty = Settings::default();
    let settings = if config.relations && config.enabled {
        &config.relation_styles
    } else {
        &empty
    };
    let data: HashMap<_, _> = rows
        .iter()
        .filter_map(|row| row["uid"].as_str().map(|uid| (uid, row)))
        .collect();
    let assertions: HashMap<_, _> = rows
        .iter()
        .flat_map(|row| {
            row["relation_context"]["assertions"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|row| row["uid"].as_str().map(|uid| (uid, row)))
        .collect();
    for (uid, entity) in records {
        let row = data.get(uid.as_str()).copied().unwrap_or(&Value::Null);
        set(
            world,
            *entity,
            settings.evaluate(Target::Card, row, &Value::Null, &Value::Null, &Value::Null),
        );
    }
    let links: Vec<_> = world
        .query::<(Entity, &super::RelationLink, &crate::arrow_sand::ArrowSand)>()
        .iter(world)
        .filter(|(_, link, _)| link.owner == owner)
        .map(|(entity, link, arrow)| (entity, link.uid.clone(), arrow.from, arrow.to))
        .collect();
    for (entity, uid, from, to) in links {
        let source = world
            .get::<crate::protein_area::RecordBinding>(from)
            .and_then(|binding| data.get(binding.uid.as_str()))
            .copied()
            .unwrap_or(&Value::Null);
        let destination = world
            .get::<crate::protein_area::RecordBinding>(to)
            .and_then(|binding| data.get(binding.uid.as_str()))
            .copied()
            .unwrap_or(&Value::Null);
        let link = assertions
            .get(uid.as_str())
            .copied()
            .unwrap_or(&Value::Null);
        set(
            world,
            entity,
            settings.evaluate(Target::Link, &Value::Null, source, destination, link),
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
                        crate::token_style::resolve_base(world, current, token)
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
                        crate::token_style::resolve_base(world, current, *reference).0
                    }
                });
            }
        }
        ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    None
}
