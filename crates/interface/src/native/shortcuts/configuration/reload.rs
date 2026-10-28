use super::*;
use crate::native::{actions::Action, layout::desktop::input_settings::{button, text}};
use lince_editor::watch::{Changes, Watch};
use std::{collections::BTreeSet, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

struct Loaded {
    path: PathBuf,
    targets: BTreeSet<PathBuf>,
    event_driven: bool,
    missing: bool,
    result: Result<(Vec<u8>, Settings, LayoutSettings), String>,
}

#[derive(Resource, Default)]
struct Reload {
    retired: Arc<AtomicBool>,
    path: Option<PathBuf>,
    watch: Option<Arc<Mutex<Watch>>>,
    pending: Option<Arc<Mutex<Changes>>>,
    targets: BTreeSet<PathBuf>,
    job: Option<tokio::task::JoinHandle<()>>,
    job_deadline: Option<Instant>,
    timed_out: bool,
    origin: Option<Option<Vec<u8>>>,
    received: Arc<Mutex<Option<Loaded>>>,
    clock: Option<(Instant, tokio::task::JoinHandle<()>)>,
    first: Option<Instant>,
    last: Option<Instant>,
    force: bool,
    fallback: bool,
    status: String,
}

impl Drop for Reload {
    fn drop(&mut self) {
        self.retired.store(true, Ordering::Release);
        if let Some((_, clock)) = self.clock.take() { clock.abort(); }
        if let Some(job) = self.job.take() { job.abort(); }
    }
}

fn delivered(result: &Mutex<Option<Loaded>>, retired: &AtomicBool, wake: Option<&crate::host::WakeSignal>, loaded: Loaded) {
    let mut received = result.lock().unwrap_or_else(|error| error.into_inner());
    if retired.load(Ordering::Acquire) { return; }
    *received = Some(loaded);
    drop(received);
    if !retired.load(Ordering::Acquire) && let Some(wake) = wake { wake.ring(); }
}

#[derive(Component)]
struct Status;

#[derive(Clone, Copy)]
struct Refresh;

impl Action for Refresh {
    fn apply(&self, world: &mut World, _: Entity) {
        if crate::native::shortcuts::keyboard_owned(world) { return; }
        world.init_resource::<Reload>();
        world.resource_mut::<Reload>().force = true;
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    }
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    crate::native::edit_mode::label(world, parent, "Interface configuration", 20.0);
    let status = crate::native::edit_mode::label(world, parent, "", 14.0);
    world.entity_mut(status).insert((Status, TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter)));
    let mut node = world.get_mut::<Node>(status).unwrap();
    node.width = percent(100);
    node.min_width = px(0);
    button(world, parent, parent, "Read configuration now", Refresh);
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Reload>().add_systems(Update, update.before(crate::native::protein_area::UpdateProteinAreas));
}

fn existing_parent(path: &Path) -> Option<PathBuf> {
    let mut parent = path.parent();
    for _ in 0..64 {
        let value = parent?;
        if value.is_dir() { return Some(value.to_owned()); }
        parent = value.parent();
    }
    None
}

fn load(path: PathBuf, watcher: Arc<Mutex<Watch>>, fallback: bool) -> Loaded {
    let mut targets = BTreeSet::from([path.clone()]);
    let mut link = path.clone();
    for _ in 0..32 {
        let Ok(next) = std::fs::read_link(&link) else { break; };
        let next = if next.is_absolute() { next } else { link.parent().unwrap_or(Path::new(".")).join(next) };
        let next = next.parent().and_then(|parent| std::fs::canonicalize(parent).ok())
            .zip(next.file_name()).map(|(parent, name)| parent.join(name)).unwrap_or(next);
        if !targets.insert(next.clone()) { break; }
        link = next;
    }
    if let Ok(target) = std::fs::canonicalize(&path) { targets.insert(target); }
    let directories = targets.iter().filter_map(|path| existing_parent(path)).collect();
    let mut watcher = watcher.lock().unwrap_or_else(|error| error.into_inner());
    if fallback { watcher.use_polling(); }
    let watched = watcher.set(directories).map_err(|error| error.to_string());
    let event_driven = watcher.event_driven();
    drop(watcher);
    let mut missing = false;
    let result = watched.and_then(|_| {
        let bytes = read(&path)?.ok_or_else(|| { missing = true; "The selected interface configuration is missing. Last accepted settings remain active.".to_owned() })?;
        let (settings, layout) = decode(&bytes)?;
        Ok((bytes, settings, layout))
    });
    Loaded { path, targets, event_driven, missing, result }
}

fn deadline(state: &Reload) -> Option<Instant> {
    Some((state.last? + Duration::from_millis(150)).min(state.first? + Duration::from_millis(500)))
}

fn arm(world: &World, state: &mut Reload) {
    let next = if state.job.is_some() { state.job_deadline } else { deadline(state) };
    if state.clock.as_ref().map(|(deadline, _)| *deadline) == next { return; }
    if let Some((_, clock)) = state.clock.take() { clock.abort(); }
    if let Some(deadline) = next
        && let Some(wake) = world.get_resource::<crate::host::WakeSignal>().cloned()
        && let Ok(runtime) = tokio::runtime::Handle::try_current() {
        state.clock = Some((deadline, runtime.spawn(async move { tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await; wake.ring(); })));
    }
}

fn accepted(world: &mut World, state: &mut Reload, loaded: Loaded) {
    state.job = None;
    state.job_deadline = None;
    state.targets = loaded.targets;
    if std::mem::take(&mut state.timed_out) { state.force = true; state.status = "The timed-out read finished; checking the latest configuration before applying anything.".into(); return; }
    if state.path.as_ref() != Some(&loaded.path) { state.force = true; return; }
    let origin = state.origin.take();
    if origin.as_ref() != world.get_resource::<Storage>().map(|storage| &storage.source) {
        state.force = true;
        state.status = "Configuration changed during the read; checking the latest file.".into();
        return;
    }
    match loaded.result {
        Ok((bytes, settings, layout)) => {
            let mut storage = world.resource_mut::<Storage>();
            if storage.source.as_ref() != Some(&bytes) { storage.revision = storage.revision.saturating_add(1); }
            storage.source = Some(bytes);
            storage.error = None;
            state.status = if loaded.event_driven { "External edits are watched. Valid replacements apply atomically; open drafts remain available for review.".into() }
                else { "Native file notifications are unavailable; the existing filesystem polling fallback is active and may detect edits later.".into() };
            publish(world, settings, layout);
        }
        Err(_) if loaded.missing && world.resource::<Storage>().source.is_none() => {
            state.status = "No interface configuration has been saved yet. The selected directory is watched; normal Save creates the file.".into();
            if !loaded.event_driven { state.status.push_str(" Native notifications are unavailable; filesystem polling may detect edits later."); }
        }
        Err(error) => {
            let changed = world.resource::<Storage>().error.as_ref() != Some(&error);
            world.resource_mut::<Storage>().error = Some(error.clone());
            state.status = format!("Last accepted settings remain active: {error}");
            if !loaded.event_driven { state.status.push_str(" Native notifications are unavailable; filesystem polling may detect recovery later."); }
            if changed { crate::native::notifications::report(world, "Interface configuration", &state.status); }
        }
    }
}

fn update(world: &mut World) {
    let mut state = world.remove_resource::<Reload>().unwrap_or_default();
    let selected = world.get_resource::<Storage>().and_then(|storage| storage.path.clone());
    if selected != state.path {
        state.path = selected.clone();
        state.targets = selected.iter().cloned().collect();
        state.force = selected.is_some();
        if selected.is_none() {
            if let Some(job) = state.job.take() { job.abort(); }
            state.job_deadline = None;
            state.timed_out = false;
            state.origin = None;
            state.received = Default::default();
            state.watch = None;
            state.pending = None;
            state.first = None;
            state.last = None;
        }
    }
    if state.watch.is_none() && selected.is_some() {
        let wake = world.get_resource::<crate::host::WakeSignal>().cloned();
        let retired = state.retired.clone();
        match Watch::new(move || { if !retired.load(Ordering::Acquire) && let Some(wake) = &wake { wake.ring(); } }) {
            Ok(watch) => { state.pending = Some(watch.pending()); state.watch = Some(Arc::new(Mutex::new(watch))); }
            Err(error) => state.status = format!("Configuration watching is unavailable: {error}"),
        }
    }
    let loaded = state.received.lock().unwrap_or_else(|error| error.into_inner()).take();
    if let Some(loaded) = loaded { accepted(world, &mut state, loaded); }
    if state.job_deadline.is_some_and(|deadline| deadline <= Instant::now()) {
        state.job_deadline = None;
        state.timed_out = true;
        state.status = "Configuration reading timed out. Last accepted settings remain active; no second reader will start while this read is still running.".into();
    }
    if state.job.as_ref().is_some_and(|job| job.is_finished()) {
        let loaded = state.received.lock().unwrap_or_else(|error| error.into_inner()).take();
        if let Some(loaded) = loaded { accepted(world, &mut state, loaded); }
        else {
            state.job = None;
            state.job_deadline = None;
            state.origin = None;
            state.timed_out = false;
            state.status = "The configuration reader stopped without a result. Last accepted settings remain active; use Read configuration now to retry.".into();
        }
    }
    let changes = state.pending.as_ref().map(|pending| std::mem::take(&mut *pending.lock().unwrap_or_else(|error| error.into_inner()))).unwrap_or_default();
    let relevant = changes.rescan || changes.error.is_some() || changes.paths.iter().any(|path| state.targets.iter().any(|target| target == path || target.starts_with(path)));
    if relevant {
        let now = Instant::now();
        state.first.get_or_insert(now);
        state.last = Some(now);
        state.fallback |= changes.error.is_some();
    }
    if state.job.is_none() && (state.force || deadline(&state).is_some_and(|deadline| deadline <= Instant::now()))
        && let (Some(path), Some(watcher)) = (selected, state.watch.clone()) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            state.force = false;
            state.first = None;
            state.last = None;
            state.origin = world.get_resource::<Storage>().map(|storage| storage.source.clone());
            let result = state.received.clone();
            let wake = world.get_resource::<crate::host::WakeSignal>().cloned();
            let retired = state.retired.clone();
            let fallback = std::mem::take(&mut state.fallback);
            state.status = "Reading the selected configuration…".into();
            state.job_deadline = Some(Instant::now() + Duration::from_secs(10));
            state.timed_out = false;
            state.job = Some(runtime.spawn_blocking(move || {
                let loaded = load(path, watcher, fallback);
                delivered(&result, &retired, wake.as_ref(), loaded);
            }));
        } else {
            state.force = false;
            state.first = None;
            state.last = None;
            state.status = "Configuration watching needs the interface runtime. Existing settings and Save controls remain available.".into();
        }
    }
    arm(world, &mut state);
    let status = if state.path.is_none() { "Interface preferences apply to this session; no configuration file is selected.".to_owned() }
        else { format!("{}\nAccepted revision: {}", state.status, world.get_resource::<Storage>().map_or(0, |storage| storage.revision)) };
    let labels: Vec<_> = world.query_filtered::<Entity, With<Status>>().iter(world).collect();
    for label in labels { text(world, label, &status); }
    world.insert_resource(state);
}

#[cfg(test)]
mod tests;
