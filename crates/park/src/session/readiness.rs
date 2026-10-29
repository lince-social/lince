use std::{io, path::PathBuf, process::{Child, Command}, time::{Duration, Instant}};

pub(super) struct Readiness {
    program: Option<PathBuf>,
    child: Option<Child>,
    started: Option<Instant>,
    finished: bool,
}

impl Readiness {
    pub(super) fn new() -> io::Result<Self> {
        let program = match std::env::var_os("LINCE_PARK_UWSM") {
            None => None,
            Some(program) => {
                let program = PathBuf::from(program);
                if !program.is_absolute() || std::env::var_os("NOTIFY_SOCKET").is_none_or(|value| value.is_empty()) {
                    return Err(io::Error::other("The UWSM Park session needs an absolute manager executable and its notification socket"));
                }
                Some(program)
            }
        };
        Ok(Self { program, child: None, started: None, finished: false })
    }

    pub(super) fn advance(&mut self, interactive: bool) -> io::Result<()> {
        if self.finished { return Ok(()); }
        if self.child.is_none() {
            if !interactive { return Ok(()); }
            let Some(program) = &self.program else { self.finished = true; return Ok(()); };
            let mut command = Command::new(program);
            command.args(["finalize", "XDG_CURRENT_DESKTOP"]);
            crate::launch::restore_graphics_environment(&mut command, |name| std::env::var_os(name))?;
            self.child = Some(command.spawn()?);
            self.started = Some(Instant::now());
        }
        let child = self.child.as_mut().ok_or_else(|| io::Error::other("Missing UWSM readiness process"))?;
        if let Some(status) = child.try_wait()? {
            if !status.success() { return Err(io::Error::other(format!("UWSM readiness failed: {status}"))); }
            self.child = None;
            self.finished = true;
            eprintln!("Park UWSM readiness completed");
        } else if self.started.is_some_and(|started| started.elapsed() >= Duration::from_secs(5)) {
            return Err(io::Error::other("UWSM readiness exceeded its five-second deadline"));
        }
        Ok(())
    }

    pub(super) fn timeout(&self) -> Option<Duration> {
        self.child.as_ref().map(|_| Duration::from_millis(20))
    }
}

impl Drop for Readiness {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
