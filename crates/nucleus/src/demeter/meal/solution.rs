//! Solves a meal plan and prepares its daily and shopping summaries.
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

use chrono::Days;

use super::{DaySummary, Input, Outcome, Shopping, SourceKind, date, fingerprint};
use crate::optimization::{self, Options, Status};

impl Input {
    /// Solves this meal plan and totals its daily nutrients and shopping requirements.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid plan inputs or when a solved plan cannot
    /// be summarized within the supported date and package limits.
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
        let mut acquired = BTreeMap::new();
        for day in 0..self.days {
            let mut summary = DaySummary {
                date: start
                    .checked_add_days(Days::new(u64::from(day)))
                    .ok_or("Plan date is out of range")?
                    .to_string(),
                foods_g: BTreeMap::new(),
                nutrients: BTreeMap::new(),
            };
            for (allocation, amount) in compiled
                .allocations
                .iter()
                .zip(&outcome.solution.values)
                .filter(|(allocation, _)| allocation.day == day)
            {
                let amount = amount.max(0.0);
                let food = &self.foods[allocation.food];
                *summary.foods_g.entry(food.choice.food.clone()).or_default() += amount;
                *outcome
                    .totals_g
                    .entry(food.choice.food.clone())
                    .or_default() += amount;
                if allocation.kind == SourceKind::Offer {
                    *acquired.entry(allocation.source.as_str()).or_default() += amount;
                }
                if allocation.kind == SourceKind::Forecast {
                    outcome.forecast_cost += amount
                        * self
                            .forecasts
                            .iter()
                            .find(|forecast| forecast.stock.id == allocation.source)
                            .ok_or("Forecast allocation has no matching source")?
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
            let needed = acquired.get(offer.id.as_str()).copied().unwrap_or(0.0);
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
