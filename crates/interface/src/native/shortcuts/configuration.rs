use super::model::{ModKey, Scope, Settings, Shortcut};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: usize = 256 * 1024;
const TARGET: &str = "interface-config";
const NAMESPACE: &str = "lince.interface";

pub(crate) mod reload;
pub(crate) mod review;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ApplicationOpening {
    #[default]
    Normal,
    Maximized,
}

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LayoutSettings {
    pub width_presets: Vec<[u16; 2]>,
    pub height_presets: Vec<[u16; 2]>,
    pub default_width: [u16; 2],
    pub move_step: u32,
    pub resize_step: u32,
    pub gaps: u16,
    pub application_opening: ApplicationOpening,
    pub workspace_back_and_forth: bool,
    pub focus: crate::host::input::FocusPolicy,
    pub opening_rules: Vec<String>,
    pub gestures: crate::host::input::GestureSettings,
    pub overview: crate::layout::desktop::OverviewSettings,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            width_presets: vec![[1, 1], [1, 2], [1, 3], [2, 3]],
            height_presets: vec![[1, 1], [1, 2], [1, 3], [2, 3]],
            default_width: [1, 2],
            move_step: 64,
            resize_step: 64,
            gaps: 8,
            application_opening: ApplicationOpening::Normal,
            workspace_back_and_forth: false,
            focus: Default::default(),
            opening_rules: Vec::new(),
            gestures: Default::default(),
            overview: Default::default(),
        }
    }
}

impl LayoutSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.gestures.validate()?;
        self.overview.validate()?;
        if self.opening_rules.len() > nucleus::window_opening::MAX_RULES
            || self.opening_rules.iter().any(|uid| !nucleus::valid_uid(uid, "r"))
            || self.opening_rules.iter().collect::<BTreeSet<_>>().len() != self.opening_rules.len()
        {
            return Err("Choose up to 256 distinct opening-rule Records.".into());
        }
        if self.gaps > 256 {
            return Err("Use a whole gap from 0 to 256.".into());
        }
        let valid = |ratio: &[u16; 2]| ratio[0] > 0 && ratio[0] <= ratio[1];
        if !valid(&self.default_width) {
            return Err("Choose a positive default width fraction no greater than one.".into());
        }
        if !(1..=10_000).contains(&self.resize_step) {
            return Err("Resize step must be between 1 and 10000 logical pixels.".into());
        }
        if !(1..=10_000).contains(&self.move_step) {
            return Err("Movement step must be between 1 and 10000 canvas units.".into());
        }
        for (name, presets) in [
            ("Width", &self.width_presets),
            ("Row height", &self.height_presets),
        ] {
            if presets.is_empty() || presets.len() > 32 || presets.iter().any(|ratio| !valid(ratio))
            {
                return Err(format!(
                    "Choose 1–32 positive {} fractions no greater than one.",
                    name.to_lowercase()
                ));
            }
            for (index, first) in presets.iter().enumerate() {
                if presets[index + 1..].iter().any(|second| {
                    u32::from(first[0]) * u32::from(second[1])
                        == u32::from(second[0]) * u32::from(first[1])
                }) {
                    return Err(format!("{name} presets must not repeat the same fraction."));
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    command: String,
    chord: Option<String>,
    scope: Scope,
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Document {
    bindings: Vec<Binding>,
    mod_key: ModKey,
    width_presets: Vec<[u16; 2]>,
    height_presets: Vec<[u16; 2]>,
    default_width: [u16; 2],
    move_step: u32,
    resize_step: u32,
    gaps: u16,
    application_opening: ApplicationOpening,
    workspace_back_and_forth: bool,
    focus: crate::host::input::FocusPolicy,
    opening_rules: Vec<String>,
    gestures: crate::host::input::GestureSettings,
    overview: crate::layout::desktop::OverviewSettings,
}

impl Default for Document {
    fn default() -> Self {
        let layout = LayoutSettings::default();
        Self {
            bindings: Vec::new(),
            mod_key: ModKey::default(),
            width_presets: layout.width_presets,
            height_presets: layout.height_presets,
            default_width: layout.default_width,
            move_step: layout.move_step,
            resize_step: layout.resize_step,
            gaps: layout.gaps,
            application_opening: layout.application_opening,
            workspace_back_and_forth: layout.workspace_back_and_forth,
            focus: layout.focus,
            opening_rules: layout.opening_rules,
            gestures: layout.gestures,
            overview: layout.overview,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct Storage {
    path: Option<PathBuf>,
    source: Option<Vec<u8>>,
    pub error: Option<String>,
    revision: u64,
}

fn decode(bytes: &[u8]) -> Result<(Settings, LayoutSettings), String> {
    if bytes.len() > LIMIT {
        return Err("Interface configuration exceeds 256 KiB.".into());
    }
    let source = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let ast = anicca::parse(source).map_err(|error| error.to_string())?;
    if ast.declarations.iter().any(|declaration| {
        !matches!(
            declaration,
            anicca::grammar::grammar::Declaration::Record(_)
                | anicca::grammar::grammar::Declaration::Extension(_)
        )
    }) {
        return Err(
            "Interface configuration accepts Records and Extensions only; scripts are never run."
                .into(),
        );
    }
    let records: Vec<_> = ast
        .declarations
        .iter()
        .filter_map(|declaration| {
            if let anicca::grammar::grammar::Declaration::Record(record) = declaration {
                Some(record)
            } else {
                None
            }
        })
        .collect();
    let extensions: Vec<_> = ast
        .declarations
        .iter()
        .filter_map(|declaration| {
            if let anicca::grammar::grammar::Declaration::Extension(extension) = declaration {
                Some(extension)
            } else {
                None
            }
        })
        .collect();
    if records.len() != 1
        || records[0].header.subject.slug() != Some(TARGET)
        || extensions.len() != 1
    {
        return Err("Use one @interface-config Record and its lince.interface Extension.".into());
    }
    let extension = anicca::extension::project(extensions[0]).map_err(|error| error.to_string())?;
    if extension.target_slug != TARGET || extension.namespace != NAMESPACE {
        return Err("The interface configuration Extension must target @interface-config and lince.interface.".into());
    }
    let document: Document =
        serde_json::from_value(extension.fields.clone()).map_err(|error| error.to_string())?;
    if document.bindings.len() > Shortcut::ALL.len() {
        return Err("Interface configuration has too many command bindings.".into());
    }
    let mut seen = BTreeSet::new();
    let mut overrides = Vec::new();
    for binding in document.bindings {
        let command = Shortcut::from_id(&binding.command)
            .ok_or_else(|| format!("Unknown command: {}", binding.command))?;
        if command.scope() != binding.scope {
            return Err(format!("{} has the wrong input scope.", binding.command));
        }
        if !seen.insert(command) {
            return Err(format!("{} is configured more than once.", binding.command));
        }
        overrides.push((command, binding.chord));
    }
    let settings = Settings::from_overrides(document.mod_key, overrides)?;
    let layout = LayoutSettings {
        width_presets: document.width_presets,
        height_presets: document.height_presets,
        default_width: document.default_width,
        move_step: document.move_step,
        resize_step: document.resize_step,
        gaps: document.gaps,
        application_opening: document.application_opening,
        workspace_back_and_forth: document.workspace_back_and_forth,
        focus: document.focus,
        opening_rules: document.opening_rules,
        gestures: document.gestures,
        overview: document.overview,
    };
    layout.validate()?;
    Ok((settings, layout))
}

fn encode(settings: &Settings, layout: &LayoutSettings) -> Result<Vec<u8>, String> {
    settings.validate()?;
    layout.validate()?;
    let document = Document {
        bindings: settings
            .overrides()
            .map(|(command, chord)| Binding {
                command: command.id().into(),
                chord: chord.map(str::to_string),
                scope: command.scope(),
            })
            .collect(),
        mod_key: settings.modifier(),
        width_presets: layout.width_presets.clone(),
        height_presets: layout.height_presets.clone(),
        default_width: layout.default_width,
        move_step: layout.move_step,
        resize_step: layout.resize_step,
        gaps: layout.gaps,
        application_opening: layout.application_opening,
        workspace_back_and_forth: layout.workspace_back_and_forth,
        focus: layout.focus,
        opening_rules: layout.opening_rules.clone(),
        gestures: layout.gestures,
        overview: layout.overview,
    };
    let extension = anicca::extension::render(
        TARGET,
        NAMESPACE,
        &serde_json::to_value(document).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let source = format!("Interface configuration (@interface-config: 0) {{\n}}\n{extension}");
    let canonical = anicca::canonicalize(&source)
        .map_err(|error| error.to_string())?
        .into_bytes();
    decode(&canonical)?;
    Ok(canonical)
}

fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Interface configuration exceeds 256 KiB.".into());
    }
    Ok(Some(bytes))
}

pub(crate) fn load(world: &mut World) {
    if world.contains_resource::<Storage>() {
        return;
    }
    let path = world
        .get_resource::<crate::native::workspace::WorkspaceFile>()
        .map(|file| file.directory().join("interface.lingua"));
    let mut storage = Storage {
        path,
        revision: 1,
        ..Default::default()
    };
    let mut settings = Settings::default();
    let mut layout = LayoutSettings::default();
    if let Some(path) = &storage.path {
        match read(path) {
            Ok(source) => {
                storage.source = source;
                if let Some(bytes) = &storage.source {
                    match decode(bytes) {
                        Ok(loaded) => (settings, layout) = loaded,
                        Err(error) => storage.error = Some(error),
                    }
                }
            }
            Err(error) => storage.error = Some(error),
        }
    }
    if let Some(error) = &storage.error {
        crate::native::notifications::report(
            world,
            "Interface configuration",
            &format!("Using default shortcuts: {error}"),
        );
    }
    world.insert_resource(settings);
    world.insert_resource(layout);
    world.insert_resource(storage);
}

pub(crate) fn commit(
    world: &mut World,
    settings: Settings,
    layout: LayoutSettings,
) -> Result<String, String> {
    let bytes = encode(&settings, &layout)?;
    let path = world
        .get_resource::<Storage>()
        .and_then(|storage| storage.path.clone());
    let mut message = "Applied for this session.".to_string();
    if let Some(path) = path {
        let source = read(&path)?;
        if source.as_ref() != world.resource::<Storage>().source.as_ref() {
            return Err(
                "Interface configuration changed on disk. Reload it before saving this draft."
                    .into(),
            );
        }
        let parent = path
            .parent()
            .ok_or("The configuration directory is unavailable.")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".lince-interface-")
            .tempfile_in(parent)
            .map_err(|error| error.to_string())?;
        temporary
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temporary
            .persist(&path)
            .map_err(|error| error.error.to_string())?;
        message = "Saved.".into();
        #[cfg(unix)]
        if let Err(error) = std::fs::File::open(parent).and_then(|directory| directory.sync_all()) {
            message = format!("Saved, but directory durability could not be confirmed: {error}");
        }
        let mut storage = world.resource_mut::<Storage>();
        if storage.source.as_ref() != Some(&bytes) { storage.revision = storage.revision.saturating_add(1); }
        storage.source = Some(bytes);
        storage.error = None;
    }
    publish(world, settings, layout);
    Ok(message)
}

fn publish(world: &mut World, settings: Settings, layout: LayoutSettings) {
    let changed = world.get_resource::<Settings>() != Some(&settings) || world.get_resource::<LayoutSettings>() != Some(&layout);
    if world.get_resource::<Settings>() != Some(&settings) { world.insert_resource(settings); super::synchronize(world); }
    if world.get_resource::<LayoutSettings>() != Some(&layout) { world.insert_resource(layout); }
    if changed && let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_mouse_overrides_round_trip_without_becoming_keyboard_bindings() {
        let mut settings = Settings::default();
        settings
            .stage(Shortcut::PointerMove, Some("Alt+MouseMiddle"))
            .unwrap();
        settings.stage(Shortcut::PointerResize, None).unwrap();
        let source = encode(&settings, &LayoutSettings::default()).unwrap();
        let (restored, layout) = decode(&source).unwrap();
        assert_eq!(restored.text(Shortcut::PointerMove), "Alt+MouseMiddle");
        assert!(restored.chord(Shortcut::PointerMove).is_none());
        assert_eq!(
            restored.pointer(Shortcut::PointerMove).unwrap().button,
            MouseButton::Middle
        );
        assert!(restored.pointer(Shortcut::PointerResize).is_none());
        assert_eq!(layout, LayoutSettings::default());
    }

    #[test]
    fn accepted_session_defaults_are_distinct_and_explicit_overrides_survive_save_and_startup() {
        let defaults = Settings::default();
        defaults.validate().unwrap();
        for (command, chord) in [
            (Shortcut::Terminal, "Mod+N"),
            (Shortcut::Maximize, "Mod+F"),
            (Shortcut::Fullscreen, "Mod+Shift+F"),
            (Shortcut::Logout, "Mod+Shift+E"),
        ] {
            assert_eq!(defaults.text(command), chord);
            assert!(!command.repeats());
        }
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = interface(directory.path());
        let mut configured = defaults;
        configured
            .apply([
                (Shortcut::Terminal, Some("Mod+Alt+N".into())),
                (Shortcut::Maximize, Some("Mod+Alt+F".into())),
                (Shortcut::Fullscreen, Some("Mod+Alt+Shift+F".into())),
                (Shortcut::Logout, Some("Mod+Alt+Shift+E".into())),
            ])
            .unwrap();
        commit(
            app.world_mut(),
            configured.clone(),
            LayoutSettings::default(),
        )
        .unwrap();
        let bytes = std::fs::read(directory.path().join("interface.lingua")).unwrap();
        assert_eq!(decode(&bytes).unwrap().0, configured);
        let (restarted, _) = interface(directory.path());
        assert_eq!(restarted.world().resource::<Settings>(), &configured);
        assert_eq!(
            restarted.world().resource::<Settings>().overrides().count(),
            4
        );
        assert_eq!(
            std::fs::read(directory.path().join("interface.lingua")).unwrap(),
            bytes
        );
    }

    #[test]
    fn layout_preferences_validate_round_trip_and_keep_binding_overrides() {
        let mut settings = Settings::default();
        settings.set(Shortcut::Terminal, "Mod+Alt+N").unwrap();
        let mut layout = LayoutSettings {
            gaps: 0,
            workspace_back_and_forth: true,
            focus: crate::host::input::FocusPolicy { click: false, pointer_follow: true, warp: true },
            height_presets: vec![[1, 4], [1, 2], [1, 1]],
            resize_step: 37,
            application_opening: ApplicationOpening::Maximized,
            ..Default::default()
        };
        let bytes = encode(&settings, &layout).unwrap();
        assert_eq!(decode(&bytes).unwrap(), (settings, layout.clone()));
        layout.gaps = 257;
        assert!(layout.validate().is_err());
        assert!(encode(&Settings::default(), &layout).is_err());
        assert!(
            serde_json::from_value::<Document>(
                serde_json::json!({ "application_opening": "fullscreen" })
            )
            .is_err()
        );
        assert_eq!(LayoutSettings::default().gaps, 8);
        for step in [0, 10_001, u32::MAX] {
            assert!(
                LayoutSettings {
                    resize_step: step,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for presets in [
            vec![],
            vec![[0, 1]],
            vec![[1, 0]],
            vec![[2, 1]],
            vec![[1, 2], [2, 4]],
        ] {
            assert!(
                LayoutSettings {
                    height_presets: presets,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert_eq!(
            LayoutSettings::default().application_opening,
            ApplicationOpening::Normal
        );
    }

    #[test]
    fn gesture_preferences_round_trip_in_lingua_without_changing_bindings() {
        let mut settings = Settings::default();
        settings.set(Shortcut::Terminal, "Mod+Alt+N").unwrap();
        let mut layout = LayoutSettings::default();
        layout.gestures = crate::host::input::GestureSettings {
            enabled: false,
            sensitivity_percent: 175,
            commit_threshold: 112,
        };
        let bytes = encode(&settings, &layout).unwrap();
        assert_eq!(decode(&bytes).unwrap(), (settings.clone(), layout.clone()));
        layout.gestures.commit_threshold = 0;
        assert!(encode(&settings, &layout).is_err());
        assert!(serde_json::from_value::<Document>(serde_json::json!({
            "gestures": { "enabled": true, "unknown": 1 }
        })).is_err());
    }

    #[test]
    fn layout_preferences_persist_and_reload_without_changing_widths_or_bindings() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = interface(directory.path());
        let mut settings = app.world().resource::<Settings>().clone();
        settings.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
        let mut layout = app.world().resource::<LayoutSettings>().clone();
        for (command, chord, path) in [
            (Shortcut::WorkspacePrevious, "Mod+F5", "/workspace/previous"),
            (Shortcut::WorkspaceNext, "Mod+F6", "/workspace/next"),
            (
                Shortcut::WorkspaceEarlier,
                "Mod+F7",
                "/workspace/move-earlier",
            ),
            (Shortcut::WorkspaceLater, "Mod+F8", "/workspace/move-later"),
        ] {
            assert!(settings.chord(command).is_none());
            assert_eq!(command.scope(), Scope::Desktop);
            assert!(!command.repeats());
            assert_eq!(Shortcut::from_operation(path), Some(command));
            settings.set(command, chord).unwrap();
        }
        layout.width_presets = vec![[2, 3], [1, 3], [1, 1]];
        for (command, chord, path, repeats) in [
            (Shortcut::ShrinkWidth, "Mod+F9", "/item/width/shrink", true),
            (Shortcut::GrowWidth, "Mod+F10", "/item/width/grow", true),
            (
                Shortcut::ShrinkHeight,
                "Mod+F11",
                "/item/height/shrink",
                true,
            ),
            (Shortcut::GrowHeight, "Mod+F12", "/item/height/grow", true),
            (
                Shortcut::CycleRowHeight,
                "Mod+Ctrl+F12",
                "/row/height",
                false,
            ),
        ] {
            assert!(settings.chord(command).is_none());
            assert_eq!(command.scope(), Scope::Desktop);
            assert_eq!(command.repeats(), repeats);
            assert_eq!(Shortcut::from_operation(path), Some(command));
            settings.set(command, chord).unwrap();
        }
        layout.height_presets = vec![[1, 4], [1, 2]];
        layout.resize_step = 37;
        layout.gaps = 0;
        layout.application_opening = ApplicationOpening::Maximized;
        commit(app.world_mut(), settings.clone(), layout.clone()).unwrap();
        let bytes = std::fs::read(directory.path().join("interface.lingua")).unwrap();
        assert_eq!(decode(&bytes).unwrap(), (settings.clone(), layout.clone()));
        let (restarted, _) = interface(directory.path());
        assert_eq!(restarted.world().resource::<LayoutSettings>(), &layout);
        assert_eq!(restarted.world().resource::<Settings>(), &settings);
    }

    fn interface(path: &Path) -> (App, Entity) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .insert_resource(crate::native::workspace::WorkspaceFile::new(
                path.join("interface.json"),
            ))
            .add_plugins((
                crate::native::workspace::WorkspacePlugin,
                crate::native::edit_mode::EditModePlugin,
            ));
        let root = app
            .world_mut()
            .spawn(crate::native::container::BoxRoot)
            .id();
        app.update();
        (app, root)
    }

    #[test]
    fn windowed_fullscreen_starts_unbound_and_uses_the_existing_atomic_configuration_and_startup() {
        assert_eq!(Settings::default().text(Shortcut::WindowedFullscreen), "Unbound");
        assert_eq!(Shortcut::WindowedFullscreen.scope(), Scope::Park);
        assert!(!Shortcut::WindowedFullscreen.repeats());
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = interface(directory.path());
        let mut settings = app.world().resource::<Settings>().clone();
        settings.set(Shortcut::WindowedFullscreen, "Mod+Alt+F").unwrap();
        commit(app.world_mut(), settings.clone(), LayoutSettings::default()).unwrap();
        let bytes = std::fs::read(directory.path().join("interface.lingua")).unwrap();
        assert_eq!(decode(&bytes).unwrap().0, settings);
        let (restarted, _) = interface(directory.path());
        assert_eq!(restarted.world().resource::<Settings>().text(Shortcut::WindowedFullscreen), "Mod+Alt+F");
        settings.set(Shortcut::WindowedFullscreen, "Unbound").unwrap();
        commit(app.world_mut(), settings.clone(), LayoutSettings::default()).unwrap();
        let (restarted, _) = interface(directory.path());
        assert_eq!(restarted.world().resource::<Settings>(), &settings);
    }

    #[test]
    fn recent_window_bindings_commit_together_and_load_unchanged_at_startup() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = interface(directory.path());
        let mut settings = app.world().resource::<Settings>().clone();
        settings
            .stage(Shortcut::RecentNext, Some("Ctrl+Tab"))
            .unwrap();
        assert!(commit(app.world_mut(), settings.clone(), LayoutSettings::default()).is_err());
        settings
            .stage(Shortcut::RecentPrevious, Some("Ctrl+Shift+Tab"))
            .unwrap();
        commit(app.world_mut(), settings.clone(), LayoutSettings::default()).unwrap();
        let path = directory.path().join("interface.lingua");
        let saved = std::fs::read(&path).unwrap();
        let (restarted, _) = interface(directory.path());
        assert_eq!(restarted.world().resource::<Settings>(), &settings);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }

    #[test]
    fn startup_loads_before_bindings_and_workspace_recovery_never_overwrites_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.lingua");
        let mut settings = Settings::default();
        settings.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
        let bytes = encode(&settings, &LayoutSettings::default()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        std::fs::write(
            directory.path().join("interface.json"),
            b"invalid workspace snapshot",
        )
        .unwrap();
        let (mut app, root) = interface(directory.path());
        assert_eq!(app.world().resource::<Settings>(), &settings);
        assert!(
            app.world()
                .get::<super::super::ConfiguredBindings>(root)
                .unwrap()
                .0
                .iter()
                .any(|binding| binding.key == KeyCode::F2)
        );
        crate::native::workspace::create(app.world_mut(), root);
        app.update();
        assert_eq!(app.world().resource::<Settings>(), &settings);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn malformed_startup_preserves_the_file_and_keeps_the_editor_accessible() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.lingua");
        std::fs::write(&path, b"invalid interface configuration").unwrap();
        let (mut app, root) = interface(directory.path());
        assert_eq!(app.world().resource::<Settings>(), &Settings::default());
        assert!(app.world().resource::<Storage>().error.is_some());
        use crate::native::actions::Action;
        crate::native::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        crate::native::edit_mode::EditAction::Park.apply(app.world_mut(), root);
        assert!(
            app.world()
                .get::<crate::native::edit_mode::EditMode>(root)
                .unwrap()
                .enabled
        );
        assert_eq!(
            std::fs::read(path).unwrap(),
            b"invalid interface configuration"
        );
    }

    #[test]
    fn registry_ids_are_unique_and_batch_swaps_commit_without_partial_updates() {
        let ids: BTreeSet<_> = Shortcut::ALL.into_iter().map(Shortcut::id).collect();
        assert_eq!(ids.len(), Shortcut::ALL.len());
        let mut settings = Settings::default();
        settings
            .apply([
                (Shortcut::FocusLeft, Some("Mod+L".into())),
                (Shortcut::FocusRight, Some("Mod+H".into())),
            ])
            .unwrap();
        assert_eq!(
            settings.chord(Shortcut::FocusLeft).unwrap().key,
            KeyCode::KeyL
        );
        let swapped = settings.clone();
        assert!(
            settings
                .apply([
                    (Shortcut::FocusLeft, Some("Mod+F2".into())),
                    (Shortcut::FocusRight, Some("invalid".into()))
                ])
                .is_err()
        );
        assert_eq!(settings, swapped);
        let alternate = Settings::from_overrides(ModKey::Alt, []).unwrap();
        assert!(alternate.chord(Shortcut::FocusLeft).unwrap().modifiers.alt);
        assert!(
            !alternate
                .chord(Shortcut::FocusLeft)
                .unwrap()
                .modifiers
                .super_key
        );
    }

    #[test]
    fn symbolic_mod_preserves_explicit_modifiers_when_the_selected_modifier_changes() {
        let explicit = Settings::from_overrides(
            ModKey::Alt,
            [(Shortcut::FocusLeft, Some("Mod+Alt+F2".into()))],
        )
        .unwrap();
        assert_eq!(explicit.text(Shortcut::FocusLeft), "Mod+Alt+F2");
        let changed = Settings::from_overrides(
            ModKey::Super,
            explicit
                .overrides()
                .map(|(command, chord)| (command, chord.map(str::to_string))),
        )
        .unwrap();
        let modifiers = changed.chord(Shortcut::FocusLeft).unwrap().modifiers;
        assert!(modifiers.alt && modifiers.super_key);
        assert!(
            Settings::from_overrides(
                ModKey::Alt,
                [(Shortcut::FocusLeft, Some("Mod+Mod+F2".into()))]
            )
            .is_err()
        );
    }

    #[test]
    fn lingua_round_trip_preserves_bindings_modifiers_and_fractional_sizes() {
        let mut settings = Settings::default();
        settings.set(Shortcut::FocusLeft, "Mod+Shift+F2").unwrap();
        settings.set(Shortcut::Forward, "Unbound").unwrap();
        let layout = LayoutSettings::default();
        let bytes = encode(&settings, &layout).unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("Extension @interface-config")
        );
        assert_eq!(decode(&bytes).unwrap(), (settings.clone(), layout.clone()));
        assert_eq!(encode(&settings, &layout).unwrap(), bytes);
    }

    #[test]
    fn configuration_rejects_scripts_unknown_commands_scopes_and_ambiguous_chords() {
        let source =
            String::from_utf8(encode(&Settings::default(), &LayoutSettings::default()).unwrap())
                .unwrap();
        for invalid in [
            format!("{source}\nlet unexpected = 1"),
            source.replace("bindings []", "bindings [{ command \"missing\" chord \"Mod+F2\" scope \"desktop\" }]"),
            source.replace("bindings []", "bindings [{ command \"focus.left\" chord \"Mod+F2\" scope \"park\" }]"),
            source.replace("bindings []", "bindings [{ command \"focus.left\" chord \"Mod+L\" scope \"desktop\" }]"),
            source.replace("bindings []", "bindings [{ command \"focus.left\" chord null scope \"desktop\" } { command \"focus.left\" chord null scope \"desktop\" }]"),
            source.replace("default_width [1 2]", "default_width [1 0]"),
            source.replace("default_width [1 2]", "unknown true"),
        ] {
            assert_ne!(invalid, source);
            assert!(decode(invalid.as_bytes()).is_err(), "{invalid}");
        }
        assert!(decode(&vec![b' '; LIMIT + 1]).is_err());
    }

    #[test]
    fn fractions_reject_equivalent_duplicates_and_invalid_denominators() {
        for move_step in [0, 10001, u32::MAX] {
            assert!(
                LayoutSettings {
                    move_step,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for presets in [
            vec![],
            vec![[1, 0]],
            vec![[2, 1]],
            vec![[0, 1]],
            vec![[1, 2], [2, 4]],
        ] {
            assert!(
                LayoutSettings {
                    width_presets: presets,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn save_is_atomic_private_and_rejects_external_edits_without_changing_active_bindings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.lingua");
        let mut world = World::new();
        world.insert_resource(Storage {
            path: Some(path.clone()),
            ..Default::default()
        });
        world.insert_resource(Settings::default());
        let mut settings = Settings::default();
        settings.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
        commit(&mut world, settings.clone(), LayoutSettings::default()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(decode(&bytes).unwrap().0, settings);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::write(&path, b"An external edit").unwrap();
        assert!(commit(&mut world, Settings::default(), LayoutSettings::default()).is_err());
        assert_eq!(world.resource::<Settings>(), &settings);
        assert_eq!(std::fs::read(&path).unwrap(), b"An external edit");
    }

    #[test]
    fn failed_save_preserves_the_active_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("not-a-directory");
        std::fs::write(&path, b"keep").unwrap();
        let mut world = World::new();
        world.insert_resource(Storage {
            path: Some(path.join("interface.lingua")),
            ..Default::default()
        });
        world.insert_resource(Settings::default());
        let mut next = Settings::default();
        next.set(Shortcut::FocusLeft, "Mod+F2").unwrap();
        assert!(commit(&mut world, next, LayoutSettings::default()).is_err());
        assert_eq!(world.resource::<Settings>(), &Settings::default());
        assert_eq!(std::fs::read(path).unwrap(), b"keep");
    }

    #[test]
    fn external_bindings_use_stable_command_ids_and_preserve_the_lingua_configuration() {
        use crate::host::external::Action;
        let mut settings = Settings::default();
        settings
            .set(
                Shortcut::External(Action::VolumeUp),
                "Ctrl+XF86AudioRaiseVolume",
            )
            .unwrap();
        settings
            .set(Shortcut::External(Action::Editor), "Mod+Z")
            .unwrap();
        let bytes = encode(&settings, &LayoutSettings::default()).unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("hardware.volume-up")
        );
        assert_eq!(decode(&bytes).unwrap().0, settings);
        let json = serde_json::to_vec(&settings).unwrap();
        assert_eq!(serde_json::from_slice::<Settings>(&json).unwrap(), settings);
    }
}
