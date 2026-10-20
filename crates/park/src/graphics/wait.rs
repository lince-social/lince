use smithay::backend::renderer::sync::SyncPoint;
use std::{
    io::{self, Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    sync::{Arc, Mutex, atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc::{SyncSender, TryRecvError, sync_channel}},
    thread::JoinHandle,
};

enum Target {
    Descriptor(OwnedFd),
    Fence(SyncPoint, Option<OwnedFd>),
}

impl Target {
    fn descriptor(&self) -> Option<&OwnedFd> {
        match self { Self::Descriptor(fd) => Some(fd), Self::Fence(_, fd) => fd.as_ref() }
    }

    fn reached(&self) -> bool {
        match self { Self::Descriptor(_) => false, Self::Fence(fence, _) => fence.is_reached() }
    }
}

struct Job {
    completed: Arc<AtomicBool>,
    target: Target,
    cancelled: Arc<AtomicBool>,
}

pub(crate) struct Watch {
    completed: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
    control: Arc<Mutex<UnixStream>>,
}

impl Watch {
    pub(crate) fn ready(&self) -> bool { self.completed.load(Ordering::Acquire) }
}

fn signal(control: &Mutex<UnixStream>) {
    let _ = control.lock().unwrap_or_else(|error| error.into_inner()).write(&[1]);
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.count.fetch_sub(1, Ordering::AcqRel);
        signal(&self.control);
    }
}

pub(crate) struct Reactor {
    sender: Option<SyncSender<Job>>,
    control: Arc<Mutex<UnixStream>>,
    count: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}

impl Reactor {
    pub(crate) fn new(wake: impl Fn() + Send + 'static) -> io::Result<Self> {
        let (control, mut input) = UnixStream::pair()?;
        control.set_nonblocking(true)?;
        input.set_nonblocking(true)?;
        let control = Arc::new(Mutex::new(control));
        let (sender, receiver) = sync_channel::<Job>(256);
        let worker = std::thread::Builder::new().name("park-fence-events".into()).spawn(move || {
            let mut jobs: Vec<Job> = Vec::new();
            loop {
                loop {
                    match receiver.try_recv() {
                        Ok(job) => jobs.push(job),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                jobs.retain(|job| !job.cancelled.load(Ordering::Acquire));
                let (ready, control_ready) = {
                    let mut descriptors = vec![rustix::event::PollFd::new(&input, rustix::event::PollFlags::IN)];
                    let mut indices = Vec::new();
                    for (index, job) in jobs.iter().enumerate() {
                        if let Some(fd) = job.target.descriptor() {
                            descriptors.push(rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN));
                            indices.push(index);
                        }
                    }
                    let fallback = rustix::time::Timespec { tv_sec: 0, tv_nsec: 5_000_000 };
                    let timeout = jobs.iter().any(|job| job.target.descriptor().is_none()).then_some(&fallback);
                    if let Err(error) = rustix::event::poll(&mut descriptors, timeout) {
                        if error == rustix::io::Errno::INTR { continue; }
                        wake();
                        return;
                    }
                    let ready: Vec<_> = indices.into_iter().zip(&descriptors[1..]).filter_map(|(index, fd)| {
                        fd.revents().intersects(rustix::event::PollFlags::IN | rustix::event::PollFlags::HUP | rustix::event::PollFlags::ERR | rustix::event::PollFlags::NVAL).then_some(index)
                    }).collect();
                    (ready, descriptors[0].revents().contains(rustix::event::PollFlags::IN))
                };
                if control_ready {
                    let mut bytes = [0; 256];
                    while input.read(&mut bytes).is_ok_and(|count| count > 0) {}
                }
                let mut index = 0;
                let mut changed = false;
                jobs.retain(|job| {
                    let done = ready.contains(&index) || job.target.reached();
                    if done { job.completed.store(true, Ordering::Release); }
                    index += 1;
                    changed |= done;
                    !done && !job.cancelled.load(Ordering::Acquire)
                });
                if changed { wake(); }
            }
        })?;
        Ok(Self { sender: Some(sender), control, count: Arc::new(AtomicUsize::new(0)), worker: Some(worker) })
    }

    fn register(&self, target: Target) -> io::Result<Watch> {
        self.count.try_update(Ordering::AcqRel, Ordering::Acquire, |count| (count < 256).then_some(count + 1))
            .map_err(|_| io::Error::other("Park fence watch limit reached"))?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let completed = Arc::new(AtomicBool::new(false));
        let watch = Watch { completed: completed.clone(), cancelled: cancelled.clone(), count: self.count.clone(), control: self.control.clone() };
        self.sender.as_ref().ok_or_else(|| io::Error::other("Fence reactor stopped"))?
            .try_send(Job { target, cancelled, completed }).map_err(|_| io::Error::other("Fence event queue is unavailable"))?;
        signal(&self.control);
        Ok(watch)
    }

    pub(crate) fn descriptor(&self, fd: OwnedFd) -> io::Result<Watch> {
        self.register(Target::Descriptor(fd))
    }

    pub(crate) fn fence(&self, fence: SyncPoint) -> io::Result<Watch> {
        let fd = fence.export();
        self.register(Target::Fence(fence, fd))
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        self.sender.take();
        signal(&self.control);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_completion_wakes_without_periodic_polling() {
        let (sent, received) = std::sync::mpsc::channel();
        let reactor = Reactor::new(move || { let _ = sent.send(()); }).unwrap();
        let (reader, mut writer) = UnixStream::pair().unwrap();
        let watch = reactor.descriptor(reader.into()).unwrap();
        writer.write_all(&[1]).unwrap();
        received.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        drop(watch);
        assert_eq!(reactor.count.load(Ordering::Acquire), 0);
    }
}
