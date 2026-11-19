use super::*;
use crate::host::performance::{self, Count, Stage};

pub(in crate::native::protein_area) struct Preparation {
    cursor: bevy::ecs::message::MessageCursor<CellMessage>,
    pickers: QueryState<&'static mut Picker>,
    pending: QueryState<(Entity, &'static Pending)>,
    owners: QueryState<(Entity, &'static Picker)>,
    messages: Vec<Arc<ServerMessage>>,
    outgoing: Vec<ClientMessage>,
    done: Vec<Entity>,
    pub(super) popups: Vec<(Entity, Entity, Entity, Entity)>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            cursor: Default::default(),
            pickers: world.query(),
            pending: world.query(),
            owners: world.query(),
            messages: Vec::new(),
            outgoing: Vec::new(),
            done: Vec::new(),
            popups: Vec::new(),
        }
    }
}

impl Preparation {
    pub(super) fn receive(&mut self, world: &mut World) {
        let _receive = performance::Span::new(Stage::AssigneePickerReceive);
        self.messages.clear();
        self.outgoing.clear();
        let messages = world.resource::<Messages<CellMessage>>();
        if self.pickers.iter(world).next().is_none() {
            self.cursor.clear(messages);
            self.messages.shrink_to_fit();
            self.outgoing.shrink_to_fit();
            return;
        }
        self.messages.extend(
            self.cursor
                .read(messages)
                .filter(|message| {
                    matches!(
                        message.0.as_ref(),
                        ServerMessage::Snapshot { .. }
                            | ServerMessage::Update { .. }
                            | ServerMessage::Error { .. }
                    )
                })
                .map(|message| message.0.clone()),
        );
        performance::add(
            Count::AssigneePickerMessagesPrepared,
            self.messages.len() as u64,
        );
        for message in self.messages.drain(..) {
            let predicate_id = match message.as_ref() {
                ServerMessage::Snapshot { id, .. } => id.strip_suffix("-predicate"),
                _ => None,
            };
            for mut picker in self.pickers.iter_mut(world) {
                match message.as_ref() {
                    ServerMessage::Snapshot { id, rows }
                        if predicate_id == Some(picker.subscription.as_str()) =>
                    {
                        self.outgoing
                            .push(ClientMessage::Unsubscribe { id: id.clone() });
                        if let Some(binding) = rows
                            .iter()
                            .flat_map(|row| {
                                row["feature_bindings"].as_array().into_iter().flatten()
                            })
                            .find(|binding| binding["feature"] == "assignment")
                        {
                            let uid = binding["write_concept_uid"].as_str().unwrap_or("");
                            if picker.predicate_uid != uid {
                                picker.predicate_uid.clear();
                                picker.predicate_uid.push_str(uid);
                            }
                            let ready = !uid.is_empty();
                            if picker.predicate_ready != ready {
                                picker.predicate_ready = ready;
                            }
                        } else {
                            set_message(
                                &mut picker,
                                "Assignment meaning unavailable; inspect feature meanings",
                            );
                        }
                    }
                    ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                        if id == &picker.subscription =>
                    {
                        if !Arc::ptr_eq(&picker.rows, rows) {
                            picker.rows = rows.clone();
                        }
                        set_message(&mut picker, "");
                    }
                    ServerMessage::Error { id, message, .. }
                        if id.starts_with(&picker.subscription)
                            || id == crate::native::cell_bridge::CONNECTION =>
                    {
                        set_message(&mut picker, message);
                    }
                    _ => {}
                }
            }
            world
                .resource_mut::<Runtime>()
                .outgoing
                .extend(self.outgoing.drain(..));
        }
    }

    pub(super) fn retire(&mut self, world: &mut World) {
        let _prepare = performance::Span::new(Stage::AssigneePickerPrepare);
        self.done.clear();
        self.done.extend(
            self.pending
                .iter(world)
                .filter(|(_, pending)| {
                    pending.confirmed
                        && world.get::<Field>(pending.field).is_none_or(|field| {
                            data(world, field)["assignees"]
                                .as_array()
                                .is_some_and(|values| {
                                    values.iter().any(|value| value["uid"] == pending.uid)
                                })
                                == pending.selected
                        })
                })
                .map(|(entity, _)| entity),
        );
        if self.done.is_empty() {
            self.done.shrink_to_fit();
        }
        for entity in self.done.drain(..) {
            world.despawn(entity);
        }
    }

    pub(super) fn capture(&mut self, world: &World) {
        let _prepare = performance::Span::new(Stage::AssigneePickerPrepare);
        self.popups.clear();
        self.popups.extend(
            self.owners
                .iter(world)
                .map(|(entity, picker)| (entity, picker.field, picker.input, picker.list)),
        );
        performance::add(Count::AssigneePickersPrepared, self.popups.len() as u64);
        if self.popups.is_empty() {
            self.popups.shrink_to_fit();
        }
    }

    pub(super) fn finish(&mut self, world: &World) {
        if self.owners.iter(world).next().is_none() {
            self.messages.shrink_to_fit();
            self.outgoing.shrink_to_fit();
            self.done.shrink_to_fit();
            self.popups.shrink_to_fit();
        }
    }
}

fn set_message(picker: &mut Mut<'_, Picker>, message: &str) {
    if picker.message != message {
        picker.message.clear();
        picker.message.push_str(message);
    }
}
