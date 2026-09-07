use {
    fiote::{
        CubSpec, Supervisor,
        binary::{FioteBinary, Origin},
    },
    std::{path::PathBuf, time::Duration},
};

fn echo_binary() -> Option<FioteBinary> {
    for candidate in ["/bin/cat", "/usr/bin/cat"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Some(FioteBinary {
                path,
                origin: Origin::Path,
            });
        }
    }
    None
}

#[tokio::test]
async fn the_supervisor_spawns_pipes_a_line_in_and_out_then_stops_the_cub() {
    let Some(binary) = echo_binary() else {
        return;
    };

    let supervisor = Supervisor::new(binary);
    let spec = CubSpec::new("lince-test", std::env::temp_dir());
    let cub = supervisor.spawn(spec).expect("cub spawns");

    let (_backlog, mut events) = cub.attach();
    cub.send(&serde_json::json!({ "type": "ping", "id": "probe" }))
        .await
        .expect("command is accepted");

    let echoed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("event stream stays open");
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&event.line)
                && value["id"] == "probe"
            {
                return value;
            }
        }
    })
    .await
    .expect("the line comes back");

    assert_eq!(echoed["type"], "ping");

    cub.stop();
    cub.wait().await;
    assert!(!cub.is_running());
}
