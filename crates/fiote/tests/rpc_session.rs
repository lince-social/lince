use {
    fiote::{CubSpec, Supervisor, locate},
    std::{path::PathBuf, time::Duration},
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[tokio::test]
async fn a_cub_answers_get_state_over_rpc() {
    let root = workspace_root();
    let binary = match locate(&root) {
        Ok(binary) => binary,
        Err(reason) => {
            if std::env::var_os("FIOTE_SKIP_PI_TESTS").is_some() {
                return;
            }
            panic!(
                "{reason}\nInstall it with `npm install --prefix vendor/pi \
                 @earendil-works/pi-coding-agent`, or set FIOTE_SKIP_PI_TESTS=1 \
                 to skip the cubs on this machine."
            );
        }
    };

    let supervisor = Supervisor::new(binary);
    let mut spec = CubSpec::new("lince-test", &root);
    spec.persist_session = false;
    spec.context_files = false;
    let cub = supervisor.spawn(spec).expect("cub spawns");

    let (_backlog, mut events) = cub.attach();
    cub.send(&serde_json::json!({ "type": "get_state", "id": "probe" }))
        .await
        .expect("command is accepted");

    let answered = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let event = events.recv().await.expect("event stream stays open");
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&event.line) else {
                continue;
            };
            if value["type"] == "response" && value["id"] == "probe" {
                return value;
            }
        }
    })
    .await
    .expect("pi answers within a minute");

    assert_eq!(answered["command"], "get_state");
    assert_eq!(answered["success"], true);
    assert!(answered["data"]["sessionId"].is_string());

    cub.stop();
    cub.wait().await;
    assert!(!cub.is_running());
}
