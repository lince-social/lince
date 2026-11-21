use crate::{BinaryOperator, DecimalPrecision, DecimalValue, FailurePath, LocalId};

use super::{EvaluationError, EvaluationErrorCode, EvaluationLimits, RoundingNote};

#[derive(Clone, Copy)]
pub(super) enum Path<'a> {
    Node,
    InputSource(&'a LocalId),
    Expression(&'a LocalId),
    Parameter,
    DelayInput,
    Output(&'a LocalId),
}

#[derive(Clone, Copy)]
pub(super) struct Site<'a> {
    pub(super) node: &'a LocalId,
    path: Path<'a>,
}

impl<'a> Site<'a> {
    pub(super) const fn node(node: &'a LocalId) -> Self {
        Self {
            node,
            path: Path::Node,
        }
    }

    pub(super) const fn at(self, path: Path<'a>) -> Self {
        Self { path, ..self }
    }

    fn failure_path(self) -> FailurePath {
        let node = self.node.as_str();
        let path = match self.path {
            Path::Node => format!("/nodes/{node}"),
            Path::InputSource(input) => format!("/nodes/{node}/inputs/{}/source", input.as_str()),
            Path::Expression(expression) => format!(
                "/nodes/{node}/operation/expressions/{}",
                expression.as_str()
            ),
            Path::Parameter => format!("/nodes/{node}/operation/source/parameter"),
            Path::DelayInput => format!("/nodes/{node}/operation/input"),
            Path::Output(output) => format!("/outputs/{}", output.as_str()),
        };
        // A LocalId contains only lowercase ASCII letters, digits, and underscores.
        FailurePath::new(path).expect("validated IDs form unescaped JSON pointer segments")
    }
}

pub(super) struct FuelMeter {
    pub(super) limits: EvaluationLimits,
    pub(super) used: u64,
    pub(super) rounding: Vec<RoundingNote>,
}

impl FuelMeter {
    pub(super) const fn new(limits: EvaluationLimits) -> Self {
        Self {
            limits,
            used: 0,
            rounding: Vec::new(),
        }
    }

    pub(super) fn note_rounding(
        &mut self,
        site: Site<'_>,
        operator: BinaryOperator,
        precision: &DecimalPrecision,
        result: DecimalValue,
    ) {
        self.rounding.push(RoundingNote {
            node: site.node.clone(),
            path: site.failure_path(),
            operator,
            scale: precision.scale,
            rounding: precision.rounding,
            result,
        });
    }

    pub(super) fn consume(&mut self, site: Site<'_>) -> Result<(), EvaluationError> {
        if self.used >= self.limits.fuel {
            return Err(self.error(
                EvaluationErrorCode::FuelExhausted,
                site,
                "deterministic evaluation fuel exhausted",
            ));
        }
        self.used += 1;
        Ok(())
    }

    pub(super) fn error(
        &self,
        code: EvaluationErrorCode,
        site: Site<'_>,
        message: impl Into<String>,
    ) -> EvaluationError {
        EvaluationError {
            code,
            message: message.into(),
            path: site.failure_path(),
            node: Some(site.node.clone()),
            fuel_used: self.used,
        }
    }

    pub(super) fn invariant(&self, site: Site<'_>, message: impl Into<String>) -> EvaluationError {
        self.error(EvaluationErrorCode::InvariantViolation, site, message)
    }
}
