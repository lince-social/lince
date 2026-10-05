use super::*;
use std::path::PathBuf;

pub(super) const NOTE: &str = "An edited practice file.\n";

#[derive(Clone)]
pub(super) struct Paths {
    pub project: PathBuf,
    pub note: PathBuf,
    pub source: PathBuf,
    pub document: PathBuf,
}

#[derive(Default)]
pub(super) struct State {
    receiver: Option<Mutex<mpsc::Receiver<Result<Paths, String>>>>,
    pub(super) paths: Option<Paths>,
    failed: bool,
}

pub(super) fn prepare(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if !matches!(
        practice.runner.lesson.subject,
        "ide" | "language-tools" | "documents" | "terminal" | "external-files"
    ) || practice.setup.is_some()
        || practice.records.is_empty()
    {
        return;
    }
    let result = practice
        .local
        .receiver
        .as_ref()
        .and_then(|receiver| receiver.lock().ok()?.try_recv().ok());
    if let Some(result) = result {
        world.get_mut::<Practice>(root).unwrap().local.receiver = None;
        match result {
            Ok(paths) => world.get_mut::<Practice>(root).unwrap().local.paths = Some(paths),
            Err(error) => {
                let mut practice = world.get_mut::<Practice>(root).unwrap();
                practice.local.failed = true;
                practice.runner.phase = Phase::Unavailable(error);
                input::release(world, root);
                render(world, root);
            }
        }
    }
    let practice = world.get::<Practice>(root).unwrap();
    if practice.local.paths.is_none() {
        if practice.local.receiver.is_none() && !practice.local.failed {
            let Some(base) = world
                .resource::<crate::practice_cells::PracticeCells>()
                .directories
                .get(&practice.source)
                .cloned()
            else {
                return;
            };
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                return;
            };
            let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
            let (sender, receiver) = mpsc::channel();
            let task = handle.spawn(async move {
                let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                    let project = base.join("practice-project");
                    tokio::fs::create_dir_all(&project)
                        .await
                        .map_err(|error| error.to_string())?;
                    let paths = Paths {
                        note: project.join("note.txt"),
                        source: project.join("main.rs"),
                        document: project.join("practice.pdf"),
                        project,
                    };
                    tokio::fs::write(&paths.note, "A practice file.\n")
                        .await
                        .map_err(|error| error.to_string())?;
                    tokio::fs::write(
                        &paths.source,
                        "fn main() { println!(\"A practice project.\"); }\n",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    tokio::fs::write(
                        &paths.document,
                        include_bytes!("../../../fixtures/drop.pdf"),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    Ok::<_, String>(paths)
                })
                .await
                .unwrap_or_else(|_| {
                    Err(
                        "Preparing the local files timed out. Skip or Close remains available."
                            .into(),
                    )
                });
                let _ = sender.send(result);
                if let Some(wake) = wake {
                    wake.ring();
                }
            });
            let mut practice = world.get_mut::<Practice>(root).unwrap();
            practice.local.receiver = Some(Mutex::new(receiver));
            practice.tasks.push(task);
        }
        return;
    }
    let cancelled = practice.runner.current().and_then(|step| step.operation)
        == Some(Operation::CancelFileChoices)
        && crate::external_drop::choice_cancelled(world, root);
    if !cancelled
        && find(world, root, Role::Feature).is_none()
        && let Err(error) = view(world, root)
    {
        world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable(error);
        input::release(world, root);
        render(world, root);
    }
    if world.get::<Practice>(root).unwrap().pending.is_some()
        && let Some(operation) = world
            .get::<Practice>(root)
            .unwrap()
            .runner
            .current()
            .and_then(|step| step.operation)
    {
        let _ = execute(world, root, operation);
    }
}

fn view(world: &mut World, root: Entity) -> Result<Entity, String> {
    let practice = world.get::<Practice>(root).unwrap();
    let paths = practice.local.paths.clone().unwrap();
    let subject = practice.runner.lesson.subject;
    let workspace = practice.workspace;
    let source = practice.source.clone();
    let owner = match subject {
        "ide" | "language-tools" => {
            let active = if subject == "language-tools" {
                paths.source
            } else {
                paths.note
            };
            crate::ide::spawn(
                world,
                root,
                workspace,
                DVec2::new(800.0, 0.0),
                crate::ide::Ide {
                    explorer: crate::file_explorer::FileExplorer {
                        roots: vec![paths.project],
                        ..default()
                    },
                    paths: vec![active.clone()],
                    active: Some(active),
                    ..default()
                },
            )
        }
        "documents" => crate::document_viewer::spawn(
            world,
            root,
            workspace,
            DVec2::new(800.0, 0.0),
            crate::document_viewer::DocumentViewer::with_path(paths.document.to_string_lossy()),
        ),
        "terminal" => crate::sand_store::spawn_scoped_sand(
            world,
            root,
            workspace,
            crate::sand_store::SandKind::Terminal,
            DVec2::new(800.0, 0.0),
            source,
        ),
        "external-files" => {
            crate::external_drop::prepare_choice(world, root, paths.document);
            return crate::external_drop::choice_owner(world, root)
                .map(|owner| {
                    world.entity_mut(owner).insert(WorkspaceMember(workspace));
                    own(world, root, owner, Role::Feature);
                    owner
                })
                .ok_or("The file choice is unavailable.".into());
        }
        _ => return Err("This local-file lesson is unavailable.".into()),
    };
    own(world, root, owner, Role::Feature);
    Ok(owner)
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    let Some(owner) = find(world, root, Role::Feature) else {
        return Ok(());
    };
    match operation {
        Operation::OpenPracticeFile => {}
        Operation::EditSavePracticeFile => crate::ide::edit_and_save(world, owner, NOTE),
        Operation::InspectLanguageTools => crate::ide::inspect_tools(world, owner),
        Operation::NextDocumentPage => crate::document_viewer::next_page(world, owner),
        Operation::InspectTerminal => {}
        Operation::InspectFileChoices => {}
        Operation::CancelFileChoices => crate::external_drop::cancel_choice(world, root),
        _ => return Err("This local-file lesson is unavailable.".into()),
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    if operation == Operation::CancelFileChoices {
        return crate::external_drop::choice_cancelled(world, root);
    }
    let Some(owner) = find(world, root, Role::Feature) else {
        return false;
    };
    match operation {
        Operation::OpenPracticeFile => crate::ide::file_ready(world, owner),
        Operation::EditSavePracticeFile => crate::ide::saved_text(world, owner, NOTE),
        Operation::InspectLanguageTools => crate::ide::tools_visible(world, owner),
        Operation::NextDocumentPage => crate::document_viewer::page_visible(world, owner, 1),
        Operation::InspectTerminal => crate::terminal::controls_visible(world, owner),
        Operation::InspectFileChoices => crate::external_drop::choice_ready(world, root),
        _ => false,
    }
}
