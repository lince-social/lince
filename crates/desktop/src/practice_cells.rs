use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub(crate) struct PracticeCells {
    pub cells: HashMap<String, cell::CellRuntime>,
    pub records: HashMap<String, String>,
    pub retired: HashSet<String>,
}

#[derive(Component)]
pub(crate) struct PracticeRecord;

#[derive(Component)]
pub(crate) struct PracticeArea(pub String);

#[derive(Component)]
pub(crate) struct PracticeSource(pub String);

pub(crate) fn sender(
    world: &World,
    mut owner: Entity,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    loop {
        if let Some(source) = world.get::<PracticeSource>(owner) {
            return crate::protein_area::auxiliary_sender(
                world,
                &crate::protein_area::Source::Organ(source.0.clone()),
            );
        }
        let Some(parent) = world.get::<ChildOf>(owner) else {
            break;
        };
        owner = parent.parent();
    }
    world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .map(|bridge| bridge.outgoing.clone())
}

pub(crate) fn send(world: &World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    sender(world, owner)
        .ok_or("The sample Cell is disconnected.")?
        .try_send(message)
        .map_err(|_| "The Cell is busy or disconnected. Retry when it is available.".into())
}

pub(crate) fn permits_area(world: &World, entity: Entity, uid: &str) -> bool {
    let Some(area) = world.get::<PracticeArea>(entity) else {
        return true;
    };
    world
        .get_resource::<PracticeCells>()
        .is_some_and(|cells| cells.records.get(uid) == Some(&area.0))
}

pub(crate) fn owns_source(world: &World, source: &crate::protein_area::Source) -> bool {
    let crate::protein_area::Source::Organ(organ) = source else {
        return false;
    };
    world
        .get_resource::<PracticeCells>()
        .is_some_and(|cells| cells.cells.contains_key(organ))
}

pub(crate) fn route(world: &World, id: String, action: &engine::actions::Action) -> Option<bool> {
    use engine::actions::Action;
    let uid = match action {
        Action::PreviewAreaTransition { target, .. } => target,
        Action::ApplyAreaTransition { preview, .. } => &preview.target,
        _ => return None,
    };
    let cells = world.get_resource::<PracticeCells>()?;
    if cells.retired.contains(uid) {
        return Some(false);
    }
    let source = cells.records.get(uid)?;
    Some(
        crate::protein_area::auxiliary_sender(
            world,
            &crate::protein_area::Source::Organ(source.clone()),
        )
        .is_some_and(|sender| {
            sender
                .try_send(ClientMessage::Act {
                    id,
                    action: action.clone(),
                })
                .is_ok()
        }),
    )
}

pub(crate) fn remote(
    world: &World,
    organ: &str,
) -> Option<Result<crate::protein_area::Remote, String>> {
    let runtime = world
        .get_resource::<PracticeCells>()?
        .cells
        .get(organ)?
        .clone();
    let result = (|| {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| "No live runtime")?;
        let wake = world
            .get_resource::<crate::wake::WakeSignal>()
            .cloned()
            .ok_or("No Interface wake signal")?;
        let mut bridge = crate::cell_bridge::connect(runtime, wake.clone());
        let (outgoing, mut requests) = tokio::sync::mpsc::channel(32);
        let (responses, incoming) = tokio::sync::mpsc::channel(32);
        let organ = organ.to_owned();
        let task = handle.spawn(async move {
            if responses
                .send(ServerMessage::SessionAuthenticated {
                    id: "practice".into(),
                    session_id: organ,
                    person: String::new(),
                    key_id: String::new(),
                })
                .await
                .is_err()
            {
                return;
            }
            wake.ring();
            loop {
                tokio::select! {
                    request = requests.recv() => {
                        let Some(request) = request else { break };
                        if bridge.outgoing.send(request).await.is_err() { break }
                    }
                    response = bridge.incoming.recv() => {
                        let Some(response) = response else { break };
                        if responses.send(response).await.is_err() { break }
                        wake.ring();
                    }
                }
            }
        });
        Ok(crate::protein_area::Remote {
            outgoing,
            incoming,
            task,
        })
    })();
    Some(result)
}
