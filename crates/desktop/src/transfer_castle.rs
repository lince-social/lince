mod detail;
mod forms;
mod karma;
mod model;
mod persistence;
mod runtime;
#[cfg(test)]
mod tests;
mod ui;

use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use crate::{
    actions::Action,
    cell_bridge::{CellMessage, ReceiveCell},
    workspace::WorkspaceMember,
};
use model::{Form, array, capability, display, text, title};
pub(crate) use persistence::{SavedTransferCastle, snapshot};

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct TransferCastle {
    pub search: String,
    pub mine: bool,
    pub filter: String,
    pub sort: String,
    pub tree: bool,
    pub selected: String,
    pub person: String,
    pub form: Option<Form>,
    #[serde(default)]
    pub shared_simulation: Option<String>,
}

impl Default for TransferCastle {
    fn default() -> Self {
        Self {
            search: String::new(),
            mine: false,
            filter: "all".into(),
            sort: "attention".into(),
            tree: false,
            selected: String::new(),
            person: String::new(),
            form: None,
            shared_simulation: None,
        }
    }
}

#[derive(Component)]
struct View {
    controls: Entity,
    summary: Entity,
    list: Entity,
    detail: Entity,
    form: Entity,
    status: Entity,
    rows: Vec<Value>,
    records: Vec<Value>,
    context: Value,
    ready: bool,
    pending: Option<Pending>,
    page: usize,
    expanded: HashSet<String>,
    preview: Option<Value>,
    preview_request: Option<String>,
    disclosure_preview: Option<Option<String>>,
    selected_occurrences: HashSet<String>,
    notice: Option<String>,
}

struct Pending {
    id: String,
    form: Form,
}

#[derive(Resource, Default)]
struct Requests(
    HashMap<String, (Entity, String)>,
    HashMap<String, tokio::sync::mpsc::Sender<cell::ClientMessage>>,
);

pub struct TransferCastlePlugin;

impl Plugin for TransferCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_systems(
                Update,
                (runtime::receive.after(ReceiveCell), runtime::maintain).chain(),
            )
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    mut castle: TransferCastle,
) -> Entity {
    world.init_resource::<Requests>();
    if castle.form.as_ref().is_some_and(|form| form.step.is_none()) {
        castle.form = None;
    }
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(1140.0, 820.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(14)),
                row_gap: px(10),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            castle,
        ))
        .id();
    let header = ui::row(world, owner);
    let headings = ui::stack(world, header);
    world.get_mut::<Node>(headings).unwrap().width = Val::Auto;
    crate::edit_mode::label(world, headings, "COMMITMENTS", 10.0);
    crate::edit_mode::label(world, headings, "Transfers", 22.0);
    ui::button(
        world,
        header,
        owner,
        "+ New transfer",
        ui::Command::New(String::new()),
    );
    ui::picker(
        world,
        header,
        owner,
        "Use preset…",
        model::PRESETS
            .iter()
            .map(|preset| (preset.to_string(), ui::Command::New(preset.to_string())))
            .collect(),
    );
    let search = world.get::<TransferCastle>(owner).unwrap().search.clone();
    ui::input(
        world,
        header,
        owner,
        None,
        "Search transfers",
        &search,
        false,
    );
    ui::button(world, header, owner, "Refresh", ui::Command::Refresh);
    let controls = ui::row(world, owner);
    let summary = ui::row(world, owner);
    let status = crate::edit_mode::label(world, owner, "Connecting to the Cell…", 12.0);
    let body = ui::row(world, owner);
    {
        let mut node = world.get_mut::<Node>(body).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
        node.align_items = AlignItems::Stretch;
        node.flex_wrap = FlexWrap::NoWrap;
        node.overflow = Overflow::clip();
    }
    let list = ui::scroll(world, body);
    world.get_mut::<Node>(list).unwrap().width = percent(36);
    let detail = ui::scroll(world, body);
    world.get_mut::<Node>(detail).unwrap().width = percent(64);
    let form = ui::scroll(world, owner);
    world.get_mut::<Node>(form).unwrap().display = Display::None;
    world.entity_mut(owner).insert(View {
        controls,
        summary,
        list,
        detail,
        form,
        status,
        rows: Vec::new(),
        records: Vec::new(),
        context: Value::Null,
        ready: false,
        pending: None,
        page: 0,
        expanded: HashSet::new(),
        preview: None,
        preview_request: None,
        disclosure_preview: None,
        selected_occurrences: HashSet::new(),
        notice: None,
    });
    ui::render(world, owner);
    forms::render(world, owner);
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Transfer Castle",
        "Commitments, negotiation, delivery, settlement, and proof.",
        ui::Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, TransferCastle::default()),
    );
}

pub(crate) fn agreement_level(world: &World, owner: Entity) -> Option<u64> {
    let castle = world.get::<TransferCastle>(owner)?;
    let row = selected(world, owner)?;
    array(&row, "parties")
        .iter()
        .find(|party| party["actor"] == castle.person)
        .and_then(|party| party["level"].as_u64())
}

pub(crate) fn set_agreement(world: &mut World, owner: Entity, level: u64) {
    if agreement_level(world, owner).is_some_and(|current| current >= level) {
        return;
    }
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if !view.ready
        || view.pending.is_some()
        || world.get::<TransferCastle>(owner).unwrap().form.is_some()
    {
        return;
    }
    let Some(row) = selected(world, owner) else {
        return;
    };
    let person = person(world, owner);
    let capability_name = if level == 1 { "review" } else { "commit" };
    if !array(&row, "parties")
        .iter()
        .any(|party| party["actor"] == person && capability(party, capability_name))
    {
        return;
    }
    let mut action = detail::action_base(&row, "set-transfer-agreement-level", &person);
    action["level"] = json!(level);
    ui::Command::Open(Form::action(
        if level == 1 {
            "Checked · ready to agree"
        } else {
            "Agree"
        },
        action,
    ))
    .apply(world, owner);
    ui::Command::Submit.apply(world, owner);
}

pub(crate) fn has_occurrence(world: &World, owner: Entity, promise: &str) -> bool {
    selected(world, owner).is_some_and(|row| {
        array(&row, "occurrences").iter().any(|occurrence| {
            occurrence["promise"] == promise || occurrence["promise_uid"] == promise
        })
    })
}

pub(crate) fn activate_promise(world: &mut World, owner: Entity, promise: &str) {
    if has_occurrence(world, owner, promise) {
        return;
    }
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if !view.ready
        || view.pending.is_some()
        || world.get::<TransferCastle>(owner).unwrap().form.is_some()
    {
        return;
    }
    let Some(row) = selected(world, owner) else {
        return;
    };
    if !array(&row, "promises")
        .iter()
        .any(|item| item["uid"] == promise && capability(item, "activate"))
    {
        return;
    }
    let mut action =
        detail::action_base(&row, "activate-transfer-occurrence", &person(world, owner));
    action["promise"] = json!(promise);
    ui::Command::Open(Form::action("Activate occurrence", action)).apply(world, owner);
    ui::Command::Submit.apply(world, owner);
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        let entity = view.status;
        world.get_mut::<Text>(entity).unwrap().0 = message.into();
    }
}

fn selected(world: &World, owner: Entity) -> Option<Value> {
    let castle = world.get::<TransferCastle>(owner)?;
    world
        .get::<View>(owner)?
        .rows
        .iter()
        .find(|row| text(row, "uid") == castle.selected)
        .cloned()
}

fn person(world: &World, owner: Entity) -> String {
    let castle = world.get::<TransferCastle>(owner).unwrap();
    let view = world.get::<View>(owner).unwrap();
    if view.context["viewer"]["local"] == true {
        castle.person.clone()
    } else {
        text(&view.context["viewer"], "person")
    }
}
