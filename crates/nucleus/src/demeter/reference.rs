use super::{Bounds, Unit, exact};
use crate::{DecimalValue, optimization::decimal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceGroup {
    Male,
    Female,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub age: u8,
    pub group: ReferenceGroup,
    pub weight_kg: DecimalValue,
    pub height_cm: DecimalValue,
    pub activity_factor: DecimalValue,
    pub energy_override_kcal: Option<DecimalValue>,
}

impl Profile {
    pub fn energy(&self) -> Result<DecimalValue, String> {
        if !(19..=120).contains(&self.age)
            || self.weight_kg.to_f64() <= 0.0
            || self.height_cm.to_f64() <= 0.0
            || self.activity_factor.to_f64() <= 0.0
        {
            return Err("Adult references require age 19–120 and positive body measurements and activity factor".into());
        }
        let value = self
            .energy_override_kcal
            .map(DecimalValue::to_f64)
            .unwrap_or_else(|| {
                let constant = match self.group {
                    ReferenceGroup::Male => 5.0,
                    ReferenceGroup::Female => -161.0,
                };
                (10.0 * self.weight_kg.to_f64() + 6.25 * self.height_cm.to_f64()
                    - 5.0 * f64::from(self.age)
                    + constant)
                    * self.activity_factor.to_f64()
            });
        if !(0.0..=20_000.0).contains(&value) || value == 0.0 {
            return Err("Enter a positive plausible energy target".into());
        }
        decimal(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReferenceCase {
    pub sex: ReferenceGroup,
    pub age_min: u8,
    pub age_max: u8,
    pub lower: Option<String>,
    pub upper: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EnergyFraction {
    pub lower: String,
    pub upper: String,
    pub kcal_per_unit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Reference {
    pub nutrient: String,
    pub unit: Unit,
    pub kind: String,
    pub scope: String,
    pub source_url: String,
    pub cases: Vec<ReferenceCase>,
    pub per_kg: Option<String>,
    pub energy_fraction: Option<EnergyFraction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NutrientBound {
    pub nutrient: String,
    pub unit: Unit,
    pub bounds: Bounds,
    pub scope: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnergyShare {
    pub nutrient: String,
    pub lower: DecimalValue,
    pub upper: DecimalValue,
    pub kcal_per_g: DecimalValue,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    pub bounds: Vec<NutrientBound>,
    #[serde(default)]
    pub energy_shares: Vec<EnergyShare>,
}

pub fn select_references(
    references: &[Reference],
    profile: &Profile,
    overrides: &[NutrientBound],
    disabled: &[String],
) -> Result<Requirements, String> {
    let energy = profile.energy()?;
    let mut selected = Requirements::default();
    let mut seen = BTreeSet::new();
    for reference in references {
        if !seen.insert(reference.nutrient.clone()) {
            return Err(
                "Choose one reference dataset; duplicate nutrient references are ambiguous".into(),
            );
        }
        if disabled.contains(&reference.nutrient) {
            continue;
        }
        let cases: Vec<_> = reference
            .cases
            .iter()
            .filter(|c| c.sex == profile.group && (c.age_min..=c.age_max).contains(&profile.age))
            .collect();
        if cases.len() != 1 {
            return Err(format!(
                "No unique adult reference case for {}",
                reference.nutrient
            ));
        }
        let case = cases[0];
        let mut lower = case.lower.as_deref().map(exact).transpose()?;
        if let Some(per_kg) = &reference.per_kg {
            lower = Some(decimal(
                exact(per_kg)?.to_f64() * profile.weight_kg.to_f64(),
            )?);
        }
        selected.bounds.push(NutrientBound {
            nutrient: reference.nutrient.clone(),
            unit: reference.unit,
            bounds: Bounds {
                lower,
                upper: case.upper.as_deref().map(exact).transpose()?,
            },
            scope: reference.scope.clone(),
            source: reference.source_url.clone(),
        });
        if let Some(fraction) = &reference.energy_fraction {
            selected.energy_shares.push(EnergyShare {
                nutrient: reference.nutrient.clone(),
                lower: exact(&fraction.lower)?,
                upper: exact(&fraction.upper)?,
                kcal_per_g: exact(&fraction.kcal_per_unit)?,
            });
        }
    }
    if !disabled.iter().any(|field| field == "energy_kcal") {
        selected.bounds.push(NutrientBound {
            nutrient: "energy_kcal".into(),
            unit: Unit::Kcal,
            bounds: Bounds {
                lower: Some(energy),
                upper: None,
            },
            scope: "total".into(),
            source: "https://pubmed.ncbi.nlm.nih.gov/2305711/; explicit activity factor".into(),
        });
    }
    let mut overridden = BTreeSet::new();
    for bound in overrides {
        if !overridden.insert(bound.nutrient.clone()) {
            return Err("Duplicate personal nutrient overrides".into());
        }
        selected.bounds.retain(|r| r.nutrient != bound.nutrient);
        selected.bounds.push(bound.clone());
    }
    Ok(selected)
}
