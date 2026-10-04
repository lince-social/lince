use engine::{
    Engine,
    actions::Action,
    workspace_sync::{Change, Client, Command, Element, Layout, Request},
};
use nucleus::{
    canvas::{Component, Geometry},
    component::ComponentState,
};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    time::{Duration, Instant},
};
use store::sqlx::Connection as _;

async fn act(engine: &Engine, command: Command) -> Value {
    engine
        .act(
            Action::Workspace {
                request: Request {
                    client: Client::default(),
                    command,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "Run by the process crash regression"]
async fn workspace_crash_child() {
    let directory = std::env::var("LINCE_WORKSPACE_CRASH_DIRECTORY").unwrap();
    let engine = Engine::open(&format!("sqlite://{directory}/host.sqlite"))
        .await
        .unwrap();
    let command =
        serde_json::from_slice(&std::fs::read(format!("{directory}/request.json")).unwrap())
            .unwrap();
    std::fs::write(format!("{directory}/ready"), b"ready").unwrap();
    act(&engine, command).await;
}

#[tokio::test]
async fn killed_host_rolls_back_consequences_and_retries_the_durable_review_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("host.sqlite");
    let url = format!("sqlite://{}", path.display());
    let engine = Engine::open(&url).await.unwrap();
    let mut layout = Layout::default();
    let mut records = vec![];
    for index in 0..128 {
        let record = store::records::create(
            &engine.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Crash {index}"),
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap()
        .uid;
        records.push(record.clone());
        layout.elements.push(Element {
            id: nucleus::new_uid("placement"),
            component: Component::Builtin {
                state: ComponentState::Record {
                    record,
                    mode: Default::default(),
                    start_call: None,
                },
            },
            geometry: Geometry {
                position: [10000.0, 0.0],
                size: [20.0; 2],
            },
        });
    }
    let area = nucleus::new_uid("placement");
    layout.elements.push(Element {
        id: area.clone(),
        component: Component::Builtin {
            state: ComponentState::Area {
                immunity: Default::default(),
                strength: 0,
            },
        },
        geometry: Geometry {
            position: [0.0; 2],
            size: [1000.0; 2],
        },
    });
    layout.areas.insert(
        area.clone(),
        engine::area_transition::RecordChanges {
            quantity: Some("+=1".into()),
            ..Default::default()
        },
    );
    let workspace = engine.act(Action::Workspace { request: Request { client: Client::default(), command: Command::Publish { name: "Crash host".into(), policy: json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[{"operation":"update","selector":{"kind_eq":"plain"},"properties":["quantity"],"assertions_add":[],"assertions_remove":[]}]}}), layout: serde_json::to_value(layout).unwrap() } } }, None).await.unwrap().created.unwrap();
    let pending = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision: 1,
            change: Change::Move {
                element: area,
                position: [10000.0, 0.0],
            },
        },
    )
    .await;
    let command = Command::Review {
        workspace: workspace.clone(),
        proposal: pending["proposal"].as_str().unwrap().into(),
        request_id: nucleus::new_uid("request"),
        expected_revision: 1,
        approve: true,
    };
    std::fs::write(
        directory.path().join("request.json"),
        serde_json::to_vec(&command).unwrap(),
    )
    .unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "workspace_crash_child",
            "--nocapture",
        ])
        .env("LINCE_WORKSPACE_CRASH_DIRECTORY", directory.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut monitor = store::sqlx::SqliteConnection::connect_with(
        &store::sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .busy_timeout(Duration::ZERO),
    )
    .await
    .unwrap();
    let start = Instant::now();
    let mut killed_in_transaction = false;
    while start.elapsed() < Duration::from_secs(30) {
        if directory.path().join("ready").exists() {
            match store::sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut monitor)
                .await
            {
                Ok(_) => {
                    store::sqlx::query("ROLLBACK")
                        .execute(&mut monitor)
                        .await
                        .unwrap();
                }
                Err(error) if error.to_string().contains("locked") => {
                    child.kill().unwrap();
                    killed_in_transaction = true;
                    break;
                }
                Err(error) => panic!("Unexpected monitor error: {error}"),
            }
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    if !killed_in_transaction {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        killed_in_transaction,
        "Did not observe the host transaction: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    drop(monitor);
    let recovered = Engine::open(&url).await.unwrap();
    let mut before = vec![];
    for record in &records {
        before.push(
            store::records::get(&recovered.store.pool, record)
                .await
                .unwrap()
                .unwrap()
                .quantity,
        );
    }
    assert!(
        before.iter().all(|value| value == &before[0]),
        "No partial consequence batch may survive"
    );
    let result = act(&recovered, command.clone()).await;
    assert_eq!(result["state"], "applied");
    assert_eq!(act(&recovered, command).await, result);
    for record in &records {
        assert_eq!(
            store::records::get(&recovered.store.pool, record)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .to_string(),
            "1"
        );
    }
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid IN (SELECT uid FROM record WHERE kind='plain' AND head LIKE 'Crash %')").fetch_one(&recovered.store.pool).await.unwrap();
    assert_eq!(count, 128);
    println!(
        "Crash recovery: killed during a write transaction; {} Records recovered and the durable review replayed once; {} Area facts",
        records.len(),
        count
    );
}
