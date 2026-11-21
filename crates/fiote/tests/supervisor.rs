use std::{
    env::{split_paths, temp_dir, var_os},
    path::PathBuf,
    time::Duration,
};

use fiote::{
    CubSpec, Supervisor,
    binary::{FioteBinary, Origin},
};
use serde_json::{Value, from_str, json};

fn echo_binary() -> Option<FioteBinary> {
    let search = var_os("PATH").unwrap_or_default();
    ["/bin/cat", "/usr/bin/cat"]
        .into_iter()
        .map(PathBuf::from)
        .chain(
            split_paths(&search)
                .filter(|path| path.is_absolute())
                .map(|path| path.join("cat")),
        )
        .find(|path| path.is_file())
        .map(|path| FioteBinary {
            path,
            origin: Origin::Path,
        })
}

#[tokio::test]
async fn the_supervisor_spawns_pipes_a_line_in_and_out_then_stops_the_cub() {
    let Some(binary) = echo_binary() else {
        return;
    };

    let supervisor = Supervisor::new(binary);
    let spec = CubSpec::new("lince-test", temp_dir());
    let cub = supervisor.spawn(spec).expect("cub spawns");

    let (_backlog, mut events) = cub.attach();
    cub.send(&json!({ "type": "ping", "id": "probe" }))
        .await
        .expect("command is accepted");

    let echoed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("event stream stays open");
            if let Ok(value) = from_str::<Value>(&event.line)
                && value.get("id").and_then(Value::as_str) == Some("probe")
            {
                return value;
            }
        }
    })
    .await
    .expect("the line comes back");

    assert_eq!(echoed.get("type").and_then(Value::as_str), Some("ping"));
    assert!(supervisor.forget_finished().is_empty());

    supervisor.stop(&cub.id).expect("child is registered");
    cub.wait().await;
    assert!(!cub.is_running());
    assert_eq!(supervisor.forget_finished(), [cub.id.as_str()]);
    assert!(supervisor.get(&cub.id).is_none());
}
