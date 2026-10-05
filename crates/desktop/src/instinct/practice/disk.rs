use super::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const BODY: &str = "This note was exported from the disposable practice Cell.";
const EDITED: &str = "I edited the exported practice file.";

#[derive(Component)]
struct View {
    path: PathBuf,
    receiver: tokio::sync::watch::Receiver<(bool, bool, Option<String>)>,
    prepared: bool,
    edit: Option<tokio::sync::oneshot::Receiver<Result<(), String>>>,
}

#[derive(Clone, Copy)]
struct EditFile;

impl Action for EditFile {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        if view.edit.is_some() || view.receiver.borrow().1 {
            return;
        }
        let path = view.path.clone();
        let Some(root) = super::root(world, owner) else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = handle.spawn(async move {
            let result = async {
                let metadata = tokio::fs::symlink_metadata(&path)
                    .await
                    .map_err(|error| error.to_string())?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() > 1_048_576
                {
                    return Err(
                        "The prepared export changed. Retry the lesson in a fresh sample.".into(),
                    );
                }
                tokio::fs::write(path, EDITED)
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        world.get_mut::<View>(owner).unwrap().edit = Some(receiver);
        world.get_mut::<Practice>(root).unwrap().tasks.push(task);
    }
}

pub(super) async fn fixture(runtime: &cell::CellRuntime, path: &Path) -> Result<Value, String> {
    let record = runtime
        .engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("practice-disk-note".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Practice disk note".into(),
                body: BODY.into(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?
        .created
        .ok_or("The sample note was not confirmed.")?;
    let protein = runtime.engine.act(engine::actions::Action::SaveProtein {
        slug: "practice-file-selection".into(), head: "Only the practice disk note".into(),
        ast: json!({"source":"record","where":[{"all":[{"kind_eq":"plain"},{"text_contains":"Practice disk note"}]}]}),
    }, None).await.map_err(|error| error.to_string())?.created.ok_or("The sample selection was not confirmed.")?;
    let directory = path.join("record-files");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    Ok(
        json!({"record":record,"protein":protein,"directory":directory,"file":directory.join("Practice disk note.md")}),
    )
}

pub(super) fn view(world: &mut World, root: Entity, data: &Value) -> Result<Entity, String> {
    let practice = world.get::<Practice>(root).unwrap();
    let source = practice.source.clone();
    let runtime = world
        .resource::<crate::practice_cells::PracticeCells>()
        .cells[&source]
        .clone();
    let owner = crate::sand_store::spawn_scoped_sand(
        world,
        root,
        practice.workspace,
        crate::sand_store::SandKind::Sync,
        DVec2::new(800.0, 0.0),
        source.clone(),
    );
    own(world, root, owner, Role::Feature);
    let directory = PathBuf::from(data["directory"].as_str().unwrap());
    let path = PathBuf::from(data["file"].as_str().unwrap());
    let file = path.clone();
    let uid = data["record"].as_str().unwrap().to_owned();
    let protein = data["protein"].as_str().unwrap().to_owned();
    let (sender, receiver) = tokio::sync::watch::channel((false, false, None));
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let handle =
        tokio::runtime::Handle::try_current().map_err(|_| "The sample runtime is unavailable.")?;
    let worker = handle.spawn(async move {
        let mut state = engine::file_sync::FileSyncState::new();
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            timer.tick().await;
            let result = async {
                let organ = store::organs::local(&runtime.store.pool)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or("The sample Organ is missing.")?;
                let config = store::records::get_extension(
                    &runtime.store.pool,
                    &organ.uid,
                    "lince.file_sync",
                )
                .await
                .map_err(|error| error.to_string())?
                .unwrap_or_default();
                if config["enabled"] == true
                    && config["protein"] == protein
                    && config["path"].as_str() == directory.to_str()
                {
                    runtime
                        .engine
                        .file_sync_tick(&directory, &organ.uid, &mut state)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                let exported = tokio::fs::metadata(&file).await.is_ok();
                let edited = store::records::get(&runtime.store.pool, &uid)
                    .await
                    .map_err(|error| error.to_string())?
                    .is_some_and(|record| record.body == EDITED);
                Ok::<_, String>((exported, edited))
            }
            .await;
            let next = match result {
                Ok((exported, edited)) => (exported, edited, None),
                Err(error) => (false, false, Some(error)),
            };
            if *sender.borrow() != next {
                let _ = sender.send(next);
                if let Some(wake) = &wake {
                    wake.ring();
                }
            }
        }
    });
    world
        .resource_mut::<crate::practice_cells::PracticeCells>()
        .workers
        .insert(source, crate::practice_cells::Worker(worker));
    world.entity_mut(owner).insert(View {
        path,
        receiver,
        prepared: false,
        edit: None,
    });
    crate::description::button(world, owner, owner, "Apply the example file edit", EditFile);
    Ok(owner)
}

pub(super) fn prepare(world: &mut World, root: Entity) {
    if world.get::<Practice>(root).unwrap().runner.lesson.subject != "learn-sync" {
        return;
    }
    let Some(owner) = find(world, root, Role::Feature) else {
        return;
    };
    let data = world
        .get::<Practice>(root)
        .unwrap()
        .community
        .data
        .clone()
        .unwrap();
    if world.get::<View>(owner).is_some_and(|view| !view.prepared)
        && crate::sync_castle::prepare_directory(
            world,
            owner,
            data["protein"].as_str().unwrap(),
            data["directory"].as_str().unwrap(),
        )
    {
        world.get_mut::<View>(owner).unwrap().prepared = true;
    }
    let result = world
        .get_mut::<View>(owner)
        .and_then(|mut view| view.edit.as_mut()?.try_recv().ok());
    if let Some(Err(error)) = result {
        world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable(error);
        input::release(world, root);
        render(world, root);
    }
    let error = world
        .get::<View>(owner)
        .and_then(|view| view.receiver.borrow().2.clone());
    if let Some(error) = error {
        let practice = world.get_mut::<Practice>(root).unwrap();
        if practice.runner.phase != Phase::Unavailable(error.clone()) {
            drop(practice);
            world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable(error);
            input::release(world, root);
            render(world, root);
        }
    }
}

pub(super) fn execute(
    world: &mut World,
    root: Entity,
    owner: Entity,
    data: &Value,
    operation: Operation,
) -> Result<(), String> {
    match operation {
        Operation::ExportPracticeFiles => {
            crate::sync_castle::prepare_directory(
                world,
                owner,
                data["protein"].as_str().unwrap(),
                data["directory"].as_str().unwrap(),
            );
            crate::sync_castle::save_directory(world, owner);
        }
        Operation::EditPracticeFile => {
            EditFile.apply(world, owner);
            if find(world, root, Role::Record).is_none() {
                let source = crate::protein_area::Source::Organ(
                    world.get::<Practice>(root).unwrap().source.clone(),
                );
                if let Some(record) =
                    crate::full_record::open(world, root, data["record"].as_str().unwrap(), source)
                {
                    own(world, root, record, Role::Record);
                }
            }
        }
        Operation::StopPracticeSync => crate::sync_castle::stop_directory(world, owner),
        _ => {}
    }
    Ok(())
}

pub(super) fn complete(world: &World, owner: Entity, operation: Operation) -> bool {
    let Some(view) = world.get::<View>(owner) else {
        return false;
    };
    match operation {
        Operation::ExportPracticeFiles => view.receiver.borrow().0,
        Operation::EditPracticeFile => view.receiver.borrow().1,
        Operation::StopPracticeSync => crate::sync_castle::configured(world, owner, false),
        _ => false,
    }
}
