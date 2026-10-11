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
        .get_resource::<crate::workspace::WorkspaceFile>()
        .map(|file| file.directory().to_path_buf())
        .or_else(|| {
            world
                .get_resource::<crate::topology::assets::AssetDirectory>()
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
    sources.cache = base.join("maps/tiles");
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
    let service = lince_map::galileo::render::text::text_service::TextService::initialize(
        lince_map::galileo::render::text::RustybuzzRasterizer::default(),
    );
    service.load_font(Arc::new(
        include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
    ));
    service.load_font(Arc::new(
        include_bytes!("../../../../institute/assets/fonts/DejaVuSans/DejaVuSans.ttf").to_vec(),
    ));
}

#[derive(Component)]
struct Form {
    local: Entity,
    archives: Entity,
    server: Entity,
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
}

impl crate::actions::Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
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
                crate::sand_panel::status(
                    world,
                    status,
                    match result {
                        Ok(()) => "Map Settings saved".into(),
                        Err(error) => error,
                    },
                );
            }
            Self::Open => {}
        }
    }
}

pub(crate) fn settings_button(world: &mut World, controls: Entity, parent: Entity) {
    crate::sand_panel::button(world, controls, parent, "Map Settings", Change::Open);
}

fn mount(world: &mut World, parent: Entity) {
    let settings = world.resource::<Sources>().settings.clone();
    let body = crate::sand_panel::column(world, parent);
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
        crate::token_style::background(crate::tokens::Token::Surface),
    ));
    crate::scroll_sand::attach(world, body);
    crate::sand_panel::button(world, body, parent, "Close Map Settings", Change::Open);
    crate::edit_mode::label(world, body, "Map sources", 18.0);
    let local = crate::sand_panel::field(
        world,
        body,
        "Primary local tile folder (Shortbread z/x/y.pbf)",
        &settings.local_directory.to_string_lossy(),
    );
    let archives = crate::sand_panel::field(
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
    let server = crate::sand_panel::field(
        world,
        body,
        "Supporting HTTPS tile server with {z}/{x}/{y}",
        &settings.server,
    );
    let zoom = crate::sand_panel::field(
        world,
        body,
        "Maximum downloaded detail (0–14)",
        &settings.max_zoom.to_string(),
    );
    let storage = crate::sand_panel::field(
        world,
        body,
        "Downloaded map storage limit in MB",
        &settings.storage_megabytes.to_string(),
    );
    let controls = crate::sand_panel::row(world, body);
    let toggle = crate::sand_panel::button(
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
    crate::sand_panel::button(
        world,
        controls,
        body,
        "Use public VersaTiles source",
        Change::Preset,
    );
    crate::sand_panel::button(world, controls, body, "Save", Change::Save);
    let status = crate::edit_mode::label(
        world,
        body,
        "Local tiles are used first. The overview always works offline.",
        13.0,
    );
    world.entity_mut(body).insert(Form {
        local,
        archives,
        server,
        storage,
        zoom,
        online: settings.online,
        toggle,
        status,
    });
}

fn save_form(world: &mut World, target: Entity) -> Result<(), String> {
    let form = world.get::<Form>(target).ok_or("Map Settings closed")?;
    let value =
        |entity| crate::sand_panel::value(world, entity).map(|value| value.trim().to_string());
    let settings = Settings {
        local_directory: PathBuf::from(value(form.local)?),
        archives: value(form.archives)?
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect(),
        server: value(form.server)?,
        online: form.online,
        max_zoom: value(form.zoom)?
            .parse()
            .map_err(|_| "Use a detail level between 0 and 14")?,
        storage_megabytes: value(form.storage)?
            .parse()
            .map_err(|_| "Enter a map storage limit in MB")?,
    };
    settings.validate()?;
    let sources = world.resource::<Sources>();
    let path = sources
        .path
        .as_ref()
        .ok_or("Map storage is not ready")?
        .clone();
    let store = TileStore::new(settings.clone(), &sources.cache)?;
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
    fs::create_dir_all(path.parent().ok_or("Invalid Settings path")?)
        .map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", nucleus::new_uid("map")));
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())?;
    let mut sources = world.resource_mut::<Sources>();
    sources.settings = settings;
    sources.store = Some(store);
    sources.error = None;
    sources.revision += 1;
    Ok(())
}

pub(super) mod tests {
    use super::*;
    use crate::actions::Action;

    #[cfg_attr(test, test)]
    fn settings_reject_invalid_edits_and_replace_the_source_after_a_saved_change() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = crate::sand_panel::tests::app();
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
        save_form(app.world_mut(), body).unwrap();
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

    crate::laboratory_cases! {
        settings_reject_invalid_edits_and_replace_the_source_after_a_saved_change,
    }
}
