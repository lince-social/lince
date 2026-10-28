use super::*;
use crate::optimization::{Status, decimal};
use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_TASKS: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Material {
    pub item: String,
    pub quantity: String,
    pub unit: String,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    Establish,
    #[default]
    Care,
    Harvest,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Stage {
    #[serde(default)]
    pub ongoing: bool,
    #[serde(default)]
    pub kind: StageKind,
    pub offset_days: u32,
    pub title: String,
    #[serde(default)]
    pub repeat_every_days: Option<u32>,
    #[serde(default)]
    pub until_days: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Template {
    #[serde(default)]
    pub food_outputs: Vec<String>,
    pub id: String,
    pub species: String,
    pub method: String,
    pub first_harvest_days: u32,
    pub harvest_interval_days: u32,
    pub harvest_count: u32,
    pub usable_g_per_harvest: String,
    pub season_months: Vec<u32>,
    pub requires_confirmation: bool,
    pub assumption: String,
    pub inputs: Vec<Material>,
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub food: String,
    pub template: String,
    pub confirmed: bool,
    pub start_on: String,
    pub yield_override_g: Option<DecimalValue>,
    pub lead_days_override: Option<u32>,
    pub season_months: Vec<u32>,
    pub usable_yield: DecimalValue,
    pub shelf_life_days: u32,
    pub max_units: Option<u32>,
    pub existing_units: Option<u32>,
    pub cost_per_usable_g: DecimalValue,
    pub currency: String,
    pub supermarket_fallback: bool,
    #[serde(default)]
    pub resources_per_unit: BTreeMap<String, DecimalValue>,
    #[serde(default)]
    pub resource_capacity: BTreeMap<String, DecimalValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Task {
    pub key: String,
    pub food: String,
    pub process: String,
    pub kind: String,
    pub title: String,
    pub on: String,
    pub amount: DecimalValue,
    pub unit: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Projection {
    pub tasks: Vec<Task>,
    pub forecasts: Vec<Forecast>,
    pub gaps: Vec<String>,
    pub surplus_g: BTreeMap<String, f64>,
    pub earliest_harvest: BTreeMap<String, String>,
}

fn plus(at: NaiveDate, days: u64) -> Result<NaiveDate, String> {
    at.checked_add_days(Days::new(days))
        .ok_or_else(|| "Production date overflow".into())
}

pub fn validate(template: &Template, choice: &Choice, currency: &str) -> Result<(), String> {
    if !template.food_outputs.is_empty() && !template.food_outputs.contains(&choice.food) {
        return Err(
            "This production template does not declare the selected food preparation as an output"
                .into(),
        );
    }
    date(&choice.start_on)?;
    if template.harvest_count == 0
        || template.harvest_count > 3660
        || template.first_harvest_days > 36_600
        || template.harvest_interval_days > 3660
        || (template.harvest_count > 1 && template.harvest_interval_days == 0)
        || choice.lead_days_override.is_some_and(|d| d > 36_600)
        || choice.shelf_life_days > 3660
        || template.inputs.len() > 100
        || template.stages.len() > 100
        || choice.currency != currency
        || choice.season_months.is_empty()
        || choice.season_months.iter().any(|m| !(1..=12).contains(m))
        || choice.resources_per_unit.len() > 100
        || choice.resource_capacity.len() > 100
    {
        return Err("Check production duration, season, currency, and template limits".into());
    }
    if exact(&template.usable_g_per_harvest)?.to_f64() <= 0.0
        || choice.yield_override_g.is_some_and(|y| y.to_f64() <= 0.0)
        || choice.usable_yield.to_f64() <= 0.0
        || choice.cost_per_usable_g.to_f64() < 0.0
        || choice
            .resources_per_unit
            .values()
            .any(|v| v.to_f64() <= 0.0)
        || choice.resource_capacity.values().any(|v| v.to_f64() < 0.0)
    {
        return Err("Production yields and resource coefficients must be positive; cost and capacity may be zero".into());
    }
    for input in &template.inputs {
        if exact(&input.quantity)?.to_f64() < 0.0 || input.item.is_empty() || input.unit.is_empty()
        {
            return Err("Production inputs need names, units, and nonnegative quantities".into());
        }
    }
    for stage in &template.stages {
        if stage.title.len() > 2048
            || stage.offset_days > 73_200
            || stage.repeat_every_days == Some(0)
            || stage.until_days.is_some_and(|d| d > 73_200)
        {
            return Err("Invalid production task recurrence".into());
        }
    }
    Ok(())
}

fn task(
    projection: &mut Projection,
    food: &str,
    process: &str,
    kind: &str,
    title: String,
    at: NaiveDate,
    amount: f64,
    unit: &str,
    key: String,
) -> Result<(), String> {
    if projection.tasks.len() >= MAX_TASKS {
        return Err(
            "Production projection exceeds 10000 tasks; shorten the horizon or reduce selections"
                .into(),
        );
    }
    projection.tasks.push(Task {
        key,
        food: food.into(),
        process: process.into(),
        kind: kind.into(),
        title,
        on: at.to_string(),
        amount: decimal(amount)?,
        unit: unit.into(),
    });
    Ok(())
}

fn season(mut at: NaiveDate, months: &[u32]) -> Result<NaiveDate, String> {
    for _ in 0..367 {
        if months.contains(&at.month()) {
            return Ok(at);
        }
        at = plus(at, 1)?;
    }
    Err("No usable planting season".into())
}

pub fn project(
    input: &Input,
    outcome: &Outcome,
    choices: &[Choice],
    templates: &[Template],
    horizon_days: u16,
    buy_all: bool,
) -> Result<Projection, String> {
    input.validate()?;
    if horizon_days == 0
        || horizon_days > 3660
        || choices.len() > 600
        || templates.len() > 600
        || outcome.input_hash != fingerprint(input)?
        || !matches!(outcome.solution.status, Status::Optimal | Status::Feasible)
        || outcome.days.len() != usize::from(input.days)
    {
        return Err("Project only a current feasible meal pattern over 1–3660 days".into());
    }
    let start = date(&input.start)?;
    let end = plus(start, u64::from(horizon_days - 1))?;
    let mut projection = Projection::default();
    let mut choice_ids = std::collections::BTreeSet::new();
    for choice in choices {
        if !choice_ids.insert(&choice.food) {
            return Err("Choose one production process per food".into());
        }
    }
    for food in &input.foods {
        let id = &food.choice.food;
        let pattern: Vec<f64> = outcome
            .days
            .iter()
            .map(|d| d.foods_g.get(id).copied().unwrap_or(0.0))
            .collect();
        if pattern.iter().any(|g| !g.is_finite() || *g < 0.0) {
            return Err("Invalid recurring food quantities".into());
        }
        let daily = pattern.iter().sum::<f64>() / f64::from(input.days);
        let choice = choices.iter().find(|c| &c.food == id);
        let grow = !buy_all && food.choice.acquisition == Acquisition::Grow;
        let ongoing =
            choice.is_some_and(|c| c.confirmed && c.existing_units.is_some_and(|n| n > 0));
        if daily <= 1e-8 && !ongoing {
            continue;
        }
        let mut fallback = !grow;
        if grow || ongoing {
            if let Some(choice) = choice {
                fallback = !grow || choice.supermarket_fallback;
                if let Some(template) = templates.iter().find(|t| t.id == choice.template) {
                    validate(template, choice, &input.currency)?;
                    if choice.confirmed {
                        batches(&mut projection, input, template, choice, start, end, daily)?;
                    } else {
                        projection.gaps.push(format!(
                            "{id}: confirm or replace the production assumptions before planning"
                        ));
                    }
                } else {
                    projection
                        .gaps
                        .push(format!("{id}: choose a visible production template"));
                }
            } else {
                projection
                    .gaps
                    .push(format!("{id}: production parameters are missing"));
            }
        }
        let mut supplies: Vec<(NaiveDate, Option<NaiveDate>, f64)> = input
            .stock
            .iter()
            .filter(|s| &s.food == id)
            .map(|s| {
                Ok((
                    date(&s.available_on)?,
                    s.expires_on.as_deref().map(date).transpose()?,
                    s.usable_g.to_f64(),
                ))
            })
            .collect::<Result<_, String>>()?;
        let expected: Vec<_> = input
            .forecasts
            .iter()
            .chain(projection.forecasts.iter().filter(|proposed| {
                !input.forecasts.iter().any(|known| {
                    known.stock.food == proposed.stock.food
                        && known.process == proposed.process
                        && known.stock.available_on == proposed.stock.available_on
                })
            }))
            .filter(|f| &f.stock.food == id)
            .collect();
        if grow {
            for forecast in &expected {
                supplies.push((
                    date(&forecast.stock.available_on)?,
                    forecast.stock.expires_on.as_deref().map(date).transpose()?,
                    forecast.stock.usable_g.to_f64(),
                ));
            }
        }
        supplies.sort_by_key(|s| s.1.unwrap_or(NaiveDate::MAX));
        let mut needs = BTreeMap::<(u16, &'static str), f64>::new();
        for day in 0..horizon_days {
            let at = plus(start, u64::from(day))?;
            let mut remaining = pattern[usize::from(day % input.days)];
            for (from, until, amount) in &mut supplies {
                if at >= *from && until.is_none_or(|until| at <= until) {
                    let used = remaining.min(*amount);
                    remaining -= used;
                    *amount -= used;
                }
                if remaining <= 1e-8 {
                    break;
                }
            }
            if remaining > 1e-8 {
                *needs
                    .entry((day / 7 * 7, if fallback { "buy" } else { "unresolved" }))
                    .or_default() += remaining;
            }
        }
        projection.surplus_g.insert(
            id.clone(),
            supplies.iter().map(|s| s.2).sum::<f64>()
                + if grow {
                    0.0
                } else {
                    expected
                        .iter()
                        .map(|f| f.stock.usable_g.to_f64())
                        .sum::<f64>()
                },
        );
        for ((day, kind), amount) in needs {
            let at = plus(start, u64::from(day))?;
            task(
                &mut projection,
                id,
                "market",
                kind,
                format!(
                    "{} {}: {:.0} usable g over the next week",
                    if kind == "buy" {
                        "Acquire"
                    } else {
                        "Unresolved supply for"
                    },
                    food.label,
                    amount
                ),
                at,
                amount,
                "g",
                format!("{id}:{kind}:{at}"),
            )?;
        }
    }
    projection
        .tasks
        .sort_by(|a, b| (&a.on, &a.key).cmp(&(&b.on, &b.key)));
    Ok(projection)
}

fn batches(
    projection: &mut Projection,
    input: &Input,
    template: &Template,
    choice: &Choice,
    start: NaiveDate,
    end: NaiveDate,
    daily: f64,
) -> Result<(), String> {
    let lead = choice
        .lead_days_override
        .unwrap_or(template.first_harvest_days);
    let output = choice
        .yield_override_g
        .unwrap_or(exact(&template.usable_g_per_harvest)?)
        .to_f64()
        * choice.usable_yield.to_f64();
    let span = if template.harvest_count > 1 {
        u64::from(template.harvest_interval_days) * u64::from(template.harvest_count)
    } else {
        7
    };
    let wanted = (daily * span as f64 / (output * f64::from(template.harvest_count)))
        .ceil()
        .max(1.0);
    if wanted > 1e9 || !wanted.is_finite() {
        return Err("Required production count exceeds its limit".into());
    }
    let mut at = date(&choice.start_on)?;
    let existing = choice.existing_units;
    if existing.is_none() {
        at = season(at.max(start), &choice.season_months)?;
    }
    let mut active = Vec::<(NaiveDate, u32)>::new();
    let mut first = true;
    loop {
        if at > end {
            break;
        }
        if projection.tasks.len() >= MAX_TASKS {
            return Err("Production projection exceeds its task limit".into());
        }
        let first_harvest = plus(at, u64::from(lead))?;
        if first_harvest > end && !first {
            break;
        }
        let last_harvest = plus(
            first_harvest,
            u64::from(template.harvest_interval_days) * u64::from(template.harvest_count - 1),
        )?;
        let mut capacity = choice.max_units.unwrap_or(u32::MAX);
        for (resource, amount) in &choice.resources_per_unit {
            let Some(available) = choice.resource_capacity.get(resource) else {
                return Err(format!(
                    "Supply capacity for production resource {resource}"
                ));
            };
            capacity = capacity.min(
                (available.to_f64() / amount.to_f64())
                    .floor()
                    .max(0.0)
                    .min(f64::from(u32::MAX)) as u32,
            );
        }
        active.retain(|(until, _)| *until >= at);
        let occupied = active
            .iter()
            .map(|(_, n)| u64::from(*n))
            .sum::<u64>()
            .min(u64::from(u32::MAX)) as u32;
        let units = if first {
            existing.unwrap_or((wanted as u32).min(capacity.saturating_sub(occupied)))
        } else {
            (wanted as u32).min(capacity.saturating_sub(occupied))
        };
        if f64::from(units) < wanted {
            projection.gaps.push(format!(
                "{} on {at}: wanted {:.0} units; capacity permits {units}",
                choice.food, wanted
            ));
        }
        if units > 0 {
            let last_stage = template
                .stages
                .iter()
                .map(|s| s.until_days.unwrap_or(s.offset_days))
                .max()
                .unwrap_or(0);
            let care_end = if template.stages.iter().any(|s| s.ongoing) {
                end
            } else {
                last_harvest.max(plus(at, u64::from(last_stage))?)
            };
            active.push((care_end, units));
            let batch = format!("{}:{}:{at}", choice.food, template.id);
            if at >= start && existing.is_none() {
                let instruction = template
                    .stages
                    .iter()
                    .find(|s| s.kind == StageKind::Establish)
                    .map(|s| s.title.as_str())
                    .unwrap_or("Establish production");
                task(
                    projection,
                    &choice.food,
                    &template.id,
                    "establish",
                    format!(
                        "{instruction}: {units} units of @{} ({})",
                        template.species, template.method
                    ),
                    at,
                    f64::from(units),
                    "each",
                    format!("{batch}:establish"),
                )?;
            }
            let next = if first_harvest < start && template.harvest_interval_days > 0 {
                let elapsed = (start - first_harvest).num_days();
                let interval = i64::from(template.harvest_interval_days);
                ((elapsed + interval - 1) / interval) as u32
            } else {
                0
            };
            if next < template.harvest_count {
                let next = plus(
                    first_harvest,
                    u64::from(next) * u64::from(template.harvest_interval_days),
                )?
                .to_string();
                projection
                    .earliest_harvest
                    .entry(choice.food.clone())
                    .and_modify(|v| {
                        if next < *v {
                            *v = next.clone();
                        }
                    })
                    .or_insert(next);
            }
            for n in 0..template.harvest_count {
                let harvest = plus(
                    first_harvest,
                    u64::from(n) * u64::from(template.harvest_interval_days),
                )?;
                if harvest < start || harvest > end {
                    continue;
                }
                let quantity = output * f64::from(units);
                let key = format!("{batch}:harvest:{n}");
                let until = plus(harvest, u64::from(choice.shelf_life_days))?;
                projection.forecasts.push(Forecast {
                    stock: Stock {
                        id: key.clone(),
                        food: choice.food.clone(),
                        usable_g: decimal(quantity)?,
                        available_on: harvest.to_string(),
                        expires_on: Some(until.to_string()),
                    },
                    cost_per_usable_g: choice.cost_per_usable_g,
                    currency: input.currency.clone(),
                    process: template.id.clone(),
                });
                let instruction = template
                    .stages
                    .iter()
                    .find(|s| s.kind == StageKind::Harvest)
                    .map(|s| s.title.as_str())
                    .unwrap_or("Inspect maturity; harvest and weigh");
                task(
                    projection,
                    &choice.food,
                    &template.id,
                    "harvest",
                    format!("{instruction}: @{}", template.species),
                    harvest,
                    quantity,
                    "g",
                    key,
                )?;
            }
            for (index, stage) in template
                .stages
                .iter()
                .enumerate()
                .filter(|(_, stage)| stage.kind == StageKind::Care)
            {
                let mut offset = u64::from(stage.offset_days);
                let stop = if stage.ongoing {
                    (end - at).num_days().max(0) as u64
                } else {
                    u64::from(
                        stage
                            .until_days
                            .unwrap_or(
                                lead + template
                                    .harvest_interval_days
                                    .saturating_mul(template.harvest_count - 1),
                            )
                            .max(stage.offset_days),
                    )
                };
                if let Some(every) = stage.repeat_every_days {
                    let elapsed = (start - at).num_days().max(0) as u64;
                    if elapsed > offset {
                        let every = u64::from(every);
                        offset += (elapsed - offset).div_ceil(every) * every;
                    }
                }
                loop {
                    let on = plus(at, offset)?;
                    if on > end || offset > stop {
                        break;
                    }
                    if on >= start {
                        task(
                            projection,
                            &choice.food,
                            &template.id,
                            "care",
                            stage.title.clone(),
                            on,
                            f64::from(units),
                            "each",
                            format!("{batch}:stage:{index}:{offset}"),
                        )?;
                    }
                    let Some(every) = stage.repeat_every_days else {
                        break;
                    };
                    offset += u64::from(every);
                }
            }
            for (index, material) in template.inputs.iter().enumerate() {
                let per_day = material.unit.strip_suffix("/day");
                if per_day.is_none() && at < start {
                    continue;
                }
                let mut on = at.max(start);
                loop {
                    if on > end || on > care_end {
                        break;
                    }
                    let days = if per_day.is_some() {
                        (end.min(care_end) - on).num_days().min(6) as f64 + 1.0
                    } else {
                        1.0
                    };
                    let quantity = exact(&material.quantity)?.to_f64() * f64::from(units) * days;
                    task(
                        projection,
                        &choice.food,
                        &template.id,
                        "input",
                        format!("Acquire {} for @{}", material.item, template.species),
                        on,
                        quantity,
                        per_day.unwrap_or(&material.unit),
                        format!("{batch}:input:{index}:{on}"),
                    )?;
                    if per_day.is_none() {
                        break;
                    }
                    on = plus(on, 7)?;
                }
            }
        }
        if existing.is_some() || span > (end - at).num_days().max(0) as u64 {
            break;
        }
        at = season(plus(at, span)?, &choice.season_months)?;
        first = false;
    }
    Ok(())
}
