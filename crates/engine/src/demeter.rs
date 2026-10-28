mod catalog;
mod persistence;

use crate::{
    Engine, EngineError,
    actions::ActionOutcome,
    optimization::{Run, invalid},
};
use chrono::{DateTime, Utc};
use nucleus::{
    DecimalValue,
    demeter::{
        self, FoodChoice, Forecast, NutrientBound, Profile, Requirements, Stock, production,
    },
    optimization::Options,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const PLAN: &str = "lince.demeter.plan";
pub const OFFER: &str = "lince.demeter.offer";
pub const TASK: &str = "lince.demeter.task";

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub start: String,
    pub days: u16,
    pub currency: String,
    pub profile: Option<Profile>,
    pub manual: Requirements,
    #[serde(default)]
    pub energy_share_overrides: Option<Vec<demeter::EnergyShare>>,
    pub overrides: Vec<NutrientBound>,
    pub disabled_nutrients: Vec<String>,
    pub references: Vec<String>,
    pub foods: Vec<FoodChoice>,
    pub offers: Vec<String>,
    pub stock: Vec<Stock>,
    pub forecasts: Vec<Forecast>,
    pub production: Vec<production::Choice>,
    pub horizon_days: u16,
    pub buy_all: bool,
    pub volume_ml_per_day: Option<DecimalValue>,
}

impl Config {
    pub fn new(start: String) -> Self {
        Self {
            name: "My meal plan".into(),
            start,
            days: 7,
            currency: "brl".into(),
            profile: None,
            manual: Requirements::default(),
            energy_share_overrides: None,
            overrides: Vec::new(),
            disabled_nutrients: Vec::new(),
            references: Vec::new(),
            foods: Vec::new(),
            offers: Vec::new(),
            stock: Vec::new(),
            forecasts: Vec::new(),
            production: Vec::new(),
            horizon_days: 365,
            buy_all: false,
            volume_ml_per_day: None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        demeter::date(&self.start)?;
        if self.name.is_empty()
            || self.name.len() > 256
            || self.days == 0
            || self.days > 31
            || self.horizon_days == 0
            || self.horizon_days > 3660
            || self.foods.len() > 600
            || self.references.len() > 100
            || self.offers.len() > 2000
            || self.stock.len() + self.forecasts.len() > 2000
            || self.production.len() > 600
            || self.overrides.len() > 100
            || self.disabled_nutrients.len() > 100
            || serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 500_000
        {
            return Err("Check plan name, dates, duration, and selection limits".into());
        }
        if let Some(profile) = &self.profile {
            profile.energy()?;
        }
        for food in &self.foods {
            for bound in &food.bounds {
                bound.bounds.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfferDraft {
    pub id: String,
    pub food: String,
    pub currency: String,
    pub price_predicate: String,
    pub price: DecimalValue,
    pub package_g: DecimalValue,
    pub usable_yield: DecimalValue,
    pub vendor: String,
    pub observed_on: String,
    pub source_url: String,
    pub available_on: String,
    pub valid_until: Option<String>,
    pub max_packages: Option<DecimalValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Catalog,
    Vocabulary {
        currency: String,
    },
    Recommend {
        profile: Profile,
        references: Vec<String>,
    },
    Save {
        plan: Option<String>,
        expected_revision: u64,
        config: Config,
    },
    Inspect {
        plan: String,
    },
    SaveOffer {
        offer: OfferDraft,
    },
    Solve {
        plan: String,
        #[serde(default)]
        options: Options,
    },
    Poll {
        run: String,
    },
    Cancel {
        run: String,
    },
    Accept {
        plan: String,
        run: String,
    },
    Project {
        plan: String,
        run: String,
    },
    Acquisition {
        plan: String,
        expected_revision: u64,
        food: Option<String>,
        supermarket: bool,
    },
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Catalog
            | Self::Recommend { .. }
            | Self::Inspect { .. }
            | Self::Project { .. } => "record:read",
            Self::Vocabulary { .. } => "permission:assign",
            Self::Save { plan: None, .. } | Self::SaveOffer { .. } | Self::Solve { .. } => {
                "record:create"
            }
            _ => "record:update",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub controller: Option<String>,
    pub revision: u64,
    pub config: Config,
    pub last_run: Option<String>,
    pub accepted_hash: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct MarketOffer {
    pub controller: Option<String>,
    pub offer: demeter::Offer,
    pub predicate: String,
    pub vendor: String,
    pub observed_on: String,
    pub source_url: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub plan: String,
    pub revision: u64,
    pub input: demeter::Input,
    pub choices: Vec<production::Choice>,
    pub templates: Vec<production::Template>,
    pub horizon_days: u16,
    pub buy_all: bool,
    pub sources: BTreeMap<String, String>,
}

impl Engine {
    pub(crate) async fn demeter_command(
        &self,
        request: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, request.permission()).await?;
        let _gate = self.meal_runtime.gate.lock().await;
        match request {
            Command::Catalog => Ok(ActionOutcome {
                data: Some(self.demeter_catalog(actor).await?),
                ..Default::default()
            }),
            Command::Vocabulary { currency } => {
                self.demeter_vocabulary(&currency, actor, now).await
            }
            Command::Recommend {
                profile,
                references,
            } => {
                let mut records = BTreeMap::new();
                let references = self
                    .demeter_references(&references, actor, &mut records)
                    .await?;
                Ok(ActionOutcome {
                    data: Some(
                        json!({"requirements":demeter::select_references(&references,&profile,&[],&[]).map_err(invalid)?}),
                    ),
                    ..Default::default()
                })
            }
            Command::Save {
                plan,
                expected_revision,
                config,
            } => {
                self.save_demeter(plan.as_deref(), expected_revision, config, actor, now)
                    .await
            }
            Command::SaveOffer { offer } => self.save_demeter_offer(offer, actor, now).await,
            Command::Inspect { plan } => {
                let (uid, saved) = self.load_demeter(&plan, actor).await?;
                let snapshot = self.demeter_snapshot(&uid, &saved, actor).await;
                let hash = snapshot
                    .as_ref()
                    .ok()
                    .and_then(|s| demeter::fingerprint(s).ok());
                let last = if let Some(run) = &saved.last_run {
                    let (_, run) = self.load_optimization_run(run, actor).await?;
                    Some(
                        json!({"state":run.state,"hash":run.hash,"stale":hash.as_deref()!=Some(run.hash.as_str())}),
                    )
                } else {
                    None
                };
                Ok(ActionOutcome {
                    data: Some(
                        json!({"plan":uid,"saved":saved,"hash":hash,"last":last,"input":snapshot.as_ref().ok().map(|s|&s.input),"error":snapshot.as_ref().err().map(|e|e.to_string()),"tasks":self.demeter_tasks(&uid,actor).await?}),
                    ),
                    ..Default::default()
                })
            }
            Command::Solve { plan, options } => {
                let (uid, mut saved) = self.load_demeter(&plan, actor).await?;
                let before = serde_json::to_value(&saved)?;
                let snapshot = self.demeter_snapshot(&uid, &saved, actor).await?;
                let hash = demeter::fingerprint(&snapshot).map_err(invalid)?;
                let input = snapshot.input.clone();
                let run = Run {
                    controller: actor.map(str::to_owned),
                    plan: Some(uid.clone()),
                    kind: "demeter".into(),
                    hash,
                    state: "running".into(),
                    snapshot: serde_json::to_value(&snapshot)?,
                    created_at: now.to_rfc3339(),
                    options,
                    message: None,
                };
                let mut outcome = self
                    .start_optimization_run(run, actor, now, move |cancelled| {
                        input
                            .solve(options, &cancelled)
                            .and_then(|out| serde_json::to_value(out).map_err(|e| e.to_string()))
                    })
                    .await?;
                saved.last_run = outcome.created.clone();
                outcome.facts.extend(
                    self.managed_write(
                        &uid,
                        PLAN,
                        &serde_json::to_value(&saved)?,
                        Some(&before),
                        actor,
                        now,
                    )
                    .await?,
                );
                Ok(outcome)
            }
            Command::Poll { run } => self.poll_optimization(&run, actor, now, false).await,
            Command::Cancel { run } => self.poll_optimization(&run, actor, now, true).await,
            Command::Accept { plan, run } => self.accept_demeter(&plan, &run, actor, now).await,
            Command::Project { plan, run } => {
                let (uid, saved) = self.load_demeter(&plan, actor).await?;
                let (_, run) = self.load_optimization_run(&run, actor).await?;
                let snapshot = self.demeter_snapshot(&uid, &saved, actor).await?;
                if run.kind != "demeter"
                    || run.plan.as_deref() != Some(&uid)
                    || run.hash != demeter::fingerprint(&snapshot).map_err(invalid)?
                {
                    return Err(invalid("Result is stale; solve again before projecting"));
                }
                let (meal, projection) = tokio::task::spawn_blocking(move || {
                    let meal = snapshot
                        .input
                        .solve(run.options, &std::sync::atomic::AtomicBool::new(false))?;
                    let projection = production::project(
                        &snapshot.input,
                        &meal,
                        &snapshot.choices,
                        &snapshot.templates,
                        snapshot.horizon_days,
                        snapshot.buy_all,
                    )?;
                    Ok::<_, String>((meal, projection))
                })
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
                Ok(ActionOutcome {
                    data: Some(json!({"plan":uid,"projection":projection,"result":meal})),
                    ..Default::default()
                })
            }
            Command::Acquisition {
                plan,
                expected_revision,
                food,
                supermarket,
            } => {
                let (uid, mut saved) = self.load_demeter(&plan, actor).await?;
                if saved.revision != expected_revision {
                    return Err(invalid("Plan changed; reload before changing acquisition"));
                }
                if let Some(food) = food {
                    let food = self.planning_read(&food, actor).await?;
                    let choice = saved
                        .config
                        .foods
                        .iter_mut()
                        .find(|f| f.food == food)
                        .ok_or_else(|| invalid("Choose a selected food"))?;
                    choice.acquisition = if supermarket {
                        demeter::Acquisition::Buy
                    } else {
                        demeter::Acquisition::Grow
                    };
                } else {
                    saved.config.buy_all = supermarket;
                }
                self.save_demeter(Some(&uid), expected_revision, saved.config, actor, now)
                    .await
            }
        }
    }
}
