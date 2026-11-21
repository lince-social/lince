use std::{collections::BTreeMap, str::FromStr};

use super::{KarmaDslError, Parser};
use crate::{
    Confidence, DecimalPrecision, DecimalValue, DeclaredUnit, DurationMs, ExpressionAst,
    LiteralValue, LocalId, Probability, ResolvedReference, Slug, TimestampMs, TypedUid, ValueType,
};

impl Parser<'_> {
    pub(super) fn parse_expression(
        &mut self,
        depth: usize,
    ) -> Result<ExpressionAst, KarmaDslError> {
        self.check_depth(depth)?;
        let kind = self.take_word("expression constructor")?;
        self.expect_symbol('(')?;
        let expression = match kind {
            "literal" => ExpressionAst::Literal {
                value: self.parse_literal(depth + 1)?,
            },
            "input" => ExpressionAst::Input {
                input: self.parse_local_id("expression input")?,
            },
            "unary" => {
                let operator = self.parse_enum_atom("unary operator")?;
                self.comma()?;
                let value = Box::new(self.parse_expression(depth + 1)?);
                ExpressionAst::Unary { operator, value }
            }
            "binary" => {
                let operator = self.parse_enum_atom("binary operator")?;
                self.comma()?;
                let left = Box::new(self.parse_expression(depth + 1)?);
                self.comma()?;
                let right = Box::new(self.parse_expression(depth + 1)?);
                let precision = if self.consume_symbol(',') {
                    Some(self.parse_precision()?)
                } else {
                    None
                };
                ExpressionAst::Binary {
                    operator,
                    left,
                    right,
                    precision,
                }
            }
            "if" => {
                let condition = Box::new(self.parse_expression(depth + 1)?);
                self.comma()?;
                let then_value = Box::new(self.parse_expression(depth + 1)?);
                self.comma()?;
                let else_value = Box::new(self.parse_expression(depth + 1)?);
                ExpressionAst::If {
                    condition,
                    then_value,
                    else_value,
                }
            }
            _ => return Err(self.unexpected(format!("unknown expression constructor {kind:?}"))),
        };
        self.expect_symbol(')')?;
        Ok(expression)
    }

    pub(super) fn parse_type(&mut self, depth: usize) -> Result<ValueType, KarmaDslError> {
        self.check_depth(depth)?;
        let kind = self.take_word("type")?;
        let value_type = match kind {
            "bool" => Ok(ValueType::Bool),
            "i64" => Ok(ValueType::I64),
            "prob" => Ok(ValueType::Probability),
            "conf" => Ok(ValueType::Confidence),
            "text" => Ok(ValueType::Text),
            "duration" => Ok(ValueType::Duration),
            "timestamp" => Ok(ValueType::Timestamp),
            "decimal" => {
                self.expect_symbol('(')?;
                let scale = self.parse_atom("decimal scale")?;
                self.expect_symbol(')')?;
                Ok(ValueType::Decimal { scale })
            }
            "quantity" => {
                self.expect_symbol('(')?;
                let scale = self.parse_atom("quantity scale")?;
                self.comma()?;
                let unit = self.parse_uid()?;
                self.expect_symbol(')')?;
                Ok(ValueType::Quantity { scale, unit })
            }
            "reference" => {
                self.expect_symbol('(')?;
                let target = self.parse_enum_atom("reference target")?;
                self.expect_symbol(')')?;
                Ok(ValueType::Reference { target })
            }
            "list" => Ok(ValueType::List {
                item: self.parse_item_type(depth)?,
            }),
            "set" => Ok(ValueType::Set {
                item: self.parse_item_type(depth)?,
            }),
            "datum" => Ok(ValueType::Datum {
                value: self.parse_item_type(depth)?,
            }),
            "estimate" => Ok(ValueType::Estimate {
                value: self.parse_item_type(depth)?,
            }),
            "map" => {
                self.expect_symbol('(')?;
                let key = Box::new(self.parse_type(depth + 1)?);
                self.comma()?;
                let value = Box::new(self.parse_type(depth + 1)?);
                self.expect_symbol(')')?;
                Ok(ValueType::Map { key, value })
            }
            "candidate" => {
                self.expect_symbol('(')?;
                let route = self.parse_enum_atom("candidate route")?;
                self.comma()?;
                let template = Slug::new(self.take_word("candidate template")?)
                    .map_err(|error| self.atom(error))?;
                self.comma()?;
                let fields = self.parse_candidate_type_fields(depth + 1)?;
                self.expect_symbol(')')?;
                Ok(ValueType::Candidate {
                    route,
                    template,
                    fields,
                })
            }
            _ => Err(self.unexpected(format!("unknown type {kind:?}"))),
        }?;
        value_type.validate().map_err(|error| self.atom(error))?;
        Ok(value_type)
    }

    fn parse_item_type(&mut self, depth: usize) -> Result<Box<ValueType>, KarmaDslError> {
        self.expect_symbol('(')?;
        let item = self.parse_type(depth + 1)?;
        self.expect_symbol(')')?;
        Ok(Box::new(item))
    }

    pub(super) fn parse_literal(&mut self, depth: usize) -> Result<LiteralValue, KarmaDslError> {
        self.check_depth(depth)?;
        let kind = self.take_word("literal constructor")?;
        self.expect_symbol('(')?;
        let literal = match kind {
            "bool" => LiteralValue::Bool {
                value: self.parse_bool()?,
            },
            "i64" => LiteralValue::I64 {
                value: self.parse_atom("i64 literal")?,
            },
            "decimal" => {
                let scale = self.parse_atom("decimal scale")?;
                self.comma()?;
                let value =
                    DecimalValue::parse_canonical(scale, &self.take_string("decimal value")?)
                        .map_err(|error| self.atom(error))?;
                LiteralValue::Decimal { value }
            }
            "prob" => LiteralValue::Probability {
                value: Probability::from_str(&self.take_string("probability")?)
                    .map_err(|error| self.atom(error))?,
            },
            "conf" => LiteralValue::Confidence {
                value: Confidence::from_str(&self.take_string("confidence")?)
                    .map_err(|error| self.atom(error))?,
            },
            "text" => LiteralValue::Text {
                value: self.take_string("text literal")?,
            },
            "duration" => LiteralValue::Duration {
                value: DurationMs::new(self.parse_atom("duration milliseconds")?),
            },
            "timestamp" => LiteralValue::Timestamp {
                value: TimestampMs::parse_canonical(&self.take_string("timestamp")?)
                    .map_err(|error| self.atom(error))?,
            },
            "quantity" => {
                let scale = self.parse_atom("quantity scale")?;
                self.comma()?;
                let amount =
                    DecimalValue::parse_canonical(scale, &self.take_string("quantity amount")?)
                        .map_err(|error| self.atom(error))?;
                self.comma()?;
                let unit = self.parse_uid()?;
                LiteralValue::Quantity { amount, unit }
            }
            "reference" => LiteralValue::Reference {
                value: self.parse_reference()?,
            },
            "datum" => {
                let value_type = Box::new(self.parse_type(depth + 1)?);
                self.comma()?;
                let state = self.parse_enum_atom("datum state")?;
                let value = if self.consume_symbol(',') {
                    Some(Box::new(self.parse_literal(depth + 1)?))
                } else {
                    None
                };
                LiteralValue::Datum {
                    value_type,
                    state,
                    value,
                }
            }
            "candidate" => {
                let route = self.parse_enum_atom("candidate route")?;
                self.comma()?;
                let template = Slug::new(self.take_word("candidate template")?)
                    .map_err(|error| self.atom(error))?;
                self.comma()?;
                let fields = self.parse_candidate_literal_fields(depth + 1)?;
                LiteralValue::Candidate {
                    route,
                    template,
                    fields,
                }
            }
            _ => return Err(self.unexpected(format!("unknown literal constructor {kind:?}"))),
        };
        self.expect_symbol(')')?;
        literal.value_type().map_err(|error| self.atom(error))?;
        Ok(literal)
    }

    fn parse_candidate_type_fields(
        &mut self,
        depth: usize,
    ) -> Result<BTreeMap<LocalId, ValueType>, KarmaDslError> {
        self.check_depth(depth)?;
        self.parse_id_map("candidate type field", ':', |parser| {
            parser.parse_type(depth)
        })
    }

    fn parse_candidate_literal_fields(
        &mut self,
        depth: usize,
    ) -> Result<BTreeMap<LocalId, LiteralValue>, KarmaDslError> {
        self.check_depth(depth)?;
        self.parse_id_map("candidate literal field", '=', |parser| {
            parser.parse_literal(depth)
        })
    }

    fn parse_precision(&mut self) -> Result<DecimalPrecision, KarmaDslError> {
        self.expect_word("precision")?;
        self.expect_symbol('(')?;
        let scale = self.parse_atom("precision scale")?;
        self.comma()?;
        let rounding = self.parse_enum_atom("rounding mode")?;
        let result_unit = if self.consume_symbol(',') {
            if self.consume_word("dimensionless") {
                Some(DeclaredUnit::Dimensionless)
            } else {
                Some(DeclaredUnit::Unit {
                    unit: self.parse_uid()?,
                })
            }
        } else {
            None
        };
        self.expect_symbol(')')?;
        Ok(DecimalPrecision {
            scale,
            rounding,
            result_unit,
        })
    }

    fn parse_uid(&mut self) -> Result<TypedUid, KarmaDslError> {
        self.expect_word("uid")?;
        self.expect_symbol('(')?;
        let kind = self.parse_enum_atom("uid kind")?;
        self.comma()?;
        let value = self.take_word("uid value")?;
        self.expect_symbol(')')?;
        TypedUid::new(kind, value).map_err(|error| self.atom(error))
    }

    fn parse_reference(&mut self) -> Result<ResolvedReference, KarmaDslError> {
        self.expect_word("ref")?;
        self.expect_symbol('(')?;
        let kind = self.parse_enum_atom("reference kind")?;
        self.comma()?;
        let uid = TypedUid::new(kind, self.take_word("reference uid")?)
            .map_err(|error| self.atom(error))?;
        self.comma()?;
        let display_slug = if self.consume_word("none") {
            None
        } else {
            Some(
                Slug::new(self.take_word("reference display slug")?)
                    .map_err(|error| self.atom(error))?,
            )
        };
        self.expect_symbol(')')?;
        Ok(ResolvedReference {
            target: uid,
            display_slug,
        })
    }

    pub(super) fn parse_optional_reference(
        &mut self,
    ) -> Result<Option<ResolvedReference>, KarmaDslError> {
        if self.consume_word("none") {
            Ok(None)
        } else {
            self.parse_reference().map(Some)
        }
    }

    pub(super) fn parse_single_reference_argument(
        &mut self,
    ) -> Result<ResolvedReference, KarmaDslError> {
        self.expect_symbol('(')?;
        let reference = self.parse_reference()?;
        self.expect_symbol(')')?;
        Ok(reference)
    }

    pub(super) fn parse_single_local_id_argument(
        &mut self,
        label: &str,
    ) -> Result<LocalId, KarmaDslError> {
        self.expect_symbol('(')?;
        let id = self.parse_local_id(label)?;
        self.expect_symbol(')')?;
        Ok(id)
    }
}
