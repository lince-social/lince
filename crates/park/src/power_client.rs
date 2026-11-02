use std::{io, time::{Duration, Instant}};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
    protocol::{wl_callback, wl_output, wl_registry}};
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1 as Manager,
    zwlr_output_power_v1::{self as power, ZwlrOutputPowerV1 as Power},
};

#[derive(Default)]
struct State {
    manager: Option<Manager>,
    outputs: Vec<wl_output::WlOutput>,
    powers: Vec<Power>,
    modes: Vec<Option<bool>>,
    synchronized: u8,
    failure: Option<&'static str>,
}

pub fn run(on: bool) -> io::Result<()> {
    if std::env::var_os("WAYLAND_SOCKET").is_none_or(|value| value.is_empty()) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Display power requires an inherited authorized Wayland connection"));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let connection = Connection::connect_to_env().map_err(io::Error::other)?;
    let mut queue = connection.new_event_queue();
    let mut state = State::default();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    connection.display().sync(&qh, 1);
    until(&connection, &mut queue, &mut state, deadline, |state| state.synchronized >= 1)?;
    let manager = state.manager.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "The connection has no output-power authority"))?;
    if state.outputs.is_empty() { return Err(io::Error::other("There are no outputs to power")); }
    for (index, output) in state.outputs.iter().enumerate() {
        state.powers.push(manager.get_output_power(output, &qh, index));
        state.modes.push(None);
    }
    connection.display().sync(&qh, 2);
    until(&connection, &mut queue, &mut state, deadline, |state| state.synchronized >= 2)?;
    if state.modes.iter().any(Option::is_none) { return Err(io::Error::other("The initial output-power state is unavailable")); }
    for (power, mode) in state.powers.iter().zip(&mut state.modes) {
        if *mode != Some(on) {
            *mode = None;
            power.set_mode(if on { power::Mode::On } else { power::Mode::Off });
        }
    }
    until(&connection, &mut queue, &mut state, deadline, |state| state.modes.iter().all(|mode| *mode == Some(on)))?;
    for power in state.powers { power.destroy(); }
    connection.flush().map_err(io::Error::other)
}

fn until(connection: &Connection, queue: &mut EventQueue<State>, state: &mut State, deadline: Instant, ready: impl Fn(&State) -> bool) -> io::Result<()> {
    loop {
        queue.dispatch_pending(state).map_err(io::Error::other)?;
        if let Some(failure) = state.failure { return Err(io::Error::other(failure)); }
        if ready(state) { return Ok(()); }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Err(io::Error::new(io::ErrorKind::TimedOut, "Output power was not confirmed within two seconds")); }
        let flushed = match connection.flush() {
            Ok(()) => true,
            Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => false,
            Err(error) => return Err(io::Error::other(error)),
        };
        if let Some(read) = queue.prepare_read() {
            let fd = read.connection_fd();
            let flags = rustix::event::PollFlags::IN | if flushed { rustix::event::PollFlags::empty() } else { rustix::event::PollFlags::OUT };
            let mut descriptors = [rustix::event::PollFd::new(&fd, flags)];
            let timeout = rustix::time::Timespec { tv_sec: remaining.as_secs() as i64, tv_nsec: i64::from(remaining.subsec_nanos()) };
            match rustix::event::poll(&mut descriptors, Some(&timeout)) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            if descriptors[0].revents().intersects(rustix::event::PollFlags::IN | rustix::event::PollFlags::HUP | rustix::event::PollFlags::ERR) {
                match read.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(io::Error::other(error)),
                }
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_output" => state.outputs.push(registry.bind(name, version.min(4), qh, ())),
                "zwlr_output_power_manager_v1" => state.manager = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<Power, usize> for State {
    fn event(state: &mut Self, _: &Power, event: power::Event, index: &usize, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            power::Event::Mode { mode: WEnum::Value(power::Mode::On) } => state.modes[*index] = Some(true),
            power::Event::Mode { mode: WEnum::Value(power::Mode::Off) } => state.modes[*index] = Some(false),
            _ => state.failure = Some("The compositor refused or invalidated an output-power request"),
        }
    }
}

impl Dispatch<wl_callback::WlCallback, u8> for State {
    fn event(state: &mut Self, _: &wl_callback::WlCallback, _: wl_callback::Event, phase: &u8, _: &Connection, _: &QueueHandle<Self>) {
        state.synchronized = *phase;
    }
}

delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore Manager);
