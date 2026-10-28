use crate::control_protocol::client::lince_park_control_v1::{self as protocol, LinceParkControlV1 as Manager};
use lince_interface::host::control::{self as contract, Invocation, Response};
use std::{io, time::{Duration, Instant}};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, protocol::{wl_callback, wl_registry}};

#[derive(Default)]
struct State {
    manager: Option<Manager>,
    ready: bool,
    expected: Option<u32>,
    retired: u32,
    reply: Option<Response>,
    changed: Option<(u64, u32)>,
    subscribed: bool,
    error: Option<String>,
}

fn error(value: impl std::fmt::Display) -> io::Error { io::Error::other(value.to_string()) }

pub struct ControlClient {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    next: u32,
}

impl ControlClient {
    pub fn connect() -> io::Result<Self> {
        let connection = Connection::connect_to_env().map_err(error)?;
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> io::Result<Self> {
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        connection.display().sync(&queue.handle(), ());
        let mut client = Self { connection, queue, state: State::default(), next: 1 };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !client.state.ready { client.pump(deadline, true)?; }
        if client.state.manager.is_none() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Park desktop control is unavailable on this connection. Use an owner-configured helper connection with the required capabilities."));
        }
        Ok(client)
    }

    fn manager(&self) -> &Manager { self.state.manager.as_ref().unwrap() }

    fn request(&mut self) -> io::Result<u32> {
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or_else(|| error("Management request identifiers are exhausted"))?;
        self.state.expected = Some(id);
        self.state.reply = None;
        Ok(id)
    }

    fn reply(&mut self) -> io::Result<Response> {
        let deadline = Instant::now() + Duration::from_secs(12);
        while self.state.reply.is_none() {
            if let Err(error) = self.pump(deadline, false) {
                if let Some(id) = self.state.expected.take() { self.state.retired = id; }
                self.state.reply = None;
                return Err(error);
            }
        }
        if let Some(id) = self.state.expected.take() { self.state.retired = id; }
        self.flush()?;
        Ok(self.state.reply.take().unwrap())
    }

    pub fn snapshot(&mut self, revision: u64, cursor: u32) -> io::Result<Response> {
        let id = self.request()?;
        self.manager().snapshot(id, revision.to_string(), cursor);
        self.reply()
    }

    pub fn invoke(&mut self, invocation: &Invocation) -> io::Result<Response> {
        invocation.validate().map_err(|failure| io::Error::new(io::ErrorKind::InvalidInput, failure.message))?;
        let body = serde_json::to_string(invocation).map_err(error)?;
        if body.len() > contract::ARGUMENT_LIMIT { return Err(io::Error::new(io::ErrorKind::InvalidInput, "The invocation exceeds 16 KiB")); }
        let id = self.request()?;
        self.manager().invoke(id, body);
        self.reply()
    }

    pub fn subscribe(&mut self, enabled: bool) -> io::Result<()> {
        self.manager().subscribe(u32::from(enabled));
        self.state.subscribed = enabled;
        self.flush()
    }

    pub fn close(mut self) -> io::Result<()> {
        if self.state.subscribed { self.subscribe(false)?; }
        self.synchronize()?;
        self.manager().destroy();
        self.synchronize()
    }

    fn synchronize(&mut self) -> io::Result<()> {
        self.state.ready = false;
        self.connection.display().sync(&self.queue.handle(), ());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.state.ready { self.pump(deadline, true)?; }
        Ok(())
    }

    pub fn next_change(&mut self, timeout: Duration) -> io::Result<(u64, u32)> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "The management wait duration is too large"))?;
        while self.state.changed.is_none() { self.pump(deadline, false)?; }
        self.flush()?;
        Ok(self.state.changed.take().unwrap())
    }

    fn flush(&self) -> io::Result<()> {
        self.flush_pending().map(|_| ())
    }

    fn flush_pending(&self) -> io::Result<bool> {
        match self.connection.flush() {
            Ok(()) => Ok(true),
            Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(failure) => Err(error(failure)),
        }
    }

    fn pump(&mut self, deadline: Instant, registry: bool) -> io::Result<()> {
        self.queue.dispatch_pending(&mut self.state).map_err(error)?;
        if let Some(message) = self.state.error.take() { return Err(error(message)); }
        let flushed = self.flush_pending()?;
        if registry && self.state.ready || !registry && (self.state.reply.is_some() || self.state.changed.is_some() && self.state.expected.is_none()) { return Ok(()); }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Err(io::Error::new(io::ErrorKind::TimedOut, "The management reply timed out. Refresh state before retrying; a command's outcome may be unknown.")); }
        if let Some(read) = self.queue.prepare_read() {
            let fd = read.connection_fd();
            let flags = rustix::event::PollFlags::IN | if flushed { rustix::event::PollFlags::empty() } else { rustix::event::PollFlags::OUT };
            let mut poll = [rustix::event::PollFd::new(&fd, flags)];
            let timeout = rustix::time::Timespec { tv_sec: remaining.as_secs().min(i64::MAX as u64) as i64, tv_nsec: i64::from(remaining.subsec_nanos()) };
            rustix::event::poll(&mut poll, Some(&timeout)).map_err(error)?;
            if poll[0].revents().intersects(rustix::event::PollFlags::IN | rustix::event::PollFlags::HUP | rustix::event::PollFlags::ERR) {
                match read.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(failure) => return Err(error(failure)),
                }
            }
        }
        self.flush_pending()?;
        self.queue.dispatch_pending(&mut self.state).map_err(error)?;
        if let Some(message) = self.state.error.take() { return Err(error(message)); }
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event
            && interface == "lince_park_control_v1" && state.manager.is_none()
        { state.manager = Some(registry.bind(name, version.min(1), qh, ())); }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(state: &mut Self, _: &wl_callback::WlCallback, event: wl_callback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_callback::Event::Done { .. } = event { state.ready = true; }
    }
}

impl Dispatch<Manager, ()> for State {
    fn event(state: &mut Self, manager: &Manager, event: protocol::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            protocol::Event::Reply { id, body, serial, token } => {
                let body = match crate::control_protocol::read_response(body) {
                    Ok(body) => body,
                    Err(error) => { state.error = Some(error.to_string()); return; }
                };
                match serde_json::from_slice::<Response>(&body) {
                    Ok(Response::Failed { failure }) if id == 0 => state.error = Some(failure.message),
                    Ok(_) if id != 0 && id <= state.retired => {}
                    Ok(response) if state.expected == Some(id) && state.reply.is_none() => state.reply = Some(response),
                    Ok(_) => { state.error = Some("The management reply does not match the outstanding request".into()); return; }
                    Err(failure) => { state.error = Some(failure.to_string()); return; }
                }
                manager.acknowledge(serial, token);
            }
            protocol::Event::Changed { revision, scope, serial, token } => {
                match revision.parse() {
                    Ok(revision) => state.changed = Some((revision, scope)),
                    Err(_) => { state.error = Some("The management revision is invalid".into()); return; }
                }
                manager.acknowledge(serial, token);
            }
        }
    }
}
