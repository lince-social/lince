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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaptureTarget { Output(OutputId), Window(SurfaceId) }

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
}

#[derive(Clone, Copy)]
struct CursorInfo { size: [i32; 2], position: Option<[i32; 2]> }

fn permitted(client: &Client) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| data.capabilities.contains(&crate::launch::Capability::Capture))
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
            sessions: Vec::new(), cursors: Vec::new(), pending: VecDeque::new(), cursor_info: BTreeMap::new(), suspended: false }
    }
}

impl State {
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
        let target = self.capture_source_target(&source);
        let size = target.and_then(|target| if cursor { Some(self.capture_cursor_size(target)) } else { self.capture_size(target) });
        if self.lock.owner.is_some() || target.is_none() || size.and_then(constraints).is_none() {
            frame.fail(CaptureFailureReason::Stopped); return;
        }
        let expected = size.unwrap();
        let valid = smithay::wayland::shm::with_buffer_contents(&frame.buffer(), |_, _, data|
            [data.width, data.height] == expected && data.format == smithay::reexports::wayland_server::protocol::wl_shm::Format::Argb8888).unwrap_or(false);
        if !valid { frame.fail(CaptureFailureReason::BufferConstraints); return; }
        if self.capture.pending.len() >= 8 { frame.fail(CaptureFailureReason::Unknown); return; }
        self.capture.pending.push_back(CaptureJob { source, target: target.unwrap(), cursor, paint_cursor, size: size.unwrap(), frame,
            deadline: Instant::now() + Duration::from_secs(2) });
        self.presentation_changed();
    }

    pub(super) fn stop_capture(&mut self) {
        self.capture.sessions.clear();
        self.capture.cursors.clear();
        self.capture.cursor_info.clear();
        for job in self.capture.pending.drain(..) { job.frame.fail(CaptureFailureReason::Stopped); }
        self.capture.copy.cleanup();
    }

    pub(super) fn refresh_capture(&mut self) {
        if self.lock.owner.is_some() { self.stop_capture(); return; }
        let sessions = std::mem::take(&mut self.capture.sessions);
        for session in sessions {
            if !session.alive() { continue; }
            let current = self.capture_source_target(&session.source()).and_then(|target| self.capture_size(target)).and_then(constraints);
            let Some(current) = current else { continue };
            if session.current_constraints().is_none_or(|old| old.size != current.size) { session.update_constraints(current); }
            self.capture.sessions.push(session);
        }
        let cursors = std::mem::take(&mut self.capture.cursors);
        for session in cursors {
            if !session.alive() { continue; }
            let available = self.capture_source_target(&session.source()).is_some_and(|target| self.capture_size(target).is_some());
            if available { self.capture.cursors.push(session); }
        }
        let suspended = self.capture.suspended;
        self.capture.pending.retain(|job| !job.frame.has_failed() && (suspended || job.deadline > Instant::now()));
        let obsolete: Vec<_> = self.capture.cursor_info.keys().copied().filter(|target| self.capture_size(*target).is_none()).collect();
        for target in obsolete { self.capture.cursor_info.remove(&target); }
        self.capture.copy.cleanup();
    }
}

impl Server {
    pub fn capture_suspend(&mut self, suspended: bool) {
        if self.state.capture.suspended && !suspended {
            let deadline = Instant::now() + Duration::from_secs(2);
            for job in &mut self.state.capture.pending { job.deadline = deadline; }
        }
        self.state.capture.suspended = suspended;
    }
    pub fn capture_jobs(&mut self) -> Vec<CaptureJob> { self.state.capture.pending.drain(..).collect() }
    pub fn capture_pending(&self) -> bool { !self.state.capture.pending.is_empty() }
    pub fn capture_job_valid(&self, job: &CaptureJob) -> bool {
        self.capture_job_failure(job).is_none()
    }
    pub fn capture_job_failure(&self, job: &CaptureJob) -> Option<CaptureFailureReason> {
        if self.locked() || job.frame.has_failed() || self.state.capture_source_target(&job.source) != Some(job.target) { return Some(CaptureFailureReason::Stopped); }
        let Some(size) = self.state.capture_size(job.target) else { return Some(CaptureFailureReason::Stopped) };
        let current = if job.cursor { self.state.capture_cursor_size(job.target) } else { size };
        if current != job.size { return Some(CaptureFailureReason::BufferConstraints); }
        if !self.state.capture.suspended && job.deadline <= Instant::now() { return Some(CaptureFailureReason::Unknown); }
        None
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
        constraints(self.capture_size(self.capture_source_target(source)?)?)
    }
    fn cursor_capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        if self.lock.owner.is_some() || self.capture.suspended || self.capture.cursors.len() >= 32 { return None; }
        let target = self.capture_source_target(source)?;
        self.capture_size(target)?;
        constraints(self.capture_cursor_size(target))
    }
    fn new_session(&mut self, session: Session) { self.capture.sessions.push(session); }
    fn new_cursor_session(&mut self, session: CursorSession) {
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
