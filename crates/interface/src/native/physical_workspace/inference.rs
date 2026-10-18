use super::*;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Route {
    placement: String,
    thread: String,
    content: Vec<nucleus::message::MessagePart>,
}

impl Route {
    pub(super) fn release_media(&mut self) {
        self.content.clear();
    }
}

struct Job {
    root: Entity,
    workspace: u64,
    request: String,
    placement: String,
    target: model::Target,
    progress: tokio::sync::watch::Receiver<model::Status>,
    stop: tokio::sync::watch::Sender<bool>,
    result: tokio::sync::mpsc::Receiver<Result<String, String>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

#[derive(Resource, Default)]
pub(super) struct Jobs(Vec<Job>);

pub(super) fn target(
    world: &mut World,
    root: Entity,
    workspace: u64,
    placement: &str,
) -> Result<(model::Target, serde_json::Value), String> {
    let owner =
        resolve(world, root, workspace, placement).ok_or("The source placement is unavailable.")?;
    let binding = world
        .get::<crate::native::protein_area::RecordBinding>(owner)
        .ok_or("Select a connected Record.")?;
    if binding.source != crate::native::protein_area::Source::Local
        || crate::native::topology::rooms::hosted(world, owner)
    {
        return Err(
            "Physical requests currently require a local Record under your control.".into(),
        );
    }
    let properties = world
        .get::<crate::native::area::RecordProperties>(owner)
        .ok_or("Wait for the Record snapshot.")?
        .0
        .clone();
    let revision = model::revision(&serde_json::json!({
        "uid":properties["uid"], "head":properties["head"], "body":properties["body"], "quantity":properties["quantity"],
        "extensions":properties["extensions"],
    }))?;
    Ok((
        model::Target {
            organ: "local".into(),
            record: binding.uid.clone(),
            revision,
        },
        properties,
    ))
}

pub(super) fn enqueue(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
    thread: &str,
    prompt: &str,
    include_frame: bool,
) -> Result<String, String> {
    permitted(world, root, workspace)?;
    if !nucleus::valid_uid(thread, "r") {
        return Err("Choose an existing Fiote conversation thread.".into());
    }
    if world
        .get_resource::<crate::native::app::CellHandle>()
        .and_then(|c| c.0.fiote.as_ref())
        .is_none()
    {
        return Err("Connect the existing Fiote service before sending requests.".into());
    }
    let snapshot = canvas_host::capture(world, root)?;
    let mut selected = snapshot
        .placements
        .iter()
        .filter(|p| p.selected && p.workspace == workspace);
    let placement = selected
        .next()
        .ok_or("Select one connected Record.")?
        .id
        .clone();
    if selected.next().is_some() {
        return Err("Select one Record per request.".into());
    }
    let (target, properties) = target(world, root, workspace, &placement)?;
    let linked = properties["threads"]
        .as_array()
        .is_some_and(|threads| threads.iter().any(|t| t["uid"].as_str() == Some(thread)))
        || properties["kind"] == "thread" && target.record == thread;
    if !linked {
        return Err("Choose a thread attached to the selected Record and include Threads in its Protein fields.".into());
    }
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    if session.routes.iter().any(|(id, route)| {
        route.thread == thread
            && session
                .runtime
                .requests
                .get(id)
                .is_some_and(|r| !r.terminal())
    }) {
        return Err("This conversation already has a physical request pending.".into());
    }
    if !session
        .bindings
        .iter()
        .any(|b| b.surface == surface && b.placement == placement)
    {
        return Err("Assign the selected Record to this surface first.".into());
    }
    let id = nucleus::new_uid("physical");
    let now = session.now_ms();
    let revision = session
        .runtime
        .surface(surface, workspace)?
        .calibration_revision;
    let latest = if include_frame {
        let observation = session
            .runtime
            .observations
            .values()
            .filter(|e| e.surface == surface && session.media.contains_key(&e.id))
            .max_by_key(|e| e.captured_ms)
            .ok_or("Capture a fresh frame before sending it.")?;
        Some((
            observation.id.clone(),
            session.media[&observation.id].content.clone(),
        ))
    } else {
        None
    };
    session.runtime.enqueue(
        model::Request {
            id: id.clone(),
            surface: surface.into(),
            user: "local-ui".into(),
            target,
            observations: latest
                .as_ref()
                .map(|(id, _)| vec![id.clone()])
                .unwrap_or_default(),
            prompt: prompt.into(),
            status: model::Status::Queued,
            calibration_revision: revision,
            result: None,
            created_ms: now,
            updated_ms: now,
        },
        workspace,
        now,
    )?;
    session.routes.insert(
        id.clone(),
        Route {
            placement,
            thread: thread.into(),
            content: latest.map(|(_, content)| vec![content]).unwrap_or_default(),
        },
    );
    Ok(id)
}

async fn run(
    host: Arc<cell::fiote::Host>,
    request: model::Request,
    route: Route,
    mut stop: tokio::sync::watch::Receiver<bool>,
    progress: tokio::sync::watch::Sender<model::Status>,
    wake: Option<crate::native::wake::WakeSignal>,
) -> Result<String, String> {
    let turn = host
        .start_physical_request_with_content(
            &request.id,
            &route.thread,
            &request.prompt,
            &route.content,
        )
        .await?;
    let started = Instant::now();
    loop {
        if *stop.borrow() || started.elapsed().as_secs() >= 120 {
            host.stop_physical_request(&turn).await?;
            return Err("Request canceled.".into());
        }
        if let Some(reply) = host.physical_reply(&turn).await? {
            return Ok(reply);
        }
        let status = if host.physical_waiting(&turn).await? {
            model::Status::Waiting
        } else {
            model::Status::Running
        };
        if progress.send_if_modified(|value| {
            let changed = *value != status;
            *value = status;
            changed
        }) && let Some(wake) = &wake
        {
            wake.ring();
        }
        tokio::select! {
            result = stop.changed() => { if result.is_err() { host.stop_physical_request(&turn).await?; return Err("Request owner closed.".into()); } }
            _ = tokio::time::sleep(std::time::Duration::from_millis(150)) => {}
        }
    }
}

pub(super) fn tick(world: &mut World) {
    let Some(mut jobs) = world.remove_resource::<Jobs>() else {
        return;
    };
    let mut retained = Vec::new();
    for mut job in jobs.0.drain(..) {
        let status = *job.progress.borrow();
        if let Some(mut installation) = world.get_mut::<Installation>(job.root)
            && let Some(session) = installation.0.get_mut(&job.workspace)
            && let Some(request) = session.runtime.requests.get_mut(&job.request)
            && !request.terminal()
        {
            request.status = status;
        }
        let relevant = active_scope(world, job.root, job.workspace)
            && target(world, job.root, job.workspace, &job.placement)
                .is_ok_and(|(target, _)| target == job.target)
            && world
                .get::<Installation>(job.root)
                .and_then(|i| i.0.get(&job.workspace))
                .and_then(|s| s.runtime.requests.get(&job.request))
                .is_some_and(|r| {
                    !r.terminal()
                        && world.get::<Installation>(job.root).unwrap().0[&job.workspace]
                            .bindings
                            .iter()
                            .any(|b| b.surface == r.surface && b.placement == job.placement)
                });
        if !relevant {
            let _ = job.stop.send(true);
            if let Some(mut installation) = world.get_mut::<Installation>(job.root)
                && let Some(session) = installation.0.get_mut(&job.workspace)
            {
                let _ = session.runtime.cancel(&job.request);
            }
        }
        match job.result.try_recv() {
            Ok(result) => {
                let route = world
                    .get::<Installation>(job.root)
                    .and_then(|i| i.0.get(&job.workspace))
                    .and_then(|s| s.routes.get(&job.request))
                    .cloned();
                let current = route.and_then(|route| {
                    target(world, job.root, job.workspace, &route.placement)
                        .ok()
                        .map(|(target, _)| target)
                });
                if let Some(mut installation) = world.get_mut::<Installation>(job.root)
                    && let Some(session) = installation.0.get_mut(&job.workspace)
                {
                    let now = session.now_ms();
                    match (result, current) {
                        (Ok(answer), Some(target)) => {
                            let result = session.runtime.finish(
                                &job.request,
                                &target,
                                "local-ui",
                                answer,
                                now,
                            );
                            session.status = result.err().unwrap_or_else(|| "Fiote answer received in its conversation and assigned surface.".into());
                        }
                        (Err(error), _) => {
                            if let Some(request) = session.runtime.requests.get_mut(&job.request)
                                && !request.terminal()
                            {
                                request.status = model::Status::Failed;
                            }
                            session.status = error;
                        }
                        (_, None) => {
                            let _ = session.runtime.cancel(&job.request);
                            session.status =
                                "Source unavailable; result remains in its conversation.".into();
                        }
                    }
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => retained.push(job),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                if let Some(mut installation) = world.get_mut::<Installation>(job.root)
                    && let Some(session) = installation.0.get_mut(&job.workspace)
                {
                    let _ = session.runtime.cancel(&job.request);
                    session.status = "Fiote worker stopped.".into();
                }
            }
        }
    }
    jobs.0 = retained;
    let host = world
        .get_resource::<crate::native::app::CellHandle>()
        .and_then(|c| c.0.fiote.clone());
    if let (Some(host), Ok(handle)) = (host, tokio::runtime::Handle::try_current()) {
        let roots: Vec<_> = world
            .query::<(Entity, &Workspaces, &Installation)>()
            .iter(world)
            .map(|(root, workspaces, _)| (root, workspaces.active))
            .collect();
        for (root, workspace) in roots {
            if permitted(world, root, workspace).is_err() || jobs.0.len() >= model::MAX_RUNNING {
                continue;
            }
            let available = model::MAX_RUNNING - jobs.0.len();
            let wake = world
                .get_resource::<crate::native::wake::WakeSignal>()
                .cloned();
            let mut installation = world.get_mut::<Installation>(root).unwrap();
            let Some(session) = installation.0.get_mut(&workspace) else {
                continue;
            };
            let now = session.now_ms();
            let tasks = session.runtime.dispatch(workspace, now).unwrap_or_default();
            let mut launch = Vec::new();
            for (index, request) in tasks.into_iter().enumerate() {
                if index >= available {
                    session
                        .runtime
                        .requests
                        .get_mut(&request.id)
                        .unwrap()
                        .status = model::Status::Queued;
                    continue;
                }
                let Some(route) = session.routes.get(&request.id).cloned() else {
                    let _ = session.runtime.cancel(&request.id);
                    continue;
                };
                session.routes.get_mut(&request.id).unwrap().release_media();
                launch.push((request, route));
            }
            drop(installation);
            for (request, route) in launch {
                if !target(world, root, workspace, &route.placement)
                    .is_ok_and(|(target, _)| target == request.target)
                {
                    if let Some(session) = world
                        .get_mut::<Installation>(root)
                        .unwrap()
                        .0
                        .get_mut(&workspace)
                    {
                        let _ = session.runtime.cancel(&request.id);
                    }
                    continue;
                }
                let (stop, receiver) = tokio::sync::watch::channel(false);
                let (progress, progress_receiver) =
                    tokio::sync::watch::channel(model::Status::Running);
                let (sender, result) = tokio::sync::mpsc::channel(1);
                jobs.0.push(Job {
                    root,
                    workspace,
                    request: request.id.clone(),
                    placement: route.placement.clone(),
                    target: request.target.clone(),
                    progress: progress_receiver,
                    stop,
                    result,
                });
                let host = host.clone();
                let wake = wake.clone();
                handle.spawn(async move {
                    let _ = sender
                        .send(run(host, request, route, receiver, progress, wake.clone()).await)
                        .await;
                    if let Some(wake) = wake {
                        wake.ring();
                    }
                });
            }
        }
    }
    world.insert_resource(jobs);
}
