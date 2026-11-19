use super::*;
use crate::native::{cell_bridge::replies, practice_cells::PracticeSource};
use bevy::ecs::{message::MessageCursor, query::QueryState};

impl Catalog {
    fn publish(&mut self, rows: &[serde_json::Value], scoped: bool) -> bool {
        let _span = nucleus::performance::Span::new("operation-catalog-preparation");
        let entries: BTreeMap<_, _> = rows
            .iter()
            .filter_map(|row| {
                let slug = row["slug"].as_str()?.trim_start_matches('@');
                let uid = row["uid"].as_str()?;
                if !scoped && (slug.is_empty() || uid.is_empty()) {
                    return None;
                }
                let head = row["head"]
                    .as_str()
                    .unwrap_or(if scoped { "" } else { slug });
                Some((slug, (uid, head)))
            })
            .collect();
        nucleus::performance::add("operation-catalog-entries-prepared", entries.len() as u64);
        let same = self.records.len() == entries.len()
            && self
                .records
                .iter()
                .zip(&entries)
                .all(|((slug, value), (key, next))| {
                    slug.as_str() == *key
                        && value.0.as_str() == next.0
                        && value.1.as_str() == next.1
                });
        if same && self.ready && self.error.is_none() {
            nucleus::performance::add("operation-catalog-publications-retained", 1);
            return false;
        }
        self.records
            .retain(|slug, _| entries.contains_key(slug.as_str()));
        for (slug, (uid, head)) in entries {
            if let Some(current) = self.records.get_mut(slug) {
                if current.0 != uid {
                    uid.clone_into(&mut current.0);
                }
                if current.1 != head {
                    head.clone_into(&mut current.1);
                }
            } else {
                self.records.insert(slug.into(), (uid.into(), head.into()));
            }
        }
        self.ready = true;
        self.error = None;
        self.revision += 1;
        true
    }

    fn fail(&mut self, message: &str, clear: bool) -> bool {
        if !self.ready
            && self.error.as_deref() == Some(message)
            && (!clear || self.records.is_empty())
        {
            return false;
        }
        self.ready = false;
        if clear {
            self.records.clear();
        }
        self.error = Some(message.into());
        self.revision += 1;
        true
    }
}

pub(super) struct Receiver {
    wave: replies::Wave,
    catalogs: QueryState<(Entity, &'static ScopedCatalog)>,
    operations: QueryState<(
        Entity,
        &'static OperationSand,
        Option<&'static PracticeSource>,
    )>,
    scoped: Vec<Entity>,
    pending: Vec<Entity>,
}

impl FromWorld for Receiver {
    fn from_world(world: &mut World) -> Self {
        Self {
            wave: Default::default(),
            catalogs: world.query(),
            operations: world.query(),
            scoped: Vec::new(),
            pending: Vec::new(),
        }
    }
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<MessageCursor<CellMessage>>,
    mut receiver: Local<Receiver>,
) {
    let _span = nucleus::performance::Span::new("operation-reply-delivery");
    let receiver = &mut *receiver;
    receiver.wave.capture(
        &mut cursor,
        world.resource::<Messages<CellMessage>>(),
        |message| replies::ordinary(message).is_some(),
    );
    for sequence in receiver.wave.drain() {
        let message = replies::message(world.resource::<Messages<CellMessage>>(), sequence);
        receiver.scoped.clear();
        if let ServerMessage::Snapshot { id, .. }
        | ServerMessage::Update { id, .. }
        | ServerMessage::Error { id, .. } = message.as_ref()
        {
            receiver.scoped.extend(
                receiver
                    .catalogs
                    .iter(world)
                    .filter_map(|(owner, catalog)| (id == &catalog.subscription).then_some(owner)),
            );
        }
        receiver.pending.clear();
        if let Some(id) = replies::action_id(message) {
            receiver
                .pending
                .extend(
                    receiver
                        .operations
                        .iter(world)
                        .filter_map(|(owner, state, source)| {
                            let (request, _) = state.pending.as_ref()?;
                            (request == id
                                || id == crate::native::cell_bridge::CONNECTION && source.is_none())
                            .then_some(owner)
                        }),
                );
        }
        let global = match message.as_ref() {
            ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } => id == RECORDS,
            ServerMessage::Error { id, .. } => {
                id == RECORDS || id == crate::native::cell_bridge::CONNECTION
            }
            _ => false,
        };
        if !global && receiver.scoped.is_empty() && receiver.pending.is_empty() {
            continue;
        }
        let message = message.clone();
        nucleus::performance::add("operation-replies-retained", 1);
        for owner in receiver.scoped.drain(..) {
            let mut scoped = world.get_mut::<ScopedCatalog>(owner).unwrap();
            let catalog = &mut scoped.bypass_change_detection().catalog;
            let changed = match message.as_ref() {
                ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } => {
                    catalog.publish(rows, true)
                }
                ServerMessage::Error { message, .. } => catalog.fail(message, false),
                _ => unreachable!(),
            };
            if changed {
                scoped.set_changed();
            }
        }
        match message.as_ref() {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == RECORDS =>
            {
                let mut catalog = world.resource_mut::<Catalog>();
                if catalog.bypass_change_detection().publish(rows, false) {
                    catalog.set_changed();
                }
            }
            ServerMessage::ActionOk { warnings, .. } => {
                acknowledge(world, &mut receiver.pending, Ok(warnings));
            }
            ServerMessage::Error { id, message, .. } => {
                if id == RECORDS || id == crate::native::cell_bridge::CONNECTION {
                    let mut catalog = world.resource_mut::<Catalog>();
                    if catalog.bypass_change_detection().fail(message, true) {
                        catalog.set_changed();
                    }
                }
                acknowledge(world, &mut receiver.pending, Err(message));
            }
            _ => {}
        }
    }
    if receiver.catalogs.iter(world).next().is_none() {
        receiver.scoped = Vec::new();
    }
    if receiver.operations.iter(world).next().is_none() {
        receiver.pending = Vec::new();
    }
}

fn acknowledge(world: &mut World, pending: &mut Vec<Entity>, result: Result<&[String], &str>) {
    if pending.is_empty() {
        return;
    }
    let suffix = match result {
        Ok(warnings) if !warnings.is_empty() => format!(" {}", warnings.join("; ")),
        _ => String::new(),
    };
    for sand in pending.drain(..) {
        let state = world.get::<OperationSand>(sand).unwrap();
        let query = &state.pending.as_ref().unwrap().1;
        let uid = result
            .is_ok()
            .then(|| {
                query
                    .trim()
                    .strip_prefix('@')
                    .and_then(|slug| catalog(world, sand)?.records.get(slug))
                    .map(|(uid, _)| uid.clone())
            })
            .flatten();
        let message = match result {
            Ok(_) => format!("{}: quantity set to zero.{suffix}", query.trim()),
            Err(error) => format!("Change not confirmed: {error}"),
        };
        let mut state = world.get_mut::<OperationSand>(sand).unwrap();
        if result.is_ok() {
            state.last_saved = uid;
        }
        state.pending = None;
        drop(state);
        status(world, sand, message);
        nucleus::performance::add("operation-acknowledgements", 1);
    }
}
