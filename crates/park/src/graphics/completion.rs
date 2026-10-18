use super::{ProbeFailure, Result};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}, mpsc::{SyncSender, RecvTimeoutError, TryRecvError, sync_channel}};
use std::thread::{JoinHandle, Thread};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct Completion {
    result: Arc<Mutex<Option<std::result::Result<(), String>>>>,
}

impl Completion {
    pub(crate) fn ready(&self) -> Result<bool> {
        match self.result.lock().unwrap_or_else(|error| error.into_inner()).as_ref() {
            None => Ok(false),
            Some(Ok(())) => Ok(true),
            Some(Err(error)) => Err(ProbeFailure::at("gpu_completion", error)),
        }
    }

    pub(crate) fn wait_for_probe(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.ready()? {
            if Instant::now() >= deadline {
                return Err(ProbeFailure::at("gpu_completion", "Probe completion deadline exceeded"));
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
        Ok(())
    }
}

struct Job {
    count: Arc<AtomicUsize>,
    started: Instant,
    submission: wgpu::SubmissionIndex,
    completion: Completion,
    notify: Thread,
}

impl Drop for Job {
    fn drop(&mut self) { self.count.fetch_sub(1, Ordering::AcqRel); }
}

pub(super) struct Driver {
    count: Arc<AtomicUsize>,
    sender: Option<SyncSender<Job>>,
    worker: Option<JoinHandle<()>>,
}

impl Driver {
    pub(super) fn new(device: wgpu::Device) -> Result<Self> {
        let (sender, receiver) = sync_channel::<Job>(32);
        let worker = std::thread::Builder::new().name("park-gpu-completion".into()).spawn(move || {
            let mut jobs: Vec<Job> = Vec::new();
            let mut disconnected = false;
            let mut failure: Option<String> = None;
            loop {
                if !disconnected {
                    let next = if jobs.is_empty() {
                        receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    } else { receiver.recv_timeout(Duration::from_millis(2)) };
                    match next {
                        Ok(job) => jobs.push(job),
                        Err(RecvTimeoutError::Disconnected) => disconnected = true,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                    loop {
                        match receiver.try_recv() {
                            Ok(job) => jobs.push(job),
                            Err(TryRecvError::Disconnected) => { disconnected = true; break; }
                            Err(TryRecvError::Empty) => break,
                        }
                    }
                }
                jobs.retain(|job| {
                    let result = if let Some(error) = &failure { Some(Err(error.clone())) } else {
                        match device.poll(wgpu::PollType::Wait {
                            submission_index: Some(job.submission.clone()), timeout: Some(Duration::ZERO),
                        }) {
                            Ok(_) => Some(Ok(())),
                            Err(wgpu::PollError::Timeout) if job.started.elapsed() < Duration::from_secs(5) => None,
                            Err(error) => Some(Err(error.to_string())),
                        }
                    };
                    if let Some(result) = result {
                        if let Err(error) = &result { failure = Some(error.clone()); }
                        *job.completion.result.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
                        job.notify.unpark();
                        false
                    } else { true }
                });
                if disconnected && jobs.is_empty() { break; }
                if disconnected { std::thread::sleep(Duration::from_millis(2)); }
            }
        }).map_err(|error| ProbeFailure::at("gpu_completion", error))?;
        Ok(Self { count: Arc::new(AtomicUsize::new(0)), sender: Some(sender), worker: Some(worker) })
    }

    pub(super) fn track(&self, submission: wgpu::SubmissionIndex) -> Result<Completion> {
        let completion = Completion { result: Arc::new(Mutex::new(None)) };
        let sender = self.sender.as_ref().ok_or_else(|| ProbeFailure::at("gpu_completion", "Completion driver stopped"))?;
        self.count.fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| (count < 32).then_some(count + 1))
            .map_err(|_| ProbeFailure::at("gpu_completion", "GPU completion limit reached"))?;
        sender.try_send(Job { count: self.count.clone(), started: Instant::now(), submission, completion: completion.clone(), notify: std::thread::current() })
            .map_err(|_| ProbeFailure::at("gpu_completion", "GPU completion queue is full or disconnected"))?;
        Ok(completion)
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_and_failed_completions_cannot_be_published_as_ready() {
        let completion = Completion { result: Arc::new(Mutex::new(None)) };
        assert!(!completion.ready().unwrap());
        let retained = completion.clone();
        *completion.result.lock().unwrap() = Some(Err("device lost".into()));
        assert!(retained.ready().is_err());
        *completion.result.lock().unwrap() = Some(Ok(()));
        assert!(retained.ready().unwrap());
    }
}
