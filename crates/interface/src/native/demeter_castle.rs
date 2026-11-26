// non-lean-yet
// Scope: mod catalog, mod editor, mod persistence, mod tests.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 224, 307, 354,
//   359, 379, 400, 414, 425, 509, 515, 522, 530, 531, 545, 548, 575, 577, 588, 632, 640, 671, 693,
//   722, 738, 767, 799, 810, 887, 892, 909, 1008, 1042.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (76, 77, 78, 248, 259, 268, 359, 390, 496, 509, 521, 535, 536, 544, 559, 570, 585,
//   603, 610, 614, 625, 640, 667, 678, 685, 692, 706, 721, 737, 754, 760, 766, 783, 786, 790, 810,
//   861, 874, 878, 887, 891, 941, 1022, 1028).

mod catalog;
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
use std::{collections::BTreeMap, sync::Arc};

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
    Plan,
    Profile,
    Requirements,
    Foods,
    Prices,
    Stock,
    Nutrition,
    Results,
    Meals,
    Shopping,
    Production,
}

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct DemeterCastle {
    pub plan: Option<String>,
    pub revision: u64,
    pub config: Config,
    pub tab: Tab,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    #[serde(skip)]
    composition: Option<Arc<Value>>,
    #[serde(skip)]
    composition_expanded: bool,
    #[serde(skip)]
    shopping: Option<Arc<Value>>,
    #[serde(skip)]
    meal_day: usize,
    #[serde(skip)]
    food_picker_open: bool,
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
    pub managed_tasks: Arc<Vec<Value>>,
    #[serde(skip)]
    pub catalog: catalog::Catalog,
    #[serde(skip)]
    pub result: Option<Arc<demeter::Outcome>>,
    #[serde(skip)]
    pub projection: Option<Arc<demeter::production::Projection>>,
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
            tab: Tab::Plan,
            sidebar_collapsed: false,
            composition: None,
            composition_expanded: false,
            shopping: None,
            meal_day: 0,
            food_picker_open: false,
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
            managed_tasks: Default::default(),
            catalog: Default::default(),
            result: None,
            projection: None,
        }
    }
}

impl DemeterCastle {
    pub fn valid(&self) -> bool {
        self.config.validate().is_ok() && self.editor_valid()
    }

    fn editor_valid(&self) -> bool {
        self.budget_ms <= 30_000
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
    sidebar: Entity,
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
    ToggleSidebar,
    ToggleFoodPicker,
    OpenShoppingRule(String),
    MealDay(i8),
    ExpandComposition,
    ProfileRecord(bool),
    UnlinkProfile,
    AutoShopping(bool),
    ShoppingReminder,
    CompleteShopping(String),
    OpenShoppingProtein,
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
    Composition(String),
    ProfileRecord(u64),
    Shopping,
    ShoppingReminder,
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
    ui::button(world, nav, owner, "Pages", Operation::ToggleSidebar);
    let layout = panel::row(world, frame);
    {
        let mut node = world.get_mut::<Node>(layout).unwrap();
        node.flex_wrap = FlexWrap::NoWrap;
        node.align_items = AlignItems::Stretch;
        node.flex_grow = 1.0;
        node.flex_basis = px(0);
        node.min_height = px(0);
    }
    let sidebar = panel::column(world, layout);
    {
        let mut node = world.get_mut::<Node>(sidebar).unwrap();
        node.width = percent(28);
        node.max_width = px(154);
        node.min_width = px(0);
        node.flex_shrink = 0.0;
        node.overflow = Overflow::scroll_y();
    }
    crate::native::scroll_sand::attach(world, sidebar);
    let body = panel::column(world, layout);
    {
        let mut node = world.get_mut::<Node>(body).unwrap();
        node.width = Val::Auto;
        node.flex_shrink = 1.0;
        node.overflow = Overflow::scroll_y();
        node.flex_grow = 1.0;
        node.flex_basis = px(0);
        node.min_width = px(0);
    }
    crate::native::scroll_sand::attach(world, body);
    let status =
        crate::native::edit_mode::label(world, frame, "Choose a saved plan or create one.", 13.0);
    world.entity_mut(owner).insert(View {
        ready: false,
        sidebar,
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

fn status(world: &mut World, owner: Entity, message: impl AsRef<str>) {
    if let Some(target) = world.get::<View>(owner).map(|view| view.status) {
        panel::status(world, target, message);
    }
}

fn solve_status(data: &Value) -> String {
    format!(
        "Solve {}{}",
        data["state"].as_str().unwrap_or("unknown"),
        data["message"]
            .as_str()
            .map(|message| format!(": {message}"))
            .unwrap_or_default()
    )
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
    for message in messages.iter().map(|message| message.as_ref()) {
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
                    | Self::ToggleSidebar
                    | Self::ToggleFoodPicker
                    | Self::OpenShoppingRule(_)
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
            Self::Remove(_)
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
                s.shopping = None;
                s.composition = None;
                s.meal_day = 0;
                s.selected_food = None;
                s.food_picker_open = false;
                s.result = None;
                s.projection = None;
                s.changed = false;
                drop(s);
                request(
                    world,
                    owner,
                    Command::Inspect { plan: plan.clone() },
                    Reply::Inspect(plan.clone()),
                );
            }
            Self::OpenShoppingRule(rule) => {
                crate::native::karma_castle::open_rule(world, owner, rule)
            }
            Self::ToggleFoodPicker => {
                let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
                state.food_picker_open = !state.food_picker_open;
                ui::render(world, owner);
            }
            Self::ToggleSidebar => {
                let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
                state.sidebar_collapsed = !state.sidebar_collapsed;
                ui::render(world, owner);
            }
            Self::Tab(tab) => {
                world.get_mut::<DemeterCastle>(owner).unwrap().tab = *tab;
                let body = world.get::<View>(owner).unwrap().body;
                if let Some(mut scroll) = world.get_mut::<ScrollPosition>(body) {
                    scroll.0 = Vec2::ZERO;
                }
                ui::render(world, owner);
                refresh_page(world, owner);
            }
            Self::MealDay(step) => {
                let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
                state.meal_day = state
                    .meal_day
                    .saturating_add_signed(isize::from(*step))
                    .min(
                        state
                            .result
                            .as_ref()
                            .map_or(0, |result| result.days.len().saturating_sub(1)),
                    );
                ui::render(world, owner);
            }
            Self::ExpandComposition => {
                let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
                state.composition_expanded = !state.composition_expanded;
                ui::render(world, owner);
            }
            Self::ProfileRecord(write) => {
                let state = world.get::<DemeterCastle>(owner).unwrap();
                let token = editor::text(state, "profile-record").trim();
                let record = (!token.is_empty())
                    .then(|| token.to_owned())
                    .or_else(|| state.config.profile_record.clone());
                let profile = if *write {
                    let Some(profile) = state.config.profile.clone() else {
                        status(world, owner, "Choose an adult profile first.");
                        return;
                    };
                    Some(profile)
                } else {
                    None
                };
                let version = state.edit_version;
                request(
                    world,
                    owner,
                    Command::ProfileRecord { record, profile },
                    Reply::ProfileRecord(version),
                );
            }
            Self::UnlinkProfile => {
                let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
                state.config.profile_record = None;
                state.drafts.remove("profile-record");
                state.changed = true;
                state.stale = true;
                state.edit_version = state.edit_version.saturating_add(1);
                ui::render(world, owner);
            }
            Self::AutoShopping(enabled) => {
                let mut state = world.get_mut::<DemeterCastle>(owner).unwrap();
                if *enabled
                    && (!state.config.forecasts.is_empty() || !state.config.production.is_empty())
                {
                    status(
                        world,
                        owner,
                        "Choose a supermarket-only plan for live shopping.",
                    );
                    return;
                }
                state.config.auto_shopping = *enabled;
                if *enabled {
                    state.config.buy_all = true;
                    state.config.horizon_days = state.config.days;
                    state.drafts.remove("horizon");
                }
                let solve = *enabled
                    && state.result.as_ref().is_some_and(|result| {
                        matches!(
                            result.solution.status,
                            nucleus::optimization::Status::Optimal
                                | nucleus::optimization::Status::Feasible
                        )
                    });
                state.changed = true;
                state.stale = true;
                state.edit_version = state.edit_version.saturating_add(1);
                ui::render(world, owner);
                Operation::Save(solve).apply(world, owner);
            }
            Self::ShoppingReminder => {
                let state = world.get::<DemeterCastle>(owner).unwrap();
                let Some(plan) = state.plan.clone() else {
                    status(world, owner, "Save the plan before scheduling a reminder.");
                    return;
                };
                if state.changed {
                    status(
                        world,
                        owner,
                        "Save your changes before scheduling a reminder.",
                    );
                    return;
                }
                let first_at = match editor::reminder_time(state) {
                    Ok(first_at) => first_at,
                    Err(error) => {
                        status(world, owner, error);
                        return;
                    }
                };
                request(
                    world,
                    owner,
                    Command::ShoppingReminder { plan, first_at },
                    Reply::ShoppingReminder,
                );
            }
            Self::CompleteShopping(need) => {
                if let Some(plan) = world
                    .get::<DemeterCastle>(owner)
                    .and_then(|state| state.plan.clone())
                {
                    request(
                        world,
                        owner,
                        Command::CompleteShopping {
                            plan,
                            need: need.clone(),
                        },
                        Reply::Shopping,
                    );
                }
            }
            Self::OpenShoppingProtein => {
                let state = world.get::<DemeterCastle>(owner).unwrap();
                let Some(data) = &state.shopping else { return };
                let Ok(query) = serde_json::from_value(data["query"].clone()) else {
                    return;
                };
                let draft = crate::native::protein_castle::ProteinDraft::from_protein(
                    format!("Shopping — {}", state.config.name),
                    String::new(),
                    query,
                );
                let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
                    return;
                };
                let workspace = world
                    .get::<crate::native::workspace::WorkspaceMember>(owner)
                    .map_or(1, |entry| entry.0);
                let position = world
                    .get::<crate::native::canvas::CanvasItem>(owner)
                    .map_or(DVec2::ZERO, |item| {
                        item.position + DVec2::new(f64::from(item.size.x) + 24.0, 0.0)
                    });
                let protein =
                    crate::native::protein_castle::spawn(world, root, workspace, position, draft);
                crate::native::protein_castle::ProteinAction::Run.apply(world, protein);
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
                    s.stale = true;
                    s.edit_version = s.edit_version.saturating_add(1);
                }
                s.composition = None;
                s.composition_expanded = false;
                s.food_picker_open = false;
                drop(s);
                ui::render(world, owner);
                refresh_page(world, owner);
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
                    "institute/trails/ergon/gaia/nutrition/Meal-preparation.lingua",
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

fn refresh_page(world: &mut World, owner: Entity) {
    let state = world.get::<DemeterCastle>(owner).unwrap();
    match state.tab {
        Tab::Nutrition => {
            if let Some(food) = state.selected_food.clone() {
                request(
                    world,
                    owner,
                    Command::FoodComposition { food: food.clone() },
                    Reply::Composition(food),
                );
            }
        }
        Tab::Shopping => {
            if let Some(plan) = state.plan.clone() {
                request(world, owner, Command::Shopping { plan }, Reply::Shopping);
            }
        }
        _ => {}
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
    let _reply = nucleus::performance::Span::new("demeter-reply-application");
    if matches!(reply, Reply::Poll)
        && let ServerMessage::ActionOk {
            data: Some(data), ..
        } = message
        && !data["result"].is_object()
    {
        world.get_mut::<View>(owner).unwrap().running =
            ["running", "cancelling"].contains(&data["state"].as_str().unwrap_or_default());
        status(world, owner, solve_status(data));
        nucleus::performance::add("demeter-status-only-replies", 1);
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
    let mut refresh_current_page = false;
    let message = match reply {
        Reply::Composition(food) => {
            if state.selected_food.as_deref() != Some(&food) {
                return;
            }
            state.composition = Some(Arc::new(data["composition"].clone()));
            "Food composition loaded.".into()
        }
        Reply::ProfileRecord(version) => {
            if state.edit_version != version {
                status(
                    world,
                    owner,
                    "Profile reply received. Your newer editor changes are preserved.",
                );
                return;
            }
            let Some(record) = data["record"].as_str() else {
                return;
            };
            let Ok(profile) = Profile::deserialize(&data["profile"]) else {
                return;
            };
            state.config.profile_record = Some(record.into());
            state.config.profile = Some(profile);
            state.drafts.remove("profile-record");
            for key in ["age", "height", "weight", "activity", "energy"] {
                state.drafts.remove(key);
            }
            state.changed = true;
            state.stale = true;
            state.edit_version = state.edit_version.saturating_add(1);
            if state.config.references.is_empty() {
                state.config.references = state.catalog["references"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry["uid"].as_str().map(str::to_owned))
                    .collect();
            }
            "Profile linked. Save the plan to use this Record.".into()
        }
        Reply::Shopping => {
            if data["plan"].as_str() != state.plan.as_deref() {
                return;
            }
            state.shopping = Some(Arc::new(data.clone()));
            "Shopping list updated.".into()
        }
        Reply::ShoppingReminder => {
            refresh_current_page = true;
            "Frequency, Karma rule, and shopping Protein saved.".into()
        }
        Reply::Catalog => {
            world.get_mut::<View>(owner).unwrap().ready = true;
            state.catalog = data.clone().into();
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
            let catalog = state.catalog.edit();
            if !catalog.is_object() {
                *catalog = json!({});
            }
            catalog["vocabulary"]["price"] = data["price_predicate"].clone();
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
            let catalog = state.catalog.edit();
            if !catalog["offers"].is_array() {
                catalog["offers"] = json!([]);
            }
            catalog["offers"]
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
                match Config::deserialize(&data["saved"]["config"]) {
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
                match Config::deserialize(&data["saved"]["config"]) {
                    Ok(config) => state.config = config,
                    Err(error) => {
                        status(world, owner, error.to_string());
                        return;
                    }
                }
                if let Ok(profile) = Profile::deserialize(&data["linked_profile"]) {
                    state.config.profile = Some(profile);
                }
                if state
                    .selected_food
                    .as_ref()
                    .is_none_or(|id| !state.config.foods.iter().any(|food| &food.food == id))
                {
                    state.selected_food = state.config.foods.first().map(|food| food.food.clone());
                }
                state.revision = data["saved"]["revision"].as_u64().unwrap_or(0);
                editor::retain_auxiliary(&mut state.drafts);
            }
            refresh_current_page = true;
            state.stale = data["last"]["stale"].as_bool().unwrap_or(true) || state.changed;
            state.managed_tasks = Arc::new(data["tasks"].as_array().cloned().unwrap_or_default());
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
            state.meal_day = 0;
            state.tab = Tab::Results;
            world.get_mut::<View>(owner).unwrap().running = true;
            "Solving in the background. You can cancel this run.".into()
        }
        Reply::Poll => {
            let running =
                ["running", "cancelling"].contains(&data["state"].as_str().unwrap_or_default());
            world.get_mut::<View>(owner).unwrap().running = running;
            if data["result"].is_object() {
                match demeter::Outcome::deserialize(&data["result"]) {
                    Ok(result) => state.result = Some(Arc::new(result)),
                    Err(error) => {
                        status(world, owner, error.to_string());
                        return;
                    }
                }
            }
            if let Some(tasks) = data["managed_tasks"].as_array() {
                state.managed_tasks = Arc::new(tasks.clone());
            }
            if let Some(error) = data["shopping_error"].as_str() {
                format!(
                    "{} Shopping list update failed: {error}",
                    solve_status(data)
                )
            } else if data["shopping_published"] == true {
                refresh_current_page = true;
                format!("{} Shopping list updated.", solve_status(data))
            } else {
                solve_status(data)
            }
        }
        Reply::Project | Reply::Accept => {
            if let Some(tasks) = data["managed_tasks"].as_array() {
                state.managed_tasks = Arc::new(tasks.clone());
            }
            if let Ok(result) = demeter::Outcome::deserialize(&data["result"]) {
                state.result = Some(Arc::new(result));
            }
            if let Ok(projection) =
                demeter::production::Projection::deserialize(&data["projection"])
            {
                state.projection = Some(Arc::new(projection));
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
    if refresh_current_page {
        refresh_page(world, owner);
    }
}
