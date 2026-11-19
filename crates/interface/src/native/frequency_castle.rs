use lince_interface::frequency::{interval, model};
mod persistence;
pub(crate) mod tests;
mod ui;

use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use model::{Draft, Frequency};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

use crate::native::{
    actions::Action,
    cell_bridge::{CellMessage, ReceiveCell, replies},
    workspace::WorkspaceMember,
};
pub(crate) use persistence::{SavedFrequencyCastle, snapshot};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct FrequencyCastle {
    pub draft: Option<Draft>,
    pub search: String,
}

#[derive(Default)]
struct Rows {
    values: Arc<Vec<Frequency>>,
    by_uid: HashMap<String, usize>,
}

impl From<Vec<Frequency>> for Rows {
    fn from(values: Vec<Frequency>) -> Self {
        let _prepare = nucleus::performance::Span::new("frequency-row-preparation");
        let mut by_uid = HashMap::with_capacity(values.len());
        for (index, row) in values.iter().enumerate() {
            by_uid.entry(row.uid.clone()).or_insert(index);
        }
        nucleus::performance::add("frequency-rows-prepared", values.len() as u64);
        Self {
            values: Arc::new(values),
            by_uid,
        }
    }
}

impl std::ops::Deref for Rows {
    type Target = [Frequency];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl Rows {
    fn by_uid(&self, uid: &str) -> Option<&Frequency> {
        self.by_uid.get(uid).map(|&index| &self.values[index])
    }
}

#[derive(Component)]
struct View {
    subscription: String,
    controls: Entity,
    form: Entity,
    list: Entity,
    status: Entity,
    rows: Rows,
    pending: Option<Pending>,
    deleting: Option<String>,
    page: usize,
    ready: bool,
}

struct Pending {
    id: String,
    draft: Option<Draft>,
    message: &'static str,
}

impl replies::Pending for View {
    fn pending_id(&self) -> Option<&str> {
        self.pending.as_ref().map(|pending| pending.id.as_str())
    }
}

#[derive(Resource, Default)]
struct Requests(
    HashMap<String, Entity>,
    HashMap<String, crate::native::cell_bridge::Sender>,
);

pub struct FrequencyCastlePlugin;

impl Plugin for FrequencyCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_systems(
                Update,
                (receive.after(ReceiveCell), maintain, ui::tick).chain(),
            )
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::native::actions::ApplyActions),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    castle: FrequencyCastle,
) -> Entity {
    world.init_resource::<Requests>();
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(crate::native::credits::ATTRIBUTIONS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(940.0, 700.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            castle,
        ))
        .id();
    let header = ui::row(world, owner);
    crate::native::edit_mode::label(world, header, "Frequency", 22.0);
    let controls = ui::row(world, header);
    world.get_mut::<Node>(controls).unwrap().width = Val::Auto;
    let search_row = ui::row(world, header);
    {
        let mut node = world.get_mut::<Node>(search_row).unwrap();
        node.width = Val::Auto;
        node.margin = UiRect::left(Val::Auto);
        node.flex_wrap = FlexWrap::NoWrap;
    }
    ui::glyph(world, search_row, crate::native::icons::Icon::Search, 17.0);
    let search = world.get::<FrequencyCastle>(owner).unwrap().search.clone();
    ui::input(world, search_row, owner, 4, "Filter frequencies", &search);
    let scroll = ui::stack(world, owner);
    {
        let mut node = world.get_mut::<Node>(scroll).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
        node.overflow = Overflow::scroll_y();
    }
    crate::native::scroll_sand::attach(world, scroll);
    let form = ui::stack(world, scroll);
    let list = ui::stack(world, scroll);
    let status = crate::native::edit_mode::label(world, owner, "", 12.0);
    world.get_mut::<Node>(status).unwrap().display = Display::None;
    world.entity_mut(owner).insert(View {
        subscription: format!("frequency-castle-{}", owner.to_bits()),
        controls,
        form,
        list,
        status,
        rows: Rows::default(),
        pending: None,
        deleting: None,
        page: 0,
        ready: false,
    });
    ui::render_form(world, owner);
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Frequency Castle",
        "Create, edit, and delete shared frequencies.",
        ui::Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, FrequencyCastle::default()),
    );
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        let entity = view.status;
        let message = message.into();
        world.get_mut::<Node>(entity).unwrap().display = if message.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        world.get_mut::<Text>(entity).unwrap().0 = message;
    }
}

fn send(world: &World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    crate::native::practice_cells::send(world, owner, message)
}

fn submit(
    world: &mut World,
    owner: Entity,
    action: engine::actions::Action,
    draft: Option<Draft>,
    message: &'static str,
) {
    if world
        .get::<View>(owner)
        .is_none_or(|view| view.pending.is_some())
        || crate::native::laboratory::suspended(world, owner)
    {
        return;
    }
    let id = nucleus::new_uid("frequency-castle");
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<View>(owner).unwrap().pending = Some(Pending { id, draft, message });
            status(world, owner, "Saving…");
        }
        Err(error) => status(world, owner, error),
    }
}

fn save(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let Some(draft) = world
        .get::<FrequencyCastle>(owner)
        .and_then(|castle| castle.draft.clone())
    else {
        return;
    };
    let frequency = match draft.definition() {
        Ok(frequency) => frequency,
        Err(error) => {
            status(world, owner, error);
            return;
        }
    };
    let request_id = nucleus::new_uid("frequency-edit");
    let action = engine::actions::Action::SaveKarmaFrequency {
        request_id,
        frequency_uid: draft.uid.clone(),
        expected_handle_revision: draft.revision,
        frequency,
        restart: draft
            .original_fields
            .as_ref()
            .is_none_or(|fields| fields[2..] != draft.fields[2..])
            || draft.original_weekdays != draft.weekdays,
    };
    submit(world, owner, action, Some(draft), "");
}

fn maintain(
    world: &mut World,
    candidates: &mut QueryState<Entity, With<FrequencyCastle>>,
    mut owners: Local<Vec<Entity>>,
) {
    let _maintain = nucleus::performance::Span::new("frequency-maintenance");
    let stale: Vec<_> = world
        .resource::<Requests>()
        .0
        .iter()
        .filter(|(_, owner)| world.get::<FrequencyCastle>(**owner).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        let sender = world.resource::<Requests>().1.get(&id);
        if sender.is_none_or(|sender| {
            sender.is_closed()
                || sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_ok()
        }) {
            world.resource_mut::<Requests>().0.remove(&id);
            world.resource_mut::<Requests>().1.remove(&id);
        }
    }
    owners.extend(candidates.iter(world));
    let count = owners.len();
    nucleus::performance::add("frequency-maintenance-owners", count as u64);
    for owner in owners.drain(..) {
        if crate::native::laboratory::suspended(world, owner) {
            continue;
        }
        let id = world.get::<View>(owner).map_or_else(
            || std::borrow::Cow::Owned(format!("frequency-castle-{}", owner.to_bits())),
            |view| std::borrow::Cow::Borrowed(view.subscription.as_str()),
        );
        if world.resource::<Requests>().0.contains_key(id.as_ref()) {
            continue;
        }
        let id = id.into_owned();
        let protein = serde_json::from_value(serde_json::json!({"source":"frequency"})).unwrap();
        match send(
            world,
            owner,
            ClientMessage::Subscribe {
                id: id.clone(),
                protein,
            },
        ) {
            Ok(()) => {
                if let Some(sender) = crate::native::practice_cells::sender(world, owner) {
                    world
                        .resource_mut::<Requests>()
                        .1
                        .insert(id.clone(), sender);
                }
                world.resource_mut::<Requests>().0.insert(id, owner);
            }
            Err(error) => status(world, owner, error),
        }
    }
    if count == 0 {
        *owners = Vec::new();
    }
}

pub(crate) use ui::Command as FrequencyAction;

pub(crate) fn next_occurrence(world: &World, owner: Entity, slug: &str) -> Option<i64> {
    world
        .get::<View>(owner)?
        .rows
        .iter()
        .find(|row| row.slug == slug)?
        .next_at_ms
}

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>,
    mut incoming: Local<replies::Wave>,
    mut pending: Local<replies::Owners<View>>,
    mut owners: Local<Vec<Entity>>,
) {
    let _receive = nucleus::performance::Span::new("frequency-reply-processing");
    incoming.capture(
        &mut cursor,
        world.resource::<Messages<CellMessage>>(),
        |message| replies::ordinary(message).is_some(),
    );
    if incoming.len() == 0 {
        *owners = Vec::new();
    }
    for sequence in incoming.drain() {
        let message = replies::message(world.resource::<Messages<CellMessage>>(), sequence);
        let subscription = {
            let _select = nucleus::performance::Span::new("frequency-reply-selection");
            match message.as_ref() {
                ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } => {
                    let Some(owner) = world.resource::<Requests>().0.get(id).copied() else {
                        continue;
                    };
                    Some(owner)
                }
                ServerMessage::ActionOk { id, .. } => {
                    pending.collect(world, id, None, false, &mut owners);
                    if owners.is_empty() {
                        continue;
                    }
                    None
                }
                ServerMessage::Error { id, .. } => {
                    let subscription = world.resource::<Requests>().0.get(id).copied();
                    pending.collect(
                        world,
                        id,
                        subscription,
                        id == crate::native::cell_bridge::CONNECTION,
                        &mut owners,
                    );
                    if owners.is_empty() {
                        continue;
                    }
                    None
                }
                _ => continue,
            }
        };
        let message = message.clone();
        nucleus::performance::add("frequency-replies-retained", 1);
        match message.as_ref() {
            ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } => {
                let owner = subscription.unwrap();
                let rows = match rows
                    .iter()
                    .map(Frequency::deserialize)
                    .collect::<Result<Vec<Frequency>, _>>()
                {
                    Ok(rows) => rows,
                    Err(error) => {
                        status(world, owner, format!("Could not read frequencies: {error}"));
                        continue;
                    }
                };
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    continue;
                };
                if view.ready && view.rows.values.as_ref() == &rows {
                    continue;
                }
                let first = !view.ready;
                let redraw = first
                    || view.rows.len() != rows.len()
                    || view
                        .rows
                        .iter()
                        .zip(&rows)
                        .any(|(before, after)| !before.same_layout(after));
                view.ready = true;
                view.rows = rows.into();
                ui::refresh_next_date(world, owner);
                if redraw {
                    ui::render_list(world, owner);
                }
                if first {
                    status(world, owner, "");
                }
            }
            ServerMessage::ActionOk { created, .. } => {
                for owner in owners.drain(..) {
                    ui::capture(world, owner);
                    let pending = world
                        .get_mut::<View>(owner)
                        .unwrap()
                        .pending
                        .take()
                        .unwrap();
                    let mut castle = world.get_mut::<FrequencyCastle>(owner).unwrap();
                    if let Some(submitted) = pending.draft {
                        if castle.draft.as_ref() == Some(&submitted) {
                            castle.draft = None;
                        } else if let Some(draft) = &mut castle.draft {
                            draft.uid = created.clone().or_else(|| submitted.uid.clone());
                            draft.revision = Some(
                                submitted
                                    .revision
                                    .map_or(2, |revision| revision.saturating_add(1)),
                            );
                            draft.original = submitted.definition().ok();
                            draft.original_fields = Some(submitted.fields);
                            draft.original_weekdays = submitted.weekdays;
                        }
                    }
                    world.get_mut::<View>(owner).unwrap().deleting = None;
                    ui::render_form(world, owner);
                    ui::render_list(world, owner);
                    status(world, owner, pending.message);
                }
            }
            ServerMessage::Error { message, .. } => {
                for owner in owners.drain(..) {
                    world.get_mut::<View>(owner).unwrap().pending = None;
                    status(world, owner, message);
                }
            }
            _ => {}
        }
    }
}
