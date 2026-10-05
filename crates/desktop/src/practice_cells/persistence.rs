use super::*;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    source: String,
    records: Vec<String>,
}

type Restored = Vec<(Manifest, PathBuf, cell::CellRuntime)>;

struct Loaded {
    cells: Restored,
    errors: Vec<String>,
}

#[derive(Resource, Default)]
pub(crate) struct Restoration {
    started: bool,
    done: bool,
    receiver: Option<Mutex<mpsc::Receiver<Result<Loaded, String>>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Restoration {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

pub(crate) fn needed(
    state: Res<Restoration>,
    file: Option<Res<crate::workspace::WorkspaceFile>>,
) -> bool {
    !state.done && file.is_some()
}

fn ordinary_directory(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("The practice directory must be an ordinary directory.".into());
    }
    Ok(())
}

pub(crate) fn directory(base: &Path, source: &str) -> Result<PathBuf, String> {
    if !nucleus::valid_uid(source, "g") {
        return Err("Invalid practice source.".into());
    }
    let root = base.join("instinct-practice");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    ordinary_directory(&root)?;
    let path = root.join(source);
    std::fs::create_dir(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

pub(crate) fn runtime(
    engine: engine::Engine,
    directory: &Path,
) -> Result<cell::CellRuntime, String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    ordinary_directory(directory)?;
    engine
        .set_command_directory(directory)
        .map_err(|error| error.to_string())?;
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                "instinct-practice".into(),
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    let engine = Arc::new(engine);
    Ok(cell::CellRuntime {
        commands: cell::terminal::commands::CommandHost::new(directory.join("commands")),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    })
}

pub(crate) fn keep(path: &Path, source: &str, records: &[String]) -> Result<(), String> {
    use std::io::Write;
    ordinary_directory(path)?;
    if path.file_name().and_then(|name| name.to_str()) != Some(source)
        || !nucleus::valid_uid(source, "g")
        || records.iter().any(|uid| !nucleus::valid_uid(uid, "r"))
    {
        return Err("Invalid retained practice identity.".into());
    }
    let bytes = serde_json::to_vec(&Manifest {
        source: source.into(),
        records: records.into(),
    })
    .map_err(|error| error.to_string())?;
    let temporary = path.join("retained.json.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path.join("retained.json"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result.map_err(|error| error.to_string())
}

pub(crate) fn discard(path: PathBuf, runtime: Option<cell::CellRuntime>) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            if let Some(runtime) = runtime {
                runtime.commands.shutdown().await;
                if let Some(wire) = runtime.wire.write().await.take() {
                    wire.endpoint().close().await;
                }
                if let Some(fiote) = runtime.fiote {
                    fiote.stop_all().await;
                }
                runtime.store.pool.close().await;
            }
            if ordinary_directory(&path).is_ok() {
                let _ = std::fs::remove_dir_all(path);
            }
        });
    } else if ordinary_directory(&path).is_ok() {
        let _ = std::fs::remove_dir_all(path);
    }
}

async fn load_entry(
    path: PathBuf,
    source: &str,
) -> Result<(Manifest, PathBuf, cell::CellRuntime), String> {
    ordinary_directory(&path)?;
    for filename in ["retained.json", "cell.db", "root.key", "sealing.json"] {
        let target = path.join(filename);
        if matches!(filename, "root.key" | "sealing.json") && !target.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(target).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("A retained practice file is not an ordinary file.".into());
        }
    }
    use std::io::Read;
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags};
        std::fs::File::from(
            rustix::fs::open(
                path.join("retained.json"),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| error.to_string())?,
        )
    };
    #[cfg(not(unix))]
    let file =
        std::fs::File::open(path.join("retained.json")).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Retained practice metadata is not a file.".into());
    }
    let mut bytes = Vec::new();
    file.take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 1_048_576 {
        return Err("Retained practice metadata is too large.".into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if manifest.source != source
        || manifest
            .records
            .iter()
            .any(|uid| !nucleus::valid_uid(uid, "r"))
    {
        return Err("Retained practice identities do not match their directory.".into());
    }
    let engine = engine::Engine::open(&format!("sqlite://{}", path.join("cell.db").display()))
        .await
        .map_err(|error| error.to_string())?;
    if path.join("root.key").exists() {
        engine.set_root_key_path(path.join("root.key"));
        engine.set_sealing_keyring_path(path.join("sealing.json"));
        if let Some(organ) = store::organs::local(&engine.store.pool)
            .await
            .map_err(|error| error.to_string())?
        {
            let signer = engine
                .operational_key_for(&organ.uid)
                .await
                .map_err(|error| error.to_string())?;
            engine
                .set_signer(signer.clone())
                .await
                .map_err(|error| error.to_string())?;
            engine
                .set_organ_signer(signer)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    let runtime = runtime(engine, &path)?;
    Ok((manifest, path, runtime))
}

async fn load(base: PathBuf) -> Result<Loaded, String> {
    let root = base.join("instinct-practice");
    let mut loaded = Loaded {
        cells: Vec::new(),
        errors: Vec::new(),
    };
    if !root.exists() {
        return Ok(loaded);
    }
    ordinary_directory(&root)?;
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(source) = name.to_str().filter(|name| nucleus::valid_uid(name, "g")) else {
            continue;
        };
        if ordinary_directory(&path).is_err() || !path.join("retained.json").exists() {
            continue;
        }
        match load_entry(path, source).await {
            Ok(cell) => loaded.cells.push(cell),
            Err(error) => loaded.errors.push(format!("{source}: {error}")),
        }
    }
    Ok(loaded)
}

pub(crate) fn restore(world: &mut World) {
    if !world.resource::<Restoration>().started {
        let base = world
            .resource::<crate::workspace::WorkspaceFile>()
            .directory()
            .to_path_buf();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            world.resource_mut::<Restoration>().done = true;
            return;
        };
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let (sender, receiver) = mpsc::channel();
        let task = handle.spawn(async move {
            let result = tokio::time::timeout(std::time::Duration::from_secs(30), load(base))
                .await
                .unwrap_or_else(|_| Err("Restoring retained practice timed out.".into()));
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        let mut state = world.resource_mut::<Restoration>();
        state.started = true;
        state.receiver = Some(Mutex::new(receiver));
        state.task = Some(task);
    }
    let result = world
        .resource::<Restoration>()
        .receiver
        .as_ref()
        .and_then(|receiver| receiver.lock().ok()?.try_recv().ok());
    let Some(result) = result else { return };
    world.resource_mut::<Restoration>().done = true;
    world.resource_mut::<Restoration>().receiver = None;
    match result {
        Ok(restored) => {
            world.init_resource::<PracticeCells>();
            for (manifest, path, runtime) in restored.cells {
                let source = manifest.source;
                let mut cells = world.resource_mut::<PracticeCells>();
                cells
                    .records
                    .entry(source.clone())
                    .or_default()
                    .extend(manifest.records);
                cells.directories.insert(source.clone(), path);
                cells.cells.insert(source.clone(), runtime);
                resume_audio(world, &source);
                crate::protein_area::release_practice_source(world, &source);
                crate::protein_area::ensure_auxiliary(
                    world,
                    &crate::protein_area::Source::Organ(source),
                );
            }
            for error in restored.errors {
                crate::notifications::report(
                    world,
                    "Instinct practice",
                    &format!(
                        "A kept example could not be restored: {error}. Its files were preserved."
                    ),
                );
            }
        }
        Err(error) => crate::notifications::report(
            world,
            "Instinct practice",
            &format!("Kept examples could not be restored: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn kept_cell_reopens_with_edits_and_discard_removes_only_its_directory() {
        let base = std::env::temp_dir().join(nucleus::new_uid("instinct"));
        std::fs::create_dir(&base).unwrap();
        let source = nucleus::new_uid("g");
        let path = directory(&base, &source).unwrap();
        let engine = engine::Engine::open(&format!("sqlite://{}", path.join("cell.db").display()))
            .await
            .unwrap();
        let created = engine
            .act(
                engine::actions::Action::CreateRecordDraft {
                    draft: engine::record_creation::Draft {
                        head: "Retained example".into(),
                        ..default()
                    },
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let relevant = |tables: Vec<store::snapshot::Table>| {
            tables
                .into_iter()
                .filter(|table| {
                    matches!(
                        table.name.as_str(),
                        "record" | "record_concept" | "record_slug" | "fact"
                    )
                })
                .collect::<Vec<_>>()
        };
        let before = relevant(engine.store.logical_snapshot().await.unwrap());
        keep(&path, &source, std::slice::from_ref(&created)).unwrap();
        engine.store.pool.close().await;
        let corrupt_source = nucleus::new_uid("g");
        let corrupt = directory(&base, &corrupt_source).unwrap();
        std::fs::write(corrupt.join("retained.json"), "invalid metadata").unwrap();
        std::fs::write(corrupt.join("cell.db"), "invalid database").unwrap();
        let loaded = load(base.clone()).await.unwrap();
        assert_eq!(loaded.errors.len(), 1);
        assert!(corrupt.exists());
        let restored = loaded.cells;
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].0.source, source);
        assert_eq!(
            relevant(restored[0].2.store.logical_snapshot().await.unwrap()),
            before
        );
        restored[0].2.store.pool.close().await;
        std::fs::write(base.join("personal"), "untouched").unwrap();
        std::fs::remove_dir_all(path).unwrap();
        assert_eq!(
            std::fs::read_to_string(base.join("personal")).unwrap(),
            "untouched"
        );
        std::fs::remove_dir_all(base).unwrap();
    }
}
