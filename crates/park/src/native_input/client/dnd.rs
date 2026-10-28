use super::{Event, State, reader::{self, Handle}};
use crate::native_input::transfer::{self, Destination, Kind, Request, Target};
use std::{io, os::{fd::AsFd, unix::net::UnixStream}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop,
    protocol::{wl_data_device, wl_data_device_manager::{self, DndAction}, wl_data_offer, wl_seat}};

struct Offer { object: wl_data_offer::WlDataOffer, mimes: Vec<String>, copy: bool, assigned: bool }
struct Active { offer: wl_data_offer::WlDataOffer, serial: u32, point: [f64; 2], mime: Option<String>, hover: Option<u64> }
struct Pending {
    token: u64, offer: wl_data_offer::WlDataOffer, mime: String, target: Target,
    cancelled: Arc<AtomicBool>, deadline: Instant, delivered: bool,
}

#[derive(Default)]
pub(super) struct Dnd {
    pub manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    device: Option<wl_data_device::WlDataDevice>,
    pub reader: Option<Handle>,
    target: Option<Target>,
    offers: Vec<Offer>,
    active: Option<Active>,
    pending: Option<Pending>,
    next_token: u64,
    next_hover: u64,
}

impl Dnd {
    pub(super) fn file_drag(&self) -> Option<u64> {
        let active = self.active.as_ref()?;
        self.offers.iter().any(|offer| offer.object == active.offer && offer.object.is_alive()
            && offer.mimes.iter().any(|mime| transfer::mime_kind(mime) == Some(Kind::Uri)))
            .then_some(active.hover).flatten()
    }

    pub(super) fn invalidate_hover(&mut self) {
        if let Some(active) = &mut self.active { active.hover = None; }
    }

    pub(super) fn initialize(&mut self, seat: Option<&wl_seat::WlSeat>, qh: &QueueHandle<State>) {
        if self.device.is_none() && let (Some(manager), Some(seat)) = (&self.manager, seat) {
            self.device = Some(manager.get_data_device(seat, qh, ()));
        }
    }

    pub(super) fn target(&mut self, target: Option<Target>) {
        if self.pending.as_ref().is_some_and(|pending| target.as_ref().is_none_or(|target| !pending.target.same_owner(target))) {
            self.cancel();
        }
        self.target = target;
        self.negotiate();
    }

    pub(super) fn suspend(&mut self) {
        self.cancel();
        self.target = None;
        self.negotiate();
    }

    fn cancel(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancelled.store(true, Ordering::Release);
            pending.offer.destroy();
        }
        self.prune();
    }

    fn prune(&mut self) {
        let active = self.active.as_ref().map(|active| &active.offer);
        let pending = self.pending.as_ref().map(|pending| &pending.offer);
        self.offers.retain(|offer| {
            let keep = !offer.assigned || Some(&offer.object) == active || Some(&offer.object) == pending;
            if !keep && offer.object.is_alive() { offer.object.destroy(); }
            keep
        });
    }

    fn negotiate(&mut self) {
        let Some(active) = &mut self.active else { return };
        let target = self.target.as_ref().filter(|target| target.pointer.iter().zip(active.point)
            .all(|(target, actual)| (target - actual).abs() < 2.0));
        let offer = self.offers.iter().find(|offer| offer.object == active.offer);
        let mime = target.zip(offer).and_then(|(target, offer)| {
            let preferred = if matches!(target.destination, Destination::Canvas(_)) { Kind::Uri } else { Kind::Text };
            offer.mimes.iter().find(|mime| transfer::mime_kind(mime) == Some(preferred))
                .or_else(|| offer.mimes.first()).cloned()
        });
        if active.mime != mime {
            active.offer.accept(active.serial, mime.clone());
            active.mime = mime;
        }
    }

    fn receive_drop(&mut self) -> Option<Event> {
        self.active.as_ref()?;
        self.cancel();
        let active = self.active.take()?;
        let target = self.target.clone().filter(|target| target.pointer.iter().zip(active.point)
            .all(|(target, actual)| (target - actual).abs() < 2.0));
        let copy = active.offer.version() < 3 || self.offers.iter().any(|offer| offer.object == active.offer && offer.copy);
        let Some((target, mime)) = target.zip(active.mime).filter(|_| copy) else {
            active.offer.destroy(); self.prune(); return None;
        };
        let Some(token) = self.next_token.checked_add(1) else {
            active.offer.destroy(); return Some(Event::DropError("Native drop identifiers are exhausted.".into()));
        };
        self.next_token = token;
        let cancelled = Arc::new(AtomicBool::new(false));
        let result = (|| {
            let reader = self.reader.as_ref().ok_or_else(|| io::Error::other("The drop reader is unavailable"))?;
            let (stream, writer) = UnixStream::pair()?;
            active.offer.receive(mime.clone(), writer.as_fd());
            reader.submit(token, stream, cancelled.clone())
        })();
        if let Err(error) = result {
            active.offer.destroy();
            cancelled.store(true, Ordering::Release);
            return Some(Event::DropError(error.to_string()));
        }
        self.pending = Some(Pending { token, offer: active.offer, mime, target, cancelled,
            deadline: Instant::now() + Duration::from_secs(3), delivered: false });
        None
    }

    pub(super) fn received(&mut self, reply: reader::Reply) -> Option<Event> {
        let pending = self.pending.as_mut().filter(|pending| pending.token == reply.token)?;
        if pending.cancelled.load(Ordering::Acquire) { self.cancel(); return None; }
        let decoded = reply.result.and_then(|bytes| transfer::decode(&pending.mime, bytes));
        match decoded {
            Ok(payload) => {
                pending.delivered = true;
                pending.deadline = Instant::now() + Duration::from_secs(5);
                Some(Event::Drop(Request { token: pending.token, target: pending.target.clone(), payload, cancelled: pending.cancelled.clone() }))
            }
            Err(message) => { self.cancel(); Some(Event::DropError(message)) }
        }
    }

    pub(super) fn finished(&mut self, token: u64, accepted: bool) -> Option<Event> {
        if self.pending.as_ref().is_none_or(|pending| pending.token != token || !pending.delivered) { return None; }
        let pending = self.pending.take().unwrap();
        let accepted = accepted && !pending.cancelled.load(Ordering::Acquire);
        if accepted && pending.offer.version() >= 3 { pending.offer.finish(); }
        pending.cancelled.store(true, Ordering::Release);
        pending.offer.destroy();
        self.prune();
        (!accepted).then(|| Event::DropError("The destination did not accept the drop.".into()))
    }

    pub(super) fn expire(&mut self) -> Option<Event> {
        if self.pending.as_ref().is_some_and(|pending| pending.deadline <= Instant::now() || pending.cancelled.load(Ordering::Acquire)) {
            self.cancel();
            return Some(Event::DropError("The native drop was cancelled before the destination confirmed it.".into()));
        }
        None
    }

    pub(super) fn timeout(&self) -> Option<Duration> { self.pending.as_ref().map(|pending| pending.deadline.saturating_duration_since(Instant::now())) }
}

impl Drop for Dnd { fn drop(&mut self) { self.cancel(); } }

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    fn event(state: &mut Self, _: &wl_data_device::WlDataDevice, event: wl_data_device::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                if state.dnd.offers.len() < 32 {
                    state.dnd.offers.push(Offer { object: id, mimes: Vec::new(), copy: false, assigned: false });
                } else { id.destroy(); }
            }
            wl_data_device::Event::Enter { serial, surface, x, y, id } => {
                state.dnd.active = None;
                if state.surface.as_ref() == Some(&surface) && let Some(offer) = id {
                    if let Some(stored) = state.dnd.offers.iter_mut().find(|stored| stored.object == offer) { stored.assigned = true; }
                    if offer.version() >= 3 { offer.set_actions(DndAction::Copy, DndAction::Copy); }
                    let hover = state.dnd.next_hover.checked_add(1);
                    if let Some(token) = hover { state.dnd.next_hover = token; }
                    state.dnd.active = Some(Active { offer, serial, point: [x, y], mime: None, hover });
                    state.dnd.negotiate();
                }
                state.dnd.prune();
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                if let Some(active) = &mut state.dnd.active { active.point = [x, y]; }
                state.dnd.negotiate();
            }
            wl_data_device::Event::Leave => {
                if let Some(active) = state.dnd.active.take()
                    && state.dnd.pending.as_ref().is_none_or(|pending| pending.offer != active.offer)
                    && active.offer.is_alive() { active.offer.destroy(); }
                state.dnd.prune();
            }
            wl_data_device::Event::Drop => {
                if let Some(event) = state.dnd.receive_drop() { state.event(event); }
            }
            wl_data_device::Event::Selection { id: Some(offer) } => {
                state.dnd.offers.retain(|existing| existing.object != offer);
                offer.destroy();
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(state: &mut Self, offer: &wl_data_offer::WlDataOffer, event: wl_data_offer::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let Some(stored) = state.dnd.offers.iter_mut().find(|existing| &existing.object == offer) else { return };
        match event {
            wl_data_offer::Event::Offer { mime_type } if transfer::mime_kind(&mime_type).is_some()
                && stored.mimes.len() < 16 && !stored.mimes.contains(&mime_type) => stored.mimes.push(mime_type),
            wl_data_offer::Event::Action { dnd_action } => stored.copy = matches!(dnd_action, WEnum::Value(action) if action == DndAction::Copy),
            _ => {}
        }
        let cancelled = !stored.copy && state.dnd.pending.as_ref().is_some_and(|pending| &pending.offer == offer);
        if cancelled {
            state.dnd.cancel();
            state.event(Event::DropError("The drop source changed its negotiated action.".into()));
        }
        state.dnd.negotiate();
    }
}

delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);
