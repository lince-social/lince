use super::{CustomizationAction, ThemeSettings, control, label, row};
use crate::tokens::document::ThemeDocument;
use bevy::prelude::*;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tokio::sync::oneshot;

type ExportResult = Result<Option<PathBuf>, String>;

#[derive(Component, Default)]
struct ExportState {
    status: String,
    pending: Option<oneshot::Receiver<ExportResult>>,
}

#[derive(Component)]
struct ExportStatus(Entity);

pub(super) fn controls(world: &mut World, root: Entity, panel: Entity) {
    let buttons = row(world, panel);
    control(
        world,
        root,
        buttons,
        CustomizationAction::ExportTheme,
        "Export theme…",
    );
    control(
        world,
        root,
        buttons,
        CustomizationAction::CopyTheme,
        "Copy theme",
    );
    label(
        world,
        panel,
        "Export colors, sizes, borders and Sand type settings as JSON.",
        12.0,
    );
    label(
        world,
        panel,
        "Individual Sand and workspace overrides stay in the workspace.",
        12.0,
    );
    let status = world
        .get::<ExportState>(root)
        .map(|state| state.status.clone())
        .unwrap_or_default();
    let status = label(world, panel, &status, 12.0);
    world.entity_mut(status).insert(ExportStatus(root));
}

fn status(world: &mut World, root: Entity, message: String) {
    if world.get::<ExportState>(root).is_none() {
        world.entity_mut(root).insert(ExportState::default());
    }
    world.get_mut::<ExportState>(root).unwrap().status = message;
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn document(world: &World) -> Result<String, String> {
    if crate::laboratory::active(world) {
        return Err("Theme export is unavailable in the Laboratory".into());
    }
    ThemeDocument::export(world.resource::<ThemeSettings>())
}

pub(super) fn copy(world: &mut World, root: Entity) {
    let result = document(world).and_then(|json| {
        world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .ok_or_else(|| "Clipboard is unavailable; use Export theme to save a file".to_string())?
            .set_text(json)
            .map_err(|error| format!("Could not copy theme: {error}"))
    });
    status(
        world,
        root,
        result.map_or_else(|error| error, |()| "Theme JSON copied".into()),
    );
}

pub(super) fn save(world: &mut World, root: Entity) {
    if world
        .get::<ExportState>(root)
        .is_some_and(|state| state.pending.is_some())
    {
        return;
    }
    let json = match document(world) {
        Ok(json) => json,
        Err(error) => {
            status(world, root, error);
            return;
        }
    };
    let name = world
        .resource::<ThemeSettings>()
        .scheme
        .name()
        .to_lowercase()
        .replace(' ', "-");
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = oneshot::channel();
    let task = std::thread::Builder::new()
        .name("theme-export".into())
        .spawn(move || {
            let result = rfd::FileDialog::new()
                .set_title("Export theme")
                .set_file_name(format!("{name}.json"))
                .add_filter("Theme JSON", &["json"])
                .save_file()
                .map(|path| write(&path, &json).map(|()| path))
                .transpose();
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    match task {
        Ok(_) => {
            status(world, root, "Choose where to save the theme…".into());
            world.get_mut::<ExportState>(root).unwrap().pending = Some(receiver);
        }
        Err(error) => status(
            world,
            root,
            format!("Could not start theme export: {error}"),
        ),
    }
}

fn write(path: &Path, json: &str) -> Result<(), String> {
    let save = || -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(json.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    };
    save().map_err(|error| format!("Could not export theme: {error}"))
}

pub(super) fn poll(world: &mut World) {
    for mut state in world.query::<&mut ExportState>().iter_mut(world) {
        let result = state.pending.as_mut().map(oneshot::Receiver::try_recv);
        let message = match result {
            Some(Ok(Ok(Some(path)))) => format!("Theme exported to {}", path.display()),
            Some(Ok(Ok(None))) => "Theme export cancelled".into(),
            Some(Ok(Err(error))) => error,
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                "Theme export stopped before saving".into()
            }
            _ => continue,
        };
        state.pending = None;
        state.status = message;
    }
    let updates: Vec<_> = world
        .query::<(Entity, &ExportStatus)>()
        .iter(world)
        .map(|(entity, owner)| {
            (
                entity,
                world
                    .get::<ExportState>(owner.0)
                    .map(|state| state.status.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    for (entity, status) in updates {
        if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != status
        {
            text.0 = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_file_can_replace_a_preset_without_losing_custom_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("comfy-pink.json");
        let mut theme = ThemeSettings {
            scheme: crate::tokens::ColorScheme::ComfyPink,
            ..default()
        };
        std::fs::write(&path, "previous theme").unwrap();
        theme.global.set(
            crate::tokens::Token::Roundness,
            crate::tokens::TokenValue::Number(28.0),
        );
        let json = ThemeDocument::export(&theme).unwrap();
        write(&path, &json).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(saved, json);
        assert_eq!(
            ThemeDocument::parse(&saved).unwrap().global.0[&crate::tokens::Token::Roundness],
            crate::tokens::TokenValue::Number(28.0)
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(write(&directory.path().join("missing/theme.json"), &json).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), json);
    }

    #[test]
    fn completion_cancellation_and_failure_survive_panel_recreation() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        for (result, expected) in [
            (
                Ok(Some(PathBuf::from("/tmp/moss.json"))),
                "Theme exported to /tmp/moss.json",
            ),
            (Ok(None), "Theme export cancelled"),
            (Err("Disk full".into()), "Disk full"),
        ] {
            let (sender, receiver) = oneshot::channel();
            world.entity_mut(root).insert(ExportState {
                status: "Saving".into(),
                pending: Some(receiver),
            });
            sender.send(result).unwrap();
            poll(&mut world);
            assert!(world.get::<ExportState>(root).unwrap().pending.is_none());
            let label = world.spawn((Text::default(), ExportStatus(root))).id();
            poll(&mut world);
            assert_eq!(world.get::<Text>(label).unwrap().0, expected);
            world.despawn(label);
        }
        let (sender, receiver) = oneshot::channel();
        world.entity_mut(root).insert(ExportState {
            status: String::new(),
            pending: Some(receiver),
        });
        drop(sender);
        poll(&mut world);
        assert_eq!(
            world.get::<ExportState>(root).unwrap().status,
            "Theme export stopped before saving"
        );
    }
}
