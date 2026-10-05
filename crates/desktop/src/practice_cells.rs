use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use std::collections::{HashMap, HashSet};

pub(crate) mod persistence;

#[derive(Resource, Default)]
pub(crate) struct PracticeCells {
    pub cells: HashMap<String, cell::CellRuntime>,
    pub records: HashMap<String, HashSet<String>>,
    pub retired: HashSet<String>,
    pub directories: HashMap<String, std::path::PathBuf>,
    pub workers: HashMap<String, Worker>,
    pub servers: HashMap<String, Vec<Worker>>,
    pub audio: HashMap<String, crate::sound::Audio>,
}

pub(crate) fn resume_audio(world: &mut World, source: &str) {
    let Some(cells) = world.get_resource::<PracticeCells>() else {
        return;
    };
    if cells.audio.contains_key(source) {
        return;
    }
    let Some(path) = cells.directories.get(source).cloned() else {
        return;
    };
    if std::fs::symlink_metadata(path.join("recordings"))
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
    {
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        world
            .resource_mut::<PracticeCells>()
            .audio
            .insert(source.to_owned(), crate::sound::Audio::open(path, wake));
    }
}

pub(crate) struct Worker(pub tokio::task::JoinHandle<()>);

impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Component)]
pub(crate) struct PracticeRecord;

#[derive(Component)]
pub(crate) struct PracticeArea(pub String);

#[derive(Component)]
pub(crate) struct PracticeSource(pub String);

pub(crate) fn sender(
    world: &World,
    owner: Entity,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    if let Some(source) = source(world, owner) {
        return crate::protein_area::auxiliary_sender(
            world,
            &crate::protein_area::Source::Organ(source),
        );
    }
    world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .map(|bridge| bridge.outgoing.clone())
}

pub(crate) fn source(world: &World, mut owner: Entity) -> Option<String> {
    loop {
        if let Some(source) = world.get::<PracticeSource>(owner) {
            return Some(source.0.clone());
        }
        let Some(parent) = world.get::<ChildOf>(owner) else {
            break;
        };
        owner = parent.parent();
    }
    None
}

pub(crate) fn engine(
    world: &World,
    source: &crate::protein_area::Source,
) -> Option<std::sync::Arc<engine::Engine>> {
    if let crate::protein_area::Source::Organ(source) = source
        && let Some(cells) = world.get_resource::<PracticeCells>()
    {
        if let Some(cell) = cells.cells.get(source) {
            return Some(cell.engine.clone());
        }
        if cells.retired.contains(source) || cells.directories.contains_key(source) {
            return None;
        }
    }
    world
        .get_resource::<crate::app::CellHandle>()
        .map(|handle| handle.0.engine.clone())
}

pub(crate) fn send(world: &World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    sender(world, owner)
        .ok_or_else(|| {
            if source(world, owner).is_some() {
                "The sample Cell is disconnected."
            } else {
                "No Cell connection"
            }
        })?
        .try_send(message)
        .map_err(|_| "The Cell is busy or disconnected. Retry when it is available.".into())
}

pub(crate) fn runtime(world: &World, owner: Entity) -> Option<cell::CellRuntime> {
    if let Some(source) = source(world, owner) {
        return world
            .get_resource::<PracticeCells>()?
            .cells
            .get(&source)
            .cloned();
    }
    world
        .get_resource::<crate::app::CellHandle>()
        .map(|handle| handle.0.clone())
}

pub(crate) fn permits_area(world: &World, entity: Entity, uid: &str) -> bool {
    let Some(area) = world.get::<PracticeArea>(entity) else {
        return true;
    };
    world.get_resource::<PracticeCells>().is_some_and(|cells| {
        cells
            .records
            .get(&area.0)
            .is_some_and(|records| records.contains(uid))
    })
}

pub(crate) fn owns_source(world: &World, source: &crate::protein_area::Source) -> bool {
    let crate::protein_area::Source::Organ(organ) = source else {
        return false;
    };
    world
        .get_resource::<PracticeCells>()
        .is_some_and(|cells| cells.cells.contains_key(organ))
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
