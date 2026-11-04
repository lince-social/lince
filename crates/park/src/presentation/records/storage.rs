use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

pub(super) struct Request {
    pub version: u64,
    pub profiles: Vec<Saved>,
    pub wake: Option<WakeSignal>,
    pub notify: Arc<AtomicBool>,
}

pub(super) struct Finished {
    pub version: u64,
    pub result: io::Result<()>,
}

pub(super) struct Writer {
    requests: Option<mpsc::SyncSender<Request>>,
    finished: Option<mpsc::Receiver<Finished>>,
    task: Option<thread::JoinHandle<()>>,
}

pub(super) fn save(path: &Path, profiles: &[Saved]) -> io::Result<()> {
    if profiles.iter().any(|profile| !profile.valid()) {
        return Err(io::Error::other("invalid window customization data"));
    }
    super::super::outputs::save(path, profiles)
}

impl Writer {
    pub(super) fn new(path: PathBuf) -> io::Result<Self> {
        Self::with_writer(move |profiles| save(&path, profiles))
    }

    pub(super) fn with_writer(
        mut write: impl FnMut(&[Saved]) -> io::Result<()> + Send + 'static,
    ) -> io::Result<Self> {
        let (requests, incoming) = mpsc::sync_channel::<Request>(1);
        let (completed, finished) = mpsc::sync_channel(1);
        let task = thread::Builder::new()
            .name("park-profile-io".into())
            .spawn(move || {
                while let Ok(request) = incoming.recv() {
                    let result = write(&request.profiles);
                    let failed = result.is_err();
                    if completed
                        .send(Finished {
                            version: request.version,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    if (failed || request.notify.load(Ordering::Acquire))
                        && let Some(wake) = request.wake
                    {
                        wake.ring();
                    }
                }
            })?;
        Ok(Self {
            requests: Some(requests),
            finished: Some(finished),
            task: Some(task),
        })
    }

    pub(super) fn request(&self, request: Request) -> io::Result<()> {
        self.requests
            .as_ref()
            .unwrap()
            .try_send(request)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => io::Error::other("Window profile saving is busy"),
                mpsc::TrySendError::Disconnected(_) => {
                    io::Error::other("Window profile saving stopped")
                }
            })
    }

    pub(super) fn poll(&self) -> io::Result<Option<Finished>> {
        match self.finished.as_ref().unwrap().try_recv() {
            Ok(finished) => Ok(Some(finished)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(io::Error::other("Window profile saving stopped"))
            }
        }
    }

    pub(super) fn finish(&self) -> io::Result<Finished> {
        self.finished
            .as_ref()
            .unwrap()
            .recv()
            .map_err(|_| io::Error::other("Window profile saving stopped"))
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.requests.take();
        self.finished.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
