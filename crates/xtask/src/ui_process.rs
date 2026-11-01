use std::{
    process::{Command, Output},
    time::Duration,
};

#[cfg(unix)]
pub(super) fn output(
    command: &mut Command,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Output, String> {
    use std::{
        io::{ErrorKind, Read},
        os::unix::process::CommandExt,
        process::{Child, Stdio},
        thread,
        time::Instant,
    };

    struct Group(Child);
    impl Drop for Group {
        fn drop(&mut self) {
            if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn drain(
        reader: &mut impl Read,
        bytes: &mut Vec<u8>,
        limit: usize,
        deadline: Instant,
    ) -> Result<(), String> {
        let mut buffer = [0; 8192];
        loop {
            if Instant::now() >= deadline {
                return Err("Command timed out".into());
            }
            match reader.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(count) => {
                    if bytes.len().saturating_add(count) > limit {
                        return Err("Command exceeded output limit".into());
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Err("Command output capture failed".into()),
            }
        }
    }

    if timeout.is_zero() || max_bytes == 0 {
        return Err("Command has no time or output budget".into());
    }
    let deadline = Instant::now() + timeout;
    let mut child = Group(
        command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Command could not start ({:?})", error.kind()))?,
    );
    let mut stdout = child.0.stdout.take().ok_or("Command stdout unavailable")?;
    let mut stderr = child.0.stderr.take().ok_or("Command stderr unavailable")?;
    for pipe in [
        &stdout as &dyn std::os::fd::AsFd,
        &stderr as &dyn std::os::fd::AsFd,
    ] {
        let flags = rustix::fs::fcntl_getfl(pipe).map_err(|_| "Command pipe flags unavailable")?;
        rustix::fs::fcntl_setfl(pipe, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(|_| "Command pipe could not become nonblocking")?;
    }
    let mut out = Vec::new();
    let mut errors = Vec::new();
    loop {
        drain(
            &mut stdout,
            &mut out,
            max_bytes.saturating_sub(errors.len()),
            deadline,
        )?;
        drain(
            &mut stderr,
            &mut errors,
            max_bytes.saturating_sub(out.len()),
            deadline,
        )?;
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|_| "Command status unavailable")?
        {
            drain(
                &mut stdout,
                &mut out,
                max_bytes.saturating_sub(errors.len()),
                deadline,
            )?;
            drain(
                &mut stderr,
                &mut errors,
                max_bytes.saturating_sub(out.len()),
                deadline,
            )?;
            return Ok(Output {
                status,
                stdout: out,
                stderr: errors,
            });
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Command timed out".into());
        }
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

#[cfg(not(unix))]
pub(super) fn output(
    _command: &mut Command,
    _timeout: Duration,
    _max_bytes: usize,
) -> Result<Output, String> {
    Err("Bounded UI subprocess capture requires a Unix host".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn command_deadline_kills_descendants_holding_output_pipes() {
        let started = Instant::now();
        let failure = output(
            Command::new("sh").args(["-c", "sleep 10 & wait"]),
            Duration::from_millis(100),
            1024,
        )
        .unwrap_err();
        assert_eq!(failure, "Command timed out");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn output_cap_and_failures_do_not_disclose_arguments_or_values() {
        let failure = output(
            Command::new("sh").args(["-c", "printf secret-value"]),
            Duration::from_secs(1),
            3,
        )
        .unwrap_err();
        assert_eq!(failure, "Command exceeded output limit");
        assert!(!failure.contains("secret"));
        let failure = output(
            &mut Command::new("unavailable-secret-command"),
            Duration::from_secs(1),
            1024,
        )
        .unwrap_err();
        assert!(!failure.contains("secret"));
        let result = output(
            Command::new("sh").args(["-c", "printf done; printf detail >&2; exit 7"]),
            Duration::from_secs(1),
            1024,
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert_eq!(result.stdout, b"done");
        assert_eq!(result.stderr, b"detail");
    }
}
