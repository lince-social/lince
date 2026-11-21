// non-lean-yet
// Scope: const MAX_TOOL_BYTES, fn spawn, struct Process, fn launch.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 55.
// - [review: §3 Ownership] Check that these inputs can be borrowed without changing ownership
//   semantics. Sites run:command → &[T] (89), run:directory → &Path (90).
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 33, 72.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   119, 125.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 10, 88.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 82.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 8, 10,
//   82, 83, 84, 85, 88.

use std::{path::Path, process::Stdio, sync::OnceLock, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

use crate::Result;

pub const MAX_TOOL_BYTES: usize = 2 * 1024 * 1024;

pub fn spawn(
    future: impl Future<Output = ()> + Send + 'static,
) -> Result<tokio::task::JoinHandle<()>> {
    static RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
        OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("editor-tools")
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
    });
    Ok(runtime.as_ref().map_err(Clone::clone)?.spawn(future))
}

pub(crate) struct Process(pub Child, Option<u32>);

impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self
            .1
            .and_then(|id| rustix::process::Pid::from_raw(id as i32))
        {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = self.0.start_kill();
    }
}

pub(crate) fn launch(command: &[String], directory: &Path) -> Result<Process> {
    let executable = command
        .first()
        .filter(|value| !value.is_empty())
        .ok_or("Enter an executable and its arguments as a JSON array")?;
    if command.len() > 32
        || command
            .iter()
            .any(|value| value.len() > 4096 || value.contains('\0'))
    {
        return Err("Tool command is too long".into());
    }
    let mut process = Command::new(executable);
    process
        .args(&command[1..])
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.as_std_mut().process_group(0);
    }
    process.spawn().map(|child| { let id = child.id(); Process(child, id) }).map_err(|error| format!("Cannot start {executable}: {error}. Install it through your distribution or Nix environment, or enter its full path."))
}

pub(crate) async fn bounded_read(input: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    input
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .await
        .map_err(|error| error.to_string())?;
    if output.len() > limit {
        return Err("Tool output exceeded the limit".into());
    }
    Ok(output)
}

pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run(
    command: Vec<String>,
    directory: std::path::PathBuf,
    input: ropey::Rope,
) -> Result<Output> {
    if input.len_bytes() > MAX_TOOL_BYTES {
        return Err("Language tools are limited to files up to 2 MiB".into());
    }
    let mut process = launch(&command, &directory)?;
    let mut stdin = process.0.stdin.take().ok_or("Missing tool stdin")?;
    let stdout = process.0.stdout.take().ok_or("Missing tool stdout")?;
    let stderr = process.0.stderr.take().ok_or("Missing tool stderr")?;
    let text = input.to_string();
    let operation = async {
        let write = async move {
            stdin
                .write_all(text.as_bytes())
                .await
                .map_err(|error| error.to_string())?;
            stdin.shutdown().await.map_err(|error| error.to_string())?;
            drop(stdin);
            Ok::<_, String>(())
        };
        let ((), stdout, stderr, exit) = tokio::try_join!(
            write,
            bounded_read(stdout, MAX_TOOL_BYTES),
            bounded_read(stderr, 64 * 1024),
            async { process.0.wait().await.map_err(|error| error.to_string()) }
        )?;
        Ok(Output {
            success: exit.success(),
            stdout: String::from_utf8(stdout).map_err(|_| "Tool output is not UTF-8")?,
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    };
    tokio::time::timeout(Duration::from_secs(15), operation)
        .await
        .map_err(|_| "Tool timed out after 15 seconds")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn commands_receive_text_without_shell_interpolation_and_bound_output() {
        let directory = tempfile::tempdir().unwrap();
        let output = run(
            vec!["cat".into()],
            directory.path().into(),
            ropey::Rope::from_str("$(touch nope) 猫\n"),
        )
        .await
        .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout, "$(touch nope) 猫\n");
        assert!(!directory.path().join("nope").exists());
        assert!(bounded_read(&b"12345"[..], 4).await.is_err());
    }
}
