use super::*;
use crate::native::{cell_bridge::CellMessage, protein_area::{RecordBinding, Source}, shortcuts::configuration::LayoutSettings};
use bevy::ecs::message::MessageCursor;
use nucleus::window_opening::{Dimension, Properties, Rule, Window, RULE_NAMESPACE, WINDOW_NAMESPACE};
use serde_json::Value;
use crate::host::OpeningRevision as Stamp;
use std::time::{Duration, Instant};

mod catalog;
mod placement;
pub mod ui;
pub use placement::{Prepared, apply_initial, prepare_reapply};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub uid: String,
    pub title: String,
    pub revision: u64,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    pub properties: Properties,
    pub matched: Option<Match>,
    pub warning: Option<String>,
    pub retained: bool,
    stamp: Option<Stamp>,
}

impl Stamp {
    fn current(world: &World) -> Option<Self> {
        let generation = world.get_resource::<Catalog>()?.generation;
        (generation != u64::MAX).then(|| Self { generation, order: world.get_resource::<LayoutSettings>().map(|settings| settings.opening_rules.clone()).unwrap_or_default() })
    }

    fn valid(&self, world: &World) -> bool { Self::current(world).as_ref() == Some(self) }
}

fn request_view(world: &World, root: Entity) -> Option<crate::host::OpeningView> {
    let spaces = world.get::<Workspaces>(root)?;
    let views = world.get_resource::<crate::native::output_views::OutputViews>().filter(|views| views.applies_to(root));
    Some(crate::host::OpeningView { canvas: spaces.canvas_id.clone(), workspace: spaces.active,
        output: views.map(|views| views.focused), outputs: views.map(|views| views.outputs.clone()).unwrap_or_default() })
}

pub fn request(world: &World, root: Entity, target: crate::host::WindowInspection) -> Result<crate::host::OpeningRequest, String> {
    if !target.valid() || target.owner != root.to_bits() { return Err("The captured application context is invalid.".into()); }
    let revision = Stamp::current(world).ok_or("Opening preferences are unavailable.")?;
    let view = request_view(world, root).ok_or("The captured canvas is unavailable.")?;
    Ok(crate::host::OpeningRequest { target, revision, view })
}

pub fn request_current(world: &World, root: Entity, request: &crate::host::OpeningRequest) -> bool {
    request.target.valid() && request.target.owner == root.to_bits() && request.revision.valid(world)
        && request_view(world, root).as_ref() == Some(&request.view)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Payload {
    Rule(Rule),
    Window(Window),
}

impl Payload {
    fn revision(&self) -> u64 {
        match self { Self::Rule(rule) => rule.revision, Self::Window(window) => window.revision }
    }

    fn namespace(&self) -> &'static str {
        match self { Self::Rule(_) => RULE_NAMESPACE, Self::Window(_) => WINDOW_NAMESPACE }
    }

    fn value(&self) -> Value {
        match self { Self::Rule(rule) => serde_json::to_value(rule), Self::Window(window) => serde_json::to_value(window) }.unwrap()
    }

    fn validate(&self) -> Result<(), String> {
        match self { Self::Rule(rule) => rule.validate(), Self::Window(window) => window.validate() }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Entry {
    title: String,
    exists: bool,
    value: Value,
    payload: Result<Payload, String>,
}

struct Feed {
    namespace: &'static str,
    selected: BTreeSet<String>,
    id: Option<String>,
    entries: BTreeMap<String, Entry>,
    deadline: Option<Instant>,
    retry: Option<Instant>,
    ready: bool,
    error: Option<String>,
    blocked: bool,
    retiring: Option<Retirement>,
}

struct Retirement {
    id: String,
    sender: tokio::sync::mpsc::Sender<cell::ClientMessage>,
}

impl Feed {
    fn new(namespace: &'static str) -> Self {
        Self { namespace, selected: BTreeSet::new(), id: None, entries: BTreeMap::new(), deadline: None, retry: None, ready: false, error: None, blocked: false, retiring: None }
    }
}

struct Pending {
    owner: Entity,
    uid: String,
    title: String,
    payload: Payload,
    creating: bool,
    created: bool,
    deadline: Instant,
}

#[derive(Resource)]
struct Catalog {
    generation: u64,
    rules: Feed,
    windows: Feed,
    window_refs: BTreeSet<String>,
    sender: Option<tokio::sync::mpsc::Sender<cell::ClientMessage>>,
    cursor: MessageCursor<CellMessage>,
    pending: HashMap<String, Pending>,
    clock: Option<(Instant, tokio::task::JoinHandle<()>)>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self { generation: 1, rules: Feed::new(RULE_NAMESPACE), windows: Feed::new(WINDOW_NAMESPACE), window_refs: BTreeSet::new(), sender: None, cursor: Default::default(), pending: HashMap::new(), clock: None }
    }
}

impl Catalog {
    fn advance(&mut self) { self.generation = self.generation.saturating_add(1); }
}

impl Drop for Catalog {
    fn drop(&mut self) {
        if let Some((_, clock)) = self.clock.take() { clock.abort(); }
        if let Some(sender) = &self.sender {
            for id in [self.rules.id.take(), self.windows.id.take()].into_iter().flatten() {
                let _ = sender.try_send(cell::ClientMessage::Unsubscribe { id });
            }
        }
        for feed in [&mut self.rules, &mut self.windows] {
            if let Some(retiring) = feed.retiring.take() { let _ = retiring.sender.try_send(cell::ClientMessage::Unsubscribe { id: retiring.id }); }
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Catalog>().add_message::<CellMessage>()
        .add_systems(Update, catalog::update.after(crate::native::protein_area::UpdateProteinAreas));
    ui::install(app);
}

pub fn window_references(world: &mut World, references: Vec<String>) {
    world.init_resource::<Catalog>();
    let selected = references.into_iter().filter(|uid| nucleus::valid_uid(uid, "r")).take(512).collect();
    if world.resource::<Catalog>().window_refs != selected { world.resource_mut::<Catalog>().window_refs = selected; }
}

pub fn ready(world: &World) -> bool {
    let configured = world.get_resource::<LayoutSettings>().map(|settings| settings.opening_rules.as_slice()).unwrap_or_default();
    configured.is_empty() || world.get_resource::<Catalog>().is_some_and(|catalog| {
        configured.iter().all(|uid| catalog.rules.selected.contains(uid))
            && (catalog.rules.ready || catalog.rules.error.is_some() && catalog.rules.deadline.is_none() && catalog.rules.retry.is_none())
    })
}

pub fn capture(world: &World, metadata: &Value, window: Option<&str>) -> Decision {
    let configured = world.get_resource::<LayoutSettings>().map(|settings| settings.opening_rules.as_slice()).unwrap_or_default();
    let Some(catalog) = world.get_resource::<Catalog>() else {
        return Decision { warning: (!configured.is_empty()).then(|| "Opening rules are not loaded; normal opening preferences were used.".into()), ..Default::default() };
    };
    let mut decision = Decision { stamp: Stamp::current(world), ..Default::default() };
    for uid in configured {
        let Some(entry) = catalog.rules.entries.get(uid) else {
            decision.warning = Some(catalog.rules.error.clone().unwrap_or_else(|| "Opening rules are loading or unavailable; normal opening preferences were used.".into()));
            break;
        };
        match &entry.payload {
            Ok(Payload::Rule(rule)) if rule.matches(metadata) => {
                decision.properties = rule.properties.clone();
                decision.matched = Some(Match { uid: uid.clone(), title: entry.title.clone(), revision: rule.revision });
                break;
            }
            Ok(Payload::Rule(_)) => {}
            Ok(_) => { decision.warning = Some("An opening rule has the wrong Record extension.".into()); break; }
            Err(error) => { decision.warning = Some(format!("Opening rule {} is unavailable: {error}", entry.title)); break; }
        }
    }
    if let Some(uid) = window {
        if let Some(Entry { payload: Ok(Payload::Window(override_)), .. }) = catalog.windows.entries.get(uid) {
            decision.properties = override_.properties.over(&decision.properties);
        } else {
            let warning = match catalog.windows.entries.get(uid).map(|entry| &entry.payload) {
                Some(Err(error)) => format!("Window opening overrides are unavailable: {error}"),
                Some(Ok(_)) => "The window Record has the wrong opening-preference type.".into(),
                None => catalog.windows.error.clone().unwrap_or_else(|| "Window opening overrides are loading; retained preferences were used where available.".into()),
            };
            decision.warning.get_or_insert(warning);
        }
    }
    decision
}

#[cfg(test)]
mod tests;
