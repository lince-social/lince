use super::*;
use lince_map::Settings;
use std::{fs, path::PathBuf};

#[derive(Resource, Default)]
pub(super) struct Sources {
    pub settings: Settings,
    pub store: Option<Arc<TileStore>>,
    pub revision: u64,
    pub error: Option<String>,
    path: Option<PathBuf>,
    cache: PathBuf,
}

pub(super) fn initialize(world: &mut World) {
    if world.resource::<Sources>().path.is_some() {
        return;
    }
    let base = world
        .get_resource::<crate::native::workspace::WorkspaceFile>()
        .map(|file| file.directory().to_path_buf())
        .or_else(|| {
            world
                .get_resource::<crate::native::topology::assets::AssetDirectory>()
                .and_then(|directory| directory.0.parent().map(std::path::Path::to_path_buf))
        });
    let Some(base) = base else { return };
    let path = base.join("maps/settings.json");
    let mut settings = Settings::default();
    let loaded = match fs::read(&path) {
        Ok(bytes) if bytes.len() <= 65_536 => {
            serde_json::from_slice::<Settings>(&bytes).map_err(|error| error.to_string())
        }
        Ok(_) => Err("Map Settings exceed their size limit".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(packages) = std::env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(|parent| parent.join("maps")))
            {
                if packages.join("tiles").is_dir() {
                    settings.local_directory = packages.join("tiles");
                }
                if let Ok(entries) = fs::read_dir(packages) {
                    settings.archives = entries
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| {
                            path.extension()
                                .is_some_and(|extension| extension == "pmtiles")
                        })
                        .take(16)
                        .collect();
                    settings.archives.sort();
                }
            }
            Ok(settings)
        }
        Err(error) => Err(error.to_string()),
    };
    let mut sources = world.resource_mut::<Sources>();
    sources.path = Some(path);
    sources.cache = base.join("maps");
    match loaded.and_then(|settings| {
        TileStore::new(settings.clone(), &sources.cache).map(|store| (settings, store))
    }) {
        Ok((settings, store)) => {
            sources.settings = settings;
            sources.store = Some(store);
            sources.revision += 1;
        }
        Err(error) => {
            sources.error = Some(error);
            sources.settings.online = false;
            sources.store = TileStore::new(sources.settings.clone(), &sources.cache).ok();
            sources.revision += 1;
        }
    }
    crate::map_view::initialize_fonts();
}

#[derive(Component)]
struct Form {
    local: Entity,
    archives: Entity,
    server: Entity,
    version: Entity,
    storage: Entity,
    zoom: Entity,
    online: bool,
    toggle: Entity,
    status: Entity,
}

#[derive(Clone)]
enum Change {
    Open,
    Toggle,
    Save,
    Preset,
    Cancel,
}

impl crate::native::actions::Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        if matches!(self, Self::Cancel) {
            world.entity_mut(target).remove::<SaveJob>();
            return;
        }
        if matches!(self, Self::Open) {
            let existing = world.get::<Children>(target).and_then(|children| {
                children
                    .iter()
                    .find(|child| world.get::<Form>(*child).is_some())
            });
            if let Some(existing) = existing {
                world.despawn(existing);
                return;
            }
            mount(world, target);
            return;
        }
        let Some(form) = world.get::<Form>(target) else {
            return;
        };
        let (status, server, toggle, online) = (form.status, form.server, form.toggle, form.online);
        match self {
            Self::Toggle => {
                world.get_mut::<Form>(target).unwrap().online = !online;
                let caption = if online {
                    "External downloads: off"
                } else {
                    "External downloads: on"
                };
                if let Some(children) = world
                    .get::<Children>(toggle)
                    .map(|children| children.iter().collect::<Vec<_>>())
                {
                    for child in children {
                        if let Some(mut text) = world.get_mut::<Text>(child) {
                            text.0 = caption.into();
                        }
                    }
                }
            }
            Self::Preset => {
                if let Some(mut input) = world.get_mut::<bevy::text::EditableText>(server) {
                    input.editor.set_text(lince_map::source::PUBLIC_SERVER);
                }
            }
            Self::Save => {
                let result = save_form(world, target);
                crate::native::sand_panel::status(
                    world,
                    status,
                    match result {
                        Ok(()) => "Checking and staging map sources…".into(),
                        Err(error) => error,
                    },
                );
            }
            Self::Open | Self::Cancel => {}
        }
    }
}

pub(crate) fn settings_button(world: &mut World, controls: Entity, parent: Entity) {
    crate::native::sand_panel::button(world, controls, parent, "Map Settings", Change::Open);
}

fn mount(world: &mut World, parent: Entity) {
    let settings = world.resource::<Sources>().settings.clone();
    let body = crate::native::sand_panel::column(world, parent);
    world.entity_mut(body).insert((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            top: px(0),
            left: px(0),
            padding: UiRect::all(px(10)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ZIndex(10),
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    crate::native::scroll_sand::attach(world, body);
    crate::native::sand_panel::button(world, body, parent, "Close Map Settings", Change::Open);
    crate::native::edit_mode::label(world, body, "Map sources", 18.0);
    let local = crate::native::sand_panel::field(
        world,
        body,
        "Primary local tile folder (Shortbread z/x/y.pbf)",
        &settings.local_directory.to_string_lossy(),
    );
    let archives = crate::native::sand_panel::field(
        world,
        body,
        "Local Shortbread PMTiles files, separated by semicolons",
        &settings
            .archives
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join(";"),
    );
    let server = crate::native::sand_panel::field(
        world,
        body,
        "Supporting HTTPS tile server with {z}/{x}/{y}",
        &settings.server,
    );
    let version = crate::native::sand_panel::field(
        world,
        body,
        "External dataset version",
        &settings.dataset_version,
    );
    let zoom = crate::native::sand_panel::field(
        world,
        body,
        "Maximum downloaded detail (0–14)",
        &settings.max_zoom.to_string(),
    );
    let storage = crate::native::sand_panel::field(
        world,
        body,
        "Downloaded map storage limit in MB",
        &settings.storage_megabytes.to_string(),
    );
    let controls = crate::native::sand_panel::row(world, body);
    crate::native::sand_panel::button(world, controls, body, "Cancel import", Change::Cancel);
    let toggle = crate::native::sand_panel::button(
        world,
        controls,
        body,
        if settings.online {
            "External downloads: on"
        } else {
            "External downloads: off"
        },
        Change::Toggle,
    );
    crate::native::sand_panel::button(
        world,
        controls,
        body,
        "Use public VersaTiles source",
        Change::Preset,
    );
    crate::native::sand_panel::button(world, controls, body, "Save", Change::Save);
    let status = crate::native::edit_mode::label(
        world,
        body,
        "Local tiles are used first. The overview always works offline.",
        13.0,
    );
    world.entity_mut(body).insert(Form {
        local,
        archives,
        server,
        version,
        storage,
        zoom,
        online: settings.online,
        toggle,
        status,
    });
}

fn form_settings(world: &World, target: Entity) -> Result<Settings, String> {
    let form = world.get::<Form>(target).ok_or("Map Settings closed")?;
    let value = |entity| {
        crate::native::sand_panel::value(world, entity).map(|value| value.trim().to_string())
    };
    let settings = Settings {
        local_directory: PathBuf::from(value(form.local)?),
        archives: value(form.archives)?
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect(),
        server: value(form.server)?,
        dataset_version: value(form.version)?,
        online: form.online,
        max_zoom: value(form.zoom)?
            .parse()
            .map_err(|_| "Use a detail level between 0 and 14")?,
        storage_megabytes: value(form.storage)?
            .parse()
            .map_err(|_| "Enter a map storage limit in MB")?,
    };
    settings.validate()?;
    Ok(settings)
}

#[derive(Component)]
struct SaveJob {
    original: Settings,
    cancelled: Arc<AtomicBool>,
    progress: Arc<lince_map::pack::InstallProgress>,
    receiver: tokio::sync::oneshot::Receiver<Result<(Settings, Arc<TileStore>), String>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for SaveJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.task.abort();
    }
}

fn save_form(world: &mut World, target: Entity) -> Result<(), String> {
    let settings = form_settings(world, target)?;
    let original = settings.clone();
    let sources = world.resource::<Sources>();
    let cache = sources.cache.clone();
    let managed = cache.join("packs");
    let cancelled = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(lince_map::pack::InstallProgress::default());
    let cancellation = cancelled.clone();
    let bytes = progress.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let runtime =
        tokio::runtime::Handle::try_current().map_err(|_| "Map import runtime is unavailable")?;
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let task = runtime.spawn(async move {
        let result = async {
            let mut settings = settings;
            let quota = settings.storage_megabytes * 1024 * 1024;
            if !settings.local_directory.as_os_str().is_empty() {
                settings.local_directory = lince_map::pack::install(
                    settings.local_directory.clone(),
                    managed.clone(),
                    quota,
                    cancellation.clone(),
                    bytes.clone(),
                )
                .await?;
            }
            for archive in &mut settings.archives {
                let root = archive
                    .parent()
                    .ok_or("Choose an archive inside a pack directory")?
                    .to_path_buf();
                let name = archive
                    .file_name()
                    .ok_or("Invalid archive name")?
                    .to_os_string();
                let installed = lince_map::pack::install(
                    root,
                    managed.clone(),
                    quota,
                    cancellation.clone(),
                    bytes.clone(),
                )
                .await?;
                *archive = installed.join(name);
            }
            let store = TileStore::new(settings.clone(), &cache)?;
            store.validate_sources().await?;
            if cancellation.load(Ordering::Relaxed) {
                return Err("Map import cancelled".into());
            }
            Ok((settings, store))
        }
        .await;
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    world.entity_mut(target).insert(SaveJob {
        original,
        cancelled,
        progress,
        receiver,
        task,
    });
    Ok(())
}

pub(super) fn cancel_pending(world: &mut World) {
    let jobs: Vec<_> = world
        .query_filtered::<Entity, With<SaveJob>>()
        .iter(world)
        .collect();
    for job in jobs {
        world.entity_mut(job).remove::<SaveJob>();
    }
}

pub(super) fn poll(world: &mut World) {
    let jobs: Vec<_> = world
        .query_filtered::<Entity, With<SaveJob>>()
        .iter(world)
        .collect();
    for entity in jobs {
        let status = world.get::<Form>(entity).unwrap().status;
        if form_settings(world, entity).ok().as_ref()
            != Some(&world.get::<SaveJob>(entity).unwrap().original)
        {
            world.entity_mut(entity).remove::<SaveJob>();
            crate::native::sand_panel::status(
                world,
                status,
                "Map inputs changed; review and save them again",
            );
            continue;
        }
        let result = world
            .get_mut::<SaveJob>(entity)
            .unwrap()
            .receiver
            .try_recv();
        match result {
            Ok(result) => {
                world.entity_mut(entity).remove::<SaveJob>();
                let result = result.and_then(|(settings, store)| commit(world, settings, store));
                crate::native::sand_panel::status(
                    world,
                    status,
                    result.map_or_else(|error| error, |()| "Map sources verified and saved".into()),
                );
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                let job = world.get::<SaveJob>(entity).unwrap();
                crate::native::sand_panel::status(
                    world,
                    status,
                    format!(
                        "Checking map sources · {} / {} bytes",
                        job.progress.copied.load(Ordering::Relaxed),
                        job.progress.total.load(Ordering::Relaxed)
                    ),
                );
                if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                    wake.after(std::time::Duration::from_millis(100));
                }
            }
            Err(_) => {
                world.entity_mut(entity).remove::<SaveJob>();
                crate::native::sand_panel::status(
                    world,
                    status,
                    "Map import ended; previous sources were kept",
                );
            }
        }
    }
}

fn commit(world: &mut World, settings: Settings, store: Arc<TileStore>) -> Result<(), String> {
    let sources = world.resource::<Sources>();
    let path = sources
        .path
        .as_ref()
        .ok_or("Map storage is not ready")?
        .clone();
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
    fs::create_dir_all(path.parent().ok_or("Invalid Settings path")?)
        .map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", nucleus::new_uid("map")));
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    fs::File::open(path.parent().ok_or("Invalid Settings directory")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    let mut sources = world.resource_mut::<Sources>();
    sources.settings = settings;
    sources.store = Some(store);
    sources.error = None;
    sources.revision += 1;
    Ok(())
}

pub(super) mod tests {
    use super::*;
    use crate::native::actions::Action;

    #[cfg_attr(test, tokio::test)]
    async fn settings_reject_invalid_edits_and_replace_the_source_after_a_saved_change() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = crate::native::sand_panel::tests::app();
        app.insert_resource(Sources {
            path: Some(directory.path().join("maps/settings.json")),
            cache: directory.path().join("cache"),
            ..Default::default()
        });
        let parent = app.world_mut().spawn(Node::default()).id();
        Change::Open.apply(app.world_mut(), parent);
        let body = app
            .world()
            .get::<Children>(parent)
            .unwrap()
            .iter()
            .find(|entity| app.world().get::<Form>(*entity).is_some())
            .unwrap();
        let server = app.world().get::<Form>(body).unwrap().server;
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(server)
            .unwrap()
            .editor
            .set_text("http://example.com/{z}/{x}/{y}");
        assert!(save_form(app.world_mut(), body).is_err());
        assert_eq!(app.world().resource::<Sources>().revision, 0);
        assert!(!directory.path().join("maps/settings.json").exists());
        Change::Preset.apply(app.world_mut(), body);
        Change::Toggle.apply(app.world_mut(), body);
        assert!(save_form(app.world_mut(), body).is_err());
        Change::Toggle.apply(app.world_mut(), body);
        save_form(app.world_mut(), body).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while app.world().get::<SaveJob>(body).is_some() {
                poll(app.world_mut());
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let settings: Settings = serde_json::from_slice(
            &std::fs::read(directory.path().join("maps/settings.json")).unwrap(),
        )
        .unwrap();
        assert!(!settings.online);
        assert_eq!(settings.server, lince_map::source::PUBLIC_SERVER);
        assert_eq!(app.world().resource::<Sources>().revision, 1);
        assert_eq!(
            app.world()
                .resource::<Sources>()
                .store
                .as_ref()
                .unwrap()
                .settings(),
            &settings
        );
        Change::Open.apply(app.world_mut(), parent);
        assert!(app.world().get_entity(body).is_err());
    }

    crate::native::laboratory_cases! {
        async settings_reject_invalid_edits_and_replace_the_source_after_a_saved_change,
    }
}
