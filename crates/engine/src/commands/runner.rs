use std::time::Duration;

use nucleus::command::{Command, CommandResponse};
use tokio::io::{AsyncRead, AsyncReadExt};

async fn output(reader: impl AsyncRead + Unpin) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take(65_537)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() > 65_536 {
        return Err("Command output exceeded 64 KiB per stream".into());
    }
    String::from_utf8(bytes).map_err(|_| "Command output is not valid UTF-8".into())
}

pub(super) async fn run(configuration: &Command) -> CommandResponse {
    run_with_timeout(configuration, Duration::from_secs(30)).await
}

async fn run_with_timeout(configuration: &Command, timeout: Duration) -> CommandResponse {
    let mut command = match configuration {
        Command::Shell { script } => {
            let mut command = tokio::process::Command::new("sh");
            command.arg("-c").arg(script);
            command
        }
        Command::Process { program, arguments } => {
            let mut command = tokio::process::Command::new(program);
            command.args(arguments);
            command
        }
    };
    command
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(target_os = "linux")]
    command.process_group(0);
    let failure = |stderr: String| CommandResponse {
        command: String::new(),
        ok: false,
        stdout: String::new(),
        stderr,
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return failure(error.to_string()),
    };
    #[cfg(target_os = "linux")]
    let _group = CommandGroup(
        child
            .id()
            .and_then(|pid| rustix::process::Pid::from_raw(pid as i32)),
    );
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    match tokio::time::timeout(timeout, async {
        tokio::try_join!(output(stdout), output(stderr), async {
            child.wait().await.map_err(|error| error.to_string())
        })
    })
    .await
    {
        Ok(Ok((stdout, stderr, status))) => CommandResponse {
            command: String::new(),
            ok: status.success(),
            stdout,
            stderr: if status.success() || !stderr.is_empty() {
                stderr
            } else {
                format!("Command exited with {status}")
            },
        },
        Ok(Err(error)) => {
            let _ = child.kill().await;
            failure(error)
        }
        Err(_) => {
            let _ = child.kill().await;
            failure(format!("Command exceeded {} ms", timeout.as_millis()))
        }
    }
}

#[cfg(target_os = "linux")]
struct CommandGroup(Option<rustix::process::Pid>);

#[cfg(target_os = "linux")]
impl Drop for CommandGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streams_exit_utf8_timeout_and_size_are_checked() {
        let shell = |script: &str| Command::Shell {
            script: script.into(),
        };
        assert_eq!(
            run(&shell("printf '42'; printf diagnostic >&2"))
                .await
                .stdout,
            "42"
        );
        assert!(!run(&shell("exit 7")).await.ok);
        assert!(run(&shell("printf '\\377'")).await.stderr.contains("UTF-8"));
        assert!(
            run_with_timeout(&shell("sleep 30"), Duration::from_millis(20))
                .await
                .stderr
                .contains("exceeded")
        );
        assert!(run(&shell("yes x")).await.stderr.contains("64 KiB"));
        let response = run(&Command::Process {
            program: "printf".into(),
            arguments: vec!["%s".into(), "$(exit 9)".into()],
        })
        .await;
        assert!(response.ok);
        assert_eq!(response.stdout, "$(exit 9)");
    }
}
