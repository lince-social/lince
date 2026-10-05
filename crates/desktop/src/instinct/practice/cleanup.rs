use super::*;

#[derive(Default)]
pub(super) struct Quiescence {
    receiver: Option<Mutex<mpsc::Receiver<Result<(), String>>>>,
    task: Option<tokio::task::JoinHandle<()>>,
    result: Option<Result<(), String>>,
}

impl Quiescence {
    pub(super) fn ready(&self) -> bool {
        self.result.as_ref().is_some_and(Result::is_ok)
    }

    pub(super) fn message(&self) -> String {
        match &self.result {
            Some(Ok(())) => "Sample automation is stopped. Property actions are disabled.".into(),
            Some(Err(error)) => format!("Could not confirm that sample automation stopped: {error}. Discard remains available."),
            None => "Stopping sample automation. Interaction is already restored. Discard remains available.".into(),
        }
    }
}

impl Drop for Quiescence {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

pub(super) fn start(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.cleanup.receiver.is_some() || practice.cleanup.result.is_some() {
        return;
    }
    let sources: Vec<_> = std::iter::once(practice.source.clone())
        .chain(practice.extra_sources.iter().cloned())
        .collect();
    let runtimes: Vec<_> = sources
        .iter()
        .filter_map(|source| {
            world
                .resource::<crate::practice_cells::PracticeCells>()
                .cells
                .get(source)
                .cloned()
        })
        .collect();
    let mut audio_stopped = Vec::new();
    for source in &sources {
        if let Some(audio) = world
            .resource_mut::<crate::practice_cells::PracticeCells>()
            .audio
            .remove(source)
        {
            audio_stopped.push(audio.shutdown());
        }
        world
            .resource_mut::<crate::practice_cells::PracticeCells>()
            .workers
            .remove(source);
        world
            .resource_mut::<crate::practice_cells::PracticeCells>()
            .servers
            .remove(source);
    }
    if runtimes.is_empty() && audio_stopped.is_empty() {
        world.get_mut::<Practice>(root).unwrap().cleanup.result = Some(Ok(()));
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        world.get_mut::<Practice>(root).unwrap().cleanup.result =
            Some(Err("The sample runtime is unavailable.".into()));
        return;
    };
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = mpsc::channel();
    let task = handle.spawn(async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            for runtime in &runtimes {
                stop(runtime).await?;
            }
            while audio_stopped
                .iter()
                .any(|stopped| !stopped.load(std::sync::atomic::Ordering::Acquire))
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Ok::<_, String>(())
        })
        .await
        .unwrap_or_else(|_| Err("Stopping the sample timed out.".into()));
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    let mut practice = world.get_mut::<Practice>(root).unwrap();
    practice.cleanup.receiver = Some(Mutex::new(receiver));
    practice.cleanup.task = Some(task);
}

async fn stop(runtime: &cell::CellRuntime) -> Result<(), String> {
    use engine::actions::Action;
    runtime
        .engine
        .act(
            Action::SetCellConfig {
                namespace: "lince.karma-runtime".into(),
                fds: serde_json::json!({"running":false}),
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    runtime.commands.shutdown().await;
    runtime
        .engine
        .act(Action::SetFileSyncEnabled { enabled: false }, None)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(wire) = runtime.wire.write().await.take() {
        wire.endpoint().close().await;
    }
    if let Some(fiote) = &runtime.fiote {
        fiote.stop_all().await;
    }
    if let Ok(blobs) = runtime.engine.blob_sync() {
        for transfer in runtime
            .engine
            .blob_transfers()
            .await
            .map_err(|error| error.to_string())?
        {
            if matches!(transfer.state.as_str(), "offered" | "accepted") {
                runtime
                    .engine
                    .stop_blob_copy(&transfer.id)
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
        blobs.shutdown().await.map_err(|error| error.to_string())?;
    }
    for rule in store::recurrence::all(&runtime.store.pool)
        .await
        .map_err(|error| error.to_string())?
    {
        if !rule.is_paused() {
            runtime
                .engine
                .act(
                    Action::SetRecurrencePaused {
                        recurrence: rule.uid,
                        expected_revision: rule.revision,
                        request_id: nucleus::new_uid("instinct-stop"),
                        paused: true,
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    for clock in store::karma::frequencies::list_handles(&runtime.store.pool)
        .await
        .map_err(|error| error.to_string())?
    {
        if clock.status == nucleus::karma::DefinitionStatus::Active {
            runtime
                .engine
                .act(
                    Action::PauseKarmaFrequency {
                        request_id: nucleus::new_uid("instinct-stop"),
                        frequency_uid: clock.record_uid,
                        expected_handle_revision: clock.handle_revision,
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn receive(world: &mut World, root: Entity) -> bool {
    let result = world
        .get::<Practice>(root)
        .unwrap()
        .cleanup
        .receiver
        .as_ref()
        .and_then(|receiver| receiver.lock().ok()?.try_recv().ok());
    let Some(result) = result else { return false };
    let mut practice = world.get_mut::<Practice>(root).unwrap();
    practice.cleanup.result = Some(result);
    practice.cleanup.receiver = None;
    practice.cleanup.task = None;
    true
}
