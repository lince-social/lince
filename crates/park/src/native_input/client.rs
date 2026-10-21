use super::{Edit, Editor, MAX_TEXT};
use super::transfer;
use crate::server::KeyEvent;
use smithay::input::keyboard::{ModifiersState, xkb};
use std::{collections::VecDeque, io::{self, Read}, os::{fd::{AsFd, OwnedFd}, unix::net::UnixStream}};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
    protocol::{wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_surface}};
use wayland_protocols::wp::text_input::zv3::client::{zwp_text_input_manager_v3, zwp_text_input_v3};

mod dnd;
mod reader;

pub(crate) enum Event {
    Drop(transfer::Request),
    DropError(String),
    Edit(Edit),
    Key { code: u32, pressed: bool, event: KeyEvent },
}

pub(crate) struct Client {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    reader: reader::Reader,
}

#[derive(Default)]
struct State {
    dnd: dnd::Dnd,
    compositor: Option<wl_compositor::WlCompositor>,
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    manager: Option<zwp_text_input_manager_v3::ZwpTextInputManagerV3>,
    text: Option<zwp_text_input_v3::ZwpTextInputV3>,
    surface: Option<wl_surface::WlSurface>,
    keymap: Option<xkb::State>,
    keyboard_entered: bool,
    entered: bool,
    requested: Option<Editor>,
    sent: Option<Editor>,
    serial: u32,
    commits: VecDeque<(u32, Option<Editor>)>,
    pending: Pending,
    events: VecDeque<Event>,
    error: Option<String>,
}

#[derive(Default)]
struct Pending {
    preedit: Option<(String, Option<(usize, usize)>)>,
    commit: Option<String>,
    delete: Option<(u32, u32)>,
    invalid: bool,
}

impl Client {
    pub(crate) fn new(stream: UnixStream) -> io::Result<Self> {
        let connection = Connection::from_socket(stream).map_err(error)?;
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let reader = reader::Reader::new()?;
        let mut state = State::default();
        state.dnd.reader = Some(reader.handle.clone());
        Ok(Self { connection, queue, state, reader })
    }

    pub(crate) fn descriptors(&self) -> io::Result<Vec<OwnedFd>> {
        Ok(vec![self.connection.as_fd().try_clone_to_owned()?, self.reader.descriptor()?])
    }

    pub(crate) fn drop_target(&mut self, target: Option<transfer::Target>) { self.state.dnd.target(target); }

    pub(crate) fn suspend_drop(&mut self) { self.state.dnd.suspend(); }

    pub(crate) fn finish_drop(&mut self, token: u64, accepted: bool) {
        if let Some(event) = self.state.dnd.finished(token, accepted) { self.state.event(event); }
    }

    pub(crate) fn timeout(&self) -> Option<std::time::Duration> { self.state.dnd.timeout() }

    pub(crate) fn ready(&self) -> bool { self.state.surface.is_some() && self.state.keymap.is_some() }

    pub(crate) fn editor(&mut self, editor: Option<Editor>) {
        if self.state.requested.as_ref().map(|editor| editor.owner) != editor.as_ref().map(|editor| editor.owner) {
            self.state.dnd.suspend();
        }
        self.state.requested = editor;
    }

    pub(crate) fn dispatch(&mut self) -> io::Result<()> {
        for reply in self.reader.drain() {
            if let Some(event) = self.state.dnd.received(reply) { self.state.event(event); }
        }
        if let Some(event) = self.state.dnd.expire() { self.state.event(event); }
        if self.reader.failed() { return Err(io::Error::other("The native drop reader stopped unexpectedly")); }
        self.queue.dispatch_pending(&mut self.state).map_err(error)?;
        for _ in 0..4 {
            self.flush()?;
            let Some(read) = self.queue.prepare_read() else {
                self.queue.dispatch_pending(&mut self.state).map_err(error)?;
                continue;
            };
            let fd = read.connection_fd();
            let mut descriptors = [rustix::event::PollFd::new(&fd, rustix::event::PollFlags::IN)];
            rustix::event::poll(&mut descriptors, Some(&rustix::time::Timespec { tv_sec: 0, tv_nsec: 0 }))?;
            if descriptors[0].revents().is_empty() { break; }
            match read.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(value)) if value.kind() == io::ErrorKind::WouldBlock => break,
                Err(value) => return Err(error(value)),
            }
            self.queue.dispatch_pending(&mut self.state).map_err(error)?;
        }
        self.state.initialize(&self.queue.handle());
        self.state.synchronize();
        self.flush()?;
        if let Some(message) = self.state.error.take() { return Err(io::Error::other(message)); }
        Ok(())
    }

    fn flush(&self) -> io::Result<()> {
        match self.connection.flush() {
            Ok(()) => Ok(()),
            Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(value) => Err(error(value)),
        }
    }

    pub(crate) fn events(&mut self) -> Vec<Event> { self.state.events.drain(..).collect() }
}

impl State {
    fn initialize(&mut self, qh: &QueueHandle<Self>) {
        self.dnd.initialize(self.seat.as_ref(), qh);
        if self.surface.is_none() && let Some(compositor) = &self.compositor {
            let surface = compositor.create_surface(qh, ());
            surface.commit();
            self.surface = Some(surface);
        }
        if self.keyboard.is_none() && let Some(seat) = &self.seat {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        if self.text.is_none() && let (Some(manager), Some(seat)) = (&self.manager, &self.seat) {
            self.text = Some(manager.get_text_input(seat, qh, ()));
        }
    }

    fn commit(&mut self, context: Option<Editor>) {
        if let Some(text) = &self.text { text.commit(); }
        self.serial = self.serial.wrapping_add(1);
        if self.commits.len() == 16 { self.commits.pop_front(); }
        self.commits.push_back((self.serial, context));
    }

    fn synchronize(&mut self) {
        if self.text.is_none() { return; }
        let requested = self.entered.then(|| self.requested.clone()).flatten();
        if requested == self.sent { return; }
        let changed_owner = self.sent.as_ref().map(|editor| (editor.owner, editor.sensitive))
            != requested.as_ref().map(|editor| (editor.owner, editor.sensitive));
        if changed_owner && self.sent.is_some() {
            self.text.as_ref().unwrap().disable();
            self.commit(None);
        }
        if let Some(editor) = &requested {
            let text = self.text.as_ref().unwrap();
            if changed_owner || self.sent.is_none() { text.enable(); }
            let hint = if editor.sensitive {
                zwp_text_input_v3::ContentHint::SensitiveData | zwp_text_input_v3::ContentHint::HiddenText
            } else { zwp_text_input_v3::ContentHint::None };
            text.set_content_type(hint, if editor.sensitive {
                zwp_text_input_v3::ContentPurpose::Password
            } else { zwp_text_input_v3::ContentPurpose::Normal });
            text.set_surrounding_text(editor.surrounding.clone(), editor.cursor as i32, editor.anchor as i32);
            text.set_text_change_cause(if editor.from_ime { zwp_text_input_v3::ChangeCause::InputMethod }
                else { zwp_text_input_v3::ChangeCause::Other });
            text.set_cursor_rectangle(editor.caret[0], editor.caret[1], editor.caret[2], editor.caret[3]);
            self.commit(requested.clone());
        }
        self.sent = requested;
    }

    fn event(&mut self, event: Event) {
        if self.events.len() >= 256 { self.error = Some("Park native input queue exceeded its bound".into()); }
        else { self.events.push_back(event); }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event,
        _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_data_device_manager" => state.dnd.manager = Some(registry.bind(name, version.min(3), qh, ())),
                "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(4), qh, ())),
                "wl_seat" if state.seat.is_none() => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                "zwp_text_input_manager_v3" => state.manager = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for State {
    fn event(state: &mut Self, _: &zwp_text_input_v3::ZwpTextInputV3, event: zwp_text_input_v3::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_text_input_v3::Event::Enter { surface } => {
                state.entered = state.surface.as_ref() == Some(&surface);
                state.sent = None;
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                state.entered = false;
                state.sent = None;
                state.pending = Pending::default();
                state.commits.clear();
                state.events.retain(|event| !matches!(event, Event::Edit(_)));
            }
            zwp_text_input_v3::Event::PreeditString { text, cursor_begin, cursor_end } => {
                let value = text.unwrap_or_default();
                let cursor = if cursor_begin == -1 && cursor_end == -1 { None }
                    else if cursor_begin >= 0 && cursor_end >= 0
                        && value.is_char_boundary(cursor_begin as usize) && value.is_char_boundary(cursor_end as usize) {
                        Some((cursor_begin as usize, cursor_end as usize))
                    } else { state.pending.invalid = true; None };
                if value.len() > MAX_TEXT { state.pending.invalid = true; }
                else { state.pending.preedit = Some((value, cursor)); }
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                if text.as_ref().is_some_and(|value| value.len() > MAX_TEXT) { state.pending.invalid = true; }
                else { state.pending.commit = text; }
            }
            zwp_text_input_v3::Event::DeleteSurroundingText { before_length, after_length } => {
                state.pending.delete = Some((before_length, after_length));
            }
            zwp_text_input_v3::Event::Done { serial } => {
                let pending = std::mem::take(&mut state.pending);
                if state.entered && !pending.invalid && serial == state.serial
                    && let Some(context) = state.commits.iter().find(|(id, _)| *id == serial).and_then(|(_, editor)| editor.clone())
                    && state.requested.as_ref() == Some(&context)
                {
                    state.event(Event::Edit(Edit { context, preedit: Some(pending.preedit.unwrap_or_default()), commit: pending.commit, delete: pending.delete }));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Enter { surface, .. } => {
                state.keyboard_entered = state.surface.as_ref() == Some(&surface);
                state.events.retain(|event| !matches!(event, Event::Key { .. }));
            }
            wl_keyboard::Event::Leave { .. } => {
                state.keyboard_entered = false;
                state.events.retain(|event| !matches!(event, Event::Key { .. }));
            }
            wl_keyboard::Event::Keymap { format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1), fd, size } => {
                if size > 8 * 1024 * 1024 { state.error = Some("Park native keymap exceeds 8 MiB".into()); return; }
                let mut bytes = Vec::new();
                let result = std::fs::File::from(fd).take(u64::from(size)).read_to_end(&mut bytes);
                if result.is_err() || bytes.len() != size as usize { state.error = Some("Could not read Park native keymap".into()); return; }
                if bytes.last() == Some(&0) { bytes.pop(); }
                if bytes.contains(&0) { state.error = Some("Park native keymap contains an embedded NUL".into()); return; }
                let Some(keymap) = String::from_utf8(bytes).ok().and_then(|value| xkb::Keymap::new_from_string(
                    &xkb::Context::new(xkb::CONTEXT_NO_FLAGS), value, xkb::KEYMAP_FORMAT_TEXT_V1, xkb::KEYMAP_COMPILE_NO_FLAGS)) else {
                    state.error = Some("Could not parse Park native keymap".into()); return;
                };
                state.keymap = Some(xkb::State::new(&keymap));
            }
            wl_keyboard::Event::Modifiers { mods_depressed, mods_latched, mods_locked, group, .. } => {
                if let Some(keymap) = &mut state.keymap { keymap.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group); }
            }
            wl_keyboard::Event::Key { key, state: WEnum::Value(key_state), .. } => {
                if key > 767 || !state.keyboard_entered { return; }
                let Some(keymap) = &state.keymap else { return };
                let symbol = keymap.key_get_one_sym((key + 8).into());
                let active = |name| keymap.mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE);
                let value = keymap.key_get_utf8((key + 8).into());
                let event = KeyEvent { symbol: symbol.raw(), text: (!value.is_empty() && !value.chars().any(char::is_control)).then_some(value),
                    modifiers: ModifiersState { ctrl: active(xkb::MOD_NAME_CTRL), alt: active(xkb::MOD_NAME_ALT),
                        shift: active(xkb::MOD_NAME_SHIFT), logo: active(xkb::MOD_NAME_LOGO), ..Default::default() } };
                state.event(Event::Key { code: key, pressed: key_state == wl_keyboard::KeyState::Pressed, event });
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore zwp_text_input_manager_v3::ZwpTextInputManagerV3);

fn error(value: impl std::fmt::Display) -> io::Error { io::Error::other(value.to_string()) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn fragmented_registry_event_preserves_the_native_input_connection() {
        let (stream, mut server) = UnixStream::pair().unwrap();
        let mut client = Client::new(stream).unwrap();
        let mut message = Vec::new();
        for word in [2_u32, 36_u32 << 16, 1, 14] { message.extend_from_slice(&word.to_ne_bytes()); }
        message.extend_from_slice(b"wl_compositor\0");
        message.extend_from_slice(&[0; 2]);
        message.extend_from_slice(&6_u32.to_ne_bytes());
        server.write_all(&message[..4]).unwrap();
        client.dispatch().unwrap();
        assert!(client.state.compositor.is_none());
        server.write_all(&message[4..]).unwrap();
        client.dispatch().unwrap();
        assert!(client.state.compositor.is_some());
    }
}
