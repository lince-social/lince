use super::*;
use std::time::Duration;

struct Fixture {
    root: PathBuf,
    host: CommandHost,
    command: String,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(nucleus::new_uid("command-test"));
        std::fs::create_dir(&root).unwrap();
        let host = CommandHost::new(root.clone());
        Self {
            root,
            host,
            command: nucleus::new_uid("r"),
        }
    }

    fn start(&self, script: &str) -> Run {
        let Response::Started { run } = self
            .host
            .handle(Request::Run {
                command: self.command.clone(),
                script: script.into(),
                cwd: self.root.display().to_string(),
            })
            .unwrap()
        else {
            panic!("Expected a run")
        };
        run
    }

    async fn finish(&self, run: &Run) -> Vec<Event> {
        let mut events = Vec::new();
        let mut offset = 0;
        for _ in 0..2000 {
            let Response::Output {
                events: page,
                next_offset,
                complete,
            } = self
                .host
                .handle(Request::Read {
                    command: self.command.clone(),
                    run: run.id.clone(),
                    offset,
                })
                .unwrap()
            else {
                panic!("Expected output")
            };
            events.extend(page);
            offset = next_offset;
            if complete {
                return events;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("Command did not finish");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(runs) = self.host.0.runs.lock() {
            for active in runs.values() {
                stop(&active.handle, active.pid);
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn output(events: &[Event]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Output { data_base64 } => Some(BASE64.decode(data_base64).unwrap()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[tokio::test]
async fn detached_runs_keep_script_directory_and_full_output_in_one_file() {
    let fixture = Fixture::new();
    let view = fixture.host.clone();
    let script = "sleep 0.05\ncat > example.toml <<'EOF'\nname = 'unchanged $HOME'\nEOF\ncat example.toml\nprintf '\\033[31mcolored\\033[0m\\n'\nexit 7";
    let run = fixture.start(script);
    drop(view);
    let events = fixture.finish(&run).await;
    assert_eq!(run.script, script);
    assert_eq!(Path::new(&run.cwd), fixture.root.canonicalize().unwrap());
    assert!(String::from_utf8_lossy(&output(&events)).contains("unchanged $HOME"));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Finished {
            exit_code: Some(7),
            ..
        }
    )));
    let directory = fixture.root.join("trash/terminal").join(&fixture.command);
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    let path = directory.join(format!("{}.jsonl", run.id));
    assert_eq!(journal::summary(&path).unwrap().script, script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let reopened = CommandHost::new(fixture.root.clone());
    assert_eq!(
        reopened.history(&fixture.command).unwrap()[0].exit_code,
        Some(7)
    );
}

#[tokio::test]
async fn interactive_input_and_resize_use_the_live_pty() {
    let fixture = Fixture::new();
    let run =
        fixture.start("read -r -p 'Name: ' answer\nprintf 'received:%s\\n' \"$answer\"\nstty size");
    fixture
        .host
        .handle(Request::Resize {
            command: fixture.command.clone(),
            run: run.id.clone(),
            cols: 111,
            rows: 33,
        })
        .unwrap();
    fixture
        .host
        .handle(Request::Input {
            command: fixture.command.clone(),
            run: run.id.clone(),
            data_base64: BASE64.encode(b"Lince\n"),
        })
        .unwrap();
    let events = fixture.finish(&run).await;
    let text = String::from_utf8_lossy(&output(&events)).into_owned();
    assert!(text.contains("received:Lince"), "{text}");
    assert!(text.contains("33 111"), "{text}");
    assert!(
        fixture
            .host
            .handle(Request::Input {
                command: fixture.command.clone(),
                run: run.id,
                data_base64: BASE64.encode(b"no")
            })
            .is_err()
    );
}

#[tokio::test]
async fn output_pages_preserve_every_byte_with_bounded_pages() {
    let fixture = Fixture::new();
    let run = fixture.start("head -c 300000 /dev/zero | tr '\\0' x");
    let events = fixture.finish(&run).await;
    assert_eq!(output(&events), vec![b'x'; 300000]);
    assert!(
        events
            .iter()
            .filter_map(|event| match event {
                Event::Output { data_base64 } => Some(BASE64.decode(data_base64).unwrap().len()),
                _ => None,
            })
            .all(|length| length <= 8192)
    );
    assert!(
        fixture
            .host
            .handle(Request::Read {
                command: fixture.command.clone(),
                run: run.id,
                offset: 1
            })
            .is_err()
    );
}

#[tokio::test]
async fn retention_keeps_ten_finished_runs_and_the_active_run() {
    let fixture = Fixture::new();
    for _ in 0..12 {
        let run = fixture.start("printf done");
        fixture.finish(&run).await;
    }
    fixture.host.prune(&fixture.command).unwrap();
    assert_eq!(fixture.host.history(&fixture.command).unwrap().len(), 10);
    let active = fixture.start("sleep 30");
    fixture.host.prune(&fixture.command).unwrap();
    assert_eq!(fixture.host.history(&fixture.command).unwrap().len(), 11);
    assert!(
        fixture
            .host
            .handle(Request::Run {
                command: fixture.command.clone(),
                script: "printf duplicate".into(),
                cwd: "~".into()
            })
            .is_err()
    );
    fixture
        .host
        .handle(Request::Stop {
            command: fixture.command.clone(),
            run: active.id.clone(),
        })
        .unwrap();
    let events = fixture.finish(&active).await;
    assert!(events.iter().any(|event| matches!(event, Event::Finished { error: Some(error), .. } if error == "Stopped by user")));
}

#[tokio::test]
async fn stop_terminates_children_and_other_commands_cannot_control_the_run() {
    let fixture = Fixture::new();
    let run = fixture.start("sleep 0.5; printf survived > survivor");
    assert!(
        fixture
            .host
            .handle(Request::Stop {
                command: nucleus::new_uid("r"),
                run: run.id.clone()
            })
            .is_err()
    );
    fixture
        .host
        .handle(Request::Stop {
            command: fixture.command.clone(),
            run: run.id.clone(),
        })
        .unwrap();
    fixture.finish(&run).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(!fixture.root.join("survivor").exists());
}

#[test]
fn history_paths_reject_traversal_and_symlinks_and_home_is_portable() {
    let fixture = Fixture::new();
    assert!(journal::directory(&fixture.root, "../escape").is_err());
    assert!(journal::path(&fixture.root, &fixture.command, "../../escape").is_err());
    assert!(working_directory("relative/path").is_err());
    assert_eq!(
        working_directory("~").unwrap(),
        dirs::home_dir().unwrap().canonicalize().unwrap()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&fixture.root, fixture.root.join("trash")).unwrap();
        assert!(journal::directory(&fixture.root, &fixture.command).is_err());
    }
}

#[tokio::test]
async fn backend_rejects_execution_of_plain_records() {
    let fixture = Fixture::new();
    let engine = engine::Engine::open_memory().await.unwrap();
    let result = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Plain".into(),
                body: "printf unexpected".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .host
            .request(
                &engine,
                Request::Run {
                    command: result.created.unwrap(),
                    script: "printf unexpected".into(),
                    cwd: "~".into()
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unicode_scripts_and_incomplete_journal_tails_remain_readable() {
    let fixture = Fixture::new();
    let script = format!(": '{}'", "🦊".repeat(9000));
    let run = fixture.start(&script);
    fixture.finish(&run).await;
    let path = journal::path(&fixture.root, &fixture.command, &run.id).unwrap();
    assert_eq!(journal::summary(&path).unwrap().script, script);
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(b"{\"event\":\"output\"").unwrap();
    let events = fixture.finish(&run).await;
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Finished {
            exit_code: Some(0),
            ..
        }
    )));
}

#[tokio::test]
async fn runtime_shutdown_stops_commands_and_saves_the_reason() {
    let fixture = Fixture::new();
    let run = fixture.start("sleep 30");
    fixture.host.shutdown().await;
    let events = fixture.finish(&run).await;
    assert!(events.iter().any(|event| matches!(event, Event::Finished { error: Some(error), .. } if error == "Stopped when Lince closed")));
}
