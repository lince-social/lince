#[cfg(unix)]
use std::{
    io::{Read, Write},
    process::Stdio,
};
use std::{
    process::Child,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub(super) struct Game {
    child: Child,
    input: mpsc::SyncSender<[u8; 2]>,
    frame: Arc<Mutex<Option<Vec<u8>>>>,
    ended: Arc<AtomicBool>,
    suspended: AtomicBool,
    _directory: tempfile::TempDir,
}

impl Game {
    #[cfg(unix)]
    pub(super) fn start(wake: Option<crate::wake::WakeSignal>) -> Result<Self, String> {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let executable = directory.path().join("freedoom");
        std::fs::write(
            &executable,
            include_bytes!(concat!(env!("OUT_DIR"), "/lince-freedoom")),
        )
        .map_err(|error| error.to_string())?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            directory.path().join("doom1.wad"),
            include_bytes!("vendor/doom1.wad"),
        )
        .map_err(|error| error.to_string())?;
        let mut child = std::process::Command::new(executable)
            .args(["-iwad", "doom1.wad", "-nosound", "-config", "default.cfg"])
            .current_dir(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not start Freedoom: {error}"))?;
        let mut output = child.stdout.take().unwrap();
        let mut input = child.stdin.take().unwrap();
        let (sender, receiver) = mpsc::sync_channel::<[u8; 2]>(256);
        let frame = Arc::new(Mutex::new(None));
        let ended = Arc::new(AtomicBool::new(false));
        let result = Self {
            child,
            input: sender,
            frame: frame.clone(),
            ended: ended.clone(),
            suspended: AtomicBool::new(false),
            _directory: directory,
        };
        std::thread::Builder::new()
            .name("freedoom-frames".into())
            .spawn(move || {
                loop {
                    let mut pixels = vec![0; 320 * 200 * 4];
                    if output.read_exact(&mut pixels).is_err() {
                        break;
                    }
                    if let Ok(mut frame) = frame.lock() {
                        *frame = Some(pixels);
                    }
                    if let Some(wake) = &wake {
                        wake.ring();
                    }
                }
                ended.store(true, Ordering::Release);
                if let Some(wake) = &wake {
                    wake.ring();
                }
            })
            .map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name("freedoom-keys".into())
            .spawn(move || {
                while let Ok(bytes) = receiver.recv() {
                    if input.write_all(&bytes).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    #[cfg(not(unix))]
    pub(super) fn start(_wake: Option<crate::wake::WakeSignal>) -> Result<Self, String> {
        Err("The native Freedoom host currently requires Unix".into())
    }

    pub(super) fn send(&self, bytes: [u8; 2]) {
        if self.input.try_send(bytes).is_err() {
            self.ended.store(true, Ordering::Release);
        }
    }

    pub(super) fn suspend(&self, suspended: bool) {
        if self.suspended.swap(suspended, Ordering::Relaxed) != suspended {
            self.send([if suspended { 2 } else { 3 }, 0]);
        }
    }

    pub(super) fn frame(&self) -> Option<Vec<u8>> {
        self.frame.lock().ok()?.take()
    }

    pub(super) fn finished(&self) -> Option<&'static str> {
        self.ended
            .load(Ordering::Acquire)
            .then_some("Game ended. Start to play again.")
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
