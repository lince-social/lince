use engine::Engine;
use nucleus::{
    RecordKind,
    plugins::{Binding, Command, Direction},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::{Mutex, broadcast},
};

struct Server {
    url: String,
    connections: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    messages: broadcast::Sender<String>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("irc://{}", listener.local_addr().unwrap());
        let connections = Arc::new(AtomicUsize::new(0));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let (messages, _) = broadcast::channel::<String>(32);
        let accepted = connections.clone();
        let captured = commands.clone();
        let sent = messages.clone();
        let task = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                accepted.fetch_add(1, Ordering::SeqCst);
                let captured = captured.clone();
                let mut messages = sent.subscribe();
                tokio::spawn(async move {
                    let (read, mut write) = socket.into_split();
                    let mut lines = BufReader::new(read).lines();
                    let mut nick = String::new();
                    loop {
                        tokio::select! {
                            result = lines.next_line() => {
                                let Ok(Some(line)) = result else { break; };
                                captured.lock().await.push(line.clone());
                                let response = if let Some(value) = line.strip_prefix("NICK ") {
                                    nick = value.into(); String::new()
                                } else if line.starts_with("CAP LS") { ":server CAP * LS :server-time\r\n".into() }
                                else if line.starts_with("CAP REQ") { ":server CAP * ACK :server-time\r\n".into() }
                                else if line == "CAP END" { format!(":server 001 {nick} :Welcome\r\n") }
                                else if let Some(channel) = line.strip_prefix("JOIN ") {
                                    format!(":{nick}!local@server JOIN :{channel}\r\n@time=2026-10-13T12:00:00Z;msgid=initial-{channel} :alice!remote@server PRIVMSG {channel} :Hello from IRC\r\nPING :keepalive\r\n")
                                } else { String::new() };
                                if write.write_all(response.as_bytes()).await.is_err() { break; }
                            }
                            message = messages.recv() => {
                                let Ok(message) = message else { break; };
                                if write.write_all(message.as_bytes()).await.is_err() { break; }
                            }
                        }
                    }
                });
            }
        });
        Self {
            url,
            connections,
            commands,
            messages,
            task,
        }
    }
}

async fn install(directory: &Path) {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-irc"))
        .arg("--install")
        .arg(directory)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn enable(engine: &Engine) {
    let package = engine.plugin_catalog().await.unwrap().packages.remove(0);
    engine
        .plugin_command(
            Command::Enable {
                plugin: package.manifest.id,
                digest: package.digest,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
}

fn binding(server: &Server, channel: &str) -> Binding {
    Binding {
        uid: nucleus::new_uid("r"),
        provider: "lince.irc/messages".into(),
        name: channel.into(),
        settings: json!({"server":server.url,"nickname":"lince-test","channel":channel,"password_file":""}),
        protein_uid: None,
        incoming_filter: json!([]),
        direction: Direction::Incoming,
        interval_seconds: 60,
        enabled: true,
        revision: 0,
    }
}

async fn save(engine: &Engine, binding: Binding) {
    let preview = if binding.direction.outgoing() {
        Some(
            engine
                .plugin_command(
                    Command::Preview {
                        binding: binding.clone(),
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap()["preview"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
    } else {
        None
    };
    engine
        .plugin_command(Command::SaveBinding { binding, preview }, None)
        .await
        .unwrap();
}

async fn wait_for_import(engine: &Engine, uid: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if engine.run_plugin_binding(uid).await.unwrap() > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn real_plugin_keeps_one_connection_for_channels_and_sends_without_replaying() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path()).await;
    let server = Server::start().await;
    let engine = Engine::open_memory().await.unwrap();
    engine
        .load_plugins(directory.path().join("plugins"))
        .await
        .unwrap();
    enable(&engine).await;
    let incoming = binding(&server, "#one");
    save(&engine, incoming.clone()).await;
    wait_for_import(&engine, &incoming.uid).await;
    assert_eq!(engine.run_plugin_binding(&incoming.uid).await.unwrap(), 0);
    let second = binding(&server, "#two");
    save(&engine, second.clone()).await;
    wait_for_import(&engine, &second.uid).await;
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
    let mut invalid = binding(&server, "#bad");
    invalid.settings["nickname"] = json!("bad\r\nOPER root");
    assert!(
        engine
            .plugin_command(
                Command::SaveBinding {
                    binding: invalid,
                    preview: None
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(engine.run_plugin_binding(&second.uid).await.unwrap(), 0);
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
    assert!(
        server
            .commands
            .lock()
            .await
            .iter()
            .any(|command| command == "PONG :keepalive")
    );
    server
        .messages
        .send(
            "@msgid=live-message :bob!remote@server PRIVMSG #one :A message between refreshes\r\n"
                .into(),
        )
        .unwrap();
    wait_for_import(&engine, &incoming.uid).await;
    assert!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|record| record.body == "A message between refreshes")
    );
    let outgoing = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Hello from Lince",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    let selection = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Protein,
            head: "Selected",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    store::records::set_extension(&engine.store.pool, &selection.uid, "lince.protein", &json!({"source":"record","where":[{"uid_eq":outgoing.uid}],"fields":["uid","head","body"]})).await.unwrap();
    let mut both = engine
        .plugin_catalog()
        .await
        .unwrap()
        .bindings
        .into_iter()
        .find(|item| item.binding.uid == incoming.uid)
        .unwrap()
        .binding;
    both.direction = Direction::Both;
    both.protein_uid = Some(selection.uid);
    save(&engine, both).await;
    assert_eq!(engine.run_plugin_binding(&incoming.uid).await.unwrap(), 1);
    assert_eq!(engine.run_plugin_binding(&incoming.uid).await.unwrap(), 0);
    let commands = server.commands.lock().await;
    assert_eq!(
        commands
            .iter()
            .filter(|command| *command == "PRIVMSG #one :Hello from Lince")
            .count(),
        1
    );
    drop(commands);
    let delivery = engine.plugin_catalog().await.unwrap().deliveries.remove(0);
    assert_eq!(delivery.state, "submitted");
    assert!(
        delivery
            .result
            .contains("recipient delivery is not confirmed")
    );
    engine
        .plugin_command(
            Command::Stop {
                binding: incoming.uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(engine.run_plugin_binding(&incoming.uid).await.is_err());
    assert!(
        engine
            .plugin_command(
                Command::Retry {
                    delivery: delivery.uid
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn received_messages_survive_a_plugin_restart_with_independent_checkpoints() {
    let directory = tempfile::tempdir().unwrap();
    let server = Server::start().await;
    let settings =
        json!({"server":server.url,"nickname":"lince-test","channel":"#one","password_file":""});
    let runtime = lince_plugin_irc::Runtime::open(directory.path().to_path_buf())
        .await
        .unwrap();
    let mut batch = Value::Null;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            batch = runtime
                .handle("messages", "pull", json!({"settings":settings}))
                .await
                .unwrap();
            if !batch["records"].as_array().unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let cursor = batch["cursor"].clone();
    let id = batch["records"][0]["external_id"].clone();
    drop(runtime);
    let runtime = lince_plugin_irc::Runtime::open(directory.path().to_path_buf())
        .await
        .unwrap();
    let replay = runtime
        .handle("messages", "pull", json!({"settings":settings,"cursor":0}))
        .await
        .unwrap();
    assert!(
        replay["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|record| record["external_id"] == id)
    );
    let resumed = runtime
        .handle(
            "messages",
            "pull",
            json!({"settings":settings,"cursor":cursor}),
        )
        .await
        .unwrap();
    assert!(
        resumed["records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["external_id"] == id)
    );
}

#[tokio::test]
async fn commands_cannot_be_injected_through_settings_or_publication_content() {
    lince_plugin_irc::manifest("runner".into())
        .validate()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let runtime = lince_plugin_irc::Runtime::open(directory.path().to_path_buf())
        .await
        .unwrap();
    let settings = json!({"server":"ircs://irc.example.test:6697","nickname":"lince","channel":"#one","password_file":""});
    for (field, value) in [
        ("nickname", "lince\r\nOPER root password"),
        ("channel", "#one\r\nJOIN #secret"),
        ("server", "irc://irc.example.test:6667"),
    ] {
        let mut malicious = settings.clone();
        malicious[field] = json!(value);
        assert!(
            runtime
                .handle("messages", "validate", json!({"settings":malicious}))
                .await
                .is_err()
        );
    }
    for text in [
        "Hello\r\nPRIVMSG #secret :injected".to_owned(),
        "x".repeat(513),
    ] {
        assert!(runtime.handle("messages", "prepare", json!({"settings":settings,"record":{"uid":"r_test","head":text,"body":"","created_at":0}})).await.is_err());
    }
}
