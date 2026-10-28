mod editor;
mod persistence;
#[cfg(test)]
mod tests;
mod ui;

use crate::native::{actions::Action, protein_area::Source, sand_panel as panel};
use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use engine::demeter::{Command, Config};
use nucleus::demeter::{
    self, Acquisition, Bounds, FoodChoice, Period, Profile, ReferenceGroup, Requirements,
};
pub(crate) use persistence::{SavedDemeterCastle, snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "microlp",
        author: "Alexey Zatelepin and Specy",
        license: include_str!("../../licenses/native/microlp-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "sprs",
        author: "Vincent Barrielle and contributors",
        license: include_str!("../../licenses/native/sprs-MIT.txt"),
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tab {
    #[default]
    Requirements,
    Foods,
    Results,
    Production,
}

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct DemeterCastle {
    pub plan: Option<String>,
    pub revision: u64,
    pub config: Config,
    pub tab: Tab,
    pub selected_food: Option<String>,
    pub search: String,
    pub drafts: BTreeMap<String, String>,
    pub run: Option<String>,
    pub run_revision: Option<u64>,
    pub budget_ms: u64,
    pub changed: bool,
    pub stale: bool,
    pub edit_version: u64,
    pub session: String,
    pub missing_page: u16,
    pub task_page: u16,
    #[serde(skip)]
    pub managed_tasks: Vec<Value>,
    #[serde(skip)]
    pub catalog: Value,
    #[serde(skip)]
    pub result: Option<demeter::Outcome>,
    #[serde(skip)]
    pub projection: Option<demeter::production::Projection>,
}

impl Default for DemeterCastle {
    fn default() -> Self {
        let mut config = Config::new(chrono::Local::now().date_naive().to_string());
        config.profile = Some(Profile {
            age: 30,
            group: ReferenceGroup::Male,
            weight_kg: demeter::exact("70").unwrap(),
            height_cm: demeter::exact("175").unwrap(),
            activity_factor: demeter::exact("1.4").unwrap(),
            energy_override_kcal: None,
        });
        Self {
            plan: None,
            revision: 0,
            config,
            tab: Tab::Requirements,
            selected_food: None,
            search: String::new(),
            drafts: BTreeMap::new(),
            run: None,
            run_revision: None,
            budget_ms: 5000,
            changed: true,
            stale: true,
            edit_version: 0,
            session: nucleus::new_uid("demeter-editor"),
            missing_page: 0,
            task_page: 0,
            managed_tasks: Vec::new(),
            catalog: Value::Null,
            result: None,
            projection: None,
        }
    }
}

impl DemeterCastle {
    pub fn valid(&self) -> bool {
        self.config.validate().is_ok()
            && self.budget_ms <= 30_000
            && self
                .plan
                .as_ref()
                .is_none_or(|v| nucleus::valid_uid(v, "r"))
            && self.run.as_ref().is_none_or(|v| nucleus::valid_uid(v, "r"))
            && self.selected_food.as_ref().is_none_or(|v| v.len() <= 128)
            && self.search.len() <= 256
            && self.drafts.len() <= 4096
            && self
                .drafts
                .iter()
                .all(|(k, v)| k.len() <= 256 && v.len() <= 4096)
            && self
                .drafts
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                <= 1_000_000
    }
}

#[derive(Component)]
struct View {
    ready: bool,
    body: Entity,
    status: Entity,
    fields: BTreeMap<String, Entity>,
    initial: BTreeMap<String, String>,
    bounds: Requirements,
    running: bool,
}

#[derive(Clone)]
enum Operation {
    Create,
    New,
    Refresh,
    Open(String),
    Tab(Tab),
    Search,
    Select(String),
    Remove(String),
    Save(bool),
    Vocabulary,
    Profile(bool),
    Group(ReferenceGroup),
    ResetOverrides,
    ToggleNutrient(String),
    RemoveShare(String),
    AddNutrient,
    Fixed,
    SaveOffer,
    UseOffer(String),
    AddStock,
    RemoveStock(String),
    Acquire(Acquisition),
    Exclude(bool),
    Template(String),
    ConfirmProduction,
    Fallback,
    AddForecast,
    RemoveForecast(String),
    AddResource,
    RemoveResource(String),
    SupermarketAll(bool),
    MissingPage(i16),
    TaskPage(i16),
    Preview,
    Accept,
    Cancel,
    Sources,
    Reference(String),
}

#[derive(Clone)]
enum Reply {
    Catalog,
    Inspect(String),
    Save(bool, u64),
    Vocabulary,
    Offer,
    Start(u64),
    Poll,
    Project,
    Accept,
}

#[derive(Resource, Default)]
struct Pending(BTreeMap<String, (Entity, Reply, String)>);

#[derive(Resource)]
struct PollClock(std::time::Instant);

pub struct DemeterCastlePlugin;

impl Plugin for DemeterCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pending>()
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .insert_resource(PollClock(std::time::Instant::now()))
            .add_systems(Update, poll.after(crate::native::cell_bridge::ReceiveCell));
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    state: DemeterCastle,
) -> Entity {
    let plan = state.plan.clone();
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(CREDITS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(720.0, 820.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            state,
        ))
        .id();
    let frame = panel::frame(world, owner, "Demeter — meal preparation");
    let nav = panel::row(world, frame);
    world.get_mut::<Node>(nav).unwrap().flex_wrap = FlexWrap::Wrap;
    for (tab, label) in [
        (Tab::Requirements, "Requirements"),
        (Tab::Foods, "Foods / prices"),
        (Tab::Results, "Results"),
        (Tab::Production, "Acquire / grow"),
    ] {
        ui::button(world, nav, owner, label, Operation::Tab(tab));
    }
    let body = panel::column(world, frame);
    {
        let mut node = world.get_mut::<Node>(body).unwrap();
        node.overflow = Overflow::scroll_y();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.flex_basis = px(0);
        node.min_width = px(0);
    }
    crate::native::scroll_sand::attach(world, body);
    let controls = panel::row(world, frame);
    world.get_mut::<Node>(controls).unwrap().flex_wrap = FlexWrap::Wrap;
    ui::button(world, controls, owner, "Save", Operation::Save(false));
    ui::button(
        world,
        controls,
        owner,
        "Save and solve",
        Operation::Save(true),
    );
    ui::button(world, controls, owner, "Refresh", Operation::Refresh);
    ui::button(
        world,
        controls,
        owner,
        "Meal preparation sources",
        Operation::Sources,
    );
    panel::credits(world, controls, frame, CREDITS);
    let status = crate::native::edit_mode::label(
        world,
        frame,
        "Import Meal Preparation in Sync, refresh the catalogue, then edit the example profile.",
        13.0,
    );
    world.entity_mut(owner).insert(View {
        ready: false,
        body,
        status,
        fields: BTreeMap::new(),
        initial: BTreeMap::new(),
        bounds: Requirements::default(),
        running: false,
    });
    ui::render(world, owner);
    request(world, owner, Command::Catalog, Reply::Catalog);
    if let Some(plan) = plan {
        request(
            world,
            owner,
            Command::Inspect { plan: plan.clone() },
            Reply::Inspect(plan),
        );
    }
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Demeter Castle",
        "Plan daily nutrition, compare local prices, and schedule food production.",
        Operation::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, DemeterCastle::default()),
    );
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    let target = world.get::<View>(owner).map(|v| v.status);
    if let Some(mut text) = target.and_then(|e| world.get_mut::<Text>(e)) {
        text.0 = message.into();
    }
}

fn request(world: &mut World, owner: Entity, command: Command, reply: Reply) {
    world.init_resource::<Pending>();
    let Some(sender) = crate::native::description::source_sender(world, &Source::Local) else {
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.ready = false;
        }
        status(world, owner, "Connect a local Cell to use Demeter.");
        return;
    };
    let id = nucleus::new_uid("demeter");
    let message = ClientMessage::Act {
        id: id.clone(),
        action: engine::actions::Action::Demeter { request: command },
    };
    match sender.try_send(message) {
        Ok(()) => {
            let session = world.get::<DemeterCastle>(owner).unwrap().session.clone();
            world
                .resource_mut::<Pending>()
                .0
                .insert(id, (owner, reply, session));
            status(world, owner, "Working…");
        }
        Err(error) => status(world, owner, error.to_string()),
    }
}

fn poll(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let messages = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for message in messages {
        receive(world, &Source::Local, &message);
    }
    if world.resource::<PollClock>().0.elapsed() < std::time::Duration::from_millis(250) {
        return;
    }
    world.resource_mut::<PollClock>().0 = std::time::Instant::now();
    let mut pending = world.remove_resource::<Pending>().unwrap();
    pending
        .0
        .retain(|_, (owner, _, _)| world.get::<DemeterCastle>(*owner).is_some());
    world.insert_resource(pending);
    if crate::native::description::source_sender(world, &Source::Local).is_some() {
        let waiting: Vec<_> = world
            .query::<(Entity, &DemeterCastle, &View)>()
            .iter(world)
            .filter(|(_, _, v)| !v.ready)
            .map(|(e, s, _)| (e, s.plan.clone()))
            .collect();
        for (owner, plan) in waiting {
            if !world
                .resource::<Pending>()
                .0
                .values()
                .any(|(e, _, _)| *e == owner)
            {
                request(world, owner, Command::Catalog, Reply::Catalog);
                if let Some(plan) = plan {
                    request(
                        world,
                        owner,
                        Command::Inspect { plan: plan.clone() },
                        Reply::Inspect(plan),
                    );
                }
            }
        }
    }
    let candidates: Vec<_> = world
        .query::<(Entity, &DemeterCastle, &View)>()
        .iter(world)
        .filter(|(_, _, view)| view.running)
        .filter_map(|(e, s, _)| s.run.clone().map(|r| (e, r)))
        .collect();
    for (owner, run) in candidates {
        if !world
            .resource::<Pending>()
            .0
            .values()
            .any(|(e, _, _)| *e == owner)
        {
            request(world, owner, Command::Poll { run }, Reply::Poll);
        }
    }
}

impl Action for Operation {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::native::workspace::Workspaces>(owner)
                .map_or(1, |w| w.active);
            let position = world
                .get::<crate::native::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |v| v.center);
            spawn(world, owner, workspace, position, DemeterCastle::default());
            return;
        }
        if !world.entities().contains(owner) {
            return;
        }
        let captured = editor::capture(world, owner);
        if let Err(error) = captured {
            status(world, owner, error);
            if !matches!(
                self,
                Self::Tab(_)
                    | Self::New
                    | Self::Refresh
                    | Self::Open(_)
                    | Self::Cancel
                    | Self::Sources
                    | Self::Reference(_)
            ) {
                return;
            }
        }
        if matches!(
            self,
            Self::Select(_)
                | Self::Remove(_)
                | Self::Profile(_)
                | Self::Group(_)
                | Self::ResetOverrides
                | Self::ToggleNutrient(_)
                | Self::RemoveShare(_)
                | Self::AddNutrient
                | Self::Fixed
                | Self::SaveOffer
                | Self::UseOffer(_)
                | Self::AddStock
                | Self::RemoveStock(_)
                | Self::Acquire(_)
                | Self::Exclude(_)
                | Self::Template(_)
                | Self::ConfirmProduction
                | Self::Fallback
                | Self::AddForecast
                | Self::RemoveForecast(_)
                | Self::AddResource
                | Self::RemoveResource(_)
                | Self::SupermarketAll(_)
        ) {
            let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
            state.edit_version = state.edit_version.saturating_add(1);
            state.changed = true;
            state.stale = true;
        }
        match self {
            Self::Create => {}
            Self::New => {
                world.entity_mut(owner).insert(DemeterCastle::default());
                ui::render(world, owner);
                request(world, owner, Command::Catalog, Reply::Catalog);
            }
            Self::Refresh => {
                let plan = world.get::<DemeterCastle>(owner).unwrap().plan.clone();
                request(world, owner, Command::Catalog, Reply::Catalog);
                if let Some(plan) = plan {
                    request(
                        world,
                        owner,
                        Command::Inspect { plan: plan.clone() },
                        Reply::Inspect(plan),
                    );
                }
            }
            Self::Open(plan) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.plan = Some(plan.clone());
                s.session = nucleus::new_uid("demeter-editor");
                s.drafts.clear();
                s.changed = false;
                drop(s);
                request(
                    world,
                    owner,
                    Command::Inspect { plan: plan.clone() },
                    Reply::Inspect(plan.clone()),
                );
            }
            Self::Tab(tab) => {
                world.get_mut::<DemeterCastle>(owner).unwrap().tab = *tab;
                let body = world.get::<View>(owner).unwrap().body;
                if let Some(mut scroll) = world.get_mut::<ScrollPosition>(body) {
                    scroll.0 = Vec2::ZERO;
                }
                ui::render(world, owner);
            }
            Self::Search => ui::render(world, owner),
            Self::Select(food) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.selected_food = Some(food.clone());
                if !s.config.foods.iter().any(|f| &f.food == food) {
                    s.config.foods.push(FoodChoice {
                        food: food.clone(),
                        bounds: Vec::new(),
                        acquisition: Acquisition::Buy,
                        volume_ml_per_g: None,
                    });
                    s.changed = true;
                }
                drop(s);
                ui::render(world, owner);
            }
            Self::Remove(food) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.config.foods.retain(|f| &f.food != food);
                s.config.stock.retain(|s| &s.food != food);
                s.config.forecasts.retain(|f| &f.stock.food != food);
                s.config.production.retain(|c| &c.food != food);
                s.selected_food = None;
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::Save(solve) => {
                let s = world.get::<DemeterCastle>(owner).unwrap();
                request(
                    world,
                    owner,
                    Command::Save {
                        plan: s.plan.clone(),
                        expected_revision: s.revision,
                        config: s.config.clone(),
                    },
                    Reply::Save(*solve, s.edit_version),
                );
            }
            Self::Vocabulary => {
                let currency = world
                    .get::<DemeterCastle>(owner)
                    .unwrap()
                    .config
                    .currency
                    .clone();
                request(
                    world,
                    owner,
                    Command::Vocabulary { currency },
                    Reply::Vocabulary,
                );
            }
            Self::Profile(enabled) => {
                editor::profile_mode(world, owner, *enabled);
                ui::render(world, owner);
            }
            Self::Group(group) => {
                if let Some(profile) = world
                    .get_mut::<DemeterCastle>(owner)
                    .unwrap()
                    .config
                    .profile
                    .as_mut()
                {
                    profile.group = *group;
                }
                world.get_mut::<DemeterCastle>(owner).unwrap().changed = true;
                ui::render(world, owner);
            }
            Self::ResetOverrides => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.config.overrides.clear();
                s.config.disabled_nutrients.clear();
                s.config.energy_share_overrides = None;
                s.drafts
                    .retain(|k, _| !k.starts_with("nutrient:") && !k.starts_with("share:"));
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::ToggleNutrient(field) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                if s.config.profile.is_none() {
                    s.config.manual.bounds.retain(|b| &b.nutrient != field);
                    s.config.overrides.retain(|b| &b.nutrient != field);
                } else if s.config.disabled_nutrients.contains(field) {
                    s.config.disabled_nutrients.retain(|f| f != field);
                } else {
                    s.config.disabled_nutrients.push(field.clone());
                    s.config.overrides.retain(|b| &b.nutrient != field);
                }
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::RemoveShare(field) => {
                let mut s = world.get::<DemeterCastle>(owner).unwrap().clone();
                match editor::requirements(&s) {
                    Ok(mut r) => {
                        r.energy_shares.retain(|v| &v.nutrient != field);
                        s.config.energy_share_overrides = Some(r.energy_shares);
                        s.changed = true;
                        world.entity_mut(owner).insert(s);
                        ui::render(world, owner);
                    }
                    Err(e) => status(world, owner, e),
                }
            }
            Self::AddNutrient
            | Self::Fixed
            | Self::SaveOffer
            | Self::AddStock
            | Self::Template(_)
            | Self::ConfirmProduction
            | Self::AddForecast
            | Self::AddResource => {
                if let Err(error) = editor::operation(world, owner, self) {
                    status(world, owner, error);
                } else {
                    ui::render(world, owner);
                }
            }
            Self::UseOffer(offer) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                if s.config.offers.contains(offer) {
                    s.config.offers.retain(|v| v != offer);
                } else {
                    s.config.offers.push(offer.clone());
                }
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::RemoveStock(id) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.config.stock.retain(|v| &v.id != id);
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::RemoveForecast(id) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.config.forecasts.retain(|v| &v.stock.id != id);
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::Exclude(exclude) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                let selected = s.selected_food.clone();
                if let Some(f) = s
                    .config
                    .foods
                    .iter_mut()
                    .find(|f| Some(&f.food) == selected.as_ref())
                {
                    f.bounds.retain(|b| b.period != Period::Day);
                    if *exclude {
                        f.bounds.push(demeter::FoodBound {
                            period: Period::Day,
                            bounds: Bounds {
                                lower: None,
                                upper: Some(demeter::exact("0").unwrap()),
                            },
                        });
                    }
                }
                if let Some(id) = selected {
                    s.drafts
                        .retain(|k, _| !k.starts_with(&format!("food:{id}:day:")));
                }
                s.changed = true;
                s.stale = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::Fallback => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                let selected = s.selected_food.clone();
                if let Some(c) = s
                    .config
                    .production
                    .iter_mut()
                    .find(|c| Some(&c.food) == selected.as_ref())
                {
                    c.supermarket_fallback = !c.supermarket_fallback;
                }
                s.changed = true;
                s.stale = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::RemoveResource(resource) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                let selected = s.selected_food.clone();
                if let Some(c) = s
                    .config
                    .production
                    .iter_mut()
                    .find(|c| Some(&c.food) == selected.as_ref())
                {
                    c.resources_per_unit.remove(resource);
                    c.resource_capacity.remove(resource);
                }
                s.changed = true;
                s.stale = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::MissingPage(delta) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.missing_page = s.missing_page.saturating_add_signed(*delta).min(1000);
                drop(s);
                ui::render(world, owner);
            }
            Self::TaskPage(delta) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                s.task_page = s.task_page.saturating_add_signed(*delta).min(100);
                drop(s);
                ui::render(world, owner);
            }
            Self::Acquire(acquisition) => {
                let mut s = world.get_mut::<DemeterCastle>(owner).unwrap();
                let selected = s.selected_food.clone();
                if let Some(food) = s
                    .config
                    .foods
                    .iter_mut()
                    .find(|f| Some(&f.food) == selected.as_ref())
                {
                    food.acquisition = *acquisition;
                }
                s.changed = true;
                drop(s);
                ui::render(world, owner);
            }
            Self::SupermarketAll(enabled) => {
                world
                    .get_mut::<DemeterCastle>(owner)
                    .unwrap()
                    .config
                    .buy_all = *enabled;
                world.get_mut::<DemeterCastle>(owner).unwrap().changed = true;
                ui::render(world, owner);
            }
            Self::Preview | Self::Accept => {
                let s = world.get::<DemeterCastle>(owner).unwrap();
                if s.changed || s.stale {
                    status(
                        world,
                        owner,
                        "Save and solve the current inputs before reviewing or accepting tasks.",
                    );
                    return;
                }
                let (Some(plan), Some(run)) = (s.plan.clone(), s.run.clone()) else {
                    status(world, owner, "Solve a saved plan first.");
                    return;
                };
                if matches!(self, Self::Preview) {
                    request(world, owner, Command::Project { plan, run }, Reply::Project);
                } else {
                    request(world, owner, Command::Accept { plan, run }, Reply::Accept);
                }
            }
            Self::Cancel => {
                let run = world.get::<DemeterCastle>(owner).unwrap().run.clone();
                if let Some(run) = run {
                    request(world, owner, Command::Cancel { run }, Reply::Poll);
                }
            }
            Self::Sources => {
                let Some(parent) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
                    return;
                };
                crate::native::sync_castle::open_source(
                    world,
                    parent,
                    "institute/trails/health/Meal-preparation.lingua",
                );
            }
            Self::Reference(reference) => {
                if let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) {
                    crate::native::full_record::open(world, root, reference, Source::Local);
                }
            }
        }
    }
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    if source != &Source::Local {
        return;
    }
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    let Some((owner, reply, session)) = world
        .get_resource_mut::<Pending>()
        .and_then(|mut p| p.0.remove(id))
    else {
        return;
    };
    if world
        .get::<DemeterCastle>(owner)
        .is_none_or(|s| s.session != session)
    {
        return;
    }
    let _ = editor::capture(world, owner);
    let data = match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => data,
        ServerMessage::Error { message, .. } => {
            if matches!(reply, Reply::Catalog) {
                world.get_mut::<View>(owner).unwrap().ready = true;
            }
            status(world, owner, message);
            if matches!(reply, Reply::Start(_) | Reply::Poll) {
                world.get_mut::<View>(owner).unwrap().running = false;
            }
            return;
        }
        _ => {
            status(world, owner, "The Cell returned no planning data.");
            return;
        }
    };
    let mut state = world.get::<DemeterCastle>(owner).unwrap().clone();
    let mut followup = None;
    let message = match reply {
        Reply::Catalog => {
            world.get_mut::<View>(owner).unwrap().ready = true;
            state.catalog = data.clone();
            if state.config.references.is_empty() && state.config.profile.is_some() {
                state.config.references = data["references"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v["uid"].as_str().map(str::to_owned))
                    .collect();
                state.edit_version = state.edit_version.saturating_add(1);
            }
            "Catalogue refreshed. Select foods and enter local offers.".to_owned()
        }
        Reply::Vocabulary => {
            if !state.catalog.is_object() {
                state.catalog = json!({});
            }
            state.catalog["vocabulary"]["price"] = data["price_predicate"].clone();
            "Local price assertion and units are ready.".into()
        }
        Reply::Offer => {
            let Some(id) = data["offer"].as_str() else {
                status(world, owner, "The offer was not returned.");
                return;
            };
            let market = &data["market"];
            if let Some(offers) = state.catalog["offers"].as_array() {
                let previous: Vec<_> = offers
                    .iter()
                    .filter(|v| {
                        v["market"]["offer"]["food"] == market["offer"]["food"]
                            && v["market"]["vendor"] == market["vendor"]
                    })
                    .filter_map(|v| v["uid"].as_str())
                    .collect();
                state
                    .config
                    .offers
                    .retain(|v| !previous.contains(&v.as_str()));
            }
            if !state.config.offers.iter().any(|v| v == id) {
                state.config.offers.push(id.into());
            }
            if !state.catalog["offers"].is_array() {
                state.catalog["offers"] = json!([]);
            }
            state.catalog["offers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"uid":id,"market":market}));
            if let Some(food) = market["offer"]["food"].as_str() {
                state.drafts.remove(&format!("{food}|offer:id"));
            }
            state.edit_version = state.edit_version.saturating_add(1);
            state.changed = true;
            state.stale = true;
            "Price assertion saved. Save the plan to use this observation.".into()
        }
        Reply::Save(solve, version) => {
            let Some(uid) = data["plan"].as_str() else {
                status(world, owner, "Plan save returned no identity.");
                return;
            };
            state.plan = Some(uid.into());
            state.revision = data["saved"]["revision"].as_u64().unwrap_or(state.revision);
            state.stale = true;
            if state.edit_version == version {
                match serde_json::from_value::<Config>(data["saved"]["config"].clone()) {
                    Ok(config) => state.config = config,
                    Err(error) => {
                        status(world, owner, error.to_string());
                        return;
                    }
                }
                state.changed = false;
                editor::retain_auxiliary(&mut state.drafts);
                if solve {
                    followup = Some((
                        Command::Solve {
                            plan: uid.into(),
                            options: nucleus::optimization::Options {
                                time_limit_ms: state.budget_ms,
                                slice_ms: 100,
                            },
                        },
                        Reply::Start(state.revision),
                    ));
                }
                "Plan saved.".into()
            } else {
                "An earlier edit was saved. Your newer edits are preserved; save them before solving.".into()
            }
        }
        Reply::Inspect(target) => {
            if state.plan.as_deref() != Some(&target) {
                return;
            }
            if !state.changed {
                match serde_json::from_value::<Config>(data["saved"]["config"].clone()) {
                    Ok(config) => state.config = config,
                    Err(error) => {
                        status(world, owner, error.to_string());
                        return;
                    }
                }
                state.revision = data["saved"]["revision"].as_u64().unwrap_or(0);
                editor::retain_auxiliary(&mut state.drafts);
            }
            state.stale = data["last"]["stale"].as_bool().unwrap_or(true) || state.changed;
            state.managed_tasks = data["tasks"].as_array().cloned().unwrap_or_default();
            state.run = data["saved"]["last_run"].as_str().map(str::to_owned);
            if !state.stale {
                state.run_revision = Some(state.revision);
            }
            if let Some(run) = &state.run {
                followup = Some((Command::Poll { run: run.clone() }, Reply::Poll));
            }
            data["error"]
                .as_str()
                .unwrap_or("Plan loaded. Local unsaved editor values are preserved.")
                .into()
        }
        Reply::Start(revision) => {
            state.run = data["run"].as_str().map(str::to_owned);
            state.run_revision = Some(revision);
            state.stale = state.changed || state.revision != revision;
            state.result = None;
            state.projection = None;
            state.tab = Tab::Results;
            world.get_mut::<View>(owner).unwrap().running = true;
            "Solving in the background. You can cancel this run.".into()
        }
        Reply::Poll => {
            let running =
                ["running", "cancelling"].contains(&data["state"].as_str().unwrap_or_default());
            world.get_mut::<View>(owner).unwrap().running = running;
            if data["result"].is_object() {
                match serde_json::from_value::<demeter::Outcome>(data["result"].clone()) {
                    Ok(result) => state.result = Some(result),
                    Err(error) => {
                        status(world, owner, error.to_string());
                        return;
                    }
                }
            }
            format!(
                "Solve {}{}",
                data["state"].as_str().unwrap_or("unknown"),
                data["message"]
                    .as_str()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            )
        }
        Reply::Project | Reply::Accept => {
            if let Some(tasks) = data["managed_tasks"].as_array() {
                state.managed_tasks = tasks.clone();
            }
            if let Ok(result) = serde_json::from_value::<demeter::Outcome>(data["result"].clone()) {
                state.result = Some(result);
            }
            if let Ok(projection) = serde_json::from_value::<demeter::production::Projection>(
                data["projection"].clone(),
            ) {
                state.projection = Some(projection);
            }
            state.tab = Tab::Production;
            if data["accepted"] == true {
                format!(
                    "Tasks accepted. {} completed, started, or ongoing-care tasks were preserved.",
                    data["preserved"].as_u64().unwrap_or(0)
                )
            } else {
                "Review the dated tasks and gaps, then accept them.".into()
            }
        }
    };
    world.entity_mut(owner).insert(state);
    ui::render(world, owner);
    status(world, owner, message);
    if let Some((command, reply)) = followup {
        request(world, owner, command, reply);
    }
}
