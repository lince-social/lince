pub(crate) mod input;
mod model;
#[cfg(test)]
mod tests;
mod ui;

use crate::{
    actions::Action,
    castle_feed::{self, Frame},
    protein_area::{Config, RecordBinding},
};
use bevy::{math::DVec2, prelude::*};
use model::Item;
pub use model::Selection;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct AssertionCastle {
    pub selection: Option<Selection>,
}

#[derive(Component)]
pub(crate) struct AssertionFeed;

#[derive(Component)]
struct View {
    controls: Entity,
    list: Entity,
    rows: Vec<Item>,
    choices: Vec<Selection>,
    feed: String,
    page: usize,
    selecting: bool,
    message: String,
    error: Option<String>,
    batch: Option<Batch>,
    manual_order: Vec<String>,
    feed_config: Option<Config>,
    dragging: bool,
}

struct Batch {
    rows: Vec<Item>,
    config: Config,
    selection: Selection,
    next: usize,
    waiting: Option<Entity>,
}

#[derive(Component)]
struct Submission(Entity);

pub struct AssertionCastlePlugin;

impl Plugin for AssertionCastlePlugin {
    fn build(&self, app: &mut App) {
        input::install(app);
        app.add_systems(
            Update,
            update.after(crate::protein_area::UpdateProteinAreas),
        );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    castle: AssertionCastle,
) -> Entity {
    let mut config = Config::default();
    config.draft.name = "Assertion list".into();
    config.bindings = ["head", "assertions"]
        .map(crate::protein_area::Binding::new)
        .to_vec();
    let owner = castle_feed::spawn(
        world,
        root,
        workspace,
        position,
        Vec2::new(900.0, 680.0),
        "Assertion Castle",
        config,
    );
    let frame = world.get::<Frame>(owner).unwrap();
    let (area, viewport) = (frame.area, frame.viewport);
    world.entity_mut(area).insert(AssertionFeed);
    world.get_mut::<Node>(viewport).unwrap().flex_direction = FlexDirection::Column;
    let controls = ui::stack(world, viewport);
    let list = ui::stack(world, viewport);
    {
        let mut node = world.get_mut::<Node>(list).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
    }
    crate::scroll_sand::attach(world, list);
    world.entity_mut(owner).insert((
        castle,
        View {
            controls,
            list,
            rows: Vec::new(),
            choices: Vec::new(),
            feed: String::new(),
            page: 0,
            selecting: false,
            message: String::new(),
            error: None,
            batch: None,
            manual_order: Vec::new(),
            feed_config: None,
            dragging: false,
        },
    ));
    ui::render(world, owner);
    owner
}

fn config(world: &World, owner: Entity) -> Option<Config> {
    let area = world.get::<Frame>(owner)?.area;
    world
        .get::<crate::area::InfluenceArea>(area)?
        .protein
        .clone()
}

fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<AssertionCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::laboratory::suspended(world, owner) {
            continue;
        }
        advance(world, owner);
        let area = world.get::<Frame>(owner).unwrap().area;
        let feed = crate::protein_area::calendar_feed(world, area)
            .map(|(_, key)| key)
            .unwrap_or_default();
        let view = world.get::<View>(owner).unwrap();
        if view.batch.is_none() && !view.dragging && view.feed != feed {
            let data = crate::protein_area::calendar_feed(world, area)
                .map(|(rows, _)| rows.to_vec())
                .unwrap_or_default();
            let selection = world
                .get::<AssertionCastle>(owner)
                .unwrap()
                .selection
                .clone();
            let rows = model::ordered(&data, selection.as_ref());
            let current_config = config(world, owner);
            let mut view = world.get_mut::<View>(owner).unwrap();
            if view.feed_config != current_config {
                view.manual_order.clear();
                view.feed_config = current_config;
            }
            view.feed = feed;
            view.choices = model::choices(&data);
            match rows {
                Ok(mut rows) => {
                    model::apply_order(&mut rows, &view.manual_order);
                    view.rows = rows;
                    view.error = None;
                }
                Err(error) => {
                    view.rows.clear();
                    view.error = Some(error);
                }
            }
            view.page = view
                .page
                .min(view.rows.len().saturating_sub(1) / ui::PAGE_SIZE);
            ui::render(world, owner);
        }
        ui::status(world, owner);
    }
}

fn renumber(world: &mut World, owner: Entity) -> Result<(), String> {
    if crate::laboratory::suspended(world, owner) {
        return Err("Workspace is suspended".into());
    }
    let view = world.get::<View>(owner).ok_or("Castle is closed")?;
    if view.batch.is_some() {
        return Err("Renumbering is already in progress".into());
    }
    if let Some(error) = &view.error {
        return Err(error.clone());
    }
    let selection = world
        .get::<AssertionCastle>(owner)
        .unwrap()
        .selection
        .clone()
        .ok_or("Choose an assertion first")?;
    let config = config(world, owner)
        .filter(|config| config.enabled)
        .ok_or("Run the Protein first")?;
    let area = world.get::<Frame>(owner).unwrap().area;
    if !crate::protein_area::feed_ready(world, area) {
        return Err("Wait for the Protein to load".into());
    }
    let (data, feed) =
        crate::protein_area::calendar_feed(world, area).ok_or("Wait for the Protein to load")?;
    if view.feed != feed {
        return Err("The list changed; review it before renumbering".into());
    }
    let mut rows = model::ordered(data, Some(&selection))?;
    model::apply_order(&mut rows, &view.manual_order);
    if rows.is_empty() {
        return Err("No Records to renumber".into());
    }
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.message.clear();
    view.selecting = false;
    view.batch = Some(Batch {
        rows,
        config,
        selection,
        next: 0,
        waiting: None,
    });
    ui::render(world, owner);
    advance(world, owner);
    Ok(())
}

fn advance(world: &mut World, owner: Entity) {
    let Some(batch) = world
        .get::<View>(owner)
        .and_then(|view| view.batch.as_ref())
    else {
        return;
    };
    if config(world, owner).as_ref() != Some(&batch.config) {
        stop(world, owner, "Protein settings changed");
        return;
    }
    if batch.waiting.is_some() {
        return;
    }
    let row = &batch.rows[batch.next];
    let binding = RecordBinding {
        area: world.get::<Frame>(owner).unwrap().area,
        uid: row.uid.clone(),
        source: batch.config.source.clone(),
    };
    let action = engine::actions::Action::ChangeRecord {
        request: engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: row.uid.clone(),
            mutation: engine::record_change::Mutation::NumberAssertion {
                predicate: batch.selection.predicate.clone(),
                position: (batch.next + 1) as u32,
            },
        },
    };
    let submission = world.spawn((Submission(owner), ChildOf(owner))).id();
    match crate::protein_area::execute(world, &binding, submission, action) {
        Ok(()) => {
            world
                .get_mut::<View>(owner)
                .unwrap()
                .batch
                .as_mut()
                .unwrap()
                .waiting = Some(submission)
        }
        Err(error) => {
            world.despawn(submission);
            stop(world, owner, &error);
        }
    }
}

fn stop(world: &mut World, owner: Entity, error: &str) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    let mut pending = None;
    if let Some(batch) = view.batch.take() {
        pending = batch.waiting;
        view.message = format!(
            "Stopped with {} of {} Records confirmed: {error}",
            batch.next,
            batch.rows.len()
        );
    }
    view.feed.clear();
    if let Some(pending) = pending
        && world.get_entity(pending).is_ok()
    {
        world.despawn(pending);
    }
    ui::render(world, owner);
}

pub(crate) fn finished(world: &mut World, submission: Entity, error: Option<String>) -> bool {
    let Some(owner) = world
        .get::<Submission>(submission)
        .map(|submission| submission.0)
    else {
        return false;
    };
    world.despawn(submission);
    if world
        .get::<View>(owner)
        .and_then(|view| view.batch.as_ref())
        .and_then(|batch| batch.waiting)
        != Some(submission)
    {
        return true;
    }
    if let Some(error) = error {
        stop(world, owner, &error);
        return true;
    }
    let mut view = world.get_mut::<View>(owner).unwrap();
    if let Some(batch) = view.batch.as_mut() {
        batch.next += 1;
        batch.waiting = None;
        if batch.next == batch.rows.len() {
            view.message = format!("Renumbered {} Records", batch.next);
            view.batch = None;
            view.feed.clear();
        }
    }
    true
}

#[derive(Clone)]
enum Command {
    Create,
    Choose,
    Select(Selection),
    Renumber,
    Page(bool),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Create) {
            let Some(workspace) = world
                .get::<crate::workspace::Workspaces>(owner)
                .map(|spaces| spaces.active)
            else {
                return;
            };
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(
                world,
                owner,
                workspace,
                position,
                AssertionCastle::default(),
            );
            return;
        }
        if world
            .get::<View>(owner)
            .is_none_or(|view| view.batch.is_some())
        {
            return;
        }
        match self {
            Self::Choose => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.selecting = !view.selecting;
            }
            Self::Select(selection) => {
                if !world
                    .get::<View>(owner)
                    .unwrap()
                    .choices
                    .contains(selection)
                {
                    return;
                }
                world.get_mut::<AssertionCastle>(owner).unwrap().selection =
                    Some(selection.clone());
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.feed.clear();
                view.manual_order.clear();
                view.page = 0;
                view.selecting = false;
                view.message.clear();
                update(world);
            }
            Self::Renumber => {
                if let Err(error) = renumber(world, owner) {
                    world.get_mut::<View>(owner).unwrap().message = error;
                }
            }
            Self::Page(forward) => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.page = if *forward {
                    (view.page + 1).min(view.rows.len().saturating_sub(1) / ui::PAGE_SIZE)
                } else {
                    view.page.saturating_sub(1)
                };
            }
            Self::Create => {}
        }
        ui::render(world, owner);
        ui::status(world, owner);
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Assertion Castle",
        "Order Protein records by an assertion quantity, then renumber them from 1.",
        Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, AssertionCastle::default()),
    );
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedAssertion {
    pub frame: castle_feed::Saved,
    castle: AssertionCastle,
}

impl SavedAssertion {
    pub fn valid(&self) -> bool {
        self.frame.valid() && self.castle.selection.as_ref().is_none_or(Selection::valid)
    }
    pub fn restore(self, world: &mut World, root: Entity) {
        let owner = spawn(
            world,
            root,
            self.frame.workspace,
            DVec2::from_array(self.frame.position),
            self.castle,
        );
        self.frame.apply(world, owner);
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedAssertion> {
    world
        .query::<(Entity, &ChildOf, &AssertionCastle)>()
        .iter(world)
        .filter(|(_, parent, _)| parent.parent() == root)
        .filter_map(|(owner, _, castle)| {
            castle_feed::Saved::capture(world, owner).map(|frame| SavedAssertion {
                frame,
                castle: castle.clone(),
            })
        })
        .collect()
}
