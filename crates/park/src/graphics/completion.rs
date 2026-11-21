// non-lean-yet
// Scope: const LIMIT, const TIMEOUT, const TIMED_OUT, const STOPPED.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 137, 138, 159.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 134.

use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread::{self, JoinHandle, Thread},
    time::{Duration, Instant},
};

use super::{ProbeFailure, Result};

const LIMIT: usize = 32;
const TIMEOUT: Duration = Duration::from_secs(5);
const TIMED_OUT: &str = "GPU submission did not complete before its deadline";
const STOPPED: &str = "GPU completion worker stopped with a pending submission";

type Outcome = std::result::Result<(), String>;

struct State {
    signaled: AtomicBool,
    outcome: OnceLock<Outcome>,
    deadline: Instant,
}

#[derive(Clone)]
pub(crate) struct Completion(Arc<State>);

impl Completion {
    fn new(started: Instant) -> Self {
        Self(Arc::new(State {
            signaled: AtomicBool::new(false),
            outcome: OnceLock::new(),
            deadline: started + TIMEOUT,
        }))
    }

    pub(crate) fn pending_until(&self) -> Result<Option<Instant>> {
        let outcome = match self.0.outcome.get() {
            None if Instant::now() >= self.0.deadline => {
                Some(self.0.outcome.get_or_init(|| Err(TIMED_OUT.into())))
            }
            outcome => outcome,
        };
        match outcome {
            None => Ok(Some(self.0.deadline)),
            Some(Ok(())) => Ok(None),
            Some(Err(error)) => Err(ProbeFailure::at("gpu_completion", error)),
        }
    }

    pub(crate) fn ready(&self) -> Result<bool> {
        Ok(self.pending_until()?.is_none())
    }

    pub(crate) fn wait_for_probe(&self) -> Result<()> {
        while let Some(deadline) = self.pending_until()? {
            let remaining = deadline.saturating_duration_since(Instant::now());
            thread::park_timeout(remaining.min(Duration::from_millis(10)));
        }
        Ok(())
    }
}

struct Job {
    count: Arc<AtomicUsize>,
    completion: Completion,
    notify: Thread,
}

impl Job {
    fn resolve(&self, outcome: Outcome) {
        #[cfg(feature = "native-presentation")]
        let succeeded = outcome.is_ok();
        let inserted = self.completion.0.outcome.set(outcome).is_ok();
        #[cfg(feature = "native-presentation")]
        if inserted && lince_interface::host::performance::enabled() {
            use lince_interface::host::performance::{self, Stage};
            let stage = if succeeded {
                Stage::GpuSubmissionToCompletion
            } else {
                Stage::GpuCompletionFailure
            };
            performance::record(stage, (self.completion.0.deadline - TIMEOUT).elapsed());
        }
        if inserted {
            self.notify.unpark();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if self.completion.0.outcome.get().is_none() {
            self.resolve(Err(STOPPED.into()));
        }
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct Driver {
    count: Arc<AtomicUsize>,
    sender: Option<SyncSender<Job>>,
    worker: Option<JoinHandle<()>>,
}

impl Driver {
    pub(super) fn new(device: wgpu::Device) -> Result<Self> {
        let (sender, receiver) = sync_channel(LIMIT);
        let worker = thread::Builder::new()
            .name("park-gpu-completion".into())
            .spawn(move || {
                #[cfg(feature = "native-presentation")]
                let _observations = crate::presentation::observe_systems();
                run(receiver, || {
                    #[cfg(feature = "native-presentation")]
                    let _observation =
                        crate::presentation::observe_work("lince_park::graphics::completion::poll");
                    device
                        .poll(wgpu::PollType::Poll)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                });
            })
            .map_err(|error| ProbeFailure::at("gpu_completion", error))?;
        Ok(Self {
            count: Arc::new(AtomicUsize::new(0)),
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub(super) fn track(&self, queue: &wgpu::Queue) -> Result<Completion> {
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| ProbeFailure::at("gpu_completion", STOPPED))?;
        self.count
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < LIMIT).then_some(count + 1)
            })
            .map_err(|_| ProbeFailure::at("gpu_completion", "GPU completion limit reached"))?;
        let completion = Completion::new(Instant::now());
        sender
            .try_send(Job {
                count: self.count.clone(),
                completion: completion.clone(),
                notify: thread::current(),
            })
            .map_err(|error| ProbeFailure::at("gpu_completion", error))?;
        let state = Arc::downgrade(&completion.0);
        queue.on_submitted_work_done(move || {
            if let Some(state) = state.upgrade() {
                state.signaled.store(true, Ordering::Release);
            }
        });
        Ok(completion)
    }
}

fn run(receiver: Receiver<Job>, mut poll: impl FnMut() -> Outcome) {
    let mut jobs = Vec::new();
    while let Ok(first) = receiver.recv() {
        jobs.push(first);
        while !jobs.is_empty() {
            jobs.extend(receiver.try_iter());
            if let Err(error) = poll() {
                for job in &jobs {
                    job.resolve(Err(error.clone()));
                }
                return;
            }
            let now = Instant::now();
            let mut timed_out = false;
            jobs.retain(|job| {
                if now >= job.completion.0.deadline {
                    job.resolve(Err(TIMED_OUT.into()));
                    timed_out = true;
                } else if job.completion.0.signaled.load(Ordering::Acquire) {
                    job.resolve(Ok(()));
                } else {
                    return true;
                }
                false
            });
            if timed_out {
                for job in &jobs {
                    job.resolve(Err(TIMED_OUT.into()));
                }
                return;
            }
            if !jobs.is_empty() {
                thread::park_timeout(Duration::from_millis(1));
            }
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(started: Instant) -> Job {
        Job {
            count: Arc::new(AtomicUsize::new(1)),
            completion: Completion::new(started),
            notify: thread::current(),
        }
    }

    #[test]
    fn only_the_signaled_submission_becomes_ready() {
        let (sender, receiver) = sync_channel(LIMIT);
        let first = job(Instant::now());
        let second = job(Instant::now());
        let first_done = first.completion.clone();
        let second_done = second.completion.clone();
        assert!(sender.send(first).is_ok());
        assert!(sender.send(second).is_ok());
        drop(sender);
        let mut polls = 0;
        run(receiver, || {
            polls += 1;
            if polls == 1 {
                first_done.0.signaled.store(true, Ordering::Release);
            } else {
                assert!(first_done.ready().unwrap());
                assert!(!second_done.ready().unwrap());
                second_done.0.signaled.store(true, Ordering::Release);
            }
            Ok(())
        });
        assert_eq!(polls, 2);
        assert!(second_done.ready().unwrap());
    }

    #[test]
    fn polling_panic_fails_and_wakes_active_and_queued_jobs() {
        let (sender, receiver) = sync_channel(LIMIT);
        let first = job(Instant::now());
        let second = job(Instant::now());
        let active = first.completion.clone();
        let queued = second.completion.clone();
        assert!(sender.send(first).is_ok());
        let worker = thread::spawn(move || {
            let mut second = Some(second);
            run(receiver, || {
                assert!(sender.send(second.take().unwrap()).is_ok());
                panic!("injected GPU poll panic");
            });
        });
        assert!(worker.join().is_err());
        for completion in [active, queued] {
            assert_eq!(completion.ready().unwrap_err().detail, STOPPED);
        }
    }

    #[test]
    fn a_stalled_job_expires_without_the_worker_and_cannot_become_ready_later() {
        let job = job(Instant::now() - TIMEOUT);
        assert_eq!(job.completion.ready().unwrap_err().detail, TIMED_OUT);
        job.resolve(Ok(()));
        assert_eq!(job.completion.ready().unwrap_err().detail, TIMED_OUT);
    }
}
