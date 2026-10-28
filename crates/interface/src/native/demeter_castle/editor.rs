use super::*;
use engine::demeter::OfferDraft;
use nucleus::{
    DecimalValue,
    demeter::{FoodBound, Forecast, NutrientBound, Stock, Unit, production},
};

fn optional(value: &str) -> Result<Option<DecimalValue>, String> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        demeter::exact(value.trim()).map(Some)
    }
}
fn required(value: &str) -> Result<DecimalValue, String> {
    optional(value)?.ok_or_else(|| "Enter a number".into())
}
pub(super) fn key(s: &DemeterCastle, key: &str) -> String {
    if ["offer:", "stock:", "production:", "forecast:", "resource:"]
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        if let Some(food) = &s.selected_food {
            return format!("{food}|{key}");
        }
    }
    key.into()
}
pub(super) fn text<'a>(s: &'a DemeterCastle, name: &str) -> &'a str {
    s.drafts
        .get(&key(s, name))
        .map(String::as_str)
        .unwrap_or("")
}

pub(super) fn retain_auxiliary(drafts: &mut BTreeMap<String, String>) {
    drafts.retain(|key, _| {
        key == "search"
            || key.starts_with("new:")
            || key.ends_with(":fixed")
            || ["|offer:", "|stock:", "|forecast:", "|resource:"]
                .iter()
                .any(|part| key.contains(part))
    });
}
fn read_number(s: &DemeterCastle, key: &str) -> Result<DecimalValue, String> {
    required(text(s, key)).map_err(|e| format!("{key}: {e}"))
}
fn read_optional(s: &DemeterCastle, key: &str) -> Result<Option<DecimalValue>, String> {
    optional(text(s, key)).map_err(|e| format!("{key}: {e}"))
}
fn integer(s: &DemeterCastle, key: &str) -> Result<u32, String> {
    text(s, key)
        .trim()
        .parse()
        .map_err(|_| format!("{key}: enter a whole nonnegative number"))
}

pub(super) fn requirements(state: &DemeterCastle) -> Result<Requirements, String> {
    let mut r = if let Some(profile) = &state.config.profile {
        let refs = state.catalog["references"]
            .as_array()
            .ok_or("Refresh the catalogue after importing Nutrition Needs")?;
        let refs: Vec<_> = refs
            .iter()
            .filter(|v| state.config.references.iter().any(|uid| v["uid"] == *uid))
            .map(|v| serde_json::from_value(v["reference"].clone()).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        if refs.is_empty() {
            return Err("Import Nutrition Needs in Sync, then refresh the catalogue".into());
        }
        demeter::select_references(
            &refs,
            profile,
            &state.config.overrides,
            &state.config.disabled_nutrients,
        )?
    } else {
        let mut r = state.config.manual.clone();
        for b in &state.config.overrides {
            r.bounds.retain(|v| v.nutrient != b.nutrient);
            r.bounds.push(b.clone());
        }
        r
    };
    if let Some(shares) = &state.config.energy_share_overrides {
        r.energy_shares = shares.clone();
    }
    Ok(r)
}

pub(super) fn capture(world: &mut World, owner: Entity) -> Result<(), String> {
    let Some(view) = world.get::<View>(owner) else {
        return Ok(());
    };
    let values: Vec<_> = view
        .fields
        .iter()
        .filter_map(|(key, e)| panel::value(world, *e).ok().map(|v| (key.clone(), v)))
        .collect();
    let initial = view.initial.clone();
    let bounds = view.bounds.clone();
    let mut state = world.get::<DemeterCastle>(owner).unwrap().clone();
    let before = serde_json::to_value(&state.config).unwrap();
    let edited = values.iter().any(|(k, v)| initial.get(k) != Some(v));
    for (k, v) in values {
        state.drafts.insert(k, v);
    }
    if edited {
        state.edit_version = state.edit_version.saturating_add(1);
    }
    let semantic_edit = state.drafts.iter().any(|(key, value)| {
        initial.get(key).is_some_and(|initial| initial != value)
            && (key.starts_with("nutrient:")
                || key.starts_with("share:")
                || key.starts_with("food:")
                || key.contains("|production:")
                || [
                    "name", "start", "days", "currency", "age", "weight", "height", "activity",
                    "energy", "volume", "horizon",
                ]
                .contains(&key.as_str()))
    });
    if semantic_edit {
        state.changed = true;
        state.stale = true;
    }
    world.entity_mut(owner).insert(state.clone());
    if state.drafts.contains_key("name") {
        state.config.name = text(&state, "name").into();
        state.config.start = text(&state, "start").into();
        state.config.currency = text(&state, "currency").into();
        state.config.days = integer(&state, "days")?
            .try_into()
            .map_err(|_| "Food period is too long")?;
    }
    if state.drafts.contains_key("budget") {
        state.budget_ms = u64::from(integer(&state, "budget")?)
            .checked_mul(1000)
            .ok_or("Solve budget overflow")?;
        if state.budget_ms > 30_000 {
            return Err("Solve budget must be at most 30 seconds".into());
        }
    }
    if state.drafts.contains_key("volume") {
        state.config.volume_ml_per_day = read_optional(&state, "volume")?;
    }
    if state.config.profile.is_some() && state.drafts.contains_key("age") {
        let old = state.config.profile.as_ref().unwrap();
        let group = old.group;
        state.config.profile = Some(Profile {
            age: integer(&state, "age")?
                .try_into()
                .map_err(|_| "Age is outside the adult reference range")?,
            group,
            weight_kg: read_number(&state, "weight")?,
            height_cm: read_number(&state, "height")?,
            activity_factor: read_number(&state, "activity")?,
            energy_override_kcal: read_optional(&state, "energy")?,
        });
    }
    for bound in &bounds.bounds {
        let lower = format!("nutrient:{}:lower", bound.nutrient);
        let upper = format!("nutrient:{}:upper", bound.nutrient);
        let changed = [&lower, &upper]
            .iter()
            .any(|k| state.drafts.get(*k) != initial.get(*k));
        if changed && state.drafts.contains_key(&lower) {
            let mut b = bound.clone();
            b.bounds = Bounds {
                lower: read_optional(&state, &lower)?,
                upper: read_optional(&state, &upper)?,
            };
            b.bounds.validate()?;
            b.source = "Personal override".into();
            state.config.overrides.retain(|v| v.nutrient != b.nutrient);
            state.config.overrides.push(b);
        }
    }
    let mut shares = bounds.energy_shares.clone();
    let mut share_changed = false;
    for share in &mut shares {
        let lo = format!("share:{}:lower", share.nutrient);
        let hi = format!("share:{}:upper", share.nutrient);
        if [&lo, &hi]
            .iter()
            .any(|k| state.drafts.get(*k) != initial.get(*k))
        {
            share.lower =
                nucleus::optimization::decimal(read_number(&state, &lo)?.to_f64() / 100.0)?;
            share.upper =
                nucleus::optimization::decimal(read_number(&state, &hi)?.to_f64() / 100.0)?;
            share_changed = true;
        }
    }
    if share_changed {
        state.config.energy_share_overrides = Some(shares);
    }
    if state.drafts.contains_key("search") {
        state.search = text(&state, "search").into();
    }
    if let Some(selected) = &state.selected_food {
        if let Some(index) = state.config.foods.iter().position(|f| &f.food == selected) {
            let mut food = state.config.foods[index].clone();
            for (period, name) in [
                (Period::Day, "day"),
                (Period::Week, "week"),
                (Period::Horizon, "period"),
            ] {
                let lo = format!("food:{selected}:{name}:lower");
                let hi = format!("food:{selected}:{name}:upper");
                if state.drafts.contains_key(&lo) {
                    let bounds = Bounds {
                        lower: read_optional(&state, &lo)?,
                        upper: read_optional(&state, &hi)?,
                    };
                    bounds.validate()?;
                    food.bounds.retain(|b| b.period != period);
                    if bounds.lower.is_some() || bounds.upper.is_some() {
                        food.bounds.push(FoodBound { period, bounds });
                    }
                }
            }
            let density = format!("food:{selected}:volume");
            if state.drafts.contains_key(&density) {
                food.volume_ml_per_g = read_optional(&state, &density)?;
            }
            state.config.foods[index] = food;
        }
        if state.drafts.contains_key(&key(&state, "production:start")) {
            if let Some(index) = state
                .config
                .production
                .iter()
                .position(|c| &c.food == selected)
            {
                let mut choice = state.config.production[index].clone();
                choice.start_on = text(&state, "production:start").into();
                choice.yield_override_g = read_optional(&state, "production:yield")?;
                choice.lead_days_override = if text(&state, "production:lead").is_empty() {
                    None
                } else {
                    Some(integer(&state, "production:lead")?)
                };
                choice.max_units = if text(&state, "production:capacity").is_empty() {
                    None
                } else {
                    Some(integer(&state, "production:capacity")?)
                };
                choice.existing_units = if text(&state, "production:existing").is_empty() {
                    None
                } else {
                    Some(integer(&state, "production:existing")?)
                };
                choice.usable_yield = read_number(&state, "production:usable")?;
                choice.shelf_life_days = integer(&state, "production:shelf")?;
                choice.cost_per_usable_g = read_number(&state, "production:cost")?;
                choice.season_months = text(&state, "production:months")
                    .split(',')
                    .map(|s| {
                        s.trim()
                            .parse::<u32>()
                            .map_err(|_| "Enter month numbers separated by commas".to_owned())
                    })
                    .collect::<Result<_, _>>()?;
                choice.currency = state.config.currency.clone();
                state.config.production[index] = choice;
            }
        }
    }
    if state.drafts.contains_key("horizon") {
        state.config.horizon_days = integer(&state, "horizon")?
            .try_into()
            .map_err(|_| "Projection horizon is too long")?;
    }
    if serde_json::to_value(&state.config).unwrap() != before {
        state.changed = true;
        state.stale = true;
    }
    state.config.validate()?;
    if !state.valid() {
        return Err("Editor state exceeds its limits".into());
    }
    world.entity_mut(owner).insert(state);
    Ok(())
}

pub(super) fn profile_mode(world: &mut World, owner: Entity, enabled: bool) {
    let mut state = world.get::<DemeterCastle>(owner).unwrap().clone();
    if enabled {
        if state.config.profile.is_none() {
            let mut profile = DemeterCastle::default().config.profile.unwrap();
            if let Ok(age) = integer(&state, "age") {
                if let Ok(age) = age.try_into() {
                    profile.age = age;
                }
            }
            if let Ok(v) = read_number(&state, "weight") {
                profile.weight_kg = v;
            }
            if let Ok(v) = read_number(&state, "height") {
                profile.height_cm = v;
            }
            if let Ok(v) = read_number(&state, "activity") {
                profile.activity_factor = v;
            }
            if let Ok(v) = read_optional(&state, "energy") {
                profile.energy_override_kcal = v;
            }
            if text(&state, "profile:group") == "female" {
                profile.group = ReferenceGroup::Female;
            }
            state.config.profile = Some(profile);
        }
        if state.config.references.is_empty() {
            state.config.references = state.catalog["references"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r["uid"].as_str().map(str::to_owned))
                .collect();
        }
    } else {
        if let Ok(r) = requirements(&state) {
            state.config.manual = r;
        }
        if let Some(profile) = &state.config.profile {
            state.drafts.insert(
                "profile:group".into(),
                if profile.group == ReferenceGroup::Female {
                    "female"
                } else {
                    "male"
                }
                .into(),
            );
        }
        state.config.profile = None;
        state.config.overrides.clear();
        state.config.disabled_nutrients.clear();
    }
    state.changed = true;
    state.stale = true;
    state
        .drafts
        .retain(|k, _| !k.starts_with("nutrient:") && !k.starts_with("share:"));
    world.entity_mut(owner).insert(state);
}

pub(super) fn operation(
    world: &mut World,
    owner: Entity,
    operation: &Operation,
) -> Result<(), String> {
    let mut state = world.get::<DemeterCastle>(owner).unwrap().clone();
    let selected = state.selected_food.clone();
    match operation {
        Operation::Fixed => {
            let food = selected.ok_or("Select a food first")?;
            let value = read_number(&state, &format!("food:{food}:fixed"))?;
            if value.to_f64() < 0.0 {
                return Err("Fixed grams cannot be negative".into());
            }
            let selected = state
                .config
                .foods
                .iter_mut()
                .find(|f| f.food == food)
                .ok_or("Select a food first")?;
            selected.bounds.retain(|b| b.period != Period::Day);
            selected.bounds.push(FoodBound {
                period: Period::Day,
                bounds: Bounds {
                    lower: Some(value),
                    upper: Some(value),
                },
            });
            state
                .drafts
                .retain(|k, _| !k.starts_with(&format!("food:{food}:day:")));
        }
        Operation::AddNutrient => {
            let field = text(&state, "new:nutrient").trim().to_owned();
            let unit = Unit::for_field(&field)?;
            let bound = NutrientBound {
                nutrient: field.clone(),
                unit,
                bounds: Bounds {
                    lower: read_optional(&state, "new:lower")?,
                    upper: read_optional(&state, "new:upper")?,
                },
                scope: "total".into(),
                source: "Personal target".into(),
            };
            state.config.overrides.retain(|b| b.nutrient != field);
            state.config.overrides.push(bound);
        }
        Operation::SaveOffer => {
            let food = selected.ok_or("Select a food preparation first")?;
            let offer_key = key(&state, "offer:id");
            let id = state
                .drafts
                .entry(offer_key)
                .or_insert_with(|| nucleus::new_uid("r"))
                .clone();
            let draft = OfferDraft {
                id,
                food,
                currency: state.config.currency.clone(),
                price_predicate: state.catalog["vocabulary"]["price"]
                    .as_str()
                    .unwrap_or("price")
                    .into(),
                price: read_number(&state, "offer:price")?,
                package_g: read_number(&state, "offer:package")?,
                usable_yield: read_number(&state, "offer:yield")?,
                vendor: text(&state, "offer:vendor").into(),
                observed_on: text(&state, "offer:observed").into(),
                source_url: text(&state, "offer:url").into(),
                available_on: text(&state, "offer:available").into(),
                valid_until: (!text(&state, "offer:until").is_empty())
                    .then(|| text(&state, "offer:until").into()),
                max_packages: read_optional(&state, "offer:limit")?,
            };
            world.entity_mut(owner).insert(state);
            request(
                world,
                owner,
                Command::SaveOffer { offer: draft },
                Reply::Offer,
            );
            return Ok(());
        }
        Operation::AddStock => {
            let food = selected.ok_or("Select a food first")?;
            state.config.stock.push(Stock {
                id: nucleus::new_uid("r"),
                food,
                usable_g: read_number(&state, "stock:grams")?,
                available_on: text(&state, "stock:from").into(),
                expires_on: (!text(&state, "stock:until").is_empty())
                    .then(|| text(&state, "stock:until").into()),
            });
        }
        Operation::Template(id) => {
            let food = selected.ok_or("Select a food first")?;
            let item = state.catalog["templates"]
                .as_array()
                .and_then(|v| v.iter().find(|v| v["uid"] == *id))
                .ok_or("Refresh production templates")?;
            let template: production::Template =
                serde_json::from_value(item["template"].clone()).map_err(|e| e.to_string())?;
            state.config.production.retain(|c| c.food != food);
            state.config.production.push(production::Choice {
                food,
                template: id.clone(),
                confirmed: false,
                start_on: state.config.start.clone(),
                yield_override_g: Some(demeter::exact(&template.usable_g_per_harvest)?),
                lead_days_override: Some(template.first_harvest_days),
                season_months: template.season_months,
                usable_yield: demeter::exact("1")?,
                shelf_life_days: 7,
                max_units: None,
                existing_units: None,
                cost_per_usable_g: demeter::exact("0")?,
                currency: state.config.currency.clone(),
                supermarket_fallback: true,
                resources_per_unit: BTreeMap::new(),
                resource_capacity: BTreeMap::new(),
            });
            state.drafts.retain(|k, _| {
                !k.starts_with(&format!(
                    "{}|production:",
                    state.selected_food.as_deref().unwrap_or_default()
                ))
            });
        }
        Operation::ConfirmProduction => {
            let selected = selected.ok_or("Select a food first")?;
            let choice = state
                .config
                .production
                .iter_mut()
                .find(|c| c.food == selected)
                .ok_or("Choose a production template")?;
            choice.confirmed = !choice.confirmed;
        }
        Operation::AddForecast => {
            let food = selected.ok_or("Select a food first")?;
            let choice = state
                .config
                .production
                .iter()
                .find(|c| c.food == food)
                .ok_or("Choose a production process first")?;
            if !choice.confirmed {
                return Err(
                    "Confirm the process parameters before declaring a harvest forecast".into(),
                );
            }
            state.config.forecasts.push(Forecast {
                stock: Stock {
                    id: nucleus::new_uid("r"),
                    food,
                    usable_g: read_number(&state, "forecast:grams")?,
                    available_on: text(&state, "forecast:from").into(),
                    expires_on: (!text(&state, "forecast:until").is_empty())
                        .then(|| text(&state, "forecast:until").into()),
                },
                cost_per_usable_g: choice.cost_per_usable_g,
                currency: state.config.currency.clone(),
                process: choice.template.clone(),
            });
        }
        Operation::AddResource => {
            let food = selected.ok_or("Select a food first")?;
            let name = text(&state, "resource:name").trim().to_owned();
            if name.is_empty() {
                return Err("Name the resource and its unit, such as bed-m2".into());
            }
            let per = read_number(&state, "resource:per")?;
            let capacity = read_number(&state, "resource:capacity")?;
            let choice = state
                .config
                .production
                .iter_mut()
                .find(|c| c.food == food)
                .ok_or("Choose a production process first")?;
            choice.resources_per_unit.insert(name.clone(), per);
            choice.resource_capacity.insert(name, capacity);
        }
        _ => return Err("Unsupported editor operation".into()),
    }
    state.changed = true;
    state.stale = true;
    state.edit_version = state.edit_version.saturating_add(1);
    world.entity_mut(owner).insert(state);
    Ok(())
}
