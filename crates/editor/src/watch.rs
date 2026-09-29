use crate::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub struct Changes {
    pub paths: BTreeSet<PathBuf>,
    pub rescan: bool,
    pub error: Option<String>,
}

pub struct Watch {
    watcher: RecommendedWatcher,
    watched: BTreeSet<PathBuf>,
    changes: Arc<Mutex<Changes>>,
}

impl Watch {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let pending = changes.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let mut pending = pending.lock().expect("watch events");
            match event {
                Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
                Ok(event) => {
                    pending.rescan |= event.need_rescan();
                    if pending.paths.len() + event.paths.len() > 512 {
                        pending.rescan = true;
                    } else {
                        pending.paths.extend(event.paths);
                    }
                }
                Err(error) => {
                    pending.error = Some(error.to_string());
                    pending.rescan = true;
                }
            }
            drop(pending);
            wake();
        })
        .map_err(|e| e.to_string())?;
        Ok(Self {
            watcher,
            watched: BTreeSet::new(),
            changes,
        })
    }

    pub fn set(&mut self, wanted: BTreeSet<PathBuf>) -> Result<()> {
        for path in self
            .watched
            .difference(&wanted)
            .cloned()
            .collect::<Vec<_>>()
        {
            let _ = self.watcher.unwatch(&path);
            self.watched.remove(&path);
        }
        for path in wanted
            .difference(&self.watched)
            .cloned()
            .collect::<Vec<_>>()
        {
            self.watcher
                .watch(&path, RecursiveMode::NonRecursive)
                .map_err(|e| e.to_string())?;
            self.watched.insert(path);
        }
        Ok(())
    }

    pub fn drain(&self) -> Changes {
        std::mem::take(&mut *self.changes.lock().expect("watch events"))
    }
}
