// non-lean-yet
// Scope: fn publish_rows, enum Reply, struct Routes.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 20.

use super::*;

pub(super) fn publish_rows(previous: &mut Arc<Vec<Arc<Value>>>, rows: &[Value]) -> bool {
    let common = previous
        .iter()
        .zip(rows)
        .take_while(|(previous, row)| previous.as_ref() == *row)
        .count();
    if common == previous.len() && common == rows.len() {
        return false;
    }
    let replacement = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            previous
                .get(index)
                .filter(|previous| index < common || previous.as_ref() == row)
                .cloned()
                .unwrap_or_else(|| Arc::new(row.clone()))
        })
        .collect();
    *previous = Arc::new(replacement);
    true
}

pub(super) enum Reply {
    Execution(Entity),
    Schedule(Entity),
    Preview(Entity),
    Commands(Entity),
    Devices(ailuros_ui::OwnedReply),
    History(Entity),
    Reading(Entity),
    Rows(Entity, usize),
    Action,
    Error,
}

pub(super) struct Routes {
    views: replies::Owners<View>,
    schedules: replies::Owners<schedules_ui::ScheduleView>,
    previews: replies::Owners<preview_ui::PreviewView>,
    commands: replies::Owners<commands_ui::Commands>,
    devices: ailuros_ui::ReplyOwners,
    history: replies::Owners<history_ui::History>,
    pub(super) owners: Vec<Entity>,
}

impl FromWorld for Routes {
    fn from_world(world: &mut World) -> Self {
        Self {
            views: replies::Owners::from_world(world),
            schedules: replies::Owners::from_world(world),
            previews: replies::Owners::from_world(world),
            commands: replies::Owners::from_world(world),
            devices: ailuros_ui::ReplyOwners::from_world(world),
            history: replies::Owners::from_world(world),
            owners: Vec::new(),
        }
    }
}

impl Routes {
    pub(super) fn select(&mut self, world: &World, message: &ServerMessage) -> Option<Reply> {
        let _select = nucleus::performance::Span::new("karma-reply-selection");
        self.owners.clear();
        let id = match message {
            ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } => {
                return world
                    .resource::<Requests>()
                    .subscriptions
                    .get(id)
                    .map(|&(owner, index)| Reply::Rows(owner, index));
            }
            ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
            _ => return None,
        };
        if let Some(&owner) = world.resource::<Requests>().executions.get(id) {
            return Some(Reply::Execution(owner));
        }
        if let Some(owner) = self.schedules.first(world, id) {
            return Some(Reply::Schedule(owner));
        }
        if let Some(owner) = self.previews.first(world, id) {
            return Some(Reply::Preview(owner));
        }
        if let Some(owner) = self.commands.first(world, id) {
            return Some(Reply::Commands(owner));
        }
        if let Some(reply) = self.devices.select(world, id) {
            return Some(Reply::Devices(reply));
        }
        if let Some(owner) = self.history.first(world, id) {
            return Some(Reply::History(owner));
        }
        if let Some(&owner) = world.resource::<Requests>().readings.get(id) {
            return Some(Reply::Reading(owner));
        }
        let error = matches!(message, ServerMessage::Error { .. });
        let subscription = if error {
            world
                .resource::<Requests>()
                .subscriptions
                .get(id)
                .map(|&(owner, _)| owner)
        } else {
            None
        };
        self.views.collect(
            world,
            id,
            subscription,
            error && id == crate::native::cell_bridge::CONNECTION,
            &mut self.owners,
        );
        (!self.owners.is_empty()).then_some(if error { Reply::Error } else { Reply::Action })
    }
}
