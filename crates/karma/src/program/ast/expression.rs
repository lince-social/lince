//! Expression types, arithmetic precision, and input discovery.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{LiteralValue, LocalId, Rounding, TypedUid};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum ExpressionAst {
    Literal {
        value: LiteralValue,
    },
    Input {
        input: LocalId,
    },
    Unary {
        operator: UnaryOperator,
        value: Box<ExpressionAst>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<ExpressionAst>,
        right: Box<ExpressionAst>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        precision: Option<DecimalPrecision>,
    },
    If {
        condition: Box<ExpressionAst>,
        then_value: Box<ExpressionAst>,
        else_value: Box<ExpressionAst>,
    },
}

impl ExpressionAst {
    /// Returns the distinct input identifiers referenced by this expression.
    pub fn referenced_inputs(&self) -> BTreeSet<LocalId> {
        let mut inputs = BTreeSet::new();
        self.collect_referenced_inputs(&mut inputs);
        inputs
    }

    fn collect_referenced_inputs(&self, inputs: &mut BTreeSet<LocalId>) {
        match self {
            Self::Input { input } if !inputs.contains(input) => {
                inputs.insert(input.clone());
            }
            Self::Literal { .. } | Self::Input { .. } => {}
            Self::Unary { value, .. } => value.collect_referenced_inputs(inputs),
            Self::Binary { left, right, .. } => {
                left.collect_referenced_inputs(inputs);
                right.collect_referenced_inputs(inputs);
            }
            Self::If {
                condition,
                then_value,
                else_value,
            } => {
                condition.collect_referenced_inputs(inputs);
                then_value.collect_referenced_inputs(inputs);
                else_value.collect_referenced_inputs(inputs);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum UnaryOperator {
    Not,
    Negate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecimalPrecision {
    pub scale: u8,
    pub rounding: Rounding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_unit: Option<DeclaredUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum DeclaredUnit {
    Dimensionless,
    Unit { unit: TypedUid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    And,
    Or,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_inputs_include_each_branch_without_duplicates() {
        let input = |name| LocalId::new(name).expect("valid input name");
        let leaf = |name| Box::new(ExpressionAst::Input { input: input(name) });
        let expr = ExpressionAst::If {
            condition: leaf("condition"),
            then_value: Box::new(ExpressionAst::Binary {
                operator: BinaryOperator::Add,
                left: leaf("value"),
                right: leaf("value"),
                precision: None,
            }),
            else_value: Box::new(ExpressionAst::Unary {
                operator: UnaryOperator::Negate,
                value: leaf("fallback"),
            }),
        };

        let expected = BTreeSet::from(["condition", "value", "fallback"].map(input));
        assert_eq!(expr.referenced_inputs(), expected);
    }
}
