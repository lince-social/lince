mod meal;
pub mod production;
mod reference;

pub use meal::*;
pub use reference::*;

use crate::DecimalValue;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn date(value: &str) -> Result<NaiveDate, String> {
    if value.len() != 10 {
        return Err("Use dates in YYYY-MM-DD form".into());
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "Use a valid YYYY-MM-DD date".into())
}

pub fn exact(value: &str) -> Result<DecimalValue, String> {
    DecimalValue::parse_inferred(value).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    G,
    Mg,
    Ug,
    Kcal,
    Kj,
}

impl Unit {
    pub fn for_field(field: &str) -> Result<Self, String> {
        match field.rsplit('_').next() {
            Some("g") => Ok(Self::G),
            Some("mg") => Ok(Self::Mg),
            Some("ug") => Ok(Self::Ug),
            Some("kcal") => Ok(Self::Kcal),
            Some("kj") => Ok(Self::Kj),
            _ => Err(format!("Unknown nutrient unit in {field}")),
        }
    }

    pub fn factor(self, to: Self) -> Result<f64, String> {
        let mass = |u| match u {
            Self::G => Some(1.0),
            Self::Mg => Some(0.001),
            Self::Ug => Some(0.000001),
            _ => None,
        };
        if self == to {
            return Ok(1.0);
        }
        if let (Some(a), Some(b)) = (mass(self), mass(to)) {
            return Ok(a / b);
        }
        match (self, to) {
            (Self::Kcal, Self::Kj) => Ok(4.184),
            (Self::Kj, Self::Kcal) => Ok(1.0 / 4.184),
            _ => Err("Cannot convert mass to energy".into()),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub lower: Option<DecimalValue>,
    pub upper: Option<DecimalValue>,
}

impl Bounds {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .lower
            .into_iter()
            .chain(self.upper)
            .any(|v| v.to_f64() < 0.0)
            || self
                .lower
                .zip(self.upper)
                .is_some_and(|(a, b)| a.to_f64() > b.to_f64())
        {
            return Err("Bounds must be nonnegative and minimum cannot exceed maximum".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Day,
    Week,
    Horizon,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FoodBound {
    pub period: Period,
    pub bounds: Bounds,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Acquisition {
    #[default]
    Buy,
    Grow,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FoodChoice {
    pub food: String,
    #[serde(default)]
    pub bounds: Vec<FoodBound>,
    #[serde(default)]
    pub acquisition: Acquisition,
    pub volume_ml_per_g: Option<DecimalValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Food {
    pub choice: FoodChoice,
    pub label: String,
    pub basis_g: DecimalValue,
    pub nutrients: BTreeMap<String, serde_json::Value>,
}

impl Food {
    pub fn from_nutrition(
        choice: FoodChoice,
        label: String,
        value: &serde_json::Value,
    ) -> Result<Self, String> {
        let basis_g = exact(&value["basis_g"].to_string())?;
        let nutrients = value["nutrients"]
            .as_object()
            .ok_or("Food has no measured nutrient table")?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(Self {
            choice,
            label,
            basis_g,
            nutrients,
        })
    }

    pub fn coefficient(&self, field: &str, unit: Unit) -> Result<(f64, bool), String> {
        let measured = self
            .nutrients
            .get(field)
            .and_then(serde_json::Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0);
        let value = measured.unwrap_or(0.0) / self.basis_g.to_f64()
            * Unit::for_field(field)?.factor(unit)?;
        Ok((value, measured.is_none()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: String,
    pub food: String,
    pub currency: String,
    pub price: DecimalValue,
    pub package_g: DecimalValue,
    pub usable_yield: DecimalValue,
    pub available_on: String,
    pub valid_until: Option<String>,
    pub max_packages: Option<DecimalValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stock {
    pub id: String,
    pub food: String,
    pub usable_g: DecimalValue,
    pub available_on: String,
    pub expires_on: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Forecast {
    pub stock: Stock,
    pub cost_per_usable_g: DecimalValue,
    pub currency: String,
    pub process: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub start: String,
    pub days: u16,
    pub currency: String,
    pub requirements: Requirements,
    pub foods: Vec<Food>,
    pub offers: Vec<Offer>,
    pub stock: Vec<Stock>,
    pub forecasts: Vec<Forecast>,
    pub volume_ml_per_day: Option<DecimalValue>,
}
