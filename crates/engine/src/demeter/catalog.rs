use super::*;
use nucleus::demeter::{Acquisition, Food, Reference};

impl Engine {
    async fn planning_ids(
        &self,
        namespace: &str,
        actor: Option<&str>,
    ) -> Result<Vec<String>, EngineError> {
        let ids:Vec<String>=store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.deleted_at IS NULL ORDER BY r.head LIMIT 2000").bind(namespace).fetch_all(&self.store.pool).await?;
        let mut visible = Vec::new();
        for uid in ids {
            if self.may_read_record(actor, &uid).await? {
                visible.push(uid);
            }
        }
        Ok(visible)
    }

    pub(super) async fn planning_data(
        &self,
        token: &str,
        namespace: &str,
        actor: Option<&str>,
        hashes: &mut BTreeMap<String, String>,
    ) -> Result<(String, Value), EngineError> {
        let uid = self.planning_read(token, actor).await?;
        let record = store::records::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| invalid("Planning Record disappeared"))?;
        let data = store::records::get_extension(&self.store.pool, &uid, namespace)
            .await?
            .ok_or_else(|| invalid(format!("{} has no {namespace} data", record.head)))?;
        hashes.insert(
            format!("{uid}:{namespace}"),
            demeter::fingerprint(&json!({"uid":uid,"updated":record.updated_at,"data":data}))
                .map_err(invalid)?,
        );
        Ok((uid, data))
    }

    pub(super) async fn demeter_references(
        &self,
        tokens: &[String],
        actor: Option<&str>,
        hashes: &mut BTreeMap<String, String>,
    ) -> Result<Vec<Reference>, EngineError> {
        if tokens.is_empty() || tokens.len() > 100 {
            return Err(invalid(
                "Import Nutrition Needs and choose its reference Records",
            ));
        }
        let mut references = Vec::new();
        for token in tokens {
            let (_, data) = self
                .planning_data(token, "demeter.reference", actor, hashes)
                .await?;
            references.push(serde_json::from_value(data).map_err(invalid)?);
        }
        Ok(references)
    }

    pub(super) async fn demeter_template(
        &self,
        token: &str,
        actor: Option<&str>,
        hashes: &mut BTreeMap<String, String>,
    ) -> Result<(production::Template, String), EngineError> {
        let (uid, data) = self
            .planning_data(token, "demeter.production", actor, hashes)
            .await?;
        let mut template: production::Template = serde_json::from_value(data).map_err(invalid)?;
        let assertions =
            store::assertions::for_subjects(&self.store.pool, std::slice::from_ref(&uid)).await?;
        let products: Vec<_> = assertions
            .iter()
            .filter(|a| a.predicate == "produces")
            .filter_map(|a| a.object_uid.clone())
            .collect();
        if !products.is_empty() {
            template.food_outputs = products;
        }
        for product in &mut template.food_outputs {
            *product = self.planning_read(product, actor).await?;
        }
        let species = assertions
            .iter()
            .find(|a| a.predicate == "species")
            .and_then(|a| a.object_uid.as_ref())
            .unwrap_or(&template.species);
        let species_uid = self.planning_read(species, actor).await?;
        let species = store::records::get(&self.store.pool, &species_uid)
            .await?
            .ok_or_else(|| invalid("Species disappeared"))?;
        hashes.insert(
            format!("{}:species", species.uid),
            demeter::fingerprint(
                &json!({"head":species.head,"body":species.body,"updated":species.updated_at}),
            )
            .map_err(invalid)?,
        );
        template.id = uid;
        hashes.insert(
            format!("{}:bindings", template.id),
            demeter::fingerprint(&json!({"outputs":template.food_outputs,"species":species_uid}))
                .map_err(invalid)?,
        );
        if template.species == species_uid {
            template.species = species.slug.unwrap_or(species.head);
        }
        Ok((template, species_uid))
    }

    pub(super) async fn demeter_catalog(&self, actor: Option<&str>) -> Result<Value, EngineError> {
        let mut foods = Vec::new();
        let mut references = Vec::new();
        let mut templates = Vec::new();
        let mut offers = Vec::new();
        let mut plans = Vec::new();
        for uid in self.planning_ids("nutrition", actor).await? {
            if let Some(record) = store::records::get(&self.store.pool, &uid).await? {
                foods.push(json!({"uid":uid,"label":record.head,"slug":record.slug}));
            }
        }
        for uid in self.planning_ids("demeter.reference", actor).await? {
            let data =
                store::records::get_extension(&self.store.pool, &uid, "demeter.reference").await?;
            references.push(json!({"uid":uid,"reference":data}));
        }
        for uid in self.planning_ids("demeter.production", actor).await? {
            let mut hashes = BTreeMap::new();
            if let Ok((template, species_uid)) =
                self.demeter_template(&uid, actor, &mut hashes).await
            {
                let related = store::assertions::for_subjects(
                    &self.store.pool,
                    std::slice::from_ref(&species_uid),
                )
                .await?;
                let mut matched = Vec::new();
                for a in related {
                    if a.predicate == "related-food" {
                        if let Some(uid) = a.object_uid {
                            if self.may_read_record(actor, &uid).await? {
                                matched.push(uid);
                            }
                        }
                    }
                }
                let record = store::records::get(&self.store.pool, &uid).await?.unwrap();
                if !template.food_outputs.is_empty() {
                    matched = template.food_outputs.clone();
                }
                templates.push(
                    json!({"uid":uid,"label":record.head,"template":template,"foods":matched}),
                );
            }
        }
        for uid in self.planning_ids(OFFER, actor).await? {
            let Some(data) = store::records::get_extension(&self.store.pool, &uid, OFFER).await?
            else {
                continue;
            };
            let Ok(mut offer) = serde_json::from_value::<MarketOffer>(data) else {
                continue;
            };
            if actor.is_some() && offer.controller.as_deref() != actor {
                continue;
            }
            if !self.may_read_record(actor, &offer.offer.food).await? {
                continue;
            }
            let assertions =
                store::assertions::for_subjects(&self.store.pool, std::slice::from_ref(&uid))
                    .await?;
            let prices: Vec<_> = assertions
                .iter()
                .filter(|a| {
                    a.predicate_uid == offer.predicate
                        && a.object_uid.is_none()
                        && a.role == "ordinary"
                })
                .collect();
            let valid =
                prices.len() == 1 && prices[0].quantity.is_some() && prices[0].unit_uid.is_some();
            if valid {
                offer.offer.price = prices[0].quantity.unwrap();
                offer.offer.currency = prices[0].unit_uid.clone().unwrap();
            }
            offers.push(json!({"uid":uid,"market":offer,"valid":valid}));
        }
        for uid in self.planning_ids(PLAN, actor).await? {
            if let Ok((_, saved)) = self.load_demeter(&uid, actor).await {
                plans.push(json!({"uid":uid,"name":saved.config.name,"revision":saved.revision}));
            }
        }
        let mut vocabulary = BTreeMap::new();
        for name in ["price", "brl", "usd", "g", "each"] {
            if let Ok(Some(uid)) = store::vocabulary::resolve(&self.store.pool, name, actor).await {
                vocabulary.insert(name, uid);
            }
        }
        Ok(
            json!({"foods":foods,"references":references,"templates":templates,"offers":offers,"plans":plans,"vocabulary":vocabulary,"trails":{"entry":"institute/trails/health/Meal-preparation.lingua","needs":"institute/trails/health/Nutrition-needs.lingua","farming":"institute/trails/health/Farming.lingua"}}),
        )
    }

    pub(super) async fn demeter_snapshot(
        &self,
        uid: &str,
        saved: &Saved,
        actor: Option<&str>,
    ) -> Result<Snapshot, EngineError> {
        saved.config.validate().map_err(invalid)?;
        let config = &saved.config;
        let mut sources = BTreeMap::new();
        let currency = store::vocabulary::resolve(&self.store.pool, &config.currency, actor)
            .await?
            .ok_or_else(|| invalid("Choose or create the plan's currency unit"))?;
        let mut requirements = if let Some(profile) = &config.profile {
            let refs = self
                .demeter_references(&config.references, actor, &mut sources)
                .await?;
            demeter::select_references(
                &refs,
                profile,
                &config.overrides,
                &config.disabled_nutrients,
            )
            .map_err(invalid)?
        } else {
            let mut req = config.manual.clone();
            for bound in &config.overrides {
                req.bounds.retain(|b| b.nutrient != bound.nutrient);
                req.bounds.push(bound.clone());
            }
            req
        };
        if let Some(shares) = &config.energy_share_overrides {
            requirements.energy_shares = shares.clone();
        }
        let mut foods = Vec::new();
        let fields: std::collections::BTreeSet<_> = requirements
            .bounds
            .iter()
            .map(|b| b.nutrient.clone())
            .chain(
                requirements
                    .energy_shares
                    .iter()
                    .map(|s| s.nutrient.clone()),
            )
            .chain(std::iter::once("energy_kcal".into()))
            .collect();
        for choice in &config.foods {
            let (food_uid, nutrition) = self
                .planning_data(&choice.food, "nutrition", actor, &mut sources)
                .await?;
            let record = store::records::get(&self.store.pool, &food_uid)
                .await?
                .ok_or_else(|| invalid("Food disappeared"))?;
            let mut choice = choice.clone();
            choice.food = food_uid;
            if config.buy_all {
                choice.acquisition = Acquisition::Buy;
            }
            let mut food =
                Food::from_nutrition(choice, record.head, &nutrition).map_err(invalid)?;
            food.nutrients.retain(|field, _| fields.contains(field));
            foods.push(food);
        }
        let mut offers = Vec::new();
        for token in &config.offers {
            let (offer_uid, data) = self
                .planning_data(token, OFFER, actor, &mut sources)
                .await?;
            let mut market: MarketOffer = serde_json::from_value(data).map_err(invalid)?;
            if actor.is_some() && market.controller.as_deref() != actor {
                return Err(EngineError::Forbidden("Use your own market offer".into()));
            }
            let assertions =
                store::assertions::for_subjects(&self.store.pool, std::slice::from_ref(&offer_uid))
                    .await?;
            let prices: Vec<_> = assertions
                .iter()
                .filter(|a| {
                    a.predicate_uid == market.predicate
                        && a.object_uid.is_none()
                        && a.role == "ordinary"
                })
                .collect();
            if prices.len() != 1 {
                return Err(invalid(
                    "Offer needs exactly one current quantified price assertion",
                ));
            }
            let price = prices[0];
            market.offer.price = price
                .quantity
                .ok_or_else(|| invalid("Offer price is missing"))?;
            market.offer.currency = price
                .unit_uid
                .clone()
                .ok_or_else(|| invalid("Offer price needs a currency unit"))?;
            sources.insert(format!("{offer_uid}:price"),demeter::fingerprint(&json!({"uid":price.uid,"predicate":price.predicate_uid,"quantity":price.quantity,"unit":price.unit_uid})).map_err(invalid)?);
            if foods.iter().any(|f| f.choice.food == market.offer.food) {
                offers.push(market.offer);
            }
        }
        let mut stock = config.stock.clone();
        for s in &mut stock {
            s.food = self.planning_read(&s.food, actor).await?;
        }
        let mut forecasts = config.forecasts.clone();
        for f in &mut forecasts {
            f.stock.food = self.planning_read(&f.stock.food, actor).await?;
            f.process = self.planning_read(&f.process, actor).await?;
            f.currency = store::vocabulary::resolve(&self.store.pool, &f.currency, actor)
                .await?
                .ok_or_else(|| invalid("Choose a forecast currency"))?;
        }
        let mut choices = config.production.clone();
        let mut templates = Vec::new();
        for choice in &mut choices {
            choice.food = self.planning_read(&choice.food, actor).await?;
            let (template, _) = self
                .demeter_template(&choice.template, actor, &mut sources)
                .await?;
            choice.template = template.id.clone();
            choice.currency = store::vocabulary::resolve(&self.store.pool, &choice.currency, actor)
                .await?
                .ok_or_else(|| invalid("Choose a production-cost currency"))?;
            if choice.currency != currency {
                return Err(invalid("Production cost must use the plan currency"));
            }
            production::validate(&template, choice, &currency).map_err(invalid)?;
            if choice.confirmed {
                if let Some(units) = choice.existing_units {
                    if template.harvest_count > 3660
                        || (template.harvest_count > 1 && template.harvest_interval_days == 0)
                    {
                        return Err(invalid("Check the existing production harvest schedule"));
                    }
                    let established = demeter::date(&choice.start_on).map_err(invalid)?;
                    let first = established
                        .checked_add_days(chrono::Days::new(u64::from(
                            choice
                                .lead_days_override
                                .unwrap_or(template.first_harvest_days),
                        )))
                        .ok_or_else(|| invalid("Production date overflow"))?;
                    let start = demeter::date(&config.start).map_err(invalid)?;
                    let end = start
                        .checked_add_days(chrono::Days::new(u64::from(config.days - 1)))
                        .ok_or_else(|| invalid("Meal date overflow"))?;
                    let yield_g = choice
                        .yield_override_g
                        .unwrap_or(demeter::exact(&template.usable_g_per_harvest).map_err(invalid)?)
                        .to_f64()
                        * choice.usable_yield.to_f64()
                        * f64::from(units);
                    for index in 0..template.harvest_count {
                        let harvest = first
                            .checked_add_days(chrono::Days::new(
                                u64::from(index) * u64::from(template.harvest_interval_days),
                            ))
                            .ok_or_else(|| invalid("Harvest date overflow"))?;
                        if harvest < start || harvest > end {
                            continue;
                        }
                        let on = harvest.to_string();
                        if forecasts.iter().any(|f| {
                            f.stock.food == choice.food
                                && f.process == template.id
                                && f.stock.available_on == on
                        }) {
                            continue;
                        }
                        let until = harvest
                            .checked_add_days(chrono::Days::new(u64::from(choice.shelf_life_days)))
                            .ok_or_else(|| invalid("Harvest expiry overflow"))?;
                        forecasts.push(demeter::Forecast {
                            stock: demeter::Stock {
                                id: format!("site:{}:{}:{on}", choice.food, template.id),
                                food: choice.food.clone(),
                                usable_g: nucleus::optimization::decimal(yield_g)
                                    .map_err(invalid)?,
                                available_on: on,
                                expires_on: Some(until.to_string()),
                            },
                            cost_per_usable_g: choice.cost_per_usable_g,
                            currency: currency.clone(),
                            process: template.id.clone(),
                        });
                    }
                }
            }
            if !templates
                .iter()
                .any(|t: &production::Template| t.id == template.id)
            {
                templates.push(template);
            }
        }
        let input = demeter::Input {
            start: config.start.clone(),
            days: config.days,
            currency,
            requirements,
            foods,
            offers,
            stock,
            forecasts,
            volume_ml_per_day: config.volume_ml_per_day,
        };
        input.validate().map_err(invalid)?;
        Ok(Snapshot {
            plan: uid.into(),
            revision: saved.revision,
            input,
            choices,
            templates,
            horizon_days: config.horizon_days,
            buy_all: config.buy_all,
            sources,
        })
    }
}
