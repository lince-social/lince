use super::*;
use smithay::{
    reexports::calloop::EventLoop,
    wayland::security_context::{SecurityContext, SecurityContextHandler, SecurityContextListenerSource},
};

pub(super) struct Listeners {
    events: EventLoop<'static, Vec<UnixStream>>,
    active: Arc<AtomicUsize>,
}

struct Lease(Arc<AtomicUsize>);

impl Drop for Lease {
    fn drop(&mut self) { self.0.fetch_sub(1, Ordering::AcqRel); }
}

impl Listeners {
    pub(super) fn new() -> io::Result<Self> {
        Ok(Self { events: EventLoop::try_new().map_err(io::Error::other)?, active: Arc::new(AtomicUsize::new(0)) })
    }

    #[cfg(feature = "nested")]
    pub(super) fn event_fd(&self) -> io::Result<OwnedFd> {
        use std::os::fd::AsFd;
        self.events.as_fd().try_clone_to_owned()
    }

    pub(super) fn dispatch(&mut self, sources: Vec<SecurityContextListenerSource>) -> io::Result<Vec<UnixStream>> {
        for source in sources {
            if self.active.load(Ordering::Acquire) >= MAX_CLIENTS { break; }
            self.active.fetch_add(1, Ordering::AcqRel);
            let lease = Lease(self.active.clone());
            if let Err(error) = self.events.handle().insert_source(source, move |stream, _, clients| {
                let _ = &lease;
                if clients.len() < MAX_CLIENTS { clients.push(stream); }
            }) {
                eprintln!("Could not register sandbox clipboard context: {error}");
            }
        }
        let mut clients = Vec::new();
        if let Err(error) = self.events.dispatch(std::time::Duration::ZERO, &mut clients) {
            eprintln!("Closing sandbox clipboard listeners after a listener error: {error}");
            self.events = EventLoop::try_new().map_err(io::Error::other)?;
            clients.clear();
        }
        Ok(clients)
    }
}

impl SecurityContextHandler for State {
    fn context_created(&mut self, source: SecurityContextListenerSource, _: SecurityContext) {
        if self.security_contexts.len() < MAX_CLIENTS { self.security_contexts.push(source); }
    }
}

smithay::delegate_security_context!(State);
