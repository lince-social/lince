use super::*;
use cell::{
    fiote_communication::decision::ModelId,
    speech::{Purpose, Request, Status},
};

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Candle and Laya Rust inference",
        author: "Hugging Face Candle and Enzo Lombardi/Laya contributors",
        license: include_str!("../../../../../../vendor/laya/LICENSE"),
    },
    crate::native::credits::Attribution {
        name: "Qwen3 model weights",
        author: "Qwen Team, Alibaba Cloud",
        license: include_str!(
            "../../../../../../fiote/assets/decision/LICENSE-Qwen-Apache-2.0.txt"
        ),
    },
    crate::native::credits::Attribution {
        name: "English NLI model weights",
        author: "Moritz Laurer",
        license: include_str!("../../../../../../fiote/assets/decision/LICENSE-NLI-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Decision model provenance",
        author: "Lince adaptations and upstream sources",
        license: include_str!("../../../../../../fiote/assets/decision/CREDITS.txt"),
    },
];

#[derive(Component)]
struct Settings {
    panel: Entity,
    label: Entity,
    import: Entity,
    pending: Option<String>,
    job: Option<String>,
    next: Instant,
    cancellation: Option<crate::native::cell_bridge::SpeechCancellation>,
    saved: Option<Status>,
    routing: Option<Routing>,
}

struct Routing {
    request: EntryRequest,
    model: String,
    revision: String,
    candidates: Vec<String>,
}

impl Drop for Settings {
    fn drop(&mut self) {
        if let (Some(job), Some(cancellation)) = (&self.job, &self.cancellation) {
            let _ = cancellation.send(Request::Cancel { job: job.clone() });
        }
    }
}

pub(super) fn attach(world: &mut World, owner: Entity) {
    let header = ui::row(world, owner);
    ui::button(
        world,
        header,
        owner,
        "Local routing model settings",
        Command::Toggle,
    );
    let panel = ui::column(world, owner);
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world
        .entity_mut(panel)
        .insert(crate::native::sand_store::SandCredits(CREDITS));
    crate::native::edit_mode::label(
        world,
        panel,
        "Optional Rust local router: enable destination routing to let the installed model choose a Fiote from its descriptions. Routing accuracy still needs acceptance testing. Ambient speech admission stays unavailable until separately validated. Follow-ups keep their chosen conversation.",
        13.0,
    );
    let label = crate::native::edit_mode::label(world, panel, "Decision model: not checked", 13.0);
    let choices = ui::row(world, panel);
    for (caption, model) in [
        ("Laya decision model", ModelId::Laya),
        ("Small English classifier", ModelId::EnglishNli),
        ("Qwen local classifier", ModelId::Qwen),
    ] {
        ui::button(world, choices, owner, caption, Command::Select(model));
    }
    let import = ui::field(world, panel, "Downloaded decision model directory", "");
    let controls = ui::row(world, panel);
    for (caption, command) in [
        ("Download selected decision model", Command::Download),
        ("Import selected decision model", Command::Import),
        ("Check decision model", Command::Inspect),
        ("Cancel model operation", Command::Cancel),
    ] {
        ui::button(world, controls, owner, caption, command);
    }
    ui::credits(world, controls, panel, CREDITS);
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    world.entity_mut(owner).insert(Settings {
        panel,
        label,
        import,
        pending: None,
        job: None,
        next: Instant::now(),
        cancellation,
        saved: None,
        routing: None,
    });
}

pub(super) fn route(world: &mut World, owner: Entity, request: &EntryRequest) -> bool {
    if request.target.is_some() || !super::listening::routing(world, owner) {
        return false;
    }
    let Some(settings) = world.get::<Settings>(owner) else {
        return false;
    };
    if settings.pending.is_some() || settings.job.is_some() {
        return false;
    }
    let Some(saved) = &settings.saved else {
        return false;
    };
    let Some(information) = saved
        .decision_capabilities
        .as_ref()
        .filter(|information| information.routing_supported || information.routing_validated)
    else {
        return false;
    };
    if saved
        .decision_model
        .as_ref()
        .is_none_or(|model| !model.present || model.busy)
    {
        return false;
    }
    let (model, revision) = (information.model.clone(), information.revision.clone());
    let Some(directory) = world
        .get::<Entry>(owner)
        .and_then(|entry| entry.directory.as_ref())
        .filter(|directory| directory.next_offset.is_none())
    else {
        return false;
    };
    let candidates: Vec<_> = directory
        .fiotes
        .iter()
        .filter(|candidate| candidate.enabled && !candidate.description_truncated)
        .map(|candidate| cell::fiote_communication::decision::Candidate {
            record: candidate.record.clone(),
            title: candidate.title.clone(),
            purpose: candidate.description.clone(),
        })
        .collect();
    let input = cell::fiote_communication::decision::Input {
        request_id: request.request_id.clone(),
        generation: 1,
        text: request.text.clone(),
        context: String::new(),
        candidates,
        admission: false,
        routing: true,
    };
    if input.validate().is_err() {
        return false;
    }
    let ids = input
        .candidates
        .iter()
        .map(|candidate| candidate.record.clone())
        .collect();
    let job = nucleus::new_uid("speech");
    let mut settings = world.get_mut::<Settings>(owner).unwrap();
    settings.routing = Some(Routing {
        request: request.clone(),
        model,
        revision,
        candidates: ids,
    });
    settings.job = Some(job.clone());
    if send(world, owner, Request::Decide { job, input }).is_err() {
        let mut settings = world.get_mut::<Settings>(owner).unwrap();
        settings.routing = None;
        settings.job = None;
        return false;
    }
    true
}

pub(super) fn routing_busy(world: &World, owner: Entity) -> bool {
    world
        .get::<Settings>(owner)
        .is_some_and(|settings| settings.routing.is_some())
}

pub(super) fn cancel_routing(world: &mut World, owner: Entity) {
    if !routing_busy(world, owner) {
        return;
    }
    let mut settings = world.get_mut::<Settings>(owner).unwrap();
    settings.routing = None;
    let job = settings.job.take();
    if let Some(job) = job {
        let _ = send(world, owner, Request::Cancel { job });
    }
}

fn deliver(
    world: &mut World,
    owner: Entity,
    routing: Routing,
    decision: Option<&cell::fiote_communication::decision::Output>,
) {
    let mut request = routing.request;
    let target = decision
        .filter(|output| {
            output.request_id == request.request_id
                && output.generation == 1
                && output.model == routing.model
                && output.revision == routing.revision
        })
        .and_then(|output| output.destination.as_ref())
        .and_then(|choice| choice.value.clone())
        .filter(|target| routing.candidates.contains(target));
    request.target = target;
    if let Err(error) = super::dispatch(world, owner, request) {
        let label = world.get::<Entry>(owner).unwrap().status;
        world.get_mut::<Text>(label).unwrap().0 = error;
    }
}

fn send(world: &mut World, owner: Entity, request: Request) -> Result<(), String> {
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    if matches!(&request, Request::Cancel { .. }) {
        let cancellation = world
            .get::<Settings>(owner)
            .and_then(|settings| settings.cancellation.clone())
            .or(cancellation)
            .ok_or("The speech connection is unavailable.")?;
        let id = cancellation.send(request)?;
        let mut settings = world
            .get_mut::<Settings>(owner)
            .ok_or("Model settings closed.")?;
        settings.pending = Some(id);
        settings.cancellation = Some(cancellation);
        return Ok(());
    }
    let id = nucleus::new_uid("decision-ui");
    ui::send(
        world,
        owner,
        cell::ClientMessage::Speech {
            id: id.clone(),
            request,
        },
    )?;
    let mut settings = world
        .get_mut::<Settings>(owner)
        .ok_or("Model settings closed.")?;
    settings.pending = Some(id);
    settings.cancellation = cancellation;
    Ok(())
}

#[derive(Clone)]
enum Command {
    Toggle,
    Select(ModelId),
    Download,
    Import,
    Inspect,
    Cancel,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(settings) = world.get::<Settings>(owner) else {
            return;
        };
        let label = settings.label;
        let panel = settings.panel;
        let import = settings.import;
        let busy = settings.pending.is_some() || settings.job.is_some();
        let result = (|| {
            if matches!(self, Self::Toggle) {
                let mut node = world.get_mut::<Node>(panel).unwrap();
                node.display = if node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
                if node.display == Display::None {
                    return Ok(());
                }
                return send(world, owner, Request::Inspect);
            }
            if matches!(self, Self::Cancel) {
                world.get_mut::<Settings>(owner).unwrap().routing = None;
                let job = world.get_mut::<Settings>(owner).unwrap().job.take();
                if let Some(job) = job {
                    return send(world, owner, Request::Cancel { job });
                }
                return Ok(());
            }
            if busy {
                return Err("Wait for or cancel the current model operation.".into());
            }
            let request = match self {
                Self::Select(model) => Request::SelectDecision { model: *model },
                Self::Download | Self::Import => {
                    let source = if matches!(self, Self::Import) {
                        let path = ui::value(world, import)?;
                        if path.trim().is_empty() {
                            return Err("Choose the downloaded model directory.".into());
                        }
                        Some(PathBuf::from(path.trim()))
                    } else {
                        None
                    };
                    let job = nucleus::new_uid("speech");
                    Request::InstallDecision { job, source }
                }
                Self::Inspect => Request::Inspect,
                Self::Toggle | Self::Cancel => unreachable!(),
            };
            let job = match &request {
                Request::InstallDecision { job, .. } => Some(job.clone()),
                _ => None,
            };
            send(world, owner, request)?;
            if let Some(job) = job {
                world.get_mut::<Settings>(owner).unwrap().job = Some(job);
            }
            Ok(())
        })();
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = match result {
                Ok(()) => "Checking local model state…".into(),
                Err(error) => error,
            };
        }
    }
}

fn caption(status: &Status) -> String {
    let Some(model) = &status.decision_model else {
        return "No local decision model is configured.".into();
    };
    let capabilities = status.decision_capabilities.as_ref();
    format!(
        "{} · {} · {} / {} MB\nRouting: {} · Speech admission: {}",
        model.model,
        if model.busy {
            "working"
        } else if model.loaded {
            "loaded"
        } else if model.present {
            "installed offline"
        } else {
            "needs installation"
        },
        model.acquired_bytes / 1_000_000,
        model.required_bytes / 1_000_000,
        if capabilities.is_some_and(|capabilities| capabilities.routing_validated) {
            "validated"
        } else if capabilities.is_some_and(|capabilities| capabilities.routing_supported) {
            "available when enabled; accuracy unvalidated"
        } else {
            "unavailable"
        },
        if capabilities.is_some_and(|capabilities| capabilities.admission_validated) {
            "validated"
        } else {
            "experimental; general listening unavailable"
        }
    )
}

pub(in crate::native::fiote::session) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let events: Vec<_> = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect()
        })
        .unwrap_or_default();
    for event in events {
        let (id, result) = match event {
            cell::ServerMessage::Speech { id, status } => (id, Ok(status)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owners: Vec<_> = world
            .query::<(Entity, &Settings)>()
            .iter(world)
            .filter(|(_, settings)| settings.pending.as_deref() == Some(&id))
            .map(|(owner, _)| owner)
            .collect();
        for owner in owners {
            let mut settings = world.get_mut::<Settings>(owner).unwrap();
            settings.pending = None;
            settings.next = Instant::now() + Duration::from_millis(250);
            let label = settings.label;
            match &result {
                Err(error) => {
                    settings.job = None;
                    let routing = settings.routing.take();
                    world.get_mut::<Text>(label).unwrap().0 = error.clone();
                    if let Some(routing) = routing {
                        deliver(world, owner, routing, None);
                    }
                }
                Ok(status) => {
                    settings.saved = Some(status.clone());
                    if let Some(job) = &status.job
                        && settings.job.as_deref() == Some(&job.id)
                        && job.purpose == Purpose::Decision
                        && !job.pending
                    {
                        settings.job = None;
                        let routing = settings.routing.take();
                        if let Some(routing) = routing {
                            deliver(world, owner, routing, job.decision.as_ref());
                        }
                        continue;
                    }
                    let mut text = caption(status);
                    if let Some(job) = &status.job
                        && settings.job.as_deref() == Some(&job.id)
                        && job.purpose == Purpose::DecisionSetup
                        && !job.pending
                    {
                        text = format!(
                            "{}\n{text}",
                            job.error
                                .as_deref()
                                .or(job.text.as_deref())
                                .unwrap_or("Model operation finished.")
                        );
                        settings.job = None;
                    }
                    world.get_mut::<Text>(label).unwrap().0 = text;
                }
            }
        }
    }
    let owners: Vec<_> = world
        .query::<(Entity, &Settings)>()
        .iter(world)
        .filter(|(_, settings)| {
            settings.pending.is_none() && settings.job.is_some() && settings.next <= Instant::now()
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        let job = world.get::<Settings>(owner).unwrap().job.clone().unwrap();
        if let Err(error) = send(world, owner, Request::Poll { job }) {
            let label = world.get::<Settings>(owner).unwrap().label;
            world.get_mut::<Text>(label).unwrap().0 = error;
        }
    }
    let disconnected: Vec<_> = world
        .query::<(Entity, &Settings)>()
        .iter(world)
        .filter(|(_, settings)| {
            (settings.pending.is_some() || settings.job.is_some())
                && settings
                    .cancellation
                    .as_ref()
                    .is_some_and(|cancellation| cancellation.is_closed())
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in disconnected {
        let mut settings = world.get_mut::<Settings>(owner).unwrap();
        settings.pending = None;
        settings.job = None;
        settings.routing = None;
        let label = settings.label;
        world.get_mut::<Text>(label).unwrap().0 = "The local model connection closed. Reconnect this Cell and retry; your draft is unchanged.".into();
    }
    let pending = world
        .query::<&Settings>()
        .iter(world)
        .any(|settings| settings.job.is_some());
    if pending && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests;
