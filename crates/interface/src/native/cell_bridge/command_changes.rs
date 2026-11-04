use super::*;

pub(crate) struct CommandChanges {
    source: cell::terminal::commands::Source,
    pending: Arc<AtomicBool>,
    live: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl CommandChanges {
    pub(crate) fn retry(world: &World, owner: Entity) -> std::time::Instant {
        let now = std::time::Instant::now();
        if crate::native::practice_cells::sender(world, owner)
            .is_some_and(|sender| !sender.is_closed())
            && let Some(wake) = world.get_resource::<WakeSignal>()
        {
            let delay = std::time::Duration::from_millis(20);
            wake.after(delay);
            now + delay
        } else {
            now
        }
    }

    pub(crate) fn new(
        world: &World,
        owner: Entity,
        command: &str,
        run: Option<&str>,
    ) -> Option<Self> {
        let wake = world.get_resource::<WakeSignal>()?.clone();
        let mut changes =
            crate::native::practice_cells::command_changes(world, owner, command, run)?;
        let source = changes.source();
        let pending = Arc::new(AtomicBool::new(false));
        let changed = pending.clone();
        let live = Arc::new(AtomicBool::new(true));
        let active = live.clone();
        let task = tokio::runtime::Handle::try_current()
            .ok()?
            .spawn(async move {
                loop {
                    let open = changes.changed().await;
                    if !active.load(Ordering::Acquire) {
                        break;
                    }
                    if !changed.swap(true, Ordering::AcqRel) {
                        wake.ring();
                    }
                    if !open {
                        break;
                    }
                }
            });
        Some(Self {
            source,
            pending,
            live,
            task,
        })
    }

    pub(crate) fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn retain(&self) {
        self.pending.store(true, Ordering::Release);
    }

    pub(crate) fn current(&self, world: &World, owner: Entity) -> bool {
        crate::native::practice_cells::command_source(world, owner)
            .is_some_and(|source| self.source.same(&source))
    }

    pub(crate) fn cancel(&self) {
        self.live.store(false, Ordering::Release);
        self.pending.store(false, Ordering::Release);
        self.task.abort();
    }
}

impl Drop for CommandChanges {
    fn drop(&mut self) {
        self.cancel();
    }
}
