use super::{
    pending_input::Input,
    vt::{Frame, Ghostty},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) enum Command {
    Journal(Vec<cell::command::Event>, bool),
    Feed(Vec<u8>),
    Resize(u16, u16),
    Key {
        key: u32,
        mods: u32,
        text: String,
        action: u32,
    },
    Paste(String),
    Scroll(Option<i32>),
}

#[derive(Default)]
pub(super) struct Output {
    pub frame: Option<Arc<Frame>>,
    pub input: Input,
    pub error: Option<String>,
}

pub(super) struct Worker {
    commands: mpsc::SyncSender<Command>,
    output: Arc<Mutex<Output>>,
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    #[cfg(test)]
    pub(super) fn busy_for_test() -> (Self, mpsc::Receiver<Command>) {
        let (commands, inbox) = mpsc::sync_channel(1);
        assert!(commands.send(Command::Resize(80, 24)).is_ok());
        (Self { commands, output: Arc::new(Mutex::new(Output::default())), cancelled: Arc::new(AtomicBool::new(false)) }, inbox)
    }

    pub(super) fn start(
        cols: u16,
        rows: u16,
        wake: Option<crate::native::wake::WakeSignal>,
    ) -> Result<Self, String> {
        let (commands, inbox) = mpsc::sync_channel(128);
        let output = Arc::new(Mutex::new(Output::default()));
        let shared = output.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = cancelled.clone();
        std::thread::Builder::new()
            .name("libghostty".into())
            .spawn(move || {
                let result = run(cols, rows, inbox, &shared, &wake, &stopped);
                if let Err(error) = result
                    && let Ok(mut output) = shared.lock()
                {
                    output.error = Some(error);
                }
                if let Some(wake) = wake {
                    wake.ring();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands,
            output,
            cancelled,
        })
    }

    pub(super) fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|_| "Terminal renderer is busy or stopped".into())
    }

    pub(super) fn take(&self) -> Output {
        self.output
            .lock()
            .map(|mut output| std::mem::take(&mut *output))
            .unwrap_or_else(|_| Output {
                error: Some("Terminal renderer stopped".into()),
                ..Default::default()
            })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn run(
    cols: u16,
    rows: u16,
    inbox: mpsc::Receiver<Command>,
    output: &Mutex<Output>,
    wake: &Option<crate::native::wake::WakeSignal>,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let mut vt = Ghostty::new(cols, rows)?;
    let mut next = Some(vt.frame()?);
    let mut published: Option<Arc<Frame>> = None;
    let mut input_ready = false;
    loop {
        if stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        let changed = if let Some(frame) = next.take() {
            let changed = published.as_deref() != Some(&frame);
            if changed {
                let frame = Arc::new(frame);
                output.lock().map_err(|e| e.to_string())?.frame = Some(frame.clone());
                published = Some(frame);
                crate::host::performance::count(
                    crate::host::performance::Count::TerminalFramesPublished,
                );
            } else {
                crate::host::performance::count(
                    crate::host::performance::Count::TerminalFramesUnchanged,
                );
            }
            changed
        } else {
            false
        };
        if (changed || input_ready)
            && let Some(wake) = wake
        {
            wake.ring();
        }
        input_ready = false;
        let Ok(first) = inbox.recv() else {
            return Ok(());
        };
        let began = std::time::Instant::now();
        let mut command = Some(first);
        let mut render_needed = false;
        while let Some(current) = command {
            if stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            let input = match current {
                Command::Journal(events, live) => {
                    use base64::Engine as _;
                    let mut replies = Vec::new();
                    for event in events {
                        match event {
                            cell::command::Event::Output { data_base64 } => {
                                render_needed = true;
                                let bytes = base64::engine::general_purpose::STANDARD
                                    .decode(data_base64)
                                    .map_err(|error| error.to_string())?;
                                let input = vt.feed(&bytes)?;
                                if live {
                                    replies.extend(input);
                                }
                            }
                            cell::command::Event::Resize { cols, rows } => {
                                render_needed |= vt.resize(cols, rows)?;
                            }
                            _ => {}
                        }
                    }
                    replies
                }
                Command::Feed(bytes) => {
                    render_needed = true;
                    vt.feed(&bytes)?
                }
                Command::Resize(cols, rows) => {
                    render_needed |= vt.resize(cols, rows)?;
                    Vec::new()
                }
                Command::Key {
                    key,
                    mods,
                    text,
                    action,
                } => {
                    render_needed = true;
                    vt.scroll(None)?;
                    vt.key(key, mods, &text, action)?
                }
                Command::Paste(text) => {
                    render_needed = true;
                    vt.scroll(None)?;
                    vt.paste(&text)?
                }
                Command::Scroll(delta) => {
                    render_needed = true;
                    vt.scroll(delta)?;
                    Vec::new()
                }
            };
            if !input.is_empty() {
                let mut output = output.lock().map_err(|e| e.to_string())?;
                if !output.input.fits(input.len()) {
                    return Err("Terminal input queue is full".into());
                }
                output.input.push_back(input);
                input_ready = true;
            }
            if began.elapsed().as_millis() >= 16 {
                break;
            }
            command = inbox.try_recv().ok();
        }
        next = if render_needed {
            Some(vt.frame()?)
        } else {
            nucleus::performance::add("terminal-vt-snapshot-free-batches", 1);
            None
        };
    }
}
