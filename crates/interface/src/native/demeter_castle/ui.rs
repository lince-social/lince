use super::*;
use nucleus::demeter::{Unit, production};

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    caption: &str,
    action: Operation,
) -> Entity {
    panel::button(world, parent, owner, caption, action)
}

fn choice_button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    caption: &str,
    action: Operation,
) -> Entity {
    let entity = panel::button(world, parent, owner, caption, action);
    {
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
        node.flex_direction = FlexDirection::Column;
    }
    let children = world.get::<Children>(entity).unwrap().to_vec();
    for child in children {
        if let Some(mut node) = world.get_mut::<Node>(child) {
            node.width = percent(100);
            node.min_width = px(0);
        }
    }
    entity
}

fn label(world: &mut World, parent: Entity, text: impl AsRef<str>) {
    crate::native::edit_mode::label(world, parent, text.as_ref(), 13.0);
}
fn row(world: &mut World, parent: Entity) -> Entity {
    let row = panel::row(world, parent);
    world.get_mut::<Node>(row).unwrap().flex_wrap = FlexWrap::Wrap;
    row
}
fn field(world: &mut World, owner: Entity, name: &str, caption: &str, value: impl ToString) {
    let state = world.get::<DemeterCastle>(owner).unwrap();
    let key = editor::key(state, name);
    let value = state
        .drafts
        .get(&key)
        .cloned()
        .unwrap_or_else(|| value.to_string());
    let parent = world.get::<View>(owner).unwrap().body;
    let entity = panel::field(world, parent, caption, &value);
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.width = percent(100);
        node.min_width = px(0);
        node.flex_shrink = 1.0;
    }
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.fields.insert(key.clone(), entity);
    view.initial.insert(key, value);
}
fn number(value: Option<nucleus::DecimalValue>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}
fn unit(value: Unit) -> &'static str {
    match value {
        Unit::G => "g",
        Unit::Mg => "mg",
        Unit::Ug => "ug",
        Unit::Kcal => "kcal",
        Unit::Kj => "kJ",
    }
}
fn food_label(state: &DemeterCastle, uid: &str) -> String {
    state.catalog["foods"]
        .as_array()
        .and_then(|rows| rows.iter().find(|v| v["uid"] == uid))
        .and_then(|v| v["label"].as_str())
        .unwrap_or(uid)
        .into()
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let body = view.body;
    panel::clear(world, body);
    {
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.fields.clear();
        view.initial.clear();
        view.bounds = Requirements::default();
    }
    let state = world.get::<DemeterCastle>(owner).unwrap().clone();
    let r = row(world, body);
    button(world, r, owner, "New plan", Operation::New);
    if let Some(plans) = state.catalog["plans"].as_array() {
        for p in plans {
            if let (Some(id), Some(name)) = (p["uid"].as_str(), p["name"].as_str()) {
                choice_button(world, r, owner, name, Operation::Open(id.into()));
            }
        }
    }
    if state.changed {
        label(
            world,
            body,
            "Unsaved edits. Save and solve before accepting tasks.",
        );
    } else if state.stale {
        label(
            world,
            body,
            "The previous result is stale. Solve the current inputs.",
        );
    }
    match state.tab {
        Tab::Requirements => requirements(world, owner, &state),
        Tab::Foods => foods(world, owner, &state),
        Tab::Results => results(world, owner, &state),
        Tab::Production => production(world, owner, &state),
    }
}

fn requirements(world: &mut World, owner: Entity, state: &DemeterCastle) {
    let body = world.get::<View>(owner).unwrap().body;
    field(world, owner, "name", "Plan name", &state.config.name);
    field(
        world,
        owner,
        "start",
        "First meal date (YYYY-MM-DD)",
        &state.config.start,
    );
    field(
        world,
        owner,
        "days",
        "Food-planning days (1–31)",
        state.config.days,
    );
    field(
        world,
        owner,
        "currency",
        "Currency name or unit identity",
        &state.config.currency,
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Create local price/currency units",
        Operation::Vocabulary,
    );
    label(
        world,
        body,
        "Healthy-adult references exclude pregnancy and lactation. Example measurements must be edited before use. Manual targets remain available.",
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Adult reference profile",
        Operation::Profile(true),
    );
    button(world, r, owner, "Manual targets", Operation::Profile(false));
    button(
        world,
        r,
        owner,
        "Reset personal overrides",
        Operation::ResetOverrides,
    );
    if let Some(profile) = &state.config.profile {
        let r = row(world, body);
        button(
            world,
            r,
            owner,
            "Male reference",
            Operation::Group(ReferenceGroup::Male),
        );
        button(
            world,
            r,
            owner,
            "Female reference",
            Operation::Group(ReferenceGroup::Female),
        );
        label(
            world,
            body,
            format!("Selected reference: {:?}", profile.group),
        );
        field(world, owner, "age", "Age (years)", profile.age);
        field(
            world,
            owner,
            "weight",
            "Body weight (kg)",
            profile.weight_kg,
        );
        field(world, owner, "height", "Height (cm)", profile.height_cm);
        field(
            world,
            owner,
            "activity",
            "Explicit activity factor",
            profile.activity_factor,
        );
        field(
            world,
            owner,
            "energy",
            "Energy override (kcal/day; blank uses estimate)",
            number(profile.energy_override_kcal),
        );
        if let Ok(energy) = profile.energy() {
            label(
                world,
                body,
                format!(
                    "Estimated daily energy: {:.0} kcal. Mifflin–St Jeor resting estimate × your activity factor.",
                    energy.to_f64()
                ),
            );
        }
    }
    label(
        world,
        body,
        "Daily nutrient bounds: blank minimum omits it; blank maximum is unlimited. Missing and trace measurements contribute zero and remain visible in results.",
    );
    match editor::requirements(state) {
        Ok(requirements) => {
            world.get_mut::<View>(owner).unwrap().bounds = requirements.clone();
            for bound in &requirements.bounds {
                label(
                    world,
                    body,
                    format!(
                        "{} — {} / day ({})",
                        bound.nutrient.replace('_', " "),
                        unit(bound.unit),
                        bound.scope
                    ),
                );
                field(
                    world,
                    owner,
                    &format!("nutrient:{}:lower", bound.nutrient),
                    "Minimum",
                    number(bound.bounds.lower),
                );
                field(
                    world,
                    owner,
                    &format!("nutrient:{}:upper", bound.nutrient),
                    "Maximum",
                    number(bound.bounds.upper),
                );
                label(world, body, &bound.source);
                if let Some(reference) = state.catalog["references"]
                    .as_array()
                    .and_then(|refs| {
                        refs.iter()
                            .find(|r| r["reference"]["nutrient"] == bound.nutrient)
                    })
                    .and_then(|r| r["uid"].as_str())
                {
                    let r = row(world, body);
                    button(
                        world,
                        r,
                        owner,
                        "Reference source",
                        Operation::Reference(reference.into()),
                    );
                }
                label(
                    world,
                    body,
                    format!(
                        "Seven-day reference total: minimum {}, maximum {} {}. Daily bounds still apply each day.",
                        bound
                            .bounds
                            .lower
                            .map(|v| format!("{:.3}", v.to_f64() * 7.0))
                            .unwrap_or_else(|| "omitted".into()),
                        bound
                            .bounds
                            .upper
                            .map(|v| format!("{:.3}", v.to_f64() * 7.0))
                            .unwrap_or_else(|| "unlimited".into()),
                        unit(bound.unit)
                    ),
                );
                let r = row(world, body);
                button(
                    world,
                    r,
                    owner,
                    "Omit this target",
                    Operation::ToggleNutrient(bound.nutrient.clone()),
                );
            }
            for share in &requirements.energy_shares {
                label(
                    world,
                    body,
                    format!(
                        "{} — share of actual daily energy",
                        share.nutrient.replace('_', " ")
                    ),
                );
                field(
                    world,
                    owner,
                    &format!("share:{}:lower", share.nutrient),
                    "Minimum energy share (%)",
                    format!("{:.3}", share.lower.to_f64() * 100.0),
                );
                field(
                    world,
                    owner,
                    &format!("share:{}:upper", share.nutrient),
                    "Maximum energy share (%)",
                    format!("{:.3}", share.upper.to_f64() * 100.0),
                );
                let r = row(world, body);
                button(
                    world,
                    r,
                    owner,
                    "Remove energy-share constraint",
                    Operation::RemoveShare(share.nutrient.clone()),
                );
            }
        }
        Err(error) => label(world, body, error),
    }
    for field in &state.config.disabled_nutrients {
        let r = row(world, body);
        button(
            world,
            r,
            owner,
            &format!("Restore {field}"),
            Operation::ToggleNutrient(field.clone()),
        );
    }
    field(
        world,
        owner,
        "new:nutrient",
        "Additional measured nutrient field (for example iodine_ug)",
        "",
    );
    field(world, owner, "new:lower", "Additional daily minimum", "");
    field(world, owner, "new:upper", "Additional daily maximum", "");
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Add personal target",
        Operation::AddNutrient,
    );
    field(
        world,
        owner,
        "volume",
        "Container capacity per day (mL; optional)",
        number(state.config.volume_ml_per_day),
    );
    label(
        world,
        body,
        "A volume limit requires measured mL per usable gram for every selected food.",
    );
}

fn foods(world: &mut World, owner: Entity, state: &DemeterCastle) {
    let body = world.get::<View>(owner).unwrap().body;
    field(
        world,
        owner,
        "search",
        "Search food preparations",
        &state.search,
    );
    let r = row(world, body);
    button(world, r, owner, "Search", Operation::Search);
    label(world, body, "Selected foods");
    for food in &state.config.foods {
        let r = row(world, body);
        choice_button(
            world,
            r,
            owner,
            &food_label(state, &food.food),
            Operation::Select(food.food.clone()),
        );
        button(
            world,
            r,
            owner,
            "Remove",
            Operation::Remove(food.food.clone()),
        );
    }
    if let Some(items) = state.catalog["foods"].as_array() {
        let query = state.search.to_lowercase();
        for item in items
            .iter()
            .filter(|v| {
                v["label"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
            })
            .take(30)
        {
            if let Some(uid) = item["uid"].as_str() {
                let r = row(world, body);
                choice_button(
                    world,
                    r,
                    owner,
                    item["label"].as_str().unwrap_or(uid),
                    Operation::Select(uid.into()),
                );
            }
        }
        label(
            world,
            body,
            "Showing up to 30 matches. Search to narrow the catalogue.",
        );
    }
    let Some(id) = &state.selected_food else {
        label(
            world,
            body,
            "Select a food to set its bounds, offers, and stock.",
        );
        return;
    };
    let Some(food) = state.config.foods.iter().find(|f| &f.food == id) else {
        return;
    };
    label(
        world,
        body,
        format!("Preparation: {}", food_label(state, id)),
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Food composition and source",
        Operation::Reference(id.clone()),
    );
    label(
        world,
        body,
        "All amounts below are usable grams in this exact preparation. Maximum zero excludes a food; equal minimum and maximum fix its amount. Weekly bounds use seven-day blocks from the start date; a partial block is prorated.",
    );
    for (period, name, caption) in [
        (Period::Day, "day", "Per day"),
        (Period::Week, "week", "Per week"),
        (Period::Horizon, "period", "Over the food-planning period"),
    ] {
        let bounds = food
            .bounds
            .iter()
            .find(|b| b.period == period)
            .map(|b| b.bounds.clone())
            .unwrap_or_default();
        field(
            world,
            owner,
            &format!("food:{id}:{name}:lower"),
            &format!("{caption} — minimum g"),
            number(bounds.lower),
        );
        field(
            world,
            owner,
            &format!("food:{id}:{name}:upper"),
            &format!("{caption} — maximum g"),
            number(bounds.upper),
        );
    }
    field(
        world,
        owner,
        &format!("food:{id}:volume"),
        "Measured volume (mL per usable g; optional)",
        number(food.volume_ml_per_g),
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Use supermarket",
        Operation::Acquire(Acquisition::Buy),
    );
    button(
        world,
        r,
        owner,
        "Plan growing",
        Operation::Acquire(Acquisition::Grow),
    );
    label(world, body, format!("Acquisition: {:?}", food.acquisition));
    let excluded = food
        .bounds
        .iter()
        .any(|b| b.period == Period::Day && b.bounds.upper.is_some_and(|u| u.to_f64() == 0.0));
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        if excluded {
            "Allow this food"
        } else {
            "Exclude this food"
        },
        Operation::Exclude(!excluded),
    );
    field(
        world,
        owner,
        &format!("food:{id}:fixed"),
        "Fixed daily amount (g)",
        "",
    );
    let r = row(world, body);
    button(world, r, owner, "Fix daily amount", Operation::Fixed);
    label(
        world,
        body,
        "Local offers — prices are personal assertions. A new observation keeps the old offer in history. Explicit zero prices are allowed.",
    );
    if let Some(offers) = state.catalog["offers"].as_array() {
        for info in offers
            .iter()
            .filter(|v| v["market"]["offer"]["food"] == *id)
        {
            let market = &info["market"];
            let offer = &market["offer"];
            let uid = info["uid"].as_str().unwrap_or_default();
            let price = serde_json::from_value::<nucleus::DecimalValue>(offer["price"].clone())
                .map(|v| v.to_string())
                .unwrap_or_default();
            let currency = state.catalog["vocabulary"]
                .as_object()
                .and_then(|units| {
                    units
                        .iter()
                        .find(|(_, uid)| **uid == offer["currency"])
                        .map(|(name, _)| name.as_str())
                })
                .unwrap_or_else(|| offer["currency"].as_str().unwrap_or("currency"));
            let package =
                serde_json::from_value::<nucleus::DecimalValue>(offer["package_g"].clone())
                    .map(|v| v.to_string())
                    .unwrap_or_default();
            let usable =
                serde_json::from_value::<nucleus::DecimalValue>(offer["usable_yield"].clone())
                    .map(|v| v.to_string())
                    .unwrap_or_default();
            let caption = format!(
                "{} {} — {} {} / {package} purchased g × {usable} usable g/g ({}){}",
                if state.config.offers.iter().any(|v| v == uid) {
                    "Using"
                } else {
                    "Use"
                },
                market["vendor"].as_str().unwrap_or("offer"),
                price,
                currency,
                market["observed_on"].as_str().unwrap_or_default(),
                if info["valid"] == false {
                    " — invalid price assertion"
                } else {
                    ""
                }
            );
            let r = row(world, body);
            if info["valid"] != false || state.config.offers.iter().any(|v| v == uid) {
                choice_button(world, r, owner, &caption, Operation::UseOffer(uid.into()));
            } else {
                label(world, r, caption);
            }
        }
    }
    for uid in &state.config.offers {
        if !state.catalog["offers"]
            .as_array()
            .is_some_and(|offers| offers.iter().any(|o| o["uid"] == *uid))
        {
            let r = row(world, body);
            choice_button(
                world,
                r,
                owner,
                &format!("Remove unavailable offer {uid}"),
                Operation::UseOffer(uid.clone()),
            );
        }
    }
    field(world, owner, "offer:vendor", "Seller / market", "");
    field(
        world,
        owner,
        "offer:price",
        "Package price in the plan currency (required)",
        "",
    );
    field(
        world,
        owner,
        "offer:package",
        "Purchased package mass (g)",
        "1000",
    );
    field(
        world,
        owner,
        "offer:yield",
        "Usable g per purchased g (explicit preparation yield)",
        "1",
    );
    field(
        world,
        owner,
        "offer:observed",
        "Price observed on",
        &state.config.start,
    );
    field(
        world,
        owner,
        "offer:available",
        "Offer available from",
        &state.config.start,
    );
    field(
        world,
        owner,
        "offer:until",
        "Offer valid until (optional)",
        "",
    );
    field(
        world,
        owner,
        "offer:limit",
        "Maximum available packages (whole count; optional)",
        "",
    );
    field(
        world,
        owner,
        "offer:url",
        "Price source / receipt reference",
        "",
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Save price assertion",
        Operation::SaveOffer,
    );
    label(
        world,
        body,
        "Existing inventory has zero additional purchase cost. Count usable grams and keep availability and expiry separate.",
    );
    field(world, owner, "stock:grams", "Stock quantity (usable g)", "");
    field(
        world,
        owner,
        "stock:from",
        "Stock available from",
        &state.config.start,
    );
    field(
        world,
        owner,
        "stock:until",
        "Stock expires on (optional)",
        "",
    );
    let r = row(world, body);
    button(world, r, owner, "Add inventory lot", Operation::AddStock);
    for stock in state.config.stock.iter().filter(|s| &s.food == id) {
        label(
            world,
            body,
            format!(
                "{} g from {}, expiry {}",
                stock.usable_g,
                stock.available_on,
                stock.expires_on.as_deref().unwrap_or("none declared")
            ),
        );
        let r = row(world, body);
        button(
            world,
            r,
            owner,
            "Remove inventory lot",
            Operation::RemoveStock(stock.id.clone()),
        );
    }
}

fn results(world: &mut World, owner: Entity, state: &DemeterCastle) {
    let body = world.get::<View>(owner).unwrap().body;
    field(
        world,
        owner,
        "budget",
        "Solve budget (seconds, up to 30)",
        state.budget_ms / 1000,
    );
    let r = row(world, body);
    button(world, r, owner, "Cancel solve", Operation::Cancel);
    let Some(result) = &state.result else {
        label(
            world,
            body,
            "Save and solve to see daily grams, nutrients, and shopping quantities.",
        );
        return;
    };
    label(
        world,
        body,
        format!(
            "Result: {:?} — {} ms",
            result.solution.status, result.solution.elapsed_ms
        ),
    );
    if let Some(message) = &result.solution.message {
        label(world, body, message);
    }
    for diagnostic in &result.solution.diagnostics {
        label(
            world,
            body,
            format!(
                "{}: {} (gap {:?})",
                diagnostic.constraint, diagnostic.reason, diagnostic.shortfall
            ),
        );
    }
    if let Some(cost) = result.solution.objective {
        label(
            world,
            body,
            format!(
                "Continuous acquisition cost: {cost:.2} {}. Rounded shopping bill: {:.2}. Declared production cost included in the model: {:.2}.",
                state.config.currency, result.rounded_bill, result.forecast_cost
            ),
        );
    }
    label(
        world,
        body,
        "Package rounding is performed after the continuous solve; the rounded bill is not a proven checkout optimum.",
    );
    for item in &result.shopping {
        let vendor = state.catalog["offers"]
            .as_array()
            .and_then(|offers| offers.iter().find(|o| o["uid"] == item.offer))
            .and_then(|o| o["market"]["vendor"].as_str())
            .unwrap_or("selected offer");
        label(
            world,
            body,
            format!(
                "{} — {vendor}: {} packages, {:.0} purchased g, {:.0} usable g allocated, {:.0} usable g left; bill {:.2}",
                food_label(state, &item.food),
                item.packages,
                item.bought_g,
                item.used_usable_g,
                item.leftover_usable_g,
                item.bill
            ),
        );
    }
    label(world, body, "Food totals for the selected period");
    for (food, grams) in &result.totals_g {
        if *grams > 1e-7 {
            label(
                world,
                body,
                format!("{}: {grams:.1} g", food_label(state, food)),
            );
        }
    }
    for block in result.days.chunks(7) {
        let mut total = BTreeMap::<String, f64>::new();
        for day in block {
            for (food, grams) in &day.foods_g {
                *total.entry(food.clone()).or_default() += grams;
            }
        }
        label(
            world,
            body,
            format!("Week from {} ({} days)", block[0].date, block.len()),
        );
        for (food, grams) in total {
            if grams > 1e-7 {
                label(
                    world,
                    body,
                    format!("{}: {grams:.1} g", food_label(state, &food)),
                );
            }
        }
    }
    for (index, day) in result.days.iter().enumerate() {
        label(world, body, &day.date);
        for (food, grams) in &day.foods_g {
            if *grams > 1e-7 {
                label(
                    world,
                    body,
                    format!("{}: {grams:.1} g", food_label(state, food)),
                );
            }
        }
        for (nutrient, value) in &day.nutrients {
            let target = result
                .solution
                .achieved
                .iter()
                .find(|r| r.name == format!("nutrient:{nutrient}:{index}"));
            let limits = target
                .map(|t| {
                    format!(
                        "minimum {}, maximum {}",
                        t.lower
                            .map(|v| format!("{v:.3}"))
                            .unwrap_or_else(|| "omitted".into()),
                        t.upper
                            .map(|v| format!("{v:.3}"))
                            .unwrap_or_else(|| "unlimited".into())
                    )
                })
                .unwrap_or_default();
            label(
                world,
                body,
                format!("{}: {value:.3} ({limits})", nutrient.replace('_', " ")),
            );
        }
    }
    let pages = result.missing.len().div_ceil(120).max(1);
    let page = usize::from(state.missing_page).min(pages - 1);
    label(
        world,
        body,
        format!(
            "{} missing / null / trace measurements contribute zero. Page {}/{pages}.",
            result.missing.len(),
            page + 1
        ),
    );
    for missing in result.missing.iter().skip(page * 120).take(120) {
        label(
            world,
            body,
            format!(
                "{} — {}: {}",
                food_label(state, &missing.food),
                missing.nutrient,
                missing
                    .original
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "absent".into())
            ),
        );
    }
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Previous missing measurements",
        Operation::MissingPage(-1),
    );
    button(
        world,
        r,
        owner,
        "Next missing measurements",
        Operation::MissingPage(1),
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Preview acquire / grow tasks",
        Operation::Preview,
    );
}

fn production(world: &mut World, owner: Entity, state: &DemeterCastle) {
    let body = world.get::<View>(owner).unwrap().body;
    field(
        world,
        owner,
        "horizon",
        "Recurring demand projection (days; 365 is about 12 months)",
        state.config.horizon_days,
    );
    let r = row(world, body);
    button(
        world,
        r,
        owner,
        "Buy everything at supermarket",
        Operation::SupermarketAll(true),
    );
    button(
        world,
        r,
        owner,
        "Use individual acquisition choices",
        Operation::SupermarketAll(false),
    );
    label(
        world,
        body,
        if state.config.buy_all {
            "Whole-plan supermarket override is enabled. Save and solve to replace pending work."
        } else {
            "Individual food acquisition choices are enabled."
        },
    );
    for food in &state.config.foods {
        let r = row(world, body);
        choice_button(
            world,
            r,
            owner,
            &food_label(state, &food.food),
            Operation::Select(food.food.clone()),
        );
    }
    if let Some(food) = &state.selected_food {
        label(
            world,
            body,
            format!("Production for {}", food_label(state, food)),
        );
        let r = row(world, body);
        button(
            world,
            r,
            owner,
            "Buy this item",
            Operation::Acquire(Acquisition::Buy),
        );
        button(
            world,
            r,
            owner,
            "Grow this item",
            Operation::Acquire(Acquisition::Grow),
        );
        if let Some(templates) = state.catalog["templates"].as_array() {
            for item in templates.iter().filter(|item| {
                item["template"]["food_outputs"]
                    .as_array()
                    .is_none_or(|outputs| {
                        outputs.is_empty() || outputs.iter().any(|output| output == food)
                    })
            }) {
                if let Some(id) = item["uid"].as_str() {
                    let r = row(world, body);
                    choice_button(
                        world,
                        r,
                        owner,
                        item["label"].as_str().unwrap_or(id),
                        Operation::Template(id.into()),
                    );
                }
            }
        }
        if let Some(choice) = state.config.production.iter().find(|c| &c.food == food) {
            let template: Option<production::Template> = state.catalog["templates"]
                .as_array()
                .and_then(|v| v.iter().find(|t| t["uid"] == choice.template))
                .and_then(|v| serde_json::from_value(v["template"].clone()).ok());
            if let Some(template) = template {
                label(
                    world,
                    body,
                    format!(
                        "{} — {}. {}",
                        template.species, template.method, template.assumption
                    ),
                );
                let r = row(world, body);
                button(
                    world,
                    r,
                    owner,
                    "Production template and sources",
                    Operation::Reference(template.id),
                );
            }
            field(
                world,
                owner,
                "production:start",
                "Establish on / existing establishment date",
                &choice.start_on,
            );
            field(
                world,
                owner,
                "production:yield",
                "Confirmed output per unit per harvest (g)",
                number(choice.yield_override_g),
            );
            field(
                world,
                owner,
                "production:lead",
                "Days from establishment to first harvest",
                choice
                    .lead_days_override
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            );
            field(
                world,
                owner,
                "production:months",
                "Allowed establishment months (comma-separated)",
                choice
                    .season_months
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
            field(
                world,
                owner,
                "production:usable",
                "Usable g per declared output g",
                choice.usable_yield,
            );
            field(
                world,
                owner,
                "production:shelf",
                "Harvest shelf life (days)",
                choice.shelf_life_days,
            );
            field(
                world,
                owner,
                "production:capacity",
                "Maximum simultaneous plants / animals (optional)",
                choice.max_units.map(|v| v.to_string()).unwrap_or_default(),
            );
            field(
                world,
                owner,
                "production:existing",
                "Confirmed existing plant / animal count (optional)",
                choice
                    .existing_units
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            );
            field(
                world,
                owner,
                "production:cost",
                "Declared cost per usable g in the plan currency",
                choice.cost_per_usable_g,
            );
            let r = row(world, body);
            button(
                world,
                r,
                owner,
                if choice.confirmed {
                    "Withdraw parameter confirmation"
                } else {
                    "Confirm local parameters"
                },
                Operation::ConfirmProduction,
            );
            label(
                world,
                body,
                if choice.confirmed {
                    "Parameters confirmed. Expected output stays separate from actual inventory."
                } else {
                    "Confirm or replace the example yield, season, timing, cost, and care capacity before use."
                },
            );
            let r = row(world, body);
            button(
                world,
                r,
                owner,
                if choice.supermarket_fallback {
                    "Disable supermarket fallback"
                } else {
                    "Allow supermarket fallback"
                },
                Operation::Fallback,
            );
            field(
                world,
                owner,
                "resource:name",
                "Resource and unit (for example bed-m2)",
                "",
            );
            field(
                world,
                owner,
                "resource:per",
                "Resource per plant / animal",
                "",
            );
            field(world, owner, "resource:capacity", "Available resource", "");
            let r = row(world, body);
            button(
                world,
                r,
                owner,
                "Add resource capacity",
                Operation::AddResource,
            );
            for (name, per) in &choice.resources_per_unit {
                label(
                    world,
                    body,
                    format!(
                        "{name}: {per} per unit; capacity {:?}",
                        choice.resource_capacity.get(name).map(ToString::to_string)
                    ),
                );
                let r = row(world, body);
                button(
                    world,
                    r,
                    owner,
                    "Remove capacity constraint",
                    Operation::RemoveResource(name.clone()),
                );
            }
            label(
                world,
                body,
                "An explicitly declared future harvest can supply meals only from its availability date. Confirm actual harvest weights before recording inventory.",
            );
            field(
                world,
                owner,
                "forecast:grams",
                "Forecast harvest usable grams",
                "",
            );
            field(
                world,
                owner,
                "forecast:from",
                "Expected available date",
                demeter::date(&choice.start_on).ok().and_then(|at| {
                    let lead = choice.lead_days_override.or_else(|| state.catalog["templates"].as_array()?.iter().find(|t| t["uid"] == choice.template)?["template"]["first_harvest_days"].as_u64().and_then(|n| u32::try_from(n).ok())).unwrap_or(0);
                    at.checked_add_days(chrono::Days::new(u64::from(lead)))
                }).map(|at| at.to_string()).unwrap_or_else(|| choice.start_on.clone()),
            );
            field(
                world,
                owner,
                "forecast:until",
                "Forecast expires on (optional)",
                "",
            );
            let r = row(world, body);
            button(
                world,
                r,
                owner,
                "Add declared harvest forecast",
                Operation::AddForecast,
            );
            for forecast in state
                .config
                .forecasts
                .iter()
                .filter(|f| &f.stock.food == food)
            {
                label(
                    world,
                    body,
                    format!(
                        "Expected {} usable g from {}",
                        forecast.stock.usable_g, forecast.stock.available_on
                    ),
                );
                let r = row(world, body);
                button(
                    world,
                    r,
                    owner,
                    "Remove forecast",
                    Operation::RemoveForecast(forecast.stock.id.clone()),
                );
            }
        }
    }
    let r = row(world, body);
    button(world, r, owner, "Preview dated tasks", Operation::Preview);
    button(world, r, owner, "Accept reviewed tasks", Operation::Accept);
    if let Some(projection) = &state.projection {
        for gap in &projection.gaps {
            label(world, body, gap);
        }
        for (food, date) in &projection.earliest_harvest {
            label(
                world,
                body,
                format!("{} — earliest harvest {date}", food_label(state, food)),
            );
        }
        for (food, grams) in &projection.surplus_g {
            label(
                world,
                body,
                format!(
                    "{} — unallocated expected output {grams:.1} g",
                    food_label(state, food)
                ),
            );
        }
        let pages = projection.tasks.len().div_ceil(100).max(1);
        let page = usize::from(state.task_page).min(pages - 1);
        label(
            world,
            body,
            format!(
                "{} proposed tasks — page {}/{pages}",
                projection.tasks.len(),
                page + 1
            ),
        );
        for task in projection.tasks.iter().skip(page * 100).take(100) {
            label(
                world,
                body,
                format!(
                    "{} — {} — {} {}",
                    task.on, task.title, task.amount, task.unit
                ),
            );
        }
        let r = row(world, body);
        button(world, r, owner, "Previous tasks", Operation::TaskPage(-1));
        button(world, r, owner, "Next tasks", Operation::TaskPage(1));
    }
    if !state.managed_tasks.is_empty() {
        let pages = state.managed_tasks.len().div_ceil(100).max(1);
        let page = usize::from(state.task_page).min(pages - 1);
        label(
            world,
            body,
            "Accepted work: completed and started tasks are retained when pending work is replaced.",
        );
        for item in state.managed_tasks.iter().skip(page * 100).take(100) {
            label(
                world,
                body,
                format!(
                    "[{}] {} — {}",
                    item["state"].as_str().unwrap_or_default(),
                    item["task"]["on"].as_str().unwrap_or_default(),
                    item["task"]["title"].as_str().unwrap_or_default()
                ),
            );
        }
        label(
            world,
            body,
            format!("Accepted work — page {}/{pages}", page + 1),
        );
        if state.projection.is_none() {
            let r = row(world, body);
            button(world, r, owner, "Previous tasks", Operation::TaskPage(-1));
            button(world, r, owner, "Next tasks", Operation::TaskPage(1));
        }
    }
}
