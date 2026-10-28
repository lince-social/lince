use super::*;
use lince_smithay_capture::wayland::{
    image_capture_source::{ImageCaptureSource, ImageCaptureSourceHandler, OutputCaptureSourceHandler, OutputCaptureSourceState,
        ToplevelCaptureSourceHandler, ToplevelCaptureSourceState},
    image_copy_capture::{BufferConstraints, CaptureFailureReason, CursorSession, CursorSessionRef, Frame, FrameRef,
        ImageCopyCaptureHandler, ImageCopyCaptureState, Session, SessionRef},
};
use smithay::{utils::IsAlive, wayland::foreign_toplevel_list::ForeignToplevelHandle};
use std::time::Duration;

pub(crate) const MAX_CAPTURE_PIXELS: u64 = 16 * 1024 * 1024;

pub use lince_interface::host::capture::Target as CaptureTarget;
use lince_interface::host::capture::PolicyGate;

#[derive(Clone, Copy)]
struct SessionTag { id: u64, connection: u64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentOwner { Native, Window(SurfaceId), Layer, Unknown }

struct DemandLease {
    target: CaptureTarget,
    counts: Arc<Mutex<BTreeMap<CaptureTarget, usize>>>,
}

impl Drop for DemandLease {
    fn drop(&mut self) {
        if let Ok(mut counts) = self.counts.lock()
            && let Some(count) = counts.get_mut(&self.target)
        {
            *count = count.saturating_sub(1);
            if *count == 0 { counts.remove(&self.target); }
        }
    }
}

enum Owner {
    Output(smithay::output::WeakOutput),
    Window { surface: smithay::reexports::wayland_server::Weak<WlSurface>, identifier: String },
}

pub struct CaptureJob {
    pub source: ImageCaptureSource,
    pub target: CaptureTarget,
    pub cursor: bool,
    pub paint_cursor: bool,
    pub size: [i32; 2],
    pub frame: Frame,
    pub deadline: Instant,
    pub policy_revision: u64,
    pub mapping_revision: u64,
    pub mappings: Vec<lince_interface::host::capture::Mapping>,
    pub output_mapping: Option<lince_interface::host::mapping::Mapping>,
    pub color_revision: Option<(OutputId, u64)>,
    pub window: Option<WindowSnapshot>,
    connection: Arc<AtomicBool>,
    _demand: Option<DemandLease>,
}

pub(super) struct Capture {
    outputs: OutputCaptureSourceState,
    toplevels: ToplevelCaptureSourceState,
    copy: ImageCopyCaptureState,
    sessions: Vec<Session>,
    cursors: Vec<CursorSession>,
    pending: VecDeque<CaptureJob>,
    cursor_info: BTreeMap<CaptureTarget, CursorInfo>,
    suspended: bool,
    gate: PolicyGate,
    next_session: u64,
    connections: HashMap<ClientId, u64>,
    next_connection: u64,
    demand: Arc<Mutex<BTreeMap<CaptureTarget, usize>>>,
}

#[derive(Clone, Copy)]
struct CursorInfo { size: [i32; 2], position: Option<[i32; 2]> }

fn permitted(client: &Client) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| data.capabilities.contains(&crate::launch::Capability::Capture))
}

fn connected(client: Option<Client>) -> bool {
    client.is_some_and(|client| permitted(&client) && client.get_data::<ClientState>().is_some_and(|data| data.connected.load(Ordering::Acquire)))
}

fn constraints(size: [i32; 2]) -> Option<BufferConstraints> {
    if validate_size(size).is_err() || size[0] as u64 * size[1] as u64 > MAX_CAPTURE_PIXELS { return None; }
    Some(BufferConstraints { size: (size[0], size[1]).into(), shm: vec![smithay::reexports::wayland_server::protocol::wl_shm::Format::Argb8888], dma: None })
}

impl Capture {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        Self { outputs: OutputCaptureSourceState::new_with_filter::<State, _>(display, permitted),
            toplevels: ToplevelCaptureSourceState::new_with_filter::<State, _>(display, permitted),
            copy: ImageCopyCaptureState::new_with_filter::<State, _>(display, permitted),
            sessions: Vec::new(), cursors: Vec::new(), pending: VecDeque::new(), cursor_info: BTreeMap::new(), suspended: false, gate: PolicyGate::default(),
            next_session: 0, connections: HashMap::new(), next_connection: 0, demand: Default::default() }
    }
}

impl State {
    pub(super) fn capture_content_owner(&self, surface: Option<WlSurface>) -> ContentOwner {
        let Some(root) = surface.as_ref().and_then(|surface| self.input_root(surface)) else { return ContentOwner::Unknown; };
        if self.native_surface.as_ref() == Some(&root) { return ContentOwner::Native; }
        if let Some(window) = self.id(&root) { return ContentOwner::Window(window); }
        if self.layers.surfaces.contains_key(&root) { return ContentOwner::Layer; }
        ContentOwner::Unknown
    }

    fn capture_tag(&mut self, client: Option<Client>) -> Option<SessionTag> {
        let client = client?;
        if !permitted(&client) || !client.get_data::<ClientState>()?.connected.load(Ordering::Acquire) { return None; }
        let connection = if let Some(id) = self.capture.connections.get(&client.id()) { *id } else {
            self.capture.next_connection = self.capture.next_connection.checked_add(1)?;
            let id = self.capture.next_connection;
            self.capture.connections.insert(client.id(), id);
            id
        };
        self.capture.next_session = self.capture.next_session.checked_add(1)?;
        Some(SessionTag { id: self.capture.next_session, connection })
    }

    fn capture_cursor_size(&self, target: CaptureTarget) -> [i32; 2] { self.capture.cursor_info.get(&target).map_or([24, 24], |info| info.size) }
    fn capture_source_target(&self, source: &ImageCaptureSource) -> Option<CaptureTarget> {
        let target = *source.user_data().get::<CaptureTarget>()?;
        let valid = match (source.user_data().get::<Owner>()?, target) {
            (Owner::Output(owner), CaptureTarget::Output(id)) => owner.upgrade().is_some_and(|owner| self.outputs.get(&id).is_some_and(|registered| registered.output == owner)),
            (Owner::Window { surface, identifier }, CaptureTarget::Window(id)) => surface.upgrade().ok().is_some_and(|owner|
                self.windows.get(&id).is_some_and(|window| window.role.wl_surface() == &owner)
                    && self.foreign_handles.get(&id).is_some_and(|handle| handle.identifier() == *identifier)),
            _ => false,
        };
        valid.then_some(target)
    }

    fn capture_policy_revision(&self, target: CaptureTarget) -> Option<u64> {
        self.capture.gate.read().filter(|policy| policy.permits(target)
            && (!matches!(target, CaptureTarget::Output(_)) || self.windows.iter().all(|(id, window)| !window.mapped || policy.knows(*id))))
            .map(|policy| policy.revision())
    }

    fn capture_private_window(&self, target: CaptureTarget) -> bool {
        matches!(target, CaptureTarget::Window(surface) if self.capture.gate.read().is_some_and(|policy| policy.knows(surface) && !policy.permits(target)))
    }

    fn capture_size(&self, target: CaptureTarget) -> Option<[i32; 2]> {
        match target {
            CaptureTarget::Output(id) => self.outputs.get(&id)?.output.current_mode().map(|mode| [mode.size.w, mode.size.h]),
            CaptureTarget::Window(id) => {
                let window = self.windows.get(&id)?;
                if !window.mapped || !window.role.wl_surface().is_alive() { return None; }
                let nodes = self.nodes(window.role.wl_surface());
                let mut bounds = None;
                for node in nodes {
                    let rectangle = Rectangle::<f64, Logical>::new((f64::from(node.origin[0]), f64::from(node.origin[1])).into(),
                        (node.layout.size[0], node.layout.size[1]).into());
                    bounds = Some(bounds.map_or(rectangle, |bounds: Rectangle<f64, Logical>| bounds.merge(rectangle)));
                }
                let bounds = bounds?;
                let geometry = with_states(window.role.wl_surface(), |states| states.cached_state.get::<xdg::SurfaceCachedState>().current().geometry);
                let bounds = if let Some(geometry) = geometry { bounds.intersection(geometry.to_f64())? } else { bounds };
                Some([bounds.size.w.ceil() as i32, bounds.size.h.ceil() as i32])
            }
        }
    }

    fn queue_capture(&mut self, source: ImageCaptureSource, frame: Frame, cursor: bool, paint_cursor: bool) {
        let Some(connection) = frame.client().filter(|client| permitted(client)).and_then(|client| client.get_data::<ClientState>().map(|data| data.connected.clone()))
            .filter(|connection| connection.load(Ordering::Acquire))
        else { frame.fail(CaptureFailureReason::Stopped); return; };
        let target = self.capture_source_target(&source);
        let size = target.and_then(|target| if cursor { Some(self.capture_cursor_size(target)) } else { self.capture_size(target) });
        if self.lock.owner.is_some() || target.is_none() || size.and_then(constraints).is_none() {
            frame.fail(CaptureFailureReason::Stopped); return;
        }
        let expected = size.unwrap();
        if target.is_some_and(|target| matches!(target, CaptureTarget::Output(output) if !self.output_powered(output))) {
            frame.fail(CaptureFailureReason::Unknown); return;
        }
        let valid = smithay::wayland::shm::with_buffer_contents(&frame.buffer(), |_, _, data|
            [data.width, data.height] == expected && data.format == smithay::reexports::wayland_server::protocol::wl_shm::Format::Argb8888).unwrap_or(false);
        if !valid { frame.fail(CaptureFailureReason::BufferConstraints); return; }
        if self.capture.pending.len() >= 8 { frame.fail(CaptureFailureReason::Unknown); return; }
        let Some(policy_revision) = self.capture_policy_revision(target.unwrap()) else {
            frame.fail(if self.capture_private_window(target.unwrap()) { CaptureFailureReason::Stopped } else { CaptureFailureReason::Unknown }); return;
        };
        let demand = if let Some(target) = target.filter(|_| !cursor) {
            let Ok(mut counts) = self.capture.demand.lock() else { frame.fail(CaptureFailureReason::Stopped); return; };
            *counts.entry(target).or_default() += 1;
            Some(DemandLease { target, counts: self.capture.demand.clone() })
        } else { None };
        let mappings = self.output_viewports().iter().map(|(_, output)| output.into()).collect();
        let mapping_revision = self.output_control.revision();
        let output_mapping = match target.unwrap() { CaptureTarget::Output(output) => self.presented_mappings.get(&output).cloned(), CaptureTarget::Window(_) => None };
        let color_revision = match target.unwrap() { CaptureTarget::Output(output) => Some((output, self.output_control.color.get(&output).map_or(0, |color| color.revision))), CaptureTarget::Window(_) => None };
        let window = match target.unwrap() { CaptureTarget::Window(id) if !cursor => self.window_snapshot(id), _ => None };
        self.capture.pending.push_back(CaptureJob { source, target: target.unwrap(), cursor, paint_cursor, size: size.unwrap(), frame,
            deadline: Instant::now() + Duration::from_secs(2), policy_revision, mapping_revision,
            mappings, output_mapping, color_revision, window, connection, _demand: demand });
        self.presentation_changed();
    }

    pub(super) fn stop_capture(&mut self) {
        self.capture.sessions.clear();
        self.capture.cursors.clear();
        self.capture.cursor_info.clear();
        self.capture.connections.clear();
        for job in self.capture.pending.drain(..) { job.frame.fail(CaptureFailureReason::Stopped); }
        self.capture.copy.cleanup();
    }

    pub(super) fn refresh_capture(&mut self) {
        if self.lock.owner.is_some() { self.stop_capture(); return; }
        let sessions = std::mem::take(&mut self.capture.sessions);
        for session in sessions {
            if !session.alive() || !connected(session.client()) { continue; }
            let current = self.capture_source_target(&session.source()).filter(|target| !self.capture_private_window(*target)).and_then(|target| self.capture_size(target)).and_then(constraints);
            let Some(current) = current else { continue };
            if session.current_constraints().is_none_or(|old| old.size != current.size) { session.update_constraints(current); }
            self.capture.sessions.push(session);
        }
        let cursors = std::mem::take(&mut self.capture.cursors);
        for session in cursors {
            if !session.alive() || !connected(session.client()) { continue; }
            let available = self.capture_source_target(&session.source()).is_some_and(|target| !self.capture_private_window(target) && self.capture_size(target).is_some());
            if available { self.capture.cursors.push(session); }
        }
        let suspended = self.capture.suspended;
        self.capture.pending.retain(|job| !job.frame.has_failed() && (suspended || job.deadline > Instant::now()));
        let obsolete: Vec<_> = self.capture.cursor_info.keys().copied().filter(|target| self.capture_size(*target).is_none()).collect();
        for target in obsolete { self.capture.cursor_info.remove(&target); }
        self.capture.copy.cleanup();
        let clients: HashSet<_> = self.capture.sessions.iter().filter_map(|session| session.client()).map(|client| client.id())
            .chain(self.capture.cursors.iter().filter_map(|session| session.client()).map(|client| client.id())).collect();
        self.capture.connections.retain(|id, _| clients.contains(id));
    }
}

impl Server {
    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn capture_test_deadline(&mut self, deadline: Instant) {
        self.state.capture.pending.front_mut().unwrap().deadline = deadline;
    }

    pub fn capture_policy_gate(&self) -> PolicyGate { self.state.capture.gate.clone() }

    pub fn capture_demand(&self) -> lince_interface::host::capture::Demand {
        let mut demand = lince_interface::host::capture::Demand::default();
        if let Ok(counts) = self.state.capture.demand.lock() {
            for target in counts.keys() {
                match target {
                    CaptureTarget::Output(output) => { demand.outputs.insert(*output); }
                    CaptureTarget::Window(window) => { demand.windows.insert(*window); }
                }
            }
        }
        demand
    }

    pub fn capture_pointer_owner(&self) -> ContentOwner {
        self.state.capture_content_owner(self.state.seat.get_pointer().and_then(|pointer| pointer.current_focus()))
    }

    pub fn capture_cursor_owner(&self) -> ContentOwner {
        if matches!(self.state.cursor, CursorImageStatus::Surface(_)) {
            self.state.capture_content_owner(self.state.cursor_origin.as_ref().and_then(|surface| surface.upgrade().ok()))
        } else { self.capture_pointer_owner() }
    }

    pub fn capture_drag_owner(&self) -> ContentOwner {
        if !self.state.dragging { return ContentOwner::Unknown; }
        self.state.capture_content_owner(self.state.drag_origin.as_ref().and_then(|surface| surface.upgrade().ok()))
    }

    pub fn capture_status(&self) -> lince_interface::host::capture::Status {
        let entries = self.state.capture.sessions.iter().filter(|session| session.alive() && connected(session.client())).map(|session| (session.source(), session.user_data().get::<SessionTag>(), false))
            .chain(self.state.capture.cursors.iter().filter(|session| session.alive() && connected(session.client())).map(|session| (session.source(), session.user_data().get::<SessionTag>(), true)));
        let mut sessions: Vec<_> = entries.filter_map(|(source, tag, cursor)| {
            let tag = tag?;
            let target = self.state.capture_source_target(&source)?;
            let label = match target {
                CaptureTarget::Output(id) => self.state.outputs.get(&id)?.output.name(),
                CaptureTarget::Window(id) => {
                    let window = self.state.windows.get(&id)?;
                    with_states(window.role.wl_surface(), |states| states.data_map.get::<XdgToplevelSurfaceData>().map(|metadata|
                        bounded(metadata.lock().unwrap_or_else(|error| error.into_inner()).title.as_deref().unwrap_or_default())))?
                }
            };
            Some(lince_interface::host::capture::Session { id: tag.id, connection: tag.connection, target, label, cursor,
                paused: self.state.capture.suspended || self.locked() || self.state.capture_policy_revision(target).is_none()
                    || matches!(target, CaptureTarget::Output(output) if !self.state.output_powered(output)) })
        }).collect();
        sessions.sort_by_key(|session| session.id);
        lince_interface::host::capture::Status { available: true, paused: self.state.capture.suspended || self.locked(), sessions }
    }

    pub fn stop_capture_session(&mut self, id: u64) {
        self.state.capture.sessions.retain(|session| session.user_data().get::<SessionTag>().is_none_or(|tag| tag.id != id));
        self.state.capture.cursors.retain(|session| session.user_data().get::<SessionTag>().is_none_or(|tag| tag.id != id));
        self.state.refresh_capture();
        self.state.presentation_changed();
    }

    pub fn capture_suspend(&mut self, suspended: bool) {
        self.state.capture.suspended = suspended;
    }
    pub fn capture_jobs(&mut self) -> Vec<CaptureJob> { self.state.capture.pending.drain(..).collect() }
    pub fn capture_pending(&self) -> bool { !self.state.capture.pending.is_empty() }
    pub fn capture_job_valid(&self, job: &CaptureJob) -> bool {
        self.capture_job_failure(job).is_none()
    }
    pub fn capture_job_failure(&self, job: &CaptureJob) -> Option<CaptureFailureReason> {
        if self.state.capture_policy_revision(job.target) != Some(job.policy_revision) {
            #[cfg(test)]
            eprintln!("Capture policy retired {:?}: requested={}, current={:?}, policy={:?}", job.target, job.policy_revision,
                self.state.capture_policy_revision(job.target), self.state.capture.gate.read().and_then(|read| read.policy().cloned()));
            return Some(CaptureFailureReason::Stopped);
        }
        self.capture_job_base_failure(job)
    }
    fn capture_job_base_failure(&self, job: &CaptureJob) -> Option<CaptureFailureReason> {
        if matches!(job.target, CaptureTarget::Output(output) if !self.state.output_powered(output)) { return Some(CaptureFailureReason::Unknown); }
        if !job.connection.load(Ordering::Acquire) || self.locked() || job.frame.has_failed() || self.state.capture_source_target(&job.source) != Some(job.target) { return Some(CaptureFailureReason::Stopped); }
        if job.color_revision.is_some_and(|(output, revision)| self.state.output_control.color.get(&output).map_or(0, |color| color.revision) != revision) {
            return Some(CaptureFailureReason::Unknown);
        }
        if self.state.output_control.revision() != job.mapping_revision
            || self.state.output_viewports().iter().map(|(_, output)| lince_interface::host::capture::Mapping::from(output)).collect::<Vec<_>>() != job.mappings
        { return Some(CaptureFailureReason::Unknown); }
        if let CaptureTarget::Output(output) = job.target {
            let same = match (self.state.presented_mappings.get(&output), job.output_mapping.as_ref()) {
                (Some(current), Some(captured)) => current.same_mapping(captured),
                (None, None) => true,
                _ => false,
            };
            if !same { return Some(CaptureFailureReason::Unknown); }
        }
        let Some(size) = self.state.capture_size(job.target) else { return Some(CaptureFailureReason::Stopped) };
        let current = if job.cursor { self.state.capture_cursor_size(job.target) } else { size };
        if current != job.size { return Some(CaptureFailureReason::BufferConstraints); }
        if !self.state.capture.suspended && job.deadline <= Instant::now() { return Some(CaptureFailureReason::Unknown); }
        None
    }
    #[cfg(feature = "nested")]
    pub(crate) fn complete_capture(
        &mut self, job: CaptureJob, time: Duration, hotspot: [i32; 2],
        write: impl FnOnce(&CaptureJob) -> std::io::Result<()>,
    ) {
        if let Some(reason) = self.capture_job_failure(&job) { job.frame.fail(reason); return; }
        let gate = self.state.capture.gate.clone();
        let Some(policy) = gate.read().filter(|policy| policy.matches(job.target, job.policy_revision)) else {
            job.frame.fail(CaptureFailureReason::Stopped); return;
        };
        match write(&job) {
            Ok(()) => {
                if job.cursor { self.capture_cursor_ready(job.target, hotspot); }
                job.frame.success(Transform::Normal, None, time);
            }
            Err(error) => {
                eprintln!("Park capture readback: {error}");
                job.frame.fail(CaptureFailureReason::Unknown);
            }
        }
        drop(policy);
    }
    pub fn capture_cursor_targets(&self) -> Vec<CaptureTarget> {
        self.state.capture.cursors.iter().filter_map(|session| self.state.capture_source_target(&session.source()))
            .collect::<BTreeSet<_>>().into_iter().collect()
    }
    pub fn capture_cursor(&mut self, target: CaptureTarget, size: [i32; 2], position: Option<[i32; 2]>) {
        if self.state.capture_size(target).is_none() { return; }
        let Some(constraints) = constraints(size) else {
            self.state.capture.cursors.retain(|session| session.source().user_data().get::<CaptureTarget>() != Some(&target));
            return;
        };
        self.state.capture.cursor_info.insert(target, CursorInfo { size, position });
        for session in &self.state.capture.cursors {
            if session.source().user_data().get::<CaptureTarget>() != Some(&target) { continue; }
            if session.current_constraints().is_none_or(|old| old.size != constraints.size) { session.update_constraints(constraints.clone()); }
            session.set_cursor_pos(position.map(|position| (position[0], position[1]).into()));
        }
    }
    pub fn capture_cursor_ready(&mut self, target: CaptureTarget, hotspot: [i32; 2]) {
        for session in &self.state.capture.cursors {
            if session.source().user_data().get::<CaptureTarget>() == Some(&target) { session.set_cursor_hotspot((hotspot[0], hotspot[1])); }
        }
    }
    pub fn capture_timeout(&self) -> Option<Duration> {
        if self.state.capture.suspended { return None; }
        self.state.capture.pending.iter().map(|job| job.deadline.saturating_duration_since(Instant::now())).min()
    }
}

impl ImageCaptureSourceHandler for State {}

impl OutputCaptureSourceHandler for State {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState { &mut self.capture.outputs }
    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        if let Some(id) = self.outputs.iter().find_map(|(id, registered)| (&registered.output == output).then_some(*id)) {
            source.user_data().insert_if_missing(|| CaptureTarget::Output(id));
            source.user_data().insert_if_missing(|| Owner::Output(output.downgrade()));
        }
    }
}

impl ToplevelCaptureSourceHandler for State {
    fn toplevel_capture_source_state(&mut self) -> &mut ToplevelCaptureSourceState { &mut self.capture.toplevels }
    fn toplevel_source_created(&mut self, source: ImageCaptureSource, toplevel: &ForeignToplevelHandle) {
        if let Some(id) = self.foreign_handles.iter().find_map(|(id, handle)| (handle.identifier() == toplevel.identifier()).then_some(*id)) {
            source.user_data().insert_if_missing(|| CaptureTarget::Window(id));
            if let Some(window) = self.windows.get(&id) {
                source.user_data().insert_if_missing(|| Owner::Window { surface: window.role.wl_surface().downgrade(), identifier: toplevel.identifier() });
            }
        }
    }
}

impl ImageCopyCaptureHandler for State {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState { &mut self.capture.copy }
    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        if self.lock.owner.is_some() || self.capture.suspended || self.capture.sessions.len() >= 32 { return None; }
        let target = self.capture_source_target(source)?;
        if self.capture_private_window(target) { return None; }
        constraints(self.capture_size(target)?)
    }
    fn cursor_capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        if self.lock.owner.is_some() || self.capture.suspended || self.capture.cursors.len() >= 32 { return None; }
        let target = self.capture_source_target(source)?;
        if self.capture_private_window(target) { return None; }
        self.capture_size(target)?;
        constraints(self.capture_cursor_size(target))
    }
    fn new_session(&mut self, session: Session) {
        let Some(tag) = self.capture_tag(session.client()) else { return; };
        session.user_data().insert_if_missing(|| tag);
        self.capture.sessions.push(session);
        self.presentation_changed();
    }
    fn new_cursor_session(&mut self, session: CursorSession) {
        let Some(tag) = self.capture_tag(session.client()) else { return; };
        session.user_data().insert_if_missing(|| tag);
        if let Some(info) = session.source().user_data().get::<CaptureTarget>().and_then(|target| self.capture.cursor_info.get(target)) {
            session.set_cursor_pos(info.position.map(|position| (position[0], position[1]).into()));
        }
        self.capture.cursors.push(session); self.presentation_changed();
    }
    fn frame(&mut self, session: &SessionRef, frame: Frame) { self.queue_capture(session.source(), frame, false, session.draw_cursor()); }
    fn cursor_frame(&mut self, session: &CursorSessionRef, frame: Frame) { self.queue_capture(session.source(), frame, true, false); }
    fn frame_aborted(&mut self, frame: FrameRef) { self.capture.pending.retain(|job| *job.frame != frame); }
    fn session_destroyed(&mut self, session: SessionRef) { self.capture.sessions.retain(|owned| owned != &session); self.capture.copy.cleanup(); }
    fn cursor_session_destroyed(&mut self, session: CursorSessionRef) { self.capture.cursors.retain(|owned| owned != &session); self.capture.copy.cleanup(); }
}

lince_smithay_capture::delegate_image_capture_source!(State);
lince_smithay_capture::delegate_output_capture_source!(State);
lince_smithay_capture::delegate_toplevel_capture_source!(State);
lince_smithay_capture::delegate_image_copy_capture!(State);
