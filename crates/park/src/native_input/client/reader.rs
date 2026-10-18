use super::super::transfer::MAX_TRANSFER;
use std::{io::{self, Read, Write}, os::{fd::{AsFd, OwnedFd}, unix::net::UnixStream},
    sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, SyncSender}},
    thread::{self, JoinHandle}, time::{Duration, Instant}};

struct Job { token: u64, stream: UnixStream, cancelled: Arc<AtomicBool> }
enum Work { Read(Job), Stop }

pub(super) struct Reply { pub token: u64, pub result: Result<Vec<u8>, String> }

#[derive(Clone)]
pub(super) struct Handle(SyncSender<Work>);

impl Handle {
    pub(super) fn submit(&self, token: u64, stream: UnixStream, cancelled: Arc<AtomicBool>) -> io::Result<()> {
        stream.set_nonblocking(true)?;
        self.0.try_send(Work::Read(Job { token, stream, cancelled }))
            .map_err(|_| io::Error::other("The native drop reader is busy"))
    }
}

pub(super) struct Reader {
    pub handle: Handle,
    replies: Receiver<Reply>,
    wake: UnixStream,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Reader {
    pub(super) fn new() -> io::Result<Self> {
        let (sender, jobs) = mpsc::sync_channel(1);
        let (results, replies) = mpsc::sync_channel(2);
        let (wake, mut notify) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        notify.set_nonblocking(true)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let stopping = stopped.clone();
        let thread = thread::Builder::new().name("park-drop-reader".into()).spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                let Ok(Work::Read(job)) = jobs.recv() else { break };
                let result = read(job.stream, &job.cancelled, &stopping, Duration::from_secs(2));
                if results.try_send(Reply { token: job.token, result }).is_err() {
                    job.cancelled.store(true, Ordering::Release);
                }
                let _ = notify.write(&[1]);
            }
        })?;
        Ok(Self { handle: Handle(sender), replies, wake, stopped, thread: Some(thread) })
    }

    pub(super) fn descriptor(&self) -> io::Result<OwnedFd> { self.wake.as_fd().try_clone_to_owned() }

    pub(super) fn drain(&mut self) -> Vec<Reply> {
        let mut buffer = [0; 64];
        while self.wake.read(&mut buffer).is_ok_and(|bytes| bytes != 0) {}
        self.replies.try_iter().collect()
    }

    pub(super) fn failed(&self) -> bool {
        !self.stopped.load(Ordering::Acquire) && self.thread.as_ref().is_some_and(|thread| thread.is_finished())
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.handle.0.try_send(Work::Stop);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

fn read(mut stream: UnixStream, cancelled: &AtomicBool, stopped: &AtomicBool, timeout: Duration) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut buffer = [0; 16384];
    loop {
        if cancelled.load(Ordering::Acquire) || stopped.load(Ordering::Acquire) { return Err("The native drop was cancelled.".into()); }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Err("The drop source did not finish within two seconds.".into()); }
        let wait = remaining.min(Duration::from_millis(50));
        let timeout = rustix::time::Timespec { tv_sec: wait.as_secs() as i64, tv_nsec: i64::from(wait.subsec_nanos()) };
        let mut descriptors = [rustix::event::PollFd::new(&stream, rustix::event::PollFlags::IN)];
        match rustix::event::poll(&mut descriptors, Some(&timeout)) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(format!("Could not read the drop source: {error}")),
        }
        if descriptors[0].revents().is_empty() { continue; }
        let room = (MAX_TRANSFER + 1).saturating_sub(bytes.len()).min(buffer.len());
        if room == 0 { return Err("The dropped data exceeds 1 MiB.".into()); }
        match stream.read(&mut buffer[..room]) {
            Ok(0) => return Ok(bytes),
            Ok(count) => {
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.len() > MAX_TRANSFER { return Err("The dropped data exceeds 1 MiB.".into()); }
            }
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {}
            Err(error) => return Err(format!("Could not read the drop source: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eof_preserves_bytes_and_cancel_or_timeout_never_reads_a_payload() {
        let (source, mut writer) = UnixStream::pair().unwrap();
        source.set_nonblocking(true).unwrap();
        writer.write_all("Olá 世界".as_bytes()).unwrap();
        drop(writer);
        let cancelled = AtomicBool::new(false);
        let stopped = AtomicBool::new(false);
        assert_eq!(read(source, &cancelled, &stopped, Duration::from_secs(1)).unwrap(), "Olá 世界".as_bytes());
        let (source, _writer) = UnixStream::pair().unwrap();
        assert!(read(source, &cancelled, &stopped, Duration::ZERO).is_err());
        cancelled.store(true, Ordering::Release);
        let (source, _writer) = UnixStream::pair().unwrap();
        assert!(read(source, &cancelled, &stopped, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn oversized_sources_are_stopped_at_the_transfer_bound() {
        let (source, mut writer) = UnixStream::pair().unwrap();
        source.set_nonblocking(true).unwrap();
        let writing = thread::spawn(move || { let _ = writer.write_all(&vec![b'a'; MAX_TRANSFER + 1]); });
        assert!(read(source, &AtomicBool::new(false), &AtomicBool::new(false), Duration::from_secs(2)).is_err());
        writing.join().unwrap();
    }

    #[test]
    fn the_reader_can_stop_while_idle_with_sender_clones_alive() {
        let reader = Reader::new().unwrap();
        let handle = reader.handle.clone();
        drop(reader);
        let (stream, _other) = UnixStream::pair().unwrap();
        assert!(handle.submit(1, stream, Arc::new(AtomicBool::new(false))).is_err());
    }
}
