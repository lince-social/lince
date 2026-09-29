use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use engine::actions::Action;
use serde_json::{Value, json};

struct Peer {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Value>,
}

impl Peer {
    fn start(directory: &Path) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "peer_process", "--nocapture"])
            .env("LINCE_PAIRING_TEST_DIRECTORY", directory)
            .env("LINCE_DISCOVERY_INTERNET", "0")
            .env("RUST_MIN_STACK", "33554432")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some((_, data)) = line.split_once("LINCE_PAIRING_REPLY ") {
                    if send.send(serde_json::from_str(data).unwrap()).is_err() {
                        break;
                    }
                }
            }
        });
        let mut peer = Self {
            child,
            input,
            output,
        };
        let ready = peer.reply();
        assert_eq!(ready["ok"], true, "{ready}");
        peer
    }

    fn reply(&mut self) -> Value {
        self.output
            .recv_timeout(Duration::from_secs(50))
            .expect("Cell did not reply")
    }

    fn request(&mut self, request: Value) -> Value {
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        self.reply()
    }

    fn ok(&mut self, request: Value) -> Value {
        let response = self.request(request);
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }

    fn act(&mut self, action: Action) -> Value {
        self.ok(json!({"op":"action", "action":action}))
    }

    fn status(&mut self) -> Value {
        self.ok(json!({"op":"status"}))
    }
    fn record(&mut self, uid: &str) -> Value {
        self.ok(json!({"op":"record", "uid":uid}))
    }
    fn sync(&mut self) {
        let response = self.request(json!({"op":"sync"}));
        if response["ok"] != true {
            eprintln!("Sync retry: {}", response["error"]);
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn response(value: Value) {
    println!("LINCE_PAIRING_REPLY {value}");
    std::io::stdout().flush().unwrap();
}

#[test]
fn peer_process() {
    let Some(directory) = std::env::var_os("LINCE_PAIRING_TEST_DIRECTORY") else {
        return;
    };
    utils::logging::init().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let cell = cell::Cell::open_mobile(cell::CellOptions {
            data_dir: Some(directory.into()),
            peer_port: Some(
                std::env::var("LINCE_PAIRING_TEST_PORT")
                    .map(|port| port.parse().unwrap())
                    .unwrap_or(0),
            ),
            ..Default::default()
        })
        .await
        .unwrap();
        response(json!({"ok":true}));
        for line in std::io::stdin().lock().lines() {
            let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let result =
                tokio::time::timeout(Duration::from_secs(35), handle(&cell, request)).await;
            response(match result {
                Ok(Ok(data)) => json!({"ok":true, "data":data}),
                Ok(Err(error)) => json!({"ok":false, "error":error}),
                Err(_) => json!({"ok":false, "error":"operation timed out"}),
            });
        }
        cell.shutdown().await;
    });
}

async fn handle(cell: &cell::Cell, request: Value) -> Result<Value, String> {
    let engine = cell.engine();
    match request["op"].as_str().unwrap() {
        "action" => {
            let action: Action =
                serde_json::from_value(request["action"].clone()).map_err(|e| e.to_string())?;
            let outcome = engine.act(action, None).await.map_err(|e| e.to_string())?;
            Ok(json!({"created":outcome.created,"data":outcome.data}))
        }
        "status" => {
            let organ = store::organs::local(&engine.store.pool)
                .await
                .map_err(|e| e.to_string())?
                .unwrap();
            let device = store::cells::local(&engine.store.pool)
                .await
                .map_err(|e| e.to_string())?
                .unwrap();
            let roster = engine
                .roster_of(&organ.uid)
                .await
                .map_err(|e| e.to_string())?;
            let wire = cell.runtime().wire.read().await.clone();
            let nearby: Vec<_> = engine
                .nearby_peers()
                .into_iter()
                .map(|peer| json!({"node_id":peer.node_id,"name":peer.name}))
                .collect();
            let contacts = store::organs::contacts(&engine.store.pool)
                .await
                .map_err(|e| e.to_string())?
                .len();
            Ok(
                json!({"organ":organ.uid,"cell":device.uid,"node":wire.as_ref().map(|wire| wire.node_id().to_string()),
                "roster":roster,"may_enrol":engine.may_enrol().await.is_ok(),
                "nearby":nearby,"contacts":contacts,"local":wire.as_ref().is_some_and(|wire| wire.reach() == engine::wire::Reach::Local),
                "discovery":wire.as_ref().is_some_and(|wire| wire.local_discovery()),
                "network":wire.as_ref().map(|wire| wire.network_status())}),
            )
        }
        "record" => {
            let row = store::records::get(&engine.store.pool, request["uid"].as_str().unwrap())
                .await
                .map_err(|e| e.to_string())?;
            Ok(row.map(|row| json!({"uid":row.uid,"head":row.head,"body":row.body,"quantity":row.quantity.to_string()})).unwrap_or(Value::Null))
        }
        "sync" => engine
            .sync_now()
            .await
            .map(|count| json!(count))
            .map_err(|e| e.to_string()),
        _ => Err("Unknown test operation".into()),
    }
}

fn discovery(peer: &mut Peer) {
    peer.act(Action::SetCellConfig {
        namespace: "lince.discovery".into(),
        fds: json!({"local":true,"internet":false,"direct":false}),
    });
}

fn quantity(uid: &str, value: &str) -> Action {
    Action::ChangeRecord {
        request: engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: uid.into(),
            mutation: engine::record_change::Mutation::Quantity {
                value: value.into(),
            },
        },
    }
}

#[track_caller]
fn wait_for(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(35);
    while !check() {
        assert!(
            Instant::now() < deadline,
            "Cells did not converge before the deadline"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn fresh_cells_discover_enrol_sync_restart_and_revoke_over_lan() {
    let directory = tempfile::tempdir().unwrap();
    let laptop_dir = directory.path().join("laptop");
    let phone_dir = directory.path().join("phone");
    let mut laptop = Peer::start(&laptop_dir);
    let mut phone = Peer::start(&phone_dir);
    assert!(laptop.status()["roster"].is_null());
    assert_eq!(phone.status()["may_enrol"], true);
    discovery(&mut laptop);
    discovery(&mut phone);
    wait_for(|| {
        let a = laptop.status();
        let b = phone.status();
        a["local"] == true
            && b["local"] == true
            && a["nearby"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["node_id"] == b["node"])
            && b["nearby"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["node_id"] == a["node"])
    });
    laptop.act(Action::RosterCreateOrgan);
    let invite = laptop.act(Action::RosterEnrolToken)["data"]["code"]
        .as_str()
        .unwrap()
        .to_owned();
    phone.act(Action::RosterJoinOrgan {
        code: invite.clone(),
    });
    let a = laptop.status();
    let b = phone.status();
    assert_eq!(a["organ"], b["organ"]);
    assert_ne!(a["cell"], b["cell"]);
    assert_eq!(a["roster"]["roster"]["cells"].as_array().unwrap().len(), 2);
    assert_eq!(a["roster"], b["roster"]);
    assert_eq!(a["contacts"], 0);
    assert_eq!(b["contacts"], 0);
    let members = a["roster"]["roster"]["cells"].as_array().unwrap();
    assert_ne!(members[0]["operational_key"], members[1]["operational_key"]);
    assert_eq!(
        phone.request(json!({"op":"action","action":Action::RosterJoinOrgan{code:invite}}))["ok"],
        false
    );
    let phone_cell = b["cell"].as_str().unwrap().to_owned();
    laptop.act(Action::RosterRenameCell {
        cell_uid: phone_cell.clone(),
        label: "Test phone".into(),
    });
    wait_for(|| {
        phone.sync();
        phone.status()["roster"]["roster"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cell| cell["label"] == "Test phone")
    });
    assert_eq!(phone.request(json!({"op":"action","action":Action::RosterRenameCell{cell_uid:phone_cell.clone(),label:"Unauthorized".into()}}))["ok"], false);
    let draft = engine::record_creation::Draft {
        head: "LAN test task".into(),
        quantity: "-2".into(),
        ..Default::default()
    };
    let uid = draft.uid.clone();
    laptop.act(Action::CreateRecordDraft { draft });
    wait_for(|| {
        phone.sync();
        phone.record(&uid)["quantity"] == "-2"
    });
    let complete = quantity(&uid, "0");
    let started = Instant::now();
    phone.act(complete.clone());
    phone.act(complete);
    wait_for(|| laptop.record(&uid)["quantity"] == "0");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "a local edit waited for the idle poll"
    );
    let laptop_ports = laptop.status()["network"]["listening"].clone();
    let phone_ports = phone.status()["network"]["listening"].clone();
    for peer in [&mut laptop, &mut phone] {
        peer.act(Action::SetCellConfig {
            namespace: "lince.discovery".into(),
            fds: json!({"local": false, "internet": false, "direct": false}),
        });
    }
    wait_for(|| laptop.status()["discovery"] == false && phone.status()["discovery"] == false);
    assert_eq!(laptop.status()["network"]["listening"], laptop_ports);
    assert_eq!(phone.status()["network"]["listening"], phone_ports);
    assert!(laptop.status()["nearby"].as_array().unwrap().is_empty());
    assert!(phone.status()["nearby"].as_array().unwrap().is_empty());
    let started = Instant::now();
    laptop.act(quantity(&uid, "-3"));
    wait_for(|| phone.record(&uid)["quantity"] == "-3");
    assert!(started.elapsed() < Duration::from_secs(10));
    let started = Instant::now();
    phone.act(quantity(&uid, "0"));
    wait_for(|| laptop.record(&uid)["quantity"] == "0");
    assert!(started.elapsed() < Duration::from_secs(10));
    discovery(&mut laptop);
    discovery(&mut phone);
    wait_for(|| laptop.status()["discovery"] == true && phone.status()["discovery"] == true);
    laptop.act(Action::EditRecordText {
        target: uid.clone(),
        head: Some("Laptop title".into()),
        body: None,
    });
    phone.act(Action::EditRecordText {
        target: uid.clone(),
        head: None,
        body: Some("Texto do telefone — ação".into()),
    });
    wait_for(|| {
        laptop.sync();
        phone.sync();
        laptop.record(&uid) == phone.record(&uid)
    });
    assert_eq!(phone.record(&uid)["head"], "Laptop title");
    assert_eq!(phone.record(&uid)["body"], "Texto do telefone — ação");
    wait_for(|| laptop.ok(json!({"op":"sync"})) == 0 && phone.ok(json!({"op":"sync"})) == 0);
    drop(laptop);
    phone.act(quantity(&uid, "-2"));
    let mut laptop = Peer::start(&laptop_dir);
    assert_eq!(laptop.status()["organ"], a["organ"]);
    wait_for(|| {
        laptop.sync();
        laptop.record(&uid)["quantity"] == "-2"
    });
    let phone_draft = engine::record_creation::Draft {
        head: "Created on phone".into(),
        ..Default::default()
    };
    let phone_record = phone_draft.uid.clone();
    phone.act(Action::CreateRecordDraft { draft: phone_draft });
    wait_for(|| laptop.record(&phone_record)["head"] == "Created on phone");
    drop(phone);
    laptop.act(quantity(&uid, "-1"));
    let mut phone = Peer::start(&phone_dir);
    assert_eq!(phone.status()["organ"], a["organ"]);
    wait_for(|| {
        phone.sync();
        phone.record(&uid)["quantity"] == "-1"
    });
    phone.act(Action::DeleteRecord {
        target: uid.clone(),
    });
    wait_for(|| {
        laptop.sync();
        laptop.record(&uid).is_null()
    });
    laptop.act(Action::RosterRevokeCell {
        cell_uid: phone_cell.clone(),
    });
    assert!(
        !laptop.status()["roster"]["roster"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cell| cell["cell_uid"] == phone_cell)
    );
    let draft = engine::record_creation::Draft {
        head: "After revocation".into(),
        ..Default::default()
    };
    let private = draft.uid.clone();
    laptop.act(Action::CreateRecordDraft { draft });
    let _ = phone.request(json!({"op":"sync"}));
    assert!(phone.record(&private).is_null());
}
