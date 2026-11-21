// non-lean-yet
// Scope: const OUTPUT_LIMIT, const MAX_TIMEOUT_MS, struct RunCommand, struct Arguments.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 64.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 115.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 24.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 19.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 19, 24.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites new: String
//   (24).

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

use super::{Tool, changes::GitSnapshot};
use crate::provider::ToolDefinition;

const OUTPUT_LIMIT: usize = 32 * 1024;
const MAX_TIMEOUT_MS: u64 = 21_600_000;

pub struct RunCommand {
    directory: PathBuf,
}

impl RunCommand {
    pub fn new(directory: &Path) -> Result<Self, String> {
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        if !directory.is_dir() {
            return Err("The working directory must be an existing directory.".into());
        }
        Ok(Self { directory })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    command: String,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

fn default_timeout() -> u64 {
    120_000
}

#[cfg(unix)]
struct ProcessGroup(rustix::process::Pid);

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let _ = rustix::process::kill_process_group(self.0, rustix::process::Signal::KILL);
    }
}

async fn drain(mut reader: impl AsyncRead + Unpin, output: &mut Vec<u8>) -> std::io::Result<bool> {
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(truncated);
        }
        let retained = read.min(OUTPUT_LIMIT.saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
}

#[async_trait]
impl Tool for RunCommand {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "run_command".into(),
            description: "Execute a shell command in the configured working directory. Supports pipelines, sed, git, tests and other installed command-line tools. Runs with Lince's operating-system permissions, not a filesystem sandbox. Standard input is closed. Returns exit code, stdout and stderr (up to 32 KiB each). timeout_ms defaults to 120000 and must be between 1 and 21600000. Commands may modify files; use only for the user's requested work.".into(),
            schema: json!({"type":"object","properties":{"command":{"type":"string"},"timeout_ms":{"type":"integer","minimum":1,"maximum":MAX_TIMEOUT_MS}},"required":["command"],"additionalProperties":false}),
        }
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        let args: Arguments = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
        if args.command.trim().is_empty()
            || args.command.len() > 65536
            || args.command.contains('\0')
        {
            return Err("Provide a nonempty command of at most 64 KiB without NUL bytes.".into());
        }
        if !(1..=MAX_TIMEOUT_MS).contains(&args.timeout_ms) {
            return Err("timeout_ms must be between 1 and 21600000.".into());
        }
        let before = GitSnapshot::capture(&self.directory).await;
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/S", "/C", &args.command]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", &args.command]);
            command
        };
        command
            .current_dir(&self.directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start command: {e}"))?;
        #[cfg(unix)]
        let group = ProcessGroup(
            rustix::process::Pid::from_raw(
                child.id().ok_or("Command has no process identifier.")? as i32
            )
            .ok_or("Invalid command process identifier.")?,
        );
        let stdout = child
            .stdout
            .take()
            .ok_or("Command stdout is unavailable.")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Command stderr is unavailable.")?;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let result = tokio::time::timeout(Duration::from_millis(args.timeout_ms), async {
            let (status, stdout, stderr) = tokio::join!(
                child.wait(),
                drain(stdout, &mut out),
                drain(stderr, &mut err)
            );
            Ok::<_, std::io::Error>((status?, stdout?, stderr?))
        })
        .await;
        let (exit_code, timed_out, stdout_truncated, stderr_truncated) = match result {
            Ok(result) => {
                #[cfg(unix)]
                drop(group);
                let (status, stdout, stderr) = result.map_err(|e| e.to_string())?;
                (status.code(), false, stdout, stderr)
            }
            Err(_) => {
                #[cfg(unix)]
                drop(group);
                child.kill().await.map_err(|e| e.to_string())?;
                child.wait().await.map_err(|e| e.to_string())?;
                (
                    None,
                    true,
                    out.len() == OUTPUT_LIMIT,
                    err.len() == OUTPUT_LIMIT,
                )
            }
        };
        let diff = if let Some(before) = before {
            before.changes(&self.directory).await
        } else {
            None
        };
        Ok(
            json!({"working_directory":self.directory,"success":exit_code == Some(0) && !timed_out,"exit_code":exit_code,"timed_out":timed_out,"stdout":String::from_utf8_lossy(&out),"stderr":String::from_utf8_lossy(&err),"stdout_truncated":stdout_truncated,"stderr_truncated":stderr_truncated,"diff":diff}),
        )
    }
}
