use super::*;
use lince_interface::{host::SurfaceId, layout::desktop::PlacementId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{self, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod inspection;
mod storage;

#[cfg(test)]
mod performance;
#[cfg(test)]
mod persistence;
#[cfg(test)]
mod tests;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Saved {
    pub layout: native::layout::LayoutBox,
    pub workspace: u64,
    pub position: [f64; 2],
    pub size: [f32; 2],
    pub tokens: native::tokens::TokenOverrides,
    pub launch: Option<crate::launch::Description>,
}

impl Saved {
    fn valid(&self) -> bool {
        self.layout.valid()
            && self.workspace > 0
            && self.position.iter().all(|value| value.is_finite())
            && self
                .size
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
            && self.tokens.validate()
            && self
                .launch
                .as_ref()
                .is_none_or(crate::launch::Description::valid)
    }
}

struct Saving {
    version: u64,
    notify: Arc<AtomicBool>,
}

pub(super) struct Records {
    path: PathBuf,
    saved: BTreeMap<PlacementId, Saved>,
    dirty: bool,
    last_write: Instant,
    wake_pending: bool,
    writer: Option<storage::Writer>,
    saving: Option<Saving>,
    save_version: u64,
}

impl Records {
    pub fn load(path: PathBuf) -> io::Result<Self> {
        Ok(Self {
            saved: read(&path)?
                .into_iter()
                .map(|saved| (PlacementId(saved.layout.id), saved))
                .collect(),
            path,
            dirty: false,
            last_write: Instant::now(),
            wake_pending: false,
            writer: None,
            saving: None,
            save_version: 0,
        })
    }

    pub fn profile(&self, identity: PlacementId) -> Option<&Saved> {
        self.saved.get(&identity)
    }

    pub(super) fn capture_metadata_excluded(&self, identity: PlacementId, key: &str) -> bool {
        self.profile(identity)
            .filter(|_| key.is_empty())
            .is_none_or(|profile| profile.layout.capture_excluded)
    }

    pub(super) fn capture_exclusion(
        &mut self,
        identity: PlacementId,
        key: &str,
        excluded: bool,
    ) -> io::Result<()> {
        if !key.is_empty() || !self.saved.contains_key(&identity) {
            return Err(io::Error::other(
                "The original window profile is unavailable",
            ));
        }
        self.finish_saving()?;
        let profile = self.saved.get_mut(&identity).unwrap();
        let previous = profile.layout.capture_excluded;
        let dirty = self.dirty;
        profile.layout.capture_excluded = excluded;
        self.dirty = true;
        if let Err(error) = self.flush() {
            self.saved
                .get_mut(&identity)
                .unwrap()
                .layout
                .capture_excluded = previous;
            self.dirty = dirty;
            return Err(error);
        }
        Ok(())
    }

    pub fn capture(&mut self, world: &World, window: &clients::Window, identity: PlacementId) {
        if window.parent.is_some() {
            return;
        }
        let Some(layout) = world
            .get::<native::layout::LayoutBox>(window.entity)
            .copied()
        else {
            return;
        };
        let Some(item) = world.get::<native::canvas::CanvasItem>(window.entity) else {
            return;
        };
        let Some(member) = world.get::<native::workspace::WorkspaceMember>(window.entity) else {
            return;
        };
        let previous = self.saved.get(&identity);
        if previous.is_none() && self.saved.len() >= 65536 {
            return;
        }
        let saved = Saved {
            layout,
            workspace: member.0,
            position: item.position.to_array(),
            size: item.size.to_array(),
            tokens: world
                .get::<native::tokens::TokenOverrides>(window.entity)
                .cloned()
                .unwrap_or_default(),
            launch: previous.and_then(|saved| saved.launch.clone()),
        };
        if previous != Some(&saved) {
            self.saved.insert(identity, saved);
            self.dirty = true;
        }
    }

    pub fn tick(
        &mut self,
        world: &mut World,
        windows: &HashMap<SurfaceId, clients::Window>,
        identities: &HashMap<SurfaceId, PlacementId>,
    ) {
        self.poll_saving(world);
        for (surface, window) in windows {
            if let Some(identity) = identities.get(surface) {
                self.capture(world, window, *identity);
            }
        }
        if self.dirty
            && self.saving.is_none()
            && self.last_write.elapsed() >= Duration::from_secs(1)
        {
            if let Err(error) = self.start_saving(world.get_resource::<WakeSignal>().cloned()) {
                self.last_write = Instant::now();
                self.wake_pending = false;
                native::notifications::report(
                    world,
                    "Park window customization",
                    &error.to_string(),
                );
            }
        }
        if self.dirty
            && self.saving.is_none()
            && !self.wake_pending
            && let Some(wake) = world.get_resource::<WakeSignal>()
        {
            wake.after(Duration::from_secs(1).saturating_sub(self.last_write.elapsed()));
            self.wake_pending = true;
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.finish_saving()?;
        self.start_saving(None)?;
        self.finish_saving()?;
        Ok(())
    }

    fn start_saving(&mut self, wake: Option<WakeSignal>) -> io::Result<()> {
        if !self.dirty || self.saving.is_some() {
            return Ok(());
        }
        let version = self
            .save_version
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Window profile save version exhausted"))?;
        if self.writer.is_none() {
            self.writer = Some(storage::Writer::new(self.path.clone())?);
        }
        let profiles: Vec<_> = self.saved.values().cloned().collect();
        let notify = Arc::new(AtomicBool::new(false));
        self.writer.as_ref().unwrap().request(storage::Request {
            version,
            profiles,
            wake,
            notify: notify.clone(),
        })?;
        self.save_version = version;
        self.saving = Some(Saving { version, notify });
        self.dirty = false;
        self.last_write = Instant::now();
        self.wake_pending = false;
        Ok(())
    }

    fn saved(&mut self, finished: storage::Finished) -> io::Result<()> {
        let pending = self
            .saving
            .as_ref()
            .ok_or_else(|| io::Error::other("Unexpected window profile save result"))?;
        if pending.version != finished.version {
            return Err(io::Error::other("Outdated window profile save result"));
        }
        let pending = self.saving.take().unwrap();
        pending.notify.store(false, Ordering::Release);
        self.last_write = Instant::now();
        self.wake_pending = false;
        if let Err(error) = finished.result {
            self.dirty = true;
            return Err(error);
        }
        Ok(())
    }

    fn stopped_saving(&mut self) {
        self.saving = None;
        self.writer = None;
        self.dirty = true;
        self.last_write = Instant::now();
        self.wake_pending = false;
    }

    fn poll_saving(&mut self, world: &World) {
        if self.saving.is_none() {
            return;
        }
        let result = match self.writer.as_ref().unwrap().poll() {
            Ok(Some(finished)) => self.saved(finished),
            Ok(None) => return,
            Err(error) => {
                self.stopped_saving();
                Err(error)
            }
        };
        if let Err(error) = result {
            native::notifications::report(world, "Park window customization", &error.to_string());
        }
    }

    fn finish_saving(&mut self) -> io::Result<()> {
        if self.saving.is_none() {
            return Ok(());
        }
        match self.writer.as_ref().unwrap().finish() {
            Ok(finished) => self.saved(finished),
            Err(error) => {
                self.stopped_saving();
                Err(error)
            }
        }
    }

    pub fn launch(
        &mut self,
        identity: PlacementId,
        launch: crate::launch::Description,
    ) -> io::Result<()> {
        if !launch.valid() {
            return Err(io::Error::other("invalid window launch description"));
        }
        let saved = self
            .saved
            .get_mut(&identity)
            .ok_or_else(|| io::Error::other("the window profile is unavailable"))?;
        saved.launch = Some(launch);
        self.dirty = true;
        self.flush()
    }
}

impl Presentation {
    pub fn window_launch_description(
        &mut self,
        surface: SurfaceId,
        launch: crate::launch::Description,
    ) -> io::Result<()> {
        let identity = self
            .identities
            .get(&surface)
            .copied()
            .ok_or_else(|| io::Error::other("the window is unavailable"))?;
        self.records
            .as_mut()
            .ok_or_else(|| io::Error::other("Park is not connected to a Cell"))?
            .launch(identity, launch)
    }
}

impl Drop for Records {
    fn drop(&mut self) {
        if let Err(error) = self.flush() {
            eprintln!("Could not save Park window customizations: {error}");
        }
    }
}

fn read(path: &Path) -> io::Result<Vec<Saved>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other(
            "window customization data exceeds its limit",
        ));
    }
    let records: Vec<Saved> = serde_json::from_slice(&bytes)?;
    let mut identities = BTreeSet::new();
    if records.len() > 65536
        || records
            .iter()
            .any(|record| !record.valid() || !identities.insert(record.layout.id))
    {
        return Err(io::Error::other("invalid window customization data"));
    }
    Ok(records)
}
