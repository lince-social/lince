use tempfile::{TempDir, tempdir};
use tokio::time::{sleep, timeout};

use super::*;

struct Fixture {
    host: CommandHost,
    command: String,
    directory: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempdir().expect("command journal directory");
        Self {
            host: CommandHost::new(directory.path().into()),
            command: nucleus::new_uid("r"),
            directory,
        }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn request(&self, request: Request) -> Response {
        self.host.handle(request).expect("valid command request")
    }

    fn start(&self, script: &str) -> Run {
        match self.request(Request::Run {
            command: self.command.clone(),
            script: script.into(),
            cwd: self.root().display().to_string(),
        }) {
            Response::Started { run } => run,
            other => panic!("expected a started run, got {other:?}"),
        }
    }

    fn input(&self, run: &Run, bytes: &[u8]) {
        assert!(matches!(
            self.request(Request::Input {
                command: self.command.clone(),
                run: run.id.clone(),
                data_base64: BASE64.encode(bytes),
            }),
            Response::Ok
        ));
    }

    fn resize(&self, run: &Run, cols: u16, rows: u16) {
        assert!(matches!(
            self.request(Request::Resize {
                command: self.command.clone(),
                run: run.id.clone(),
                cols,
                rows,
            }),
            Response::Ok
        ));
    }

    fn stop(&self, run: &Run) {
        assert!(matches!(
            self.request(Request::Stop {
                command: self.command.clone(),
                run: run.id.clone(),
            }),
            Response::Ok
        ));
    }

    fn page(&self, run: &Run, offset: u64) -> (Vec<Event>, u64, bool) {
        match self.request(Request::Read {
            command: self.command.clone(),
            run: run.id.clone(),
            offset,
        }) {
            Response::Output {
                events,
                next_offset,
                complete,
            } => (events, next_offset, complete),
            other => panic!("expected an output page at {offset}, got {other:?}"),
        }
    }

    async fn finish(&self, run: &Run) -> Vec<Event> {
        let mut events = Vec::new();
        let mut offset = 0;
        for _ in 0..2000 {
            let (page, next_offset, complete) = self.page(run, offset);
            events.extend(page);
            offset = next_offset;
            if complete {
                return events;
            }
            sleep(Duration::from_millis(5)).await;
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
    }
}

fn output_frames(events: &[Event]) -> impl Iterator<Item = Vec<u8>> {
    events.iter().filter_map(|event| match event {
        Event::Output { data_base64 } => {
            Some(BASE64.decode(data_base64).expect("valid output frame"))
        }
        _ => None,
    })
}

fn output(events: &[Event]) -> Vec<u8> {
    output_frames(events).flatten().collect()
}

#[cfg(unix)]
#[test]
fn login_shell_loads_user_profile_and_finds_tools_outside_service_path() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let home = fixture.root().join("home");
    let tools = home.join("tools");
    let service = fixture.root().join("service-bin");
    std::fs::create_dir_all(&tools).expect("profile tool directory");
    std::fs::create_dir(&service).expect("service bin directory");
    let path = std::env::var_os("PATH").expect("shell search path");
    let bash = std::env::split_paths(&path)
        .map(|directory| directory.join("bash"))
        .find(|path| path.is_file())
        .expect("Bash on the shell search path");
    let git = tools.join("git");
    std::fs::write(
        &git,
        format!(
            "#!{}\nprintf 'tool:%s\\n' \"$PROFILE_SENTINEL\"\n",
            bash.display()
        ),
    )
    .expect("profile tool");
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755))
        .expect("executable profile tool");
    std::fs::write(
        home.join(".bash_profile"),
        "export PATH=\"$HOME/tools\"\nexport PROFILE_SENTINEL=loaded\n",
    )
    .expect("login profile");
    let script =
        "git\nprintf 'cwd:%s\\n' \"$PWD\"\nprintf '%s\\n' 'literal $HOME; untouched'\nexit 7";
    let mut command = shell_command(
        bash.to_str().expect("Bash path in UTF-8"),
        script,
        fixture.root(),
    );
    command.env("HOME", &home);
    command.env("PATH", &service);
    let mut spawned = spawn_command(
        pty_size(80, 24, 0, 0),
        command,
        bash.display().to_string(),
        fixture.root().display().to_string(),
    )
    .expect("test login shell");
    assert_eq!(spawned.child.wait().expect("shell exit").exit_code(), 7);
    let mut output = String::new();
    spawned
        .reader
        .read_to_string(&mut output)
        .expect("login shell output");
    assert!(output.contains("tool:loaded"), "{output}");
    assert!(
        output.contains(&format!("cwd:{}", fixture.root().display())),
        "{output}"
    );
    assert!(output.contains("literal $HOME; untouched"), "{output}");
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
    assert_eq!(
        Path::new(&run.cwd),
        fixture.root().canonicalize().expect("fixture directory")
    );
    let bytes = output(&events);
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("unchanged $HOME"));
    assert!(text.contains("\x1b[31mcolored\x1b[0m"), "{text}");
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Finished {
            exit_code: Some(7),
            ..
        }
    )));
    let directory = fixture.root().join("trash/terminal").join(&fixture.command);
    assert_eq!(
        std::fs::read_dir(&directory)
            .expect("history directory")
            .count(),
        1
    );
    let path = directory.join(format!("{}.jsonl", run.id));
    assert_eq!(journal::summary(&path).expect("run summary").script, script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path)
                .expect("run journal")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let reopened = CommandHost::new(fixture.root().to_path_buf());
    assert!(matches!(
        reopened
            .history(&fixture.command)
            .expect("retained history")
            .as_slice(),
        [Run {
            exit_code: Some(7),
            ..
        }]
    ));
}

#[tokio::test]
async fn interactive_input_and_resize_use_the_live_pty() {
    let fixture = Fixture::new();
    let run =
        fixture.start("read -r -p 'Name: ' answer\nprintf 'received:%s\\n' \"$answer\"\nstty size");
    fixture.resize(&run, 111, 33);
    fixture.input(&run, b"Lince\n");
    let events = fixture.finish(&run).await;
    let bytes = output(&events);
    let text = String::from_utf8_lossy(&bytes);
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
    let mut bytes = Vec::new();
    for frame in output_frames(&events) {
        assert!(frame.len() <= 8192);
        bytes.extend(frame);
    }
    assert_eq!(bytes, vec![b'x'; 300_000]);
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
    fixture.host.prune(&fixture.command).expect("prune history");
    assert_eq!(
        fixture
            .host
            .history(&fixture.command)
            .expect("finished history")
            .len(),
        10
    );
    let active = fixture.start("sleep 30");
    fixture
        .host
        .prune(&fixture.command)
        .expect("prune while running");
    assert_eq!(
        fixture
            .host
            .history(&fixture.command)
            .expect("active history")
            .len(),
        11
    );
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
    fixture.stop(&active);
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
    fixture.stop(&run);
    fixture.finish(&run).await;
    sleep(Duration::from_millis(600)).await;
    assert!(!fixture.root().join("survivor").exists());
}

#[test]
fn history_paths_reject_traversal_and_symlinks_and_home_is_portable() {
    let fixture = Fixture::new();
    assert!(journal::directory(fixture.root(), "../escape").is_err());
    assert!(journal::path(fixture.root(), &fixture.command, "../../escape").is_err());
    assert!(working_directory("relative/path").is_err());
    assert_eq!(
        working_directory("~").expect("home shorthand"),
        dirs::home_dir()
            .expect("test home")
            .canonicalize()
            .expect("canonical home")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(fixture.root(), fixture.root().join("trash"))
            .expect("symlink used to exercise journal rejection");
        assert!(journal::directory(fixture.root(), &fixture.command).is_err());
    }
}

#[tokio::test]
async fn backend_rejects_execution_of_plain_records() {
    let fixture = Fixture::new();
    let engine = engine::Engine::open_memory()
        .await
        .expect("in-memory Engine");
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
        .expect("plain Record");
    assert_eq!(
        fixture
            .host
            .request(
                &engine,
                Request::Run {
                    command: result.created.expect("plain Record UID"),
                    script: "printf unexpected".into(),
                    cwd: "~".into()
                }
            )
            .await
            .expect_err("plain Records cannot start a command"),
        "Choose a Command Record"
    );
}

#[tokio::test]
async fn unicode_scripts_and_incomplete_journal_tails_remain_readable() {
    let fixture = Fixture::new();
    let script = format!(": '{}'", "🦊".repeat(9000));
    let run = fixture.start(&script);
    fixture.finish(&run).await;
    let path = journal::path(fixture.root(), &fixture.command, &run.id).expect("journal path");
    assert_eq!(
        journal::summary(&path).expect("saved summary").script,
        script
    );
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open saved journal");
    file.write_all(b"{\"event\":\"output\"")
        .expect("simulate an incomplete journal event");
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

#[tokio::test]
async fn real_run_notifications_resume_saved_output_and_history_without_polling() {
    let fixture = Fixture::new();
    let mut history = fixture
        .host
        .observe(&fixture.command, None)
        .expect("history observer");
    let run = fixture.start(
        "printf 'ready\\n'; read -r first; printf 'seen:%s\\n' \"$first\"; read -r second; exit 7",
    );
    let deadline = Duration::from_secs(3);
    assert!(
        timeout(deadline, history.changed())
            .await
            .expect("start notification")
    );
    let mut changes = fixture
        .host
        .observe(&fixture.command, Some(&run.id))
        .expect("output observer");
    let mut offset = 0;
    let mut bytes = Vec::new();
    for expected in ["ready", "seen:changed"] {
        loop {
            let (page, next_offset, _) = fixture.page(&run, offset);
            bytes.extend(output_frames(&page).flatten());
            offset = next_offset;
            if String::from_utf8_lossy(&bytes).contains(expected) {
                break;
            }
            assert!(
                timeout(deadline, changes.changed())
                    .await
                    .expect("output notification")
            );
        }
        if expected == "ready" {
            fixture.input(&run, b"changed\n");
        }
    }
    assert!(
        timeout(Duration::from_millis(20), history.changed())
            .await
            .is_err()
    );
    fixture.resize(&run, 100, 30);
    assert!(
        timeout(deadline, changes.changed())
            .await
            .expect("resize notification")
    );
    fixture.input(&run, b"done\n");
    assert!(
        timeout(deadline, history.changed())
            .await
            .expect("exit notification")
    );
    let events = fixture.finish(&run).await;
    assert!(String::from_utf8_lossy(&output(&events)).contains("seen:changed"));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Finished {
            exit_code: Some(7),
            ..
        }
    )));
}
