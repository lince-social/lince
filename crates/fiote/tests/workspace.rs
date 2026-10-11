#[cfg(unix)]
use async_trait::async_trait;
use fiote::tools::Registry;
#[cfg(unix)]
use fiote::{
    provider::{Message, Provider, Reply, ToolCall, ToolDefinition},
    runtime::run,
    tools::{RunCommand, Tool},
};
use serde_json::json;
#[cfg(unix)]
use tokio::sync::watch;

fn tools(root: &std::path::Path) -> Registry {
    let mut registry = Registry::default();
    registry.register_workspace(root).unwrap();
    registry
}

#[tokio::test]
async fn workspace_tools_read_write_edit_list_and_refuse_ambiguous_edits() {
    let root = tempfile::tempdir().unwrap();
    let registry = tools(root.path());
    assert_eq!(
        registry
            .run("write_file", json!({"path":"code.rs","content":"one one"}))
            .await["ok"],
        true
    );
    assert_eq!(
        registry
            .run(
                "edit_file",
                json!({"path":"code.rs","old_text":"one","new_text":"two"})
            )
            .await["ok"],
        false
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("code.rs")).unwrap(),
        "one one"
    );
    let edited = registry
        .run(
            "edit_file",
            json!({"path":"code.rs","old_text":"one","new_text":"two","replace_all":true}),
        )
        .await;
    assert_eq!(edited["result"]["replacements"], 2);
    assert_eq!(
        registry.run("read_file", json!({"path":"code.rs"})).await["result"]["content"],
        "two two"
    );
    assert_eq!(
        registry
            .run(
                "edit_file",
                json!({"path":"code.rs","old_text":"two two","new_text":"world"})
            )
            .await["ok"],
        true
    );
    assert_eq!(
        registry.run("list_directory", json!({"path":"."})).await["result"]["entries"][0]["name"],
        "code.rs"
    );
    assert_eq!(
        registry
            .run("create_file", json!({"path":"code.rs","content":"replace"}))
            .await["ok"],
        false
    );
}

#[tokio::test]
async fn workspace_file_tools_refuse_escape_and_limit_content_before_writing() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "outside").unwrap();
    let registry = tools(root.path());
    for path in ["../secret", "/etc/passwd", "nested/../secret"] {
        assert_eq!(
            registry.run("read_file", json!({"path":path})).await["ok"],
            false
        );
        assert_eq!(
            registry
                .run("write_file", json!({"path":path,"content":"changed"}))
                .await["ok"],
            false
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        assert_eq!(
            registry
                .run("read_file", json!({"path":"escape/secret"}))
                .await["ok"],
            false
        );
        assert_eq!(
            registry
                .run(
                    "write_file",
                    json!({"path":"escape/secret","content":"changed"})
                )
                .await["ok"],
            false
        );
        assert_eq!(
            registry
                .run(
                    "edit_file",
                    json!({"path":"escape/secret","old_text":"outside","new_text":"changed"})
                )
                .await["ok"],
            false
        );
        assert_eq!(
            registry
                .run("list_directory", json!({"path":"escape"}))
                .await["ok"],
            false
        );
    }
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret")).unwrap(),
        "outside"
    );
    assert_eq!(
        registry
            .run(
                "write_file",
                json!({"path":"large","content":"x".repeat(1_048_577)})
            )
            .await["ok"],
        false
    );
    assert!(!root.path().join("large").exists());
    std::fs::write(root.path().join("large"), "x".repeat(1_048_577)).unwrap();
    assert_eq!(
        registry.run("read_file", json!({"path":"large"})).await["ok"],
        false
    );
    std::fs::write(root.path().join("small"), "x".repeat(1024)).unwrap();
    assert_eq!(registry.run("edit_file", json!({"path":"small","old_text":"x","new_text":"x".repeat(2048),"replace_all":true})).await["ok"], false);
    assert_eq!(
        std::fs::metadata(root.path().join("small")).unwrap().len(),
        1024
    );
}

#[cfg(unix)]
struct CodingProvider;

#[cfg(unix)]
#[async_trait]
impl Provider for CodingProvider {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert_eq!(system, "Complete the requested coding task.");
        assert!(tools.iter().any(|tool| tool.name == "run_command"));
        let (name, arguments) = match messages.last().unwrap() {
            Message::User(_) => ("write_file", json!({"path":"Hello.txt","content":"hello"})),
            Message::Tool { name, result, .. } if name == "write_file" => {
                assert_eq!(result["ok"], true);
                (
                    "run_command",
                    json!({"command":"sed -i 's/hello/world/' Hello.txt"}),
                )
            }
            Message::Tool { name, result, .. } if name == "run_command" => {
                assert_eq!(result["result"]["exit_code"], 0);
                ("read_file", json!({"path":"Hello.txt"}))
            }
            Message::Tool { name, result, .. } if name == "read_file" => {
                assert_eq!(result["result"]["content"], "world");
                return Ok(Reply {
                    text: "Verified Hello.txt contains world.".into(),
                    ..Default::default()
                });
            }
            other => panic!("Unexpected message: {other:?}"),
        };
        Ok(Reply {
            calls: vec![ToolCall {
                id: format!("call-{}", messages.len()),
                name: name.into(),
                arguments,
                signatures: None,
            }],
            ..Default::default()
        })
    }
}

#[cfg(unix)]
#[tokio::test]
async fn native_agent_codes_with_sed_and_verifies_the_actual_file() {
    let root = tempfile::tempdir().unwrap();
    let (_stop, receiver) = watch::channel(false);
    let result = run(
        &CodingProvider,
        "Complete the requested coding task.",
        vec![Message::User("Create Hello.txt containing world.".into())],
        &tools(root.path()),
        receiver,
    )
    .await
    .unwrap();
    assert_eq!(result, "Verified Hello.txt contains world.");
    assert_eq!(
        std::fs::read_to_string(root.path().join("Hello.txt")).unwrap(),
        "world"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn commands_keep_working_directories_separate_and_report_failure_and_bounded_output() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    let a = RunCommand::new(first.path()).unwrap();
    let b = RunCommand::new(second.path()).unwrap();
    let (left, right) = tokio::join!(
        a.run(json!({"command":"printf first > file; pwd"})),
        b.run(json!({"command":"printf second > file; pwd"}))
    );
    assert_eq!(
        left.unwrap()["stdout"].as_str().unwrap().trim(),
        first.path().to_str().unwrap()
    );
    assert_eq!(
        right.unwrap()["stdout"].as_str().unwrap().trim(),
        second.path().to_str().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(first.path().join("file")).unwrap(),
        "first"
    );
    assert_eq!(
        std::fs::read_to_string(second.path().join("file")).unwrap(),
        "second"
    );
    assert_eq!(std::env::current_dir().unwrap(), original);
    let failed = a
        .run(json!({"command":"printf failure >&2; exit 7"}))
        .await
        .unwrap();
    assert_eq!(failed["exit_code"], 7);
    assert_eq!(failed["success"], false);
    assert_eq!(failed["stderr"], "failure");
    let output = a
        .run(json!({"command":"head -c 200000 /dev/zero | tr '\\000' x"}))
        .await
        .unwrap();
    assert_eq!(output["stdout"].as_str().unwrap().len(), 32768);
    assert_eq!(output["stdout_truncated"], true);
}

#[cfg(unix)]
#[tokio::test]
async fn command_timeout_and_cancellation_stop_descendants() {
    let root = tempfile::tempdir().unwrap();
    let command = RunCommand::new(root.path()).unwrap();
    let result = command
        .run(json!({"command":"sleep 0.3; printf late > timeout-marker","timeout_ms":30}))
        .await
        .unwrap();
    assert_eq!(result["timed_out"], true);
    let mut pending =
        Box::pin(command.run(
            json!({"command":"printf ready > started; sleep 0.3; printf late > cancel-marker"}),
        ));
    tokio::select! {
        result = &mut pending => panic!("Unexpected completion: {result:?}"),
        _ = async {
            while !root.path().join("started").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        } => {}
    }
    drop(pending);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(!root.path().join("timeout-marker").exists());
    assert!(!root.path().join("cancel-marker").exists());
}

#[cfg(unix)]
struct WaitingCommandProvider;

#[cfg(unix)]
#[async_trait]
impl Provider for WaitingCommandProvider {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        Ok(Reply {
            calls: vec![ToolCall {
                id: "waiting-shell".into(),
                name: "run_command".into(),
                arguments: json!({"command":"printf ready > started; sleep 0.3; printf late > after-stop"}),
                signatures: None,
            }],
            ..Default::default()
        })
    }
}

#[cfg(unix)]
#[tokio::test]
async fn stopping_the_agent_cancels_a_running_shell_tool() {
    let root = tempfile::tempdir().unwrap();
    let registry = tools(root.path());
    let (stop, receiver) = watch::channel(false);
    let marker = root.path().join("started");
    let cancel = async {
        while !marker.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        stop.send(true).unwrap();
    };
    let turn = run(
        &WaitingCommandProvider,
        "Run the task.",
        vec![Message::User("Run a command.".into())],
        &registry,
        receiver,
    );
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        tokio::join!(turn, cancel)
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().contains("Stopped by you"));
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(!root.path().join("after-stop").exists());
}
