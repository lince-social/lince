// non-lean-yet
// Scope: fn output, fn run, fn run_with_timeout, struct CommandGroup.
// BeautifulRust (baseline lines):
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 31, 110.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 11, 96.
// - [Clippy probe: clippy::expect_used] Check the stated invariant and handle reachable failures.
//   Sites 98, 99.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 31, 110.

use std::time::Duration;

use nucleus::command::{Command, CommandResponse};
use tokio::io::{AsyncRead, AsyncReadExt};

async fn output(mut reader: impl AsyncRead + Unpin, unlimited: bool) -> Result<String, String> {
    const OUTPUT_LIMIT: usize = 65_536;
    const TRUNCATED: &str = "\n[Output truncated]\n";
    let mut bytes = Vec::new();
    (&mut reader)
        .take((OUTPUT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    let truncated = bytes.len() > OUTPUT_LIMIT;
    if truncated {
        if !unlimited {
            return Err("Command output exceeded 64 KiB per stream".into());
        }
        tokio::io::copy(&mut reader, &mut tokio::io::sink())
            .await
            .map_err(|error| error.to_string())?;
        bytes.truncate(OUTPUT_LIMIT - TRUNCATED.len());
        if let Err(error) = std::str::from_utf8(&bytes)
            && error.error_len().is_none()
        {
            bytes.truncate(error.valid_up_to());
        }
    }
    let mut text =
        String::from_utf8(bytes).map_err(|_| "Command output is not valid UTF-8".to_owned())?;
    if truncated {
        text.push_str(TRUNCATED);
    }
    Ok(text)
}

pub(super) async fn run(
    configuration: &Command,
    directory: Option<&std::path::Path>,
    unlimited: bool,
) -> CommandResponse {
    run_with_timeout(
        configuration,
        (!unlimited).then_some(Duration::from_secs(30)),
        directory,
    )
    .await
}

async fn run_with_timeout(
    configuration: &Command,
    timeout: Option<Duration>,
    directory: Option<&std::path::Path>,
) -> CommandResponse {
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
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    if timeout.is_none() {
        command.env("LINCE_COMMAND_UNLIMITED", "1");
    } else {
        command.env_remove("LINCE_COMMAND_UNLIMITED");
    }
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
    let collect = async {
        tokio::try_join!(
            output(stdout, timeout.is_none()),
            output(stderr, timeout.is_none()),
            async { child.wait().await.map_err(|error| error.to_string()) },
        )
    };
    let result = match timeout {
        Some(timeout) => tokio::time::timeout(timeout, collect)
            .await
            .map_err(|_| format!("Command exceeded {} ms", timeout.as_millis()))
            .and_then(|result| result),
        None => collect.await,
    };
    match result {
        Ok((stdout, stderr, status)) => CommandResponse {
            command: String::new(),
            ok: status.success(),
            stdout,
            stderr: if status.success() || !stderr.is_empty() {
                stderr
            } else {
                format!("Command exited with {status}")
            },
        },
        Err(error) => {
            let _ = child.kill().await;
            failure(error)
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
            run(&shell("printf '42'; printf diagnostic >&2"), None, false)
                .await
                .stdout,
            "42"
        );
        assert!(!run(&shell("exit 7"), None, false).await.ok);
        assert!(
            run(&shell("printf '\\377'"), None, false)
                .await
                .stderr
                .contains("UTF-8")
        );
        assert!(
            run_with_timeout(&shell("sleep 30"), Some(Duration::from_millis(20)), None)
                .await
                .stderr
                .contains("exceeded")
        );
        assert!(run(&shell("yes x"), None, false).await.stderr.contains("64 KiB"));
        let response = run(
            &Command::Process {
                program: "printf".into(),
                arguments: vec!["%s".into(), "$(exit 9)".into()],
            },
            None,
            false,
        )
        .await;
        assert!(response.ok);
        assert_eq!(response.stdout, "$(exit 9)");
    }

    #[tokio::test]
    async fn configured_directory_is_local_to_the_child() {
        let previous = std::env::current_dir().unwrap();
        let directory = std::env::temp_dir().join(nucleus::new_uid("command-test"));
        std::fs::create_dir(&directory).unwrap();
        let response = run(
            &Command::Process {
                program: "pwd".into(),
                arguments: Vec::new(),
            },
            Some(&directory),
            false,
        )
        .await;
        assert!(response.ok);
        assert_eq!(
            response.stdout.trim(),
            directory.canonicalize().unwrap().to_str().unwrap()
        );
        assert_eq!(std::env::current_dir().unwrap(), previous);
        std::fs::remove_dir(directory).unwrap();
    }

    #[tokio::test]
    async fn unlimited_output_is_drained_after_the_receipt_limit() {
        let response = run(
            &Command::Shell {
                script: "yes x | head -c 70000; printf finished >&2".into(),
            },
            None,
            true,
        )
        .await;
        assert!(response.ok);
        assert_eq!(response.stderr, "finished");
        assert_eq!(response.stdout.len(), 65_536);
        assert!(response.stdout.ends_with("[Output truncated]\n"));
    }
}
