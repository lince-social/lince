use super::*;
use crate::optimization::{self, Constraint, Model, Options, Status, Term, Variable, decimal};
use chrono::Days;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Offer,
    Inventory,
    Forecast,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Allocation {
    pub food: usize,
    pub day: u16,
    pub source: String,
    pub kind: SourceKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Compiled {
    pub model: Model,
    pub allocations: Vec<Allocation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissingMeasurement {
    pub food: String,
    pub nutrient: String,
    pub original: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DaySummary {
    pub date: String,
    pub foods_g: BTreeMap<String, f64>,
    pub nutrients: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Shopping {
    pub offer: String,
    pub food: String,
    pub packages: u64,
    pub used_usable_g: f64,
    pub bought_g: f64,
    pub leftover_usable_g: f64,
    pub bill: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Outcome {
    pub solution: optimization::Report,
    pub days: Vec<DaySummary>,
    pub totals_g: BTreeMap<String, f64>,
    pub shopping: Vec<Shopping>,
    pub rounded_bill: f64,
    pub forecast_cost: f64,
    pub missing: Vec<MissingMeasurement>,
    pub input_hash: String,
}

pub fn fingerprint(value: &impl Serialize) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    Ok(
        Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

fn nonnegative(value: DecimalValue) -> Result<f64, String> {
    let number = value.to_f64();
    if !number.is_finite() || !(0.0..=1e12).contains(&number) {
        return Err("Quantities must be finite, nonnegative, and at most 10^12".into());
    }
    Ok(number)
}

fn positive(value: DecimalValue) -> Result<f64, String> {
    let value = nonnegative(value)?;
    if value == 0.0 {
        return Err("Measurement bases and usable yields must be positive".into());
    }
    Ok(value)
}

fn valid_window(from: &str, until: Option<&str>) -> Result<(), String> {
    let from = date(from)?;
    if until
        .map(date)
        .transpose()?
        .is_some_and(|until| until < from)
    {
        return Err("Availability cannot follow expiry".into());
    }
    Ok(())
}

fn eligible(day: chrono::NaiveDate, from: &str, until: Option<&str>) -> bool {
    day >= date(from).unwrap() && until.is_none_or(|until| day <= date(until).unwrap())
}

impl Input {
    pub fn validate(&self) -> Result<(), String> {
        let start = date(&self.start)?;
        if self.days == 0
            || self.days > 31
            || start
                .checked_add_days(Days::new(u64::from(self.days)))
                .is_none()
            || self.currency.is_empty()
            || self.currency.len() > 128
            || self.foods.is_empty()
            || self.foods.len() > 600
            || self.offers.len() + self.stock.len() + self.forecasts.len() > 2000
            || self.requirements.bounds.len() > 100
            || self.requirements.energy_shares.len() > 10
        {
            return Err(
                "Choose 1–31 days, 1–600 foods, a currency, and a bounded set of inputs".into(),
            );
        }
        let mut food_ids = BTreeSet::new();
        for food in &self.foods {
            if food.choice.food.is_empty()
                || food.choice.food.len() > 128
                || !food_ids.insert(food.choice.food.clone())
                || food.choice.bounds.len() > 8
                || food.nutrients.len() > 256
            {
                return Err("Food choices must be unique and bounded".into());
            }
            positive(food.basis_g)?;
            for bound in &food.choice.bounds {
                bound.bounds.validate()?;
            }
            if let Some(volume) = food.choice.volume_ml_per_g {
                positive(volume)?;
            }
            if self.volume_ml_per_day.is_some() && food.choice.volume_ml_per_g.is_none() {
                return Err(
                    "Every food needs measured volume per usable gram to constrain volume".into(),
                );
            }
        }
        if let Some(volume) = self.volume_ml_per_day {
            positive(volume)?;
        }
        let mut sources = BTreeSet::new();
        for offer in &self.offers {
            if !food_ids.contains(&offer.food)
                || !sources.insert(offer.id.clone())
                || offer.id.is_empty()
                || offer.currency != self.currency
            {
                return Err("Offers need unique identities, selected food preparations, and the plan currency".into());
            }
            nonnegative(offer.price)?;
            positive(offer.package_g)?;
            positive(offer.usable_yield)?;
            if let Some(max) = offer.max_packages {
                if nonnegative(max)?.fract() != 0.0 {
                    return Err("Package availability must be a whole count".into());
                }
            }
            valid_window(&offer.available_on, offer.valid_until.as_deref())?;
        }
        for stock in self
            .stock
            .iter()
            .chain(self.forecasts.iter().map(|f| &f.stock))
        {
            if !food_ids.contains(&stock.food)
                || !sources.insert(stock.id.clone())
                || stock.id.is_empty()
            {
                return Err(
                    "Stock and forecasts need unique identities and a selected food preparation"
                        .into(),
                );
            }
            nonnegative(stock.usable_g)?;
            valid_window(&stock.available_on, stock.expires_on.as_deref())?;
        }
        for forecast in &self.forecasts {
            nonnegative(forecast.cost_per_usable_g)?;
            if forecast.currency != self.currency {
                return Err("Forecast cost must use the plan currency".into());
            }
        }
        let mut names = BTreeSet::new();
        for bound in &self.requirements.bounds {
            if !names.insert(bound.nutrient.clone()) || bound.nutrient.len() > 128 {
                return Err("Nutrient bounds must be unique".into());
            }
            Unit::for_field(&bound.nutrient)?.factor(bound.unit)?;
            bound.bounds.validate()?;
            if bound.scope != "total"
                && !bound.nutrient.contains("supplemental")
                && !["folic_acid_ug", "niacin_added_mg"].contains(&bound.nutrient.as_str())
            {
                return Err(
                    "A scoped ceiling needs a separately measured nutrient-form field".into(),
                );
            }
        }
        for share in &self.requirements.energy_shares {
            if share.lower.to_f64() < 0.0
                || share.upper.to_f64() > 1.0
                || share.lower.to_f64() > share.upper.to_f64()
            {
                return Err("Energy shares must be within zero and one".into());
            }
            positive(share.kcal_per_g)?;
            Unit::for_field(&share.nutrient)?.factor(Unit::G)?;
        }
        Ok(())
    }

    pub fn compile(&self) -> Result<Compiled, String> {
        self.validate()?;
        let mut compiled = Compiled {
            model: Model::default(),
            allocations: Vec::new(),
        };
        let start = date(&self.start)?;
        for (food_index, food) in self.foods.iter().enumerate() {
            for day in 0..self.days {
                let at = start.checked_add_days(Days::new(u64::from(day))).unwrap();
                for (index, offer) in self
                    .offers
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.food == food.choice.food)
                {
                    if eligible(at, &offer.available_on, offer.valid_until.as_deref()) {
                        compiled.add(
                            food_index,
                            day,
                            &offer.id,
                            SourceKind::Offer,
                            format!("offer:{index}:{day}"),
                            offer.price.to_f64()
                                / (offer.package_g.to_f64() * offer.usable_yield.to_f64()),
                        )?;
                    }
                }
                for (index, stock) in self
                    .stock
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.food == food.choice.food)
                {
                    if eligible(at, &stock.available_on, stock.expires_on.as_deref()) {
                        compiled.add(
                            food_index,
                            day,
                            &stock.id,
                            SourceKind::Inventory,
                            format!("stock:{index}:{day}"),
                            0.0,
                        )?;
                    }
                }
                if food.choice.acquisition == Acquisition::Grow {
                    for (index, forecast) in self
                        .forecasts
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.stock.food == food.choice.food)
                    {
                        if eligible(
                            at,
                            &forecast.stock.available_on,
                            forecast.stock.expires_on.as_deref(),
                        ) {
                            compiled.add(
                                food_index,
                                day,
                                &forecast.stock.id,
                                SourceKind::Forecast,
                                format!("forecast:{index}:{day}"),
                                forecast.cost_per_usable_g.to_f64(),
                            )?;
                        }
                    }
                }
            }
        }
        for offer in &self.offers {
            if let Some(max) = offer.max_packages {
                compiled.capacity(
                    &offer.id,
                    max.to_f64() * offer.package_g.to_f64() * offer.usable_yield.to_f64(),
                )?;
            }
        }
        for stock in self
            .stock
            .iter()
            .chain(self.forecasts.iter().map(|f| &f.stock))
        {
            compiled.capacity(&stock.id, stock.usable_g.to_f64())?;
        }
        for (food_index, food) in self.foods.iter().enumerate() {
            for (bound_index, bound) in food.choice.bounds.iter().enumerate() {
                let stride = match bound.period {
                    Period::Day => 1,
                    Period::Week => 7,
                    Period::Horizon => self.days,
                };
                for first in (0..self.days).step_by(usize::from(stride)) {
                    let last = (first + stride).min(self.days);
                    let factor = if bound.period == Period::Week {
                        f64::from(last - first) / 7.0
                    } else {
                        1.0
                    };
                    let terms = compiled
                        .allocations
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| a.food == food_index && (first..last).contains(&a.day))
                        .map(|(variable, _)| {
                            Ok(Term {
                                variable,
                                coefficient: decimal(1.0)?,
                            })
                        })
                        .collect::<Result<_, String>>()?;
                    compiled.model.constraints.push(Constraint {
                        name: format!("food:{}:{bound_index}:{first}", food.choice.food),
                        terms,
                        lower: bound
                            .bounds
                            .lower
                            .map(|v| decimal(v.to_f64() * factor))
                            .transpose()?,
                        upper: bound
                            .bounds
                            .upper
                            .map(|v| decimal(v.to_f64() * factor))
                            .transpose()?,
                    });
                }
            }
        }
        for day in 0..self.days {
            for bound in &self.requirements.bounds {
                let terms = compiled
                    .allocations
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.day == day)
                    .map(|(variable, a)| {
                        Ok(Term {
                            variable,
                            coefficient: decimal(
                                self.foods[a.food]
                                    .coefficient(&bound.nutrient, bound.unit)?
                                    .0,
                            )?,
                        })
                    })
                    .collect::<Result<_, String>>()?;
                compiled.model.constraints.push(Constraint {
                    name: format!("nutrient:{}:{day}", bound.nutrient),
                    terms,
                    lower: bound.bounds.lower,
                    upper: bound.bounds.upper,
                });
            }
            for share in &self.requirements.energy_shares {
                for (suffix, fraction, lower) in [
                    ("minimum", share.lower, true),
                    ("maximum", share.upper, false),
                ] {
                    let terms = compiled
                        .allocations
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| a.day == day)
                        .map(|(variable, a)| {
                            let food = &self.foods[a.food];
                            let coefficient = food.coefficient(&share.nutrient, Unit::G)?.0
                                * share.kcal_per_g.to_f64()
                                - fraction.to_f64()
                                    * food.coefficient("energy_kcal", Unit::Kcal)?.0;
                            Ok(Term {
                                variable,
                                coefficient: decimal(coefficient)?,
                            })
                        })
                        .collect::<Result<_, String>>()?;
                    compiled.model.constraints.push(Constraint {
                        name: format!("energy-share:{}:{suffix}:{day}", share.nutrient),
                        terms,
                        lower: lower.then(|| decimal(0.0).unwrap()),
                        upper: (!lower).then(|| decimal(0.0).unwrap()),
                    });
                }
            }
            if let Some(volume) = self.volume_ml_per_day {
                let terms = compiled
                    .allocations
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.day == day)
                    .map(|(variable, a)| Term {
                        variable,
                        coefficient: self.foods[a.food].choice.volume_ml_per_g.unwrap(),
                    })
                    .collect();
                compiled.model.constraints.push(Constraint {
                    name: format!("container:{day}"),
                    terms,
                    lower: None,
                    upper: Some(volume),
                });
            }
        }
        let missing = self.missing();
        if !missing.is_empty() {
            compiled.model.notes.push(format!("{} missing, null, or trace coefficients contribute zero; inspect the missing measurement list", missing.len()));
        }
        compiled.model.validate()?;
        Ok(compiled)
    }

    pub fn missing(&self) -> Vec<MissingMeasurement> {
        let fields: BTreeSet<_> = self
            .requirements
            .bounds
            .iter()
            .map(|b| b.nutrient.as_str())
            .chain(
                self.requirements
                    .energy_shares
                    .iter()
                    .map(|s| s.nutrient.as_str()),
            )
            .collect();
        self.foods
            .iter()
            .flat_map(|food| {
                fields.iter().filter_map(|field| {
                    food.coefficient(field, Unit::for_field(field).ok()?)
                        .ok()?
                        .1
                        .then(|| MissingMeasurement {
                            food: food.choice.food.clone(),
                            nutrient: (*field).into(),
                            original: food.nutrients.get(*field).cloned(),
                        })
                })
            })
            .collect()
    }

    pub fn solve(&self, options: Options, cancelled: &AtomicBool) -> Result<Outcome, String> {
        let compiled = self.compile()?;
        let solution = optimization::solve(&compiled.model, options, cancelled);
        let mut outcome = Outcome {
            solution,
            days: Vec::new(),
            totals_g: BTreeMap::new(),
            shopping: Vec::new(),
            rounded_bill: 0.0,
            forecast_cost: 0.0,
            missing: self.missing(),
            input_hash: fingerprint(self)?,
        };
        if !matches!(outcome.solution.status, Status::Optimal | Status::Feasible) {
            return Ok(outcome);
        }
        let start = date(&self.start)?;
        let mut acquired = BTreeMap::<String, f64>::new();
        for day in 0..self.days {
            let mut summary = DaySummary {
                date: start
                    .checked_add_days(Days::new(u64::from(day)))
                    .unwrap()
                    .to_string(),
                foods_g: BTreeMap::new(),
                nutrients: BTreeMap::new(),
            };
            for (a, amount) in compiled
                .allocations
                .iter()
                .zip(&outcome.solution.values)
                .filter(|(a, _)| a.day == day)
            {
                let amount = amount.max(0.0);
                let food = &self.foods[a.food];
                *summary.foods_g.entry(food.choice.food.clone()).or_default() += amount;
                *outcome
                    .totals_g
                    .entry(food.choice.food.clone())
                    .or_default() += amount;
                if a.kind == SourceKind::Offer {
                    *acquired.entry(a.source.clone()).or_default() += amount;
                }
                if a.kind == SourceKind::Forecast {
                    outcome.forecast_cost += amount
                        * self
                            .forecasts
                            .iter()
                            .find(|f| f.stock.id == a.source)
                            .unwrap()
                            .cost_per_usable_g
                            .to_f64();
                }
                for bound in &self.requirements.bounds {
                    *summary.nutrients.entry(bound.nutrient.clone()).or_default() +=
                        amount * food.coefficient(&bound.nutrient, bound.unit)?.0;
                }
            }
            outcome.days.push(summary);
        }
        for offer in &self.offers {
            let needed = acquired.get(&offer.id).copied().unwrap_or(0.0);
            if needed <= 1e-8 {
                continue;
            }
            let usable = offer.package_g.to_f64() * offer.usable_yield.to_f64();
            let packages = (needed / usable - 1e-9).ceil().max(1.0);
            if !packages.is_finite() || packages > 1e12 {
                return Err("Rounded package count exceeds its limits".into());
            }
            let bill = packages * offer.price.to_f64();
            outcome.rounded_bill += bill;
            outcome.shopping.push(Shopping {
                offer: offer.id.clone(),
                food: offer.food.clone(),
                packages: packages as u64,
                used_usable_g: needed,
                bought_g: packages * offer.package_g.to_f64(),
                leftover_usable_g: (packages * usable - needed).max(0.0),
                bill,
            });
        }
        Ok(outcome)
    }
}

impl Compiled {
    fn add(
        &mut self,
        food: usize,
        day: u16,
        source: &str,
        kind: SourceKind,
        name: String,
        cost: f64,
    ) -> Result<(), String> {
        if self.allocations.len() >= optimization::MAX_VARIABLES {
            return Err(
                "Too many food/day/source allocations; shorten the period or select fewer offers"
                    .into(),
            );
        }
        self.model.variables.push(Variable {
            name,
            cost: decimal(cost)?,
            lower: None,
            upper: None,
        });
        self.allocations.push(Allocation {
            food,
            day,
            source: source.into(),
            kind,
        });
        Ok(())
    }

    fn capacity(&mut self, source: &str, maximum: f64) -> Result<(), String> {
        let terms = self
            .allocations
            .iter()
            .enumerate()
            .filter(|(_, a)| a.source == source)
            .map(|(variable, _)| Term {
                variable,
                coefficient: decimal(1.0).unwrap(),
            })
            .collect();
        self.model.constraints.push(Constraint {
            name: format!("supply:{source}"),
            terms,
            lower: None,
            upper: Some(decimal(maximum)?),
        });
        Ok(())
    }
}
