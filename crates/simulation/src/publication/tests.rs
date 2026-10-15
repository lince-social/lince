use std::{io::Write, path::Path};

use crate::{artifacts, fixtures, publication, scenario::Scenario};
use nucleus::simulation::{ArtifactError, ReplayStatus};

struct Evidence(Option<tempfile::TempDir>);

impl Evidence {
    fn new() -> Self {
        Self(Some(tempfile::tempdir().unwrap()))
    }
    fn path(&self) -> &Path {
        self.0.as_ref().unwrap().path()
    }
}

impl Drop for Evidence {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "publication evidence retained at {}",
                self.0.take().unwrap().keep().display()
            );
        }
    }
}

fn scenario() -> Scenario {
    let mut scenario = fixtures::daily();
    scenario.end_ms = scenario.start_ms;
    scenario.cells[0].seed.truncate(1);
    scenario.checks.clear();
    scenario
}

fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn incomplete(directory: &Path) {
    assert!(
        matches!(
            artifacts::load(directory),
            Err(ArtifactError::Incomplete { .. })
        ),
        "partial bundle accepted: {}",
        directory.display()
    );
    assert!(matches!(
        artifacts::replay(directory, &directory.with_extension("replay"))
            .await
            .unwrap(),
        ReplayStatus::Unavailable {
            reason: ArtifactError::Incomplete { .. }
        }
    ));
    let diagnostic: String =
        crate::limits::read_json(&directory.join("diagnostic.json"), 32 * 1024).unwrap();
    assert!(diagnostic.contains("injected"), "{diagnostic}");
    assert!(diagnostic.len() <= 4096);
}

#[test]
fn write_sync_and_rename_failures_preserve_diagnostics_and_refuse_partial_runs() {
    run(async {
        let root = Evidence::new();
        for file in ["records.json", "manifest.json", "result.json"] {
            for operation in ["write", "sync", "rename", "directory-sync"] {
                let directory = root.path().join(format!("{file}-{operation}"));
                let mut session = artifacts::Session::open(scenario(), &directory, Path::new("."))
                    .await
                    .unwrap();
                while session.step().await.unwrap() {}
                let guard = publication::install(move |path, at| {
                    if path.file_name().is_some_and(|name| name == file) && at == operation {
                        return Err(format!("injected {file} {operation} failure").into());
                    }
                    Ok(())
                });
                let error = match session.finish().await {
                    Ok(_) => panic!("injected failure accepted"),
                    Err(error) => error,
                };
                drop(guard);
                assert!(error.to_string().contains("injected"), "{error}");
                assert!(
                    !directory.join("result.json").exists(),
                    "{file} {operation}: failed publication left a completed result"
                );
                if let Ok(manifest) = crate::limits::read_json::<artifacts::Manifest>(
                    &directory.join("manifest.json"),
                    crate::limits::JSON_BYTES,
                ) {
                    assert_eq!(
                        manifest.files.contains_key("result.json"),
                        file == "result.json"
                            || (file == "manifest.json" && operation == "directory-sync"),
                        "{file} {operation}"
                    );
                }
                incomplete(&directory).await;
            }
        }
    });
}

#[test]
fn source_copy_and_final_verification_failures_are_reported_and_sources_unchanged() {
    run(async {
        let root = Evidence::new();
        let source = crate::world::World::open(scenario(), &root.path().join("source"))
            .await
            .unwrap();
        let input = root.path().join("input.sqlite");
        source.nodes["a"]
            .engine()
            .store
            .snapshot_into(&input)
            .await
            .unwrap();
        source.nodes["a"].engine().store.pool.close().await;
        let before = artifacts::file_hash(&input).unwrap();
        let mut case = scenario();
        case.cells[0].database = Some(crate::scenario::Database {
            file: "input.sqlite".into(),
            hash: before.clone(),
        });
        case.cells[0].seed.clear();
        for operation in [
            "copy",
            "verify",
            "final-verify",
            "changed-copy",
            "changed-final",
        ] {
            let directory = root.path().join(operation);
            let target = directory.clone();
            let guard = publication::install(move |path, at| {
                let source = path
                    .file_name()
                    .is_some_and(|name| name == "a.input.sqlite");
                if (operation == "copy" && source && at == "copy")
                    || (operation == "verify" && source && at == "verify")
                    || (operation == "final-verify" && path == target && at == "verify")
                {
                    return Err(format!("injected {operation} failure").into());
                }
                if (operation == "changed-copy" && source && at == "verify")
                    || (operation == "changed-final" && path == target && at == "verify")
                {
                    let file = if source {
                        path.to_path_buf()
                    } else {
                        path.join("seed/a.input.sqlite")
                    };
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(file)?
                        .write_all(b"injected corruption")?;
                }
                Ok(())
            });
            let error = match artifacts::execute_with_sources(case.clone(), &directory, root.path())
                .await
            {
                Ok(_) => panic!("source failure accepted"),
                Err(error) => error,
            };
            drop(guard);
            assert!(
                error.to_string().contains("injected") || error.to_string().contains("changed"),
                "{operation}: {error}"
            );
            assert!(!directory.join("result.json").exists());
            assert!(matches!(
                artifacts::load(&directory),
                Err(ArtifactError::Incomplete { .. })
            ));
            let diagnostic: String =
                crate::limits::read_json(&directory.join("diagnostic.json"), 32 * 1024).unwrap();
            assert!(diagnostic.contains("injected") || diagnostic.contains("changed"));
            assert_eq!(artifacts::file_hash(&input).unwrap(), before);
        }
    });
}

#[test]
fn publication_child() {
    let Some(root) = std::env::var_os("LINCE_SIMULATION_PUBLICATION_CHILD") else {
        return;
    };
    let boundary = std::env::var("LINCE_SIMULATION_PUBLICATION_BOUNDARY").unwrap();
    run(async move {
        let root = std::path::PathBuf::from(root);
        let marker = root.join("reached");
        let mut session = artifacts::Session::open(scenario(), &root.join("run"), Path::new("."))
            .await
            .unwrap();
        while session.step().await.unwrap() {}
        let guard = publication::install(move |path, operation| {
            let reached = match boundary.as_str() {
                "before-manifest" => {
                    path.file_name().is_some_and(|name| name == "manifest.json")
                        && operation == "rename"
                }
                "between-manifest-result" => {
                    path.file_name().is_some_and(|name| name == "manifest.json")
                        && operation == "published"
                }
                "after-result" => {
                    path.file_name().is_some_and(|name| name == "result.json")
                        && operation == "published"
                }
                _ => panic!("unknown publication boundary"),
            };
            if reached {
                let file = std::fs::File::create(&marker)?;
                file.sync_all()?;
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        });
        session.finish().await.unwrap();
        drop(guard);
        panic!("child did not reach publication boundary");
    });
}

#[test]
fn process_interruption_distinguishes_partial_and_completed_publication() {
    run(async {
        let root = Evidence::new();
        for boundary in ["before-manifest", "between-manifest-result", "after-result"] {
            let directory = root.path().join(boundary);
            std::fs::create_dir(&directory).unwrap();
            let output = std::fs::File::create(directory.join("child.log")).unwrap();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "publication::tests::publication_child",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("LINCE_SIMULATION_PUBLICATION_CHILD", &directory)
                .env("LINCE_SIMULATION_PUBLICATION_BOUNDARY", boundary)
                .stdout(output.try_clone().unwrap())
                .stderr(output)
                .spawn()
                .unwrap();
            let reached = tokio::time::timeout(std::time::Duration::from_secs(45), async {
                loop {
                    if directory.join("reached").exists() {
                        return true;
                    }
                    if child.try_wait().unwrap().is_some() {
                        return false;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap_or(false);
            if !reached {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{boundary} did not reach its boundary; evidence at {}: {}",
                    directory.display(),
                    std::fs::read_to_string(directory.join("child.log")).unwrap()
                );
            }
            child.kill().unwrap();
            child.wait().unwrap();
            let run = directory.join("run");
            let manifest: artifacts::Manifest =
                crate::limits::read_json(&run.join("manifest.json"), crate::limits::JSON_BYTES)
                    .unwrap();
            assert_eq!(
                manifest.files.contains_key("result.json"),
                boundary != "before-manifest"
            );
            assert_eq!(run.join("result.json").exists(), boundary == "after-result");
            let loaded = artifacts::load(&run);
            let replay = artifacts::replay(&run, &directory.join("replay"))
                .await
                .unwrap();
            if boundary == "after-result" {
                assert!(loaded.is_ok(), "{boundary}: {:?}", loaded.err());
                assert!(
                    matches!(replay, ReplayStatus::Verified { .. }),
                    "{boundary}: {replay:?}"
                );
            } else {
                assert!(
                    matches!(loaded, Err(ArtifactError::Incomplete { .. })),
                    "{boundary}: partial run accepted"
                );
                assert!(
                    matches!(
                        replay,
                        ReplayStatus::Unavailable {
                            reason: ArtifactError::Incomplete { .. }
                        }
                    ),
                    "{boundary}: {replay:?}"
                );
            }
        }
    });
}
