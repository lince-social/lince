//! Value types, literals, and exact decimals for Karma Programs.

use serde::{Deserialize, Serialize};

mod decimal;
mod literal;
mod types;

pub use decimal::{DecimalValue, RoundedDecimal, Rounding};
pub use literal::LiteralValue;
pub use types::{MAX_VALUE_TYPE_DEPTH, PortContract, Sensitivity, ValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum CandidateRoute {
    Observe,
    Recommend,
    Draft,
    Ask,
    Act,
}
