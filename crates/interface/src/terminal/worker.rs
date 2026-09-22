use super::vt::{Frame, Ghostty};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub(super) enum Command {
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
    pub frame: Option<Frame>,
    pub input: VecDeque<Vec<u8>>,
    pub error: Option<String>,
}

pub(super) struct Worker {
    commands: mpsc::SyncSender<Command>,
    output: Arc<Mutex<Output>>,
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    pub(super) fn start(
        cols: u16,
        rows: u16,
        wake: Option<crate::wake::WakeSignal>,
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
    wake: &Option<crate::wake::WakeSignal>,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let mut vt = Ghostty::new(cols, rows)?;
    let mut next = Some(vt.frame()?);
    loop {
        if stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(frame) = next.take() {
            output.lock().map_err(|e| e.to_string())?.frame = Some(frame);
            if let Some(wake) = wake {
                wake.ring();
            }
        }
        let Ok(first) = inbox.recv() else {
            return Ok(());
        };
        let began = std::time::Instant::now();
        let mut command = Some(first);
        while let Some(current) = command {
            if stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            let input = match current {
                Command::Feed(bytes) => vt.feed(&bytes)?,
                Command::Resize(cols, rows) => {
                    vt.resize(cols, rows)?;
                    Vec::new()
                }
                Command::Key {
                    key,
                    mods,
                    text,
                    action,
                } => {
                    vt.scroll(None)?;
                    vt.key(key, mods, &text, action)?
                }
                Command::Paste(text) => {
                    vt.scroll(None)?;
                    vt.paste(&text)?
                }
                Command::Scroll(delta) => {
                    vt.scroll(delta)?;
                    Vec::new()
                }
            };
            if !input.is_empty() {
                let mut output = output.lock().map_err(|e| e.to_string())?;
                if output.input.iter().map(Vec::len).sum::<usize>() + input.len() > 1024 * 1024 {
                    return Err("Terminal input queue is full".into());
                }
                output.input.push_back(input);
            }
            if began.elapsed().as_millis() >= 16 {
                break;
            }
            command = inbox.try_recv().ok();
        }
        next = Some(vt.frame()?);
    }
}
