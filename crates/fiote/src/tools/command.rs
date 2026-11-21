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
    io::{self, Write},
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

use super::{
    Tool,
    changes::GitSnapshot,
    receipts::{CommandCapture, MAX_STREAM_BYTES, Part},
};
use crate::provider::ToolDefinition;

const OUTPUT_LIMIT: usize = 32 * 1024;
const TAIL_BYTES: usize = 2048;
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

#[derive(Default)]
struct Stream {
    head: Vec<u8>,
    tail: Vec<u8>,
    total: usize,
    spool: Option<tempfile::NamedTempFile>,
    saved: usize,
}

impl Stream {
    fn push(&mut self, bytes: &[u8], capture: Option<&CommandCapture<'_>>) -> io::Result<()> {
        if let Some(capture) = capture
            && self.spool.is_none()
            && self.total + bytes.len() > OUTPUT_LIMIT
        {
            let mut spool = capture.start()?;
            spool.write_all(&self.head)?;
            self.saved = self.head.len();
            self.spool = Some(spool);
        }
        if let Some(spool) = &mut self.spool {
            let retain = bytes.len().min(MAX_STREAM_BYTES.saturating_sub(self.saved));
            spool.write_all(&bytes[..retain])?;
            self.saved += retain;
            self.tail.extend_from_slice(bytes);
            let excess = self.tail.len().saturating_sub(TAIL_BYTES);
            self.tail.drain(..excess);
        }
        let retained = bytes.len().min(OUTPUT_LIMIT.saturating_sub(self.head.len()));
        self.head.extend_from_slice(&bytes[..retained]);
        self.total += bytes.len();
        Ok(())
    }

    fn finish(
        &mut self,
        capture: Option<&CommandCapture<'_>>,
        part: Part,
    ) -> io::Result<()> {
        if let (Some(file), Some(capture)) = (self.spool.take(), capture) {
            capture.finish(part, file)?;
        }
        Ok(())
    }
}

async fn drain(
    mut reader: impl AsyncRead + Unpin,
    output: &mut Stream,
    capture: Option<&CommandCapture<'_>>,
) -> io::Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(());
        }
        output.push(&buffer[..read], capture)?;
    }
}

#[async_trait]
impl Tool for RunCommand {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "run_command".into(),
            description: "Execute a shell command in the configured working directory. Supports pipelines, sed, git, tests and other installed command-line tools. Runs with Lince's operating-system permissions, not a filesystem sandbox. Standard input is closed. Returns exit code, stdout and stderr. Large streams have a bounded preview and may be read from a receipt. timeout_ms defaults to 120000 and must be between 1 and 21600000. Commands may modify files; use only for the user's requested work.".into(),
            schema: json!({"type":"object","properties":{"command":{"type":"string"},"timeout_ms":{"type":"integer","minimum":1,"maximum":MAX_TIMEOUT_MS}},"required":["command"],"additionalProperties":false}),
        }
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        self.run_captured(arguments, None).await
    }
}

impl RunCommand {
    pub(crate) async fn run_captured(
        &self,
        arguments: Value,
        capture: Option<&CommandCapture<'_>>,
    ) -> Result<Value, String> {
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
        let mut out = Stream::default();
        let mut err = Stream::default();
        let result = tokio::time::timeout(Duration::from_millis(args.timeout_ms), async {
            let (status, stdout, stderr) = tokio::join!(
                child.wait(),
                drain(stdout, &mut out, capture),
                drain(stderr, &mut err, capture)
            );
            Ok::<_, std::io::Error>((status?, stdout?, stderr?))
        })
        .await;
        let (exit_code, timed_out) = match result {
            Ok(result) => {
                #[cfg(unix)]
                drop(group);
                let (status, (), ()) = result.map_err(|e| format!("Cannot capture command output: {e}"))?;
                (status.code(), false)
            }
            Err(_) => {
                #[cfg(unix)]
                drop(group);
                child.kill().await.map_err(|e| e.to_string())?;
                child.wait().await.map_err(|e| e.to_string())?;
                (None, true)
            }
        };
        let diff = if let Some(before) = before {
            before.changes(&self.directory).await
        } else {
            None
        };
        out.finish(capture, Part::Stdout).map_err(|e| format!("Cannot save command stdout: {e}"))?;
        err.finish(capture, Part::Stderr).map_err(|e| format!("Cannot save command stderr: {e}"))?;
        let mut result = json!({"working_directory":self.directory,"success":exit_code == Some(0) && !timed_out,"exit_code":exit_code,"timed_out":timed_out,"stdout":String::from_utf8_lossy(&out.head),"stderr":String::from_utf8_lossy(&err.head),"stdout_truncated":out.total > out.head.len(),"stderr_truncated":err.total > err.head.len(),"diff":diff});
        if let Some(capture) = capture && (out.saved != 0 || err.saved != 0) {
            result["command_receipt"] = json!({
                "receipt_id": capture.id(),
                "stdout_saved_bytes": out.saved,
                "stdout_total_bytes": out.total,
                "stderr_saved_bytes": err.saved,
                "stderr_total_bytes": err.total,
            });
            result["stdout_tail"] = json!(String::from_utf8_lossy(&out.tail));
            result["stderr_tail"] = json!(String::from_utf8_lossy(&err.tail));
        }
        Ok(result)
    }
}
