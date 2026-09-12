use crate::workspace::{WorkspaceFile, Workspaces};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsSettings {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub physics: PhysicsSettings,
}

#[derive(Clone, Debug, Default)]
pub struct WorkspaceSetting {
    pub config: WorkspaceConfig,
    pub error: Option<String>,
}

#[derive(Component, Default)]
pub struct WorkspaceSettings(pub BTreeMap<u64, WorkspaceSetting>);

pub fn enabled(world: &World, root: Entity, workspace: u64) -> bool {
    if crate::laboratory::suspended(world, root) {
        return false;
    }
    world
        .get::<WorkspaceSettings>(root)
        .and_then(|settings| settings.0.get(&workspace))
        .is_some_and(|setting| setting.error.is_none() && setting.config.physics.enabled)
}

pub fn path(world: &World, workspace: u64) -> Option<PathBuf> {
    let directory = world.get_resource::<WorkspaceFile>()?.directory();
    Some(
        directory
            .join("workspaces")
            .join(workspace.to_string())
            .join("workspace.toml"),
    )
}

pub(crate) fn initialize(world: &mut World, root: Entity) {
    world.entity_mut(root).insert(WorkspaceSettings::default());
    let ids: Vec<_> = world
        .get::<Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.id)
        .collect();
    for id in ids {
        reload(world, root, id);
    }
}

pub fn reload(world: &mut World, root: Entity, workspace: u64) {
    if !world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.entries.iter().any(|entry| entry.id == workspace))
    {
        return;
    }
    if world.get::<WorkspaceSettings>(root).is_none() {
        world.entity_mut(root).insert(WorkspaceSettings::default());
    }
    let Some(path) = (if world
        .get::<crate::laboratory::LaboratoryRoot>(root)
        .is_some()
    {
        None
    } else {
        path(world, workspace)
    }) else {
        world
            .get_mut::<WorkspaceSettings>(root)
            .unwrap()
            .0
            .entry(workspace)
            .or_default();
        return;
    };
    let result = load(&path);
    let setting = match result {
        Ok(config) => WorkspaceSetting {
            config,
            error: None,
        },
        Err(error) => WorkspaceSetting {
            config: WorkspaceConfig::default(),
            error: Some(format!(
                "Physics is off. Could not read {}: {error}",
                path.display()
            )),
        },
    };
    world
        .get_mut::<WorkspaceSettings>(root)
        .unwrap()
        .0
        .insert(workspace, setting);
}

pub fn set_physics(world: &mut World, root: Entity, workspace: u64, enabled: bool) -> bool {
    if !world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.entries.iter().any(|entry| entry.id == workspace))
    {
        return false;
    }
    let result = if let Some(path) = path(world, workspace).filter(|_| {
        world
            .get::<crate::laboratory::LaboratoryRoot>(root)
            .is_none()
    }) {
        load(&path).and_then(|mut config| {
            config.physics.enabled = enabled;
            save(&path, config)?;
            Ok(config)
        })
    } else {
        Ok(WorkspaceConfig {
            physics: PhysicsSettings { enabled },
        })
    };
    if world.get::<WorkspaceSettings>(root).is_none() {
        world.entity_mut(root).insert(WorkspaceSettings::default());
    }
    let succeeded = result.is_ok();
    let setting = match result {
        Ok(config) => WorkspaceSetting {
            config,
            error: None,
        },
        Err(error) => WorkspaceSetting {
            config: WorkspaceConfig::default(),
            error: Some(format!(
                "Physics is off. Could not save workspace settings: {error}"
            )),
        },
    };
    world
        .get_mut::<WorkspaceSettings>(root)
        .unwrap()
        .0
        .insert(workspace, setting);
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
    succeeded
}

fn directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(path)?.file_type().is_dir() {
                Ok(())
            } else {
                Err(io::Error::other("workspace path is not a directory"))
            }
        }
        Err(error) => Err(error),
    }
}

fn prepare(path: &Path) -> io::Result<()> {
    let workspace = path
        .parent()
        .ok_or_else(|| io::Error::other("workspace directory missing"))?;
    let workspaces = workspace
        .parent()
        .ok_or_else(|| io::Error::other("workspaces directory missing"))?;
    directory(workspaces)?;
    directory(workspace)
}

fn load(path: &Path) -> io::Result<WorkspaceConfig> {
    prepare(path)?;
    if !path.try_exists()? {
        let config = WorkspaceConfig::default();
        save(path, config)?;
        return Ok(config);
    }
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::other(
            "workspace settings must be a regular file",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let mut source = String::new();
    options.open(path)?.take(8193).read_to_string(&mut source)?;
    if source.len() > 8192 {
        return Err(io::Error::other("workspace settings exceed 8 KiB"));
    }
    toml::from_str(&source).map_err(io::Error::other)
}

fn save(path: &Path, config: WorkspaceConfig) -> io::Result<()> {
    prepare(path)?;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(io::Error::other(
            "workspace settings must be a regular file",
        ));
    }
    let source = toml::to_string_pretty(&config).map_err(io::Error::other)?;
    let temporary = path.with_extension("toml.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn next_id(world: &World, root: Entity, mut id: u64) -> Option<u64> {
    loop {
        if !world
            .get::<WorkspaceSettings>(root)
            .is_some_and(|settings| settings.0.contains_key(&id))
            && path(world, id).is_none_or(|path| !path.parent().unwrap().exists())
        {
            return Some(id);
        }
        id = id.checked_add(1)?;
    }
}

pub(crate) fn controls(world: &mut World, root: Entity, panel: Entity) {
    let active = world.get::<Workspaces>(root).unwrap().active;
    let enabled = enabled(world, root, active);
    crate::edit_mode::control(
        world,
        root,
        panel,
        crate::edit_mode::EditAction::TogglePhysics,
        if enabled {
            "Physics: on — turn off"
        } else {
            "Physics: off — turn on"
        },
    );
    crate::edit_mode::label(
        world,
        panel,
        "Physics runs in the open workspace. Turning it off keeps every Sand in place.",
        14.0,
    );
    if let Some(path) = path(world, active) {
        crate::edit_mode::label(world, panel, &path.display().to_string(), 12.0);
        crate::edit_mode::control(
            world,
            root,
            panel,
            crate::edit_mode::EditAction::ReloadWorkspaceSettings,
            "Reload workspace settings",
        );
    }
    let error = world
        .get::<WorkspaceSettings>(root)
        .and_then(|settings| settings.0.get(&active))
        .and_then(|setting| setting.error.clone());
    if let Some(error) = error {
        crate::edit_mode::label(world, panel, &error, 14.0);
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn each_workspace_has_a_private_toml_and_restores_its_own_physics_choice() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("workspaces/1/workspace.toml");
        let second = directory.path().join("workspaces/2/workspace.toml");
        assert!(!load(&first).unwrap().physics.enabled);
        assert!(!load(&second).unwrap().physics.enabled);
        save(
            &first,
            WorkspaceConfig {
                physics: PhysicsSettings { enabled: true },
            },
        )
        .unwrap();
        assert!(load(&first).unwrap().physics.enabled);
        assert!(!load(&second).unwrap().physics.enabled);
        assert_eq!(
            fs::read_to_string(&first).unwrap(),
            "[physics]\nenabled = true\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&first).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(first.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[cfg_attr(test, test)]
    fn malformed_unknown_and_oversized_settings_are_rejected_and_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces/1/workspace.toml");
        load(&path).unwrap();
        for source in [
            "[physics]\nenabled='yes'".to_string(),
            "[physics]\nenabeld=true".to_string(),
            "x".repeat(8193),
        ] {
            fs::write(&path, &source).unwrap();
            assert!(load(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), source);
        }
    }

    #[cfg_attr(test, test)]
    fn reload_applies_manual_changes_and_failed_toggle_turns_physics_off() {
        let directory = tempfile::tempdir().unwrap();
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.insert_resource(WorkspaceFile::new(directory.path().join("interface.json")));
        let root = world.spawn(Workspaces::default()).id();
        initialize(&mut world, root);
        let path = path(&world, 1).unwrap();
        fs::write(&path, "[physics]\nenabled = true\n").unwrap();
        reload(&mut world, root, 1);
        assert!(enabled(&world, root, 1));
        fs::write(&path, "[physics]\nenabled = 17\n").unwrap();
        assert!(!set_physics(&mut world, root, 1, false));
        assert!(!enabled(&world, root, 1));
        assert!(
            world.get::<WorkspaceSettings>(root).unwrap().0[&1]
                .error
                .is_some()
        );
        assert!(fs::read_to_string(&path).unwrap().contains("17"));
        assert_eq!(next_id(&world, root, 1), Some(2));
    }

    #[cfg(unix)]
    #[cfg_attr(test, test)]
    fn configuration_does_not_follow_directory_file_or_temporary_symlinks() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspaces = directory.path().join("workspaces");
        symlink(outside.path(), &workspaces).unwrap();
        let path = workspaces.join("1/workspace.toml");
        assert!(load(&path).is_err());
        assert!(!outside.path().join("1").exists());
        fs::remove_file(&workspaces).unwrap();
        load(&path).unwrap();
        let external = outside.path().join("settings.toml");
        fs::write(&external, "keep").unwrap();
        fs::remove_file(&path).unwrap();
        symlink(&external, &path).unwrap();
        assert!(load(&path).is_err());
        assert!(save(&path, WorkspaceConfig::default()).is_err());
        fs::remove_file(&path).unwrap();
        load(&path).unwrap();
        symlink(&external, path.with_extension("toml.tmp")).unwrap();
        assert!(save(&path, WorkspaceConfig::default()).is_err());
        assert_eq!(fs::read_to_string(&external).unwrap(), "keep");
    }

    crate::laboratory_cases! {
        each_workspace_has_a_private_toml_and_restores_its_own_physics_choice,
        malformed_unknown_and_oversized_settings_are_rejected_and_kept,
        reload_applies_manual_changes_and_failed_toggle_turns_physics_off,
        #[cfg(unix)]
        configuration_does_not_follow_directory_file_or_temporary_symlinks,
    }
}
